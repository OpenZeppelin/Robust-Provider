//! Core [`RobustProvider`] implementation with retry and failover logic.

use std::time::Duration;

use alloy::{
    consensus::TrieAccount,
    eips::{BlockId, BlockNumberOrTag},
    network::{Ethereum, Network},
    primitives::{
        Address, B256, BlockHash, BlockNumber, Bytes, StorageKey, StorageValue, TxHash, U256,
    },
    providers::{Provider, RootProvider},
    rpc::{
        json_rpc::RpcRecv,
        types::{Bundle, EIP1186AccountProofResponse, EthCallResponse, FeeHistory, Filter, Log},
    },
};

use crate::{Error, Robustness, block_not_found_doc, robust_provider::RobustSubscription};

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
    robust_rpc!(fn get_accounts() -> Vec<Address>);
    robust_rpc!(fn get_blob_base_fee() -> u128);

    robust_rpc!(
        args = [(tx, "The transaction request to simulate.")]
        @clone [tx]
        fn call(tx: N::TransactionRequest) -> Bytes
    );

    robust_rpc!(
        args = [(bundles, "A slice of transaction bundles to execute.")]
        fn call_many(bundles: &[Bundle]) -> Vec<Vec<EthCallResponse>>
    );

    robust_rpc!(fn get_chain_id() -> u64);

    robust_rpc!(
        args = [(tx, "The transaction request to estimate gas for.")]
        @clone [tx]
        fn estimate_gas(tx: N::TransactionRequest) -> u64
    );

    robust_rpc!(
        args = [
            (block_count, "The number of blocks to include in the fee history."),
            (last_block, "The last block to include in the fee history."),
            (reward_percentiles, "A list of percentiles to compute reward values for.")
        ]
        fn get_fee_history(block_count: u64, last_block: BlockNumberOrTag, reward_percentiles: &[f64]) -> FeeHistory
    );

    robust_rpc!(fn get_gas_price() -> u128);
    robust_rpc!(fn get_max_priority_fee_per_gas() -> u128);

    robust_rpc!(
        args = [(address, "The address to get the account for.")]
        fn get_account(address: Address) -> TrieAccount
    );

    robust_rpc!(
        args = [(address, "The address to get the balance for.")]
        fn get_balance(address: Address) -> U256
    );

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(number, "The block number or tag.")]
        fn get_block_by_number(number: BlockNumberOrTag) -> N::BlockResponse; or BlockNotFound
    );

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(id, "The block identifier.")]
        fn get_block(id: BlockId) -> N::BlockResponse; or BlockNotFound
    );

    robust_rpc!(fn get_block_number() -> BlockNumber);

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(block_id, "The block identifier to fetch the block number for.")]
        fn get_block_number_by_id(block_id: BlockId) -> BlockNumber; or BlockNotFound
    );

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(hash, "The block hash.")]
        fn get_block_by_hash(hash: BlockHash) -> N::BlockResponse; or BlockNotFound
    );

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(block, "The block identifier (hash, number, or tag).")]
        fn get_block_receipts(block: BlockId) -> Vec<N::ReceiptResponse>; or BlockNotFound
    );

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(hash, "The block hash.")]
        fn get_block_transaction_count_by_hash(hash: BlockHash) -> u64; or BlockNotFound
    );

    robust_rpc!(
        error = [block_not_found_doc!()]
        args = [(block_number, "The block number or tag.")]
        fn get_block_transaction_count_by_number(block_number: BlockNumberOrTag) -> u64; or BlockNotFound
    );

    robust_rpc!(
        args = [(filter, "The log filter.")]
        fn get_logs(filter: &Filter) -> Vec<Log>
    );

    robust_rpc!(
        args = [(address, "The address to get the code for.")]
        fn get_code_at(address: Address) -> Bytes
    );

    robust_rpc!(
        args = [(filter_id, "The filter ID to fetch logs for.")]
        fn get_filter_logs(filter_id: U256) -> Vec<Log>
    );

    robust_rpc!(
        args = [(filter_id, "The filter ID to get changes for.")]
        fn get_filter_changes<R: RpcRecv>(filter_id: U256) -> Vec<R>
    );

    robust_rpc!(
        args = [(filter, "The filter to create.")]
        fn new_filter(filter: &Filter) -> U256
    );

    robust_rpc!(fn new_block_filter() -> U256);

    robust_rpc!(
        args = [
            (address, "The address of the account."),
            (keys, "A vector of storage keys to include in the proof.")
        ]
        @clone [keys]
        fn get_proof(address: Address, keys: Vec<StorageKey>) -> EIP1186AccountProofResponse
    );

    robust_rpc!(
        args = [
            (address, "The address of the storage."),
            (key, "The position in the storage.")
        ]
        fn get_storage_at(address: Address, key: U256) -> StorageValue
    );

    robust_rpc!(
        args = [
            (block_hash, "The hash of the block."),
            (index, "The transaction index position.")
        ]
        fn get_transaction_by_block_hash_and_index(block_hash: B256, index: usize) -> Option<N::TransactionResponse>
    );

    robust_rpc!(
        args = [
            (block_number, "The block number or tag."),
            (index, "The transaction index position.")
        ]
        fn get_transaction_by_block_number_and_index(block_number: BlockNumberOrTag, index: usize) -> Option<N::TransactionResponse>
    );

    robust_rpc!(
        args = [(hash, "The transaction hash.")]
        fn get_transaction_by_hash(hash: TxHash) -> Option<N::TransactionResponse>
    );

    robust_rpc!(
        args = [(hash, "The transaction hash.")]
        fn get_raw_transaction_by_hash(hash: TxHash) -> Option<Bytes>
    );

    robust_rpc!(
        args = [(address, "The address to get the transaction count for.")]
        fn get_transaction_count(address: Address) -> u64
    );

    robust_rpc!(
        args = [(hash, "The transaction hash.")]
        fn get_transaction_receipt(hash: TxHash) -> Option<N::ReceiptResponse>
    );

    robust_rpc!(
        args = [(block, "The block identifier (hash or number).")]
        fn get_uncle_count(block: BlockId) -> u64
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
