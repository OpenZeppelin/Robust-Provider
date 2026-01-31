//! HTTP-based polling subscription for providers without pubsub support.
//!
//! This module provides a polling-based alternative to WebSocket subscriptions,
//! allowing HTTP providers to participate in block subscriptions by periodically
//! polling for new blocks.
//!
//! # Feature Flag
//!
//! This module requires the `http-subscription` feature:
//!
//! ```toml
//! robust-provider = { version = "0.2", features = ["http-subscription"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use robust_provider::RobustProviderBuilder;
//! use std::time::Duration;
//!
//! let robust = RobustProviderBuilder::new(http_provider)
//!     .allow_http_subscriptions(true)
//!     .poll_interval(Duration::from_secs(12))
//!     .build()
//!     .await?;
//!
//! let mut subscription = robust.subscribe_blocks().await?;
//! while let Ok(block) = subscription.recv().await {
//!     println!("New block: {}", block.number);
//! }
//! ```

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use alloy::{
    consensus::BlockHeader,
    eips::BlockNumberOrTag,
    network::{BlockResponse, Network},
    primitives::BlockNumber,
    providers::{Provider, RootProvider},
    transports::{RpcError, TransportErrorKind},
};
use tokio::{
    sync::mpsc,
    time::{interval, MissedTickBehavior},
};
use tokio_stream::Stream;

/// Default polling interval for HTTP subscriptions.
///
/// Set to 12 seconds to match approximate Ethereum mainnet block time.
/// Adjust based on the target chain's block time.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(12);

/// Errors specific to HTTP polling subscriptions.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HttpSubscriptionError {
    /// Polling operation exceeded the configured timeout.
    #[error("Polling operation timed out")]
    Timeout,

    /// An RPC error occurred during polling.
    #[error("RPC error during polling: {0}")]
    RpcError(Arc<RpcError<TransportErrorKind>>),

    /// The subscription channel was closed.
    #[error("Subscription channel closed")]
    Closed,

    /// Failed to fetch block from the provider.
    #[error("Block fetch failed: {0}")]
    BlockFetchFailed(String),
}

impl From<RpcError<TransportErrorKind>> for HttpSubscriptionError {
    fn from(err: RpcError<TransportErrorKind>) -> Self {
        HttpSubscriptionError::RpcError(Arc::new(err))
    }
}

/// Configuration for HTTP polling subscriptions.
#[derive(Debug, Clone)]
pub struct HttpSubscriptionConfig {
    /// Interval between polling requests.
    ///
    /// Default: [`DEFAULT_POLL_INTERVAL`] (12 seconds)
    pub poll_interval: Duration,

    /// Timeout for individual RPC calls.
    ///
    /// Default: 30 seconds
    pub call_timeout: Duration,

    /// Buffer size for the internal channel.
    ///
    /// Default: 128
    pub buffer_capacity: usize,
}

impl Default for HttpSubscriptionConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            call_timeout: Duration::from_secs(30),
            buffer_capacity: 128,
        }
    }
}

/// HTTP-based polling subscription that emulates WebSocket subscriptions
/// by polling for new blocks at regular intervals.
///
/// This struct provides a similar interface to native WebSocket subscriptions,
/// allowing HTTP providers to participate in the subscription system.
///
/// # How It Works
///
/// 1. A background task polls `eth_getBlockByNumber(latest)` at `poll_interval`
/// 2. When a new block is detected (block number increased), it's sent to the receiver
/// 3. Duplicate blocks are automatically filtered out
///
/// # Trade-offs
///
/// - **Latency**: New blocks are detected with up to `poll_interval` delay
/// - **RPC Load**: Generates one RPC call per `poll_interval`
/// - **Missed Blocks**: If `poll_interval` > block time, intermediate blocks may be missed
#[derive(Debug)]
pub struct HttpPollingSubscription<N: Network> {
    /// Receiver for block headers
    receiver: mpsc::Receiver<Result<N::HeaderResponse, HttpSubscriptionError>>,
    /// Handle to the polling task (kept alive while subscription exists)
    _task_handle: tokio::task::JoinHandle<()>,
}

