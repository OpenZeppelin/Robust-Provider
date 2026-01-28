//! Core [`RobustProvider`] implementation with retry and failover logic.

use std::time::Duration;

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::{Ethereum, Network},
    primitives::{BlockHash, BlockNumber},
    providers::{Provider, RootProvider},
    rpc::types::{Filter, Log},
};

use crate::{Error, Robustness, robust_provider::RobustSubscription};

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

impl<N: Network> Robustness<N> for RobustProvider<N> {
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
