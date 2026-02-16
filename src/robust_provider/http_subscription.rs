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
//! ```rust,no_run
//! use alloy::providers::ProviderBuilder;
//! use robust_provider::RobustProviderBuilder;
//! use std::time::Duration;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let http = ProviderBuilder::new().connect_http("http://localhost:8545")?;
//!
//! let robust = RobustProviderBuilder::new(http)
//!     .allow_http_subscriptions(true)
//!     .poll_interval(Duration::from_secs(12))
//!     .build()
//!     .await?;
//!
//! let mut subscription = robust.subscribe_blocks().await?;
//! while let Ok(block) = subscription.recv().await {
//!     println!("New block: {}", block.number);
//! }
//! # Ok(()) }
//! ```

use std::{sync::Arc, time::Duration};

use alloy::{
    network::{BlockResponse, Network},
    primitives::BlockHash,
    providers::Provider,
    transports::{RpcError, TransportErrorKind},
};
use futures_util::{StreamExt, stream};
use tokio::sync::mpsc;
use crate::RobustProvider;

/// Default polling interval for HTTP subscriptions.
///
/// Set to 12 seconds to match approximate Ethereum mainnet block time.
/// Adjust based on the target chain's block time.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(12);

/// Default timeout for individual RPC calls during HTTP polling.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default buffer capacity for the internal subscription channel.
pub const DEFAULT_BUFFER_CAPACITY: usize = 128;

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
    /// Default: [`DEFAULT_CALL_TIMEOUT`] (30 seconds)
    pub call_timeout: Duration,

    /// Buffer size for the internal channel.
    ///
    /// Default: [`DEFAULT_BUFFER_CAPACITY`] (128)
    pub buffer_capacity: usize,
}

impl Default for HttpSubscriptionConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            call_timeout: DEFAULT_CALL_TIMEOUT,
            buffer_capacity: DEFAULT_BUFFER_CAPACITY,
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
/// Uses alloy's [`watch_blocks()`](alloy::providers::Provider::watch_blocks), which:
/// 1. Creates a block filter via `eth_newBlockFilter`
/// 2. Polls `eth_getFilterChanges` at `poll_interval` to get new block hashes
/// 3. Fetches full block headers for each hash
///
/// # Trade-offs
///
/// * **Latency**: New blocks are detected with up to `poll_interval` delay
/// * **RPC Load**: One filter poll per interval, plus one `get_block_by_hash` per new block
pub struct HttpPollingSubscription<N: Network> {
    /// Receiver for block hashes from the poller
    receiver: mpsc::Receiver<BlockHash>,
    /// Provider used to fetch block headers from hashes
    provider: RobustProvider<N>,
    /// Timeout for individual RPC calls
    call_timeout: Duration,
}

