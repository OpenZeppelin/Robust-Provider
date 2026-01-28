//! Core [`RobustProvider`] implementation with retry and failover logic.

use std::time::Duration;

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::{Ethereum, Network},
    primitives::{BlockHash, BlockNumber},
    providers::{Provider, RootProvider},
    rpc::types::{Filter, Log},
};

use crate::{Error, Resilience, robust_provider::RobustSubscription};

/// Provider wrapper with built-in retry and timeout mechanisms.
///
/// This wrapper around Alloy providers automatically handles retries,
/// timeouts, and error logging for RPC calls.
#[derive(Clone, Debug)]
pub struct RobustProvider<N: Network = Ethereum> {
    pub(crate) primary_provider: RootProvider<N>,
    pub(crate) fallback_providers: Vec<RootProvider<N>>,
    pub(crate) call_timeout: Duration,
    pub(crate) subscription_timeout: Duration,
    pub(crate) max_retries: usize,
    pub(crate) min_delay: Duration,
    pub(crate) reconnect_interval: Duration,
    pub(crate) subscription_buffer_capacity: usize,
}

impl<N: Network> Resilience<N> for RobustProvider<N> {
    fn primary(&self) -> &RootProvider<N> {
        &self.primary_provider
    }

    fn fallback_providers(&self) -> &[RootProvider<N>] {
        &self.fallback_providers
    }

    fn call_timeout(&self) -> Duration {
        self.call_timeout
    }

    fn max_retries(&self) -> usize {
        self.max_retries
    }

    fn min_delay(&self) -> Duration {
        self.min_delay
    }
}

impl<N: Network> RobustProvider<N> {
    /// Get a reference to the primary provider
    #[must_use]
    pub fn primary(&self) -> &RootProvider<N> {
        &self.primary_provider
    }

    /// Fetch a block by [`BlockNumberOrTag`] with retry and timeout.
    ///
    /// This is a wrapper function for [`Provider::get_block_by_number`].
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    /// * [`Error::BlockNotFound`] - if the block with the specified number/tag is not available.
    ///   This is verified on Anvil, Reth, and Geth; other clients may surface this condition as
    ///   [`Error::RpcError`].
    pub async fn get_block_by_number(
        &self,
        number: BlockNumberOrTag,
    ) -> Result<N::BlockResponse, Error> {
        let result = self
            .try_operation_with_failover(
                move |provider| async move { provider.get_block_by_number(number).await },
                false,
            )
            .await;

        result?.ok_or(Error::BlockNotFound)
    }

    /// Fetch a block number by [`BlockId`]  with retry and timeout.
    ///
    /// This is a wrapper function for [`Provider::get_block`].
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    /// * [`Error::BlockNotFound`] - if the block for the specified identifier is not available.
    ///   This is verified on Anvil, Reth, and Geth; other clients may surface this condition as
    ///   [`Error::RpcError`].
    pub async fn get_block(&self, id: BlockId) -> Result<N::BlockResponse, Error> {
        let result = self
            .try_operation_with_failover(
                |provider| async move { provider.get_block(id).await },
                false,
            )
            .await;
        result?.ok_or(Error::BlockNotFound)
    }

    /// Fetch the latest block number with retry and timeout.
    ///
    /// This is a wrapper function for [`Provider::get_block_number`].
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    pub async fn get_block_number(&self) -> Result<BlockNumber, Error> {
        self.try_operation_with_failover(
            move |provider| async move { provider.get_block_number().await },
            false,
        )
        .await
        .map_err(Error::from)
    }

    /// Get the block number for a given block identifier.
    ///
    /// This is a wrapper function for [`Provider::get_block_number_by_id`].
    ///
    /// # Arguments
    ///
    /// * `block_id` - The block identifier to fetch the block number for.
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    /// * [`Error::BlockNotFound`] - if the block for the specified identifier is not available.
    ///   This is verified on Anvil, Reth, and Geth; other clients may surface this condition as
    ///   [`Error::RpcError`].
    pub async fn get_block_number_by_id(&self, block_id: BlockId) -> Result<BlockNumber, Error> {
        let result = self
            .try_operation_with_failover(
                move |provider| async move { provider.get_block_number_by_id(block_id).await },
                false,
            )
            .await;
        result?.ok_or(Error::BlockNotFound)
    }

    /// Fetch the latest confirmed block number with retry and timeout.
    ///
    /// This method fetches the latest block number and subtracts the specified
    /// number of confirmations to get a "confirmed" block number.
    ///
    /// # Arguments
    ///
    /// * `confirmations` - The number of block confirmations to wait for. The returned block number
    ///   will be `latest_block - confirmations`.
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    pub async fn get_latest_confirmed(&self, confirmations: u64) -> Result<u64, Error> {
        let latest_block = self.get_block_number().await?;
        let confirmed_block = latest_block.saturating_sub(confirmations);
        Ok(confirmed_block)
    }

    /// Fetch a block by [`BlockHash`] with retry and timeout.
    ///
    /// This is a wrapper function for [`Provider::get_block_by_hash`].
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    /// * [`Error::BlockNotFound`] - if the block with the specified hash is not available. This is
    ///   verified on Anvil, Reth, and Geth; other clients may surface this condition as
    ///   [`Error::RpcError`].
    pub async fn get_block_by_hash(&self, hash: BlockHash) -> Result<N::BlockResponse, Error> {
        let result = self
            .try_operation_with_failover(
                move |provider| async move { provider.get_block_by_hash(hash).await },
                false,
            )
            .await;

        result?.ok_or(Error::BlockNotFound)
    }

