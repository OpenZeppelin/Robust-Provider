//! Core [`RobustProvider`] implementation with retry and failover logic.

use std::time::Duration;

use alloy::{
    consensus::TrieAccount,
    eips::{BlockId, BlockNumberOrTag},
    network::{Ethereum, Network},
    primitives::{Address, BlockHash, BlockNumber, Bytes, U256},
    providers::{Provider, RootProvider},
    rpc::types::{Bundle, EthCallResponse, FeeHistory, Filter, Log},
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
    robust_rpc!(
        /// Returns a list of addresses owned by the client.
        ///
        /// This is a wrapper function for [`Provider::get_accounts`] (`eth_accounts`).
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_accounts() -> Vec<Address>
    );

    robust_rpc!(
        /// Returns the base fee per blob gas in wei.
        ///
        /// This is a wrapper function for [`Provider::get_blob_base_fee`] (`eth_blobBaseFee`).
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_blob_base_fee() -> u128
    );

    robust_rpc!(
        /// Executes a call against the state of the network without creating a transaction.
        ///
        /// This is a wrapper function for [`Provider::call`] (`eth_call`).
        ///
        /// # Arguments
        ///
        /// * `tx` - The transaction request to simulate.
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn call(tx: clone N::TransactionRequest) -> Bytes
    );

    robust_rpc!(
        /// Executes multiple calls in a single request.
        ///
        /// This is a wrapper function for [`Provider::call_many`] (`eth_callMany`).
        ///
        /// # Arguments
        ///
        /// * `bundles` - A slice of transaction bundles to execute.
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn call_many(bundles: &[Bundle]) -> Vec<Vec<EthCallResponse>>
    );

    robust_rpc!(
        /// Returns the chain ID of the network.
        ///
        /// This is a wrapper function for [`Provider::get_chain_id`] (`eth_chainId`).
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_chain_id() -> u64
    );

    robust_rpc!(
        /// Estimates the gas required for a transaction.
        ///
        /// This is a wrapper function for [`Provider::estimate_gas`] (`eth_estimateGas`).
        ///
        /// # Arguments
        ///
        /// * `tx` - The transaction request to estimate gas for.
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn estimate_gas(tx: clone N::TransactionRequest) -> u64
    );

    robust_rpc!(
        /// Returns the fee history for a range of blocks.
        ///
        /// This is a wrapper function for [`Provider::get_fee_history`] (`eth_feeHistory`).
        ///
        /// # Arguments
        ///
        /// * `block_count` - The number of blocks to include in the fee history.
        /// * `last_block` - The last block to include in the fee history.
        /// * `reward_percentiles` - A list of percentiles to compute reward values for.
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_fee_history(block_count: u64, last_block: BlockNumberOrTag, reward_percentiles: &[f64]) -> FeeHistory
    );

    robust_rpc!(
        /// Returns the current gas price in wei.
        ///
        /// This is a wrapper function for [`Provider::get_gas_price`] (`eth_gasPrice`).
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_gas_price() -> u128
    );

    robust_rpc!(
        /// Retrieves account information ([`TrieAccount`]) for the given
        /// [`Address`] at the particular [`BlockId`].
        ///
        /// This is a wrapper function for [`Provider::get_account`] (`eth_getAccount`).
        ///
        /// # Arguments
        ///
        /// * `address` - The address to get the account for.
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_account(address: Address) -> TrieAccount
    );

    robust_rpc!(
        /// Returns the balance of the account at the given address.
        ///
        /// This is a wrapper function for [`Provider::get_balance`] (`eth_getBalance`).
        ///
        /// # Arguments
        ///
        /// * `address` - The address to get the balance for.
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        fn get_balance(address: Address) -> U256
    );

    robust_rpc!(
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
        fn get_block_by_number(number: BlockNumberOrTag) -> N::BlockResponse; or BlockNotFound
    );

    robust_rpc!(
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
        fn get_block(id: BlockId) -> N::BlockResponse; or BlockNotFound
    );

    robust_rpc!(
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
        fn get_block_number() -> BlockNumber
    );

    robust_rpc!(
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
        fn get_block_number_by_id(block_id: BlockId) -> BlockNumber; or BlockNotFound
    );

    robust_rpc!(
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
        fn get_block_by_hash(hash: BlockHash) -> N::BlockResponse; or BlockNotFound
    );

    robust_rpc!(
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
        fn get_logs(filter: &Filter) -> Vec<Log>
    );

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