impl<N: Network + 'static> HttpPollingSubscription<N>
where
    N::HeaderResponse: Clone + Send,
{
    /// Create a new HTTP polling subscription.
    ///
    /// Sets up a block filter and returns a subscription that polls for new blocks.
    ///
    /// # Arguments
    ///
    /// * `provider` - The HTTP provider to poll
    /// * `config` - Configuration for polling behavior
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use robust_provider::{RobustProvider, RobustProviderBuilder};
    /// use robust_provider::robust_provider::http_subscription::{HttpPollingSubscription, HttpSubscriptionConfig};
    /// use alloy::providers::ProviderBuilder;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let http = ProviderBuilder::new().connect_http("http://localhost:8545".parse()?)?;
    /// let provider = RobustProviderBuilder::new(http).build().await?;
    /// let config = HttpSubscriptionConfig {
    ///     poll_interval: Duration::from_secs(6),
    ///     ..Default::default()
    /// };
    /// let mut sub = HttpPollingSubscription::new(provider, config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(
        provider: RobustProvider<N>,
        config: HttpSubscriptionConfig,
    ) -> Result<Self, HttpSubscriptionError> {
        let (sender, receiver) = mpsc::channel(config.buffer_capacity);

        let poller = provider
            .primary()
            .watch_blocks()
            .await
            .map_err(HttpSubscriptionError::from)?
            .with_poll_interval(config.poll_interval);

        // Spawn a task to forward block hashes to the channel
        let stream = poller.into_stream().flat_map(stream::iter);
        tokio::spawn(async move {
            let mut stream = stream;
            let sender = sender;
            while let Some(hash) = stream.next().await {
                if sender.send(hash).await.is_err() {
                    // Receiver dropped, stop polling
                    break;
                }
            }
        });

        Ok(Self {
            receiver,
            provider,
            call_timeout: config.call_timeout,
        })
    }

    /// Receive the next block header.
    ///
    /// This will block until a new block is available or an error occurs.
    ///
    /// # Errors
    ///
    /// * [`HttpSubscriptionError::Closed`] - if the subscription channel is closed.
    /// * [`HttpSubscriptionError::Timeout`] - if the polling operation times out.
    /// * [`HttpSubscriptionError::RpcError`] - if an RPC error occurs during polling.
    /// * [`HttpSubscriptionError::BlockFetchFailed`] - if the block fetch fails.
    pub async fn recv(&mut self) -> Result<N::HeaderResponse, HttpSubscriptionError> {
        let block_hash = self.receiver.recv().await.ok_or(HttpSubscriptionError::Closed)?;

        let block = tokio::time::timeout(
            self.call_timeout,
            self.provider.get_block_by_hash(block_hash),
        )
        .await
        .map_err(|_| HttpSubscriptionError::Timeout)?
        .map_err(|_| HttpSubscriptionError::BlockFetchFailed("Failed to fetch block".to_string()))?;
        Ok(block.header().clone())
    }

    /// Check if the subscription channel is empty (no pending messages).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }
}

impl<N: Network> std::fmt::Debug for HttpPollingSubscription<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpPollingSubscription")
            .field("stream", &"<stream>")
            .field("provider", &"<provider>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RobustProviderBuilder;
    use alloy::{
        consensus::BlockHeader, node_bindings::Anvil,
        providers::{ext::AnvilApi, ProviderBuilder},
    };
    use std::time::Duration;

    #[tokio::test]
    async fn test_http_polling_config_defaults() {
        let config = HttpSubscriptionConfig::default();
        assert_eq!(config.poll_interval, DEFAULT_POLL_INTERVAL);
        assert_eq!(config.call_timeout, DEFAULT_CALL_TIMEOUT);
        assert_eq!(config.buffer_capacity, DEFAULT_BUFFER_CAPACITY);
    }

    #[tokio::test]
    async fn test_http_polling_receives_new_block() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let root_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
        let provider = RobustProviderBuilder::new(root_provider.clone()).build().await?;

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(50),
            call_timeout: Duration::from_secs(5),
            buffer_capacity: 16,
        };

        let mut sub = HttpPollingSubscription::new(provider, config).await?;

        // Mine a block
        root_provider.anvil_mine(Some(1), None).await?;

        // Should receive the newly mined block
        let result = tokio::time::timeout(Duration::from_secs(2), sub.recv()).await;
        assert!(result.is_ok(), "Should receive new block within timeout");
        let block = result.unwrap()?;
        assert_eq!(block.number(), 1, "Should receive block 1");

        Ok(())
    }

    #[tokio::test]
    async fn test_http_polling_receives_new_blocks() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;
        let root_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
        let provider = RobustProviderBuilder::new(root_provider.clone()).build().await?;

        let config = HttpSubscriptionConfig {
            poll_interval: Duration::from_millis(50),
            call_timeout: Duration::from_secs(5),
            buffer_capacity: 16,
        };

        let mut sub = HttpPollingSubscription::new(provider, config).await?;

        // Mine a new block
        root_provider.anvil_mine(Some(1), None).await?;

        // Should receive block 1
        let block = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .expect("timeout waiting for block 1")
            .expect("recv error on block 1");
        assert_eq!(block.number(), 1);

        // Mine another block
        root_provider.anvil_mine(Some(1), None).await?;

        // Should receive block 2
        let block = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .expect("timeout waiting for block 2")
            .expect("recv error on block 2");
        assert_eq!(block.number(), 2);

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
}