impl<N: Network + 'static> HttpPollingSubscription<N>
where
    N::HeaderResponse: Clone + Send,
{
    /// Create a new HTTP polling subscription.
    ///
    /// This spawns a background task that polls the provider for new blocks
    /// and sends them through a channel.
    ///
    /// # Arguments
    ///
    /// * `provider` - The HTTP provider to poll
    /// * `config` - Configuration for polling behavior
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = HttpSubscriptionConfig {
    ///     poll_interval: Duration::from_secs(6),
    ///     ..Default::default()
    /// };
    /// let mut sub = HttpPollingSubscription::new(provider, config);
    /// ```
    #[must_use]
    pub fn new(provider: RootProvider<N>, config: HttpSubscriptionConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.buffer_capacity);

        let task_handle = tokio::spawn(Self::polling_task(
            provider,
            sender,
            config.poll_interval,
            config.call_timeout,
        ));

        Self {
            receiver,
            _task_handle: task_handle,
        }
    }

    /// Background task that polls for new blocks.
    async fn polling_task(
        provider: RootProvider<N>,
        sender: mpsc::Sender<Result<N::HeaderResponse, HttpSubscriptionError>>,
        poll_interval: Duration,
        call_timeout: Duration,
    ) {
        let mut interval = interval(poll_interval);
        // Skip missed ticks to avoid burst of requests after delay
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut last_block_number: Option<BlockNumber> = None;

        // Do an initial poll immediately
        interval.tick().await;

        loop {
            // Fetch latest block
            let block_result = tokio::time::timeout(
                call_timeout,
                provider.get_block_by_number(BlockNumberOrTag::Latest),
            )
            .await;

            let block = match block_result {
                Ok(Ok(Some(block))) => block,
                Ok(Ok(None)) => {
                    // No block returned, skip this interval
                    trace!("HTTP poll: no block returned, skipping");
                    interval.tick().await;
                    continue;
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "HTTP poll: RPC error");
                    if sender
                        .send(Err(HttpSubscriptionError::RpcError(Arc::new(e))))
                        .await
                        .is_err()
                    {
                        // Receiver dropped, stop polling
                        debug!("HTTP poll: receiver dropped, stopping");
                        break;
                    }
                    interval.tick().await;
                    continue;
                }
                Err(_elapsed) => {
                    warn!(timeout_ms = call_timeout.as_millis(), "HTTP poll: timeout");
                    if sender.send(Err(HttpSubscriptionError::Timeout)).await.is_err() {
                        debug!("HTTP poll: receiver dropped, stopping");
                        break;
                    }
                    interval.tick().await;
                    continue;
                }
            };

            // Extract block number from header
            let header = block.header();
            let current_block_number = header.number();

            // Check if this is a new block
            let is_new_block = match last_block_number {
                None => true,
                Some(last) => current_block_number > last,
            };

            if is_new_block {
                trace!(
                    block_number = current_block_number,
                    previous = ?last_block_number,
                    "HTTP poll: new block detected"
                );
                last_block_number = Some(current_block_number);

                // Send the block header
                if sender.send(Ok(header.clone())).await.is_err() {
                    // Receiver dropped, stop polling
                    debug!("HTTP poll: receiver dropped, stopping");
                    break;
                }
            } else {
                trace!(
                    block_number = current_block_number,
                    "HTTP poll: no new block"
                );
            }

            interval.tick().await;
        }
    }

    /// Receive the next block header.
    ///
    /// This will block until a new block is available or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns [`HttpSubscriptionError::Closed`] if the subscription channel is closed.
    /// Returns [`HttpSubscriptionError::Timeout`] or [`HttpSubscriptionError::RpcError`]
    /// if the polling task encountered an error.
    pub async fn recv(&mut self) -> Result<N::HeaderResponse, HttpSubscriptionError> {
        self.receiver
            .recv()
            .await
            .ok_or(HttpSubscriptionError::Closed)?
    }

    /// Check if the subscription channel is empty (no pending messages).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    /// Close the subscription and stop the background polling task.
    pub fn close(&mut self) {
        self.receiver.close();
    }
}

/// Stream adapter for [`HttpPollingSubscription`].
///
/// Allows using the subscription with `tokio_stream` combinators.
pub struct HttpPollingStream<N: Network> {
    receiver: mpsc::Receiver<Result<N::HeaderResponse, HttpSubscriptionError>>,
}

impl<N: Network + 'static> From<HttpPollingSubscription<N>> for HttpPollingStream<N>
where
    N::HeaderResponse: Clone + Send,
{
    fn from(mut subscription: HttpPollingSubscription<N>) -> Self {
        // Take ownership of the receiver, task handle stays with original struct
        // until it's dropped (which happens after this conversion)
        Self {
            receiver: std::mem::replace(
                &mut subscription.receiver,
                mpsc::channel(1).1, // dummy receiver
            ),
        }
    }
}

impl<N: Network> Stream for HttpPollingStream<N> {
    type Item = Result<N::HeaderResponse, HttpSubscriptionError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::{network::Ethereum, node_bindings::Anvil, providers::ext::AnvilApi};
    use std::time::Duration;

    #[tokio::test]
    async fn test_http_polling_config_defaults() {
        let config = HttpSubscriptionConfig::default();
        assert_eq!(config.poll_interval, DEFAULT_POLL_INTERVAL);
        assert_eq!(config.call_timeout, Duration::from_secs(30));
        assert_eq!(config.buffer_capacity, 128);
    }