    /// Fetch logs for the given [`Filter`] with retry and timeout.
    ///
    /// This is a wrapper function for [`Provider::get_logs`].
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    pub async fn get_logs(&self, filter: &Filter) -> Result<Vec<Log>, Error> {
        self.try_operation_with_failover(
            move |provider| async move { provider.get_logs(filter).await },
            false,
        )
        .await
        .map_err(Error::from)
    }

    /// Subscribe to new block headers with automatic failover and reconnection.
    ///
    /// Returns a `RobustSubscription` that automatically:
    /// * Handles connection errors by switching to fallback providers
    /// * Detects and recovers from lagged subscriptions
    /// * Periodically attempts to reconnect to the primary provider
    ///
    /// This is a wrapper function for [`Provider::subscribe_blocks`].
    ///
    /// # Errors
    ///
    /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
    ///   by the last provider attempted on the last retry.
    /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    pub async fn subscribe_blocks(&self) -> Result<RobustSubscription<N>, Error> {
        let subscription = self
            .try_operation_with_failover(
                move |provider| async move {
                    provider
                        .subscribe_blocks()
                        .channel_size(self.subscription_buffer_capacity)
                        .await
                },
                true,
            )
            .await?;

        Ok(RobustSubscription::new(subscription, self.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robust_provider::{
        CoreError, DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY, RobustProviderBuilder,
        builder::DEFAULT_SUBSCRIPTION_TIMEOUT, resilience::Resilience,
        subscription::DEFAULT_RECONNECT_INTERVAL,
    };
    use alloy::{
        providers::{ProviderBuilder, WsConnect},
        transports::{RpcError, TransportErrorKind},
    };
    use alloy_node_bindings::Anvil;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::sleep;

    fn test_provider(timeout: u64, max_retries: usize, min_delay: u64) -> RobustProvider {
        RobustProvider {
            primary_provider: RootProvider::new_http("http://localhost:8545".parse().unwrap()),
            fallback_providers: vec![],
            call_timeout: Duration::from_millis(timeout),
            subscription_timeout: DEFAULT_SUBSCRIPTION_TIMEOUT,
            max_retries,
            min_delay: Duration::from_millis(min_delay),
            reconnect_interval: DEFAULT_RECONNECT_INTERVAL,
            subscription_buffer_capacity: DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY,
        }
    }

    #[tokio::test]
    async fn test_retry_with_timeout_succeeds_on_first_attempt() {
        let provider = test_provider(100, 3, 10);

        let call_count = AtomicUsize::new(0);

        let result = provider
            .try_operation_with_failover(
                |_| async {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let count = call_count.load(Ordering::SeqCst);
                    Ok(count)
                },
                false,
            )
            .await;

        assert!(matches!(result, Ok(1)));
    }

    #[tokio::test]
    async fn test_retry_with_timeout_retries_on_error() {
        let provider = test_provider(100, 3, 10);

        let call_count = AtomicUsize::new(0);

        let result = provider
            .try_operation_with_failover(
                |_| async {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let count = call_count.load(Ordering::SeqCst);
                    match count {
                        3 => Ok(count),
                        // retriable error
                        _ => Err(TransportErrorKind::custom_str("429 Too Many Requests")),
                    }
                },
                false,
            )
            .await;

        assert!(matches!(result, Ok(3)));
    }

    #[tokio::test]
    async fn test_retry_with_timeout_fails_after_max_retries() {
        let provider = test_provider(100, 2, 10);

        let call_count = AtomicUsize::new(0);

        let result: Result<(), CoreError> = provider
            .try_operation_with_failover(
                |_| async {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    // retriable error
                    Err(TransportErrorKind::custom_str("429 Too Many Requests"))
                },
                false,
            )
            .await;

        assert!(matches!(result, Err(CoreError::RpcError(_))));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_timeout_respects_call_timeout() {
        let call_timeout = 50;
        let provider = test_provider(call_timeout, 10, 1);

        let result = provider
            .try_operation_with_failover(
                move |_provider| async move {
                    sleep(Duration::from_millis(call_timeout + 10)).await;
                    Ok(42)
                },
                false,
            )
            .await;

        assert!(matches!(result, Err(CoreError::Timeout)));
    }

    #[tokio::test]
    async fn test_subscribe_fails_when_all_providers_lack_pubsub() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let http_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

        let robust = RobustProviderBuilder::new(http_provider.clone())
            .fallback(http_provider)
            .call_timeout(Duration::from_secs(5))
            .min_delay(Duration::from_millis(100))
            .build()
            .await?;

        let result = robust.subscribe_blocks().await.unwrap_err();

        match result {
            Error::RpcError(e) => {
                assert!(matches!(
                    e.as_ref(),
                    RpcError::Transport(TransportErrorKind::PubsubUnavailable)
                ));
            }
            other => panic!("Expected PubsubUnavailable error type, got: {other:?}"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_subscribe_succeeds_if_primary_provider_lacks_pubsub_but_fallback_supports_it()
    -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let http_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
        let ws_provider = ProviderBuilder::new()
            .connect_ws(WsConnect::new(anvil.ws_endpoint_url().as_str()))
            .await?;

        let robust = RobustProviderBuilder::fragile(http_provider)
            .fallback(ws_provider)
            .call_timeout(Duration::from_secs(5))
            .build()
            .await?;

        let result = robust.subscribe_blocks().await;
        assert!(result.is_ok());

        Ok(())
    }
}