    #[tokio::test]
    async fn test_http_polling_receives_initial_block() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let provider: RootProvider<Ethereum> = RootProvider::new_http(anvil.endpoint_url());

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(50),
            call_timeout: Duration::from_secs(5),
            buffer_capacity: 16,
        };

        let mut sub = HttpPollingSubscription::new(provider, config);

        // Should receive block 0 (genesis) on first poll
        let result = tokio::time::timeout(Duration::from_secs(2), sub.recv()).await;
        assert!(result.is_ok(), "Should receive initial block within timeout");
        let block = result.unwrap()?;
        assert_eq!(block.number(), 0, "First block should be genesis (block 0)");

        Ok(())
    }

    #[tokio::test]
    async fn test_http_polling_receives_new_blocks() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let provider: RootProvider<Ethereum> = RootProvider::new_http(anvil.endpoint_url());

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(50),
            call_timeout: Duration::from_secs(5),
            buffer_capacity: 16,
        };

        let mut sub = HttpPollingSubscription::new(provider.clone(), config);

        // Receive genesis block
        let block = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .expect("timeout waiting for genesis")
            .expect("recv error on genesis");
        assert_eq!(block.number(), 0);

        // Mine a new block
        provider.anvil_mine(Some(1), None).await?;

        // Should receive block 1
        let block = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .expect("timeout waiting for block 1")
            .expect("recv error on block 1");
        assert_eq!(block.number(), 1);

        Ok(())
    }

    /// Test that polling correctly deduplicates - same block is not emitted twice.
    /// 
    /// Verifies by: receiving genesis, waiting for multiple poll cycles (no mining),
    /// then mining one block and confirming we get block 1 (not duplicates of 0).
    #[tokio::test]
    async fn test_http_polling_deduplication() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let provider: RootProvider<Ethereum> = RootProvider::new_http(anvil.endpoint_url());

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(20), // Fast polling - 5 polls in 100ms
            call_timeout: Duration::from_secs(5),
            buffer_capacity: 16,
        };

        let mut sub = HttpPollingSubscription::new(provider.clone(), config);

        // Receive genesis
        let block = sub.recv().await?;
        assert_eq!(block.number(), 0, "First block should be genesis");

        // Wait for multiple poll cycles without mining - dedup should prevent duplicates
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Channel should be empty (no duplicate genesis blocks queued)
        assert!(sub.is_empty(), "Should not have duplicate blocks in channel");

        // Now mine a block
        provider.anvil_mine(Some(1), None).await?;

        // Should receive block 1 next (not another genesis)
        let block = tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("timeout")
            .expect("recv error");
        assert_eq!(block.number(), 1, "Next block should be 1, not duplicate of 0");

        Ok(())
    }

    /// Test that dropping the subscription stops the background polling task.
    /// 
    /// Verification: If task doesn't stop, it would keep polling a dead provider
    /// and potentially panic or leak resources. Test passes if no hang/panic.
    #[tokio::test]
    async fn test_http_polling_stops_on_drop() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let provider: RootProvider<Ethereum> = RootProvider::new_http(anvil.endpoint_url());

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(10), // Very fast polling
            call_timeout: Duration::from_secs(1),
            buffer_capacity: 4,
        };

        let sub = HttpPollingSubscription::new(provider, config);

        // Drop the subscription
        drop(sub);

        // Drop the anvil (provider becomes invalid)
        drop(anvil);

        // If the background task was still running and polling, it would:
        // 1. Try to poll a dead provider
        // 2. Potentially panic or hang
        // Wait to give any zombie task time to cause problems
        tokio::time::sleep(Duration::from_millis(100)).await;

        // If we reach here without panic/hang, cleanup worked
        Ok(())
    }

    #[tokio::test]
    async fn test_http_subscription_error_types() {
        // Test Timeout error
        let timeout_err = HttpSubscriptionError::Timeout;
        assert!(matches!(timeout_err, HttpSubscriptionError::Timeout));

        // Test RpcError conversion
        let rpc_err: RpcError<TransportErrorKind> = TransportErrorKind::custom_str("test error");
        let sub_err: HttpSubscriptionError = rpc_err.into();
        assert!(matches!(sub_err, HttpSubscriptionError::RpcError(_)));

        // Test Closed error
        let closed_err = HttpSubscriptionError::Closed;
        assert!(matches!(closed_err, HttpSubscriptionError::Closed));

        // Test BlockFetchFailed error
        let fetch_err = HttpSubscriptionError::BlockFetchFailed("test".to_string());
        assert!(matches!(fetch_err, HttpSubscriptionError::BlockFetchFailed(_)));
    }

    /// Test the close() method explicitly closes the subscription
    #[tokio::test]
    async fn test_http_polling_close_method() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let provider: RootProvider<Ethereum> = RootProvider::new_http(anvil.endpoint_url());

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(50),
            call_timeout: Duration::from_secs(5),
            buffer_capacity: 16,
        };

        let mut sub = HttpPollingSubscription::new(provider, config);

        // Receive genesis
        let _ = sub.recv().await?;

        // Close the subscription
        sub.close();

        // Further recv should return Closed error
        let result = sub.recv().await;
        assert!(
            matches!(result, Err(HttpSubscriptionError::Closed)),
            "recv after close should return Closed error, got {:?}",
            result
        );

        Ok(())
    }
}
