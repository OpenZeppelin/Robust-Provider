//! Tests for Ethereum JSON-RPC namespace methods exposed by `RobustProvider`.
//!
//! These tests verify the behavior of standard Ethereum RPC methods wrapped
//! by `RobustProvider` with retry and failover logic.

mod common;

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::TransactionBuilder,
    primitives::{B256, BlockHash, U256},
    providers::{Provider, ext::AnvilApi},
    rpc::types::{Bundle, Filter, Log, TransactionRequest},
};
use common::{setup_anvil, setup_anvil_with_blocks, setup_anvil_with_contract};
use robust_provider::Error;

// ============================================================================
// eth_getBlockByNumber
// ============================================================================

#[tokio::test]
async fn test_get_block_by_number_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let tags = [
        BlockNumberOrTag::Number(50),
        BlockNumberOrTag::Latest,
        BlockNumberOrTag::Earliest,
        BlockNumberOrTag::Safe,
        BlockNumberOrTag::Finalized,
    ];

    for tag in tags {
        let robust_block = robust.get_block_by_number(tag).await?;
        let alloy_block =
            alloy_provider.get_block_by_number(tag).await?.expect("block should exist");

        assert_eq!(robust_block.header.number, alloy_block.header.number);
        assert_eq!(robust_block.header.hash, alloy_block.header.hash);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_block_by_number_future_block_fails() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    let future_block = 999_999;
    let result = robust.get_block_by_number(BlockNumberOrTag::Number(future_block)).await;

    assert!(matches!(result, Err(Error::BlockNotFound)));

    Ok(())
}

// ============================================================================
// eth_getBlockByHash
// ============================================================================

#[tokio::test]
async fn test_get_block_by_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(50))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_block = robust.get_block_by_hash(block_hash).await?;
    let alloy_block =
        alloy_provider.get_block_by_hash(block_hash).await?.expect("block should exist");
    assert_eq!(robust_block.header.hash, alloy_block.header.hash);
    assert_eq!(robust_block.header.number, alloy_block.header.number);

    let genesis = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Earliest)
        .await?
        .expect("genesis should exist");
    let genesis_hash = genesis.header.hash;
    let robust_block = robust.get_block_by_hash(genesis_hash).await?;
    assert_eq!(robust_block.header.number, 0);
    assert_eq!(robust_block.header.hash, genesis_hash);

    Ok(())
}

#[tokio::test]
async fn test_get_block_by_hash_fails() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    let result = robust.get_block_by_hash(BlockHash::ZERO).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    Ok(())
}

// ============================================================================
// eth_blockNumber
// ============================================================================

#[tokio::test]
async fn test_get_block_number_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let robust_block_num = robust.get_block_number().await?;
    let alloy_block_num = alloy_provider.get_block_number().await?;
    assert_eq!(robust_block_num, alloy_block_num);
    assert_eq!(robust_block_num, 100);

    alloy_provider.anvil_mine(Some(10), None).await?;
    let new_block = robust.get_block_number().await?;
    assert_eq!(new_block, 110);

    Ok(())
}

// ============================================================================
// eth_getBlock (by BlockId)
// ============================================================================

#[tokio::test]
async fn test_get_block_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let block_ids = [
        BlockId::number(50),
        BlockId::latest(),
        BlockId::earliest(),
        BlockId::safe(),
        BlockId::finalized(),
    ];

    for block_id in block_ids {
        let robust_block = robust.get_block(block_id).await?;
        let alloy_block = alloy_provider.get_block(block_id).await?.expect("block should exist");

        assert_eq!(robust_block.header.number, alloy_block.header.number);
        assert_eq!(robust_block.header.hash, alloy_block.header.hash);
    }

    // test block hash
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(50))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;
    let block_id = BlockId::hash(block_hash);
    let robust_block = robust.get_block(block_id).await?;
    assert_eq!(robust_block.header.hash, block_hash);
    assert_eq!(robust_block.header.number, 50);

    Ok(())
}

#[tokio::test]
async fn test_get_block_fails() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    // Future block number
    let result = robust.get_block(BlockId::number(999_999)).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    // Non-existent hash
    let result = robust.get_block(BlockId::hash(BlockHash::ZERO)).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    Ok(())
}

// ============================================================================
// eth_getBlockNumberByBlockId (custom helper)
// ============================================================================

#[tokio::test]
async fn test_get_block_number_by_id_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let block_num = robust.get_block_number_by_id(BlockId::number(50)).await?;
    assert_eq!(block_num, 50);

    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(50))
        .await?
        .expect("block should exist");
    let block_num = robust.get_block_number_by_id(BlockId::hash(block.header.hash)).await?;
    assert_eq!(block_num, 50);

    let block_num = robust.get_block_number_by_id(BlockId::latest()).await?;
    assert_eq!(block_num, 100);

    let block_num = robust.get_block_number_by_id(BlockId::earliest()).await?;
    assert_eq!(block_num, 0);

    // Returns block number even if it doesnt 'exist' on chain
    let block_num = robust.get_block_number_by_id(BlockId::number(999_999)).await?;
    let alloy_block_num = alloy_provider
        .get_block_number_by_id(BlockId::number(999_999))
        .await?
        .expect("Should return block num");
    assert_eq!(alloy_block_num, block_num);
    assert_eq!(block_num, 999_999);

    Ok(())
}

#[tokio::test]
async fn test_get_block_number_by_id_fails() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    let result = robust.get_block_number_by_id(BlockId::hash(BlockHash::ZERO)).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    Ok(())
}

// ============================================================================
// eth_accounts
// ============================================================================

#[tokio::test]
async fn test_get_accounts_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_accounts = robust.get_accounts().await?;
    let alloy_accounts = alloy_provider.get_accounts().await?;

    assert_eq!(robust_accounts, alloy_accounts);
    // Anvil provides 10 default accounts
    assert_eq!(robust_accounts.len(), 10);

    Ok(())
}

// ============================================================================
// eth_blobBaseFee
// ============================================================================

#[tokio::test]
async fn test_get_blob_base_fee_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_fee = robust.get_blob_base_fee().await?;
    let alloy_fee = alloy_provider.get_blob_base_fee().await?;

    assert_eq!(robust_fee, alloy_fee);

    Ok(())
}

// ============================================================================
// eth_call
// ============================================================================

#[tokio::test]
async fn test_call_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    counter.increase().send().await?.watch().await?;
    counter.increase().send().await?.watch().await?;
    counter.increase().send().await?.watch().await?;

    let get_count_call = counter.getCount();
    let tx = TransactionRequest::default()
        .with_to(*counter.address())
        .with_input(get_count_call.calldata().clone());

    let robust_result = robust.call(tx.clone()).await?;
    let alloy_result = alloy_provider.call(tx).await?;

    let count = U256::from_be_slice(&robust_result);
    assert_eq!(count, 3);

    assert_eq!(robust_result, alloy_result);

    Ok(())
}

// ============================================================================
// eth_callMany
// ============================================================================

#[tokio::test]
#[ignore = "Anvil does not support 'call many' therefore this fails with timeout/method not found"]
async fn test_call_many_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    counter.increase().send().await?.watch().await?;
    counter.increase().send().await?.watch().await?;

    let get_count_call = counter.getCount();
    let tx = TransactionRequest::default()
        .with_to(*counter.address())
        .with_input(get_count_call.calldata().clone());

    let bundle = Bundle { transactions: vec![tx], block_override: None };
    let bundles = vec![bundle];

    let robust_result = robust.call_many(&bundles).await?;
    let alloy_result = alloy_provider.call_many(&bundles).await?;

    assert_eq!(robust_result, alloy_result);
    assert!(!robust_result.is_empty());

    Ok(())
}

// ============================================================================
// eth_chainId
// ============================================================================

#[tokio::test]
async fn test_get_chain_id_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_chain_id = robust.get_chain_id().await?;
    let alloy_chain_id = alloy_provider.get_chain_id().await?;

    assert_eq!(robust_chain_id, alloy_chain_id);
    // Anvil default chain ID is 31337
    assert_eq!(robust_chain_id, 31337);

    Ok(())
}

// ============================================================================
// eth_estimateGas
// ============================================================================

#[tokio::test]
async fn test_estimate_gas_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let increase_call = counter.increase();
    let tx = TransactionRequest::default()
        .with_to(*counter.address())
        .with_input(increase_call.calldata().clone());

    let robust_gas = robust.estimate_gas(tx.clone()).await?;
    let alloy_gas = alloy_provider.estimate_gas(tx).await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

// ============================================================================
// eth_feeHistory
// ============================================================================

#[tokio::test]
async fn test_get_fee_history_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let block_count = 10;
    let reward_percentiles = [25.0, 50.0, 75.0];

    let robust_fee_history =
        robust.get_fee_history(block_count, BlockNumberOrTag::Latest, &reward_percentiles).await?;
    let alloy_fee_history = alloy_provider
        .get_fee_history(block_count, BlockNumberOrTag::Latest, &reward_percentiles)
        .await?;

    assert_eq!(robust_fee_history.oldest_block, alloy_fee_history.oldest_block);
    assert_eq!(robust_fee_history.base_fee_per_gas, alloy_fee_history.base_fee_per_gas);
    assert_eq!(robust_fee_history.gas_used_ratio, alloy_fee_history.gas_used_ratio);

    Ok(())
}

// ============================================================================
// eth_gasPrice
// ============================================================================

#[tokio::test]
async fn test_get_gas_price_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_gas_price = robust.get_gas_price().await?;
    let alloy_gas_price = alloy_provider.get_gas_price().await?;

    assert_eq!(robust_gas_price, alloy_gas_price);

    Ok(())
}

// ============================================================================
// eth_getAccount
// ============================================================================

#[tokio::test]
async fn test_get_account_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let address = accounts[0];

    let robust_account = robust.get_account(address).await?;
    let alloy_account = alloy_provider.get_account(address).await?;

    assert_eq!(robust_account.nonce, alloy_account.nonce);
    assert_eq!(robust_account.balance, alloy_account.balance);
    assert_eq!(robust_account.storage_root, alloy_account.storage_root);
    assert_eq!(robust_account.code_hash, alloy_account.code_hash);

    Ok(())
}

// ============================================================================
// eth_getBalance
// ============================================================================

#[tokio::test]
async fn test_get_balance_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let address = accounts[0];

    let robust_balance = robust.get_balance(address).await?;
    let alloy_balance = alloy_provider.get_balance(address).await?;

    assert_eq!(robust_balance, alloy_balance);

    Ok(())
}

// ============================================================================
// eth_getBlockReceipts
// ============================================================================

#[tokio::test]
async fn test_get_block_receipts_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_receipts = robust.get_block_receipts(BlockId::number(block_number)).await?;
    let alloy_receipts = alloy_provider
        .get_block_receipts(BlockId::number(block_number))
        .await?
        .expect("receipts should exist");

    assert_eq!(robust_receipts.len(), alloy_receipts.len());

    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_receipts = robust.get_block_receipts(BlockId::hash(block_hash)).await?;
    let alloy_receipts = alloy_provider
        .get_block_receipts(BlockId::hash(block_hash))
        .await?
        .expect("receipts should exist");

    assert_eq!(robust_receipts.len(), alloy_receipts.len());

    Ok(())
}

#[tokio::test]
async fn test_get_block_receipts_fails() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    // Try to get receipts for a non-existent block
    let result = robust.get_block_receipts(BlockId::hash(BlockHash::ZERO)).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    // Try to get receipts for a future block
    let result = robust.get_block_receipts(BlockId::number(999_999)).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    Ok(())
}

// ============================================================================
// eth_getBlockTransactionCountByHash
// ============================================================================

#[tokio::test]
async fn test_get_block_transaction_count_by_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_count = robust.get_block_transaction_count_by_hash(block_hash).await?;
    let alloy_count = alloy_provider
        .get_block_transaction_count_by_hash(block_hash)
        .await?
        .expect("count should exist");

    assert_eq!(robust_count, alloy_count);
    assert_eq!(robust_count, 1);

    // Test genesis block
    let genesis = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Earliest)
        .await?
        .expect("genesis should exist");
    let genesis_hash = genesis.header.hash;

    let robust_count = robust.get_block_transaction_count_by_hash(genesis_hash).await?;
    let alloy_count = alloy_provider
        .get_block_transaction_count_by_hash(genesis_hash)
        .await?
        .expect("count should exist");

    assert_eq!(robust_count, alloy_count);
    assert_eq!(robust_count, 0);

    Ok(())
}

#[tokio::test]
async fn test_get_block_transaction_count_by_hash_fails() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    let result = robust.get_block_transaction_count_by_hash(BlockHash::ZERO).await;
    assert!(matches!(result, Err(Error::BlockNotFound)));

    Ok(())
}

// ============================================================================
// eth_getBlockTransactionCountByNumber
// ============================================================================

#[tokio::test]
async fn test_get_block_transaction_count_by_number_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    // Send transactions to have something in the block
    let _ = counter.increase().send().await?.watch().await?;

    // Get the latest block number
    let block_number = alloy_provider.get_block_number().await?;

    let robust_count = robust
        .get_block_transaction_count_by_number(BlockNumberOrTag::Number(block_number))
        .await?;
    let alloy_count = alloy_provider
        .get_block_transaction_count_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("count should exist");

    assert_eq!(robust_count, alloy_count);

    let tags = [
        BlockNumberOrTag::Latest,
        BlockNumberOrTag::Earliest,
        BlockNumberOrTag::Safe,
        BlockNumberOrTag::Finalized,
    ];

    for tag in tags {
        let robust_count = robust.get_block_transaction_count_by_number(tag).await?;
        let alloy_count = alloy_provider
            .get_block_transaction_count_by_number(tag)
            .await?
            .expect("count should exist");
        assert_eq!(robust_count, alloy_count);
    }

    // Genesis block should have 0 transactions
    let robust_count =
        robust.get_block_transaction_count_by_number(BlockNumberOrTag::Earliest).await?;
    assert_eq!(robust_count, 0);

    Ok(())
}

#[tokio::test]
async fn test_get_block_transaction_count_by_number_future_block() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    let result = robust
        .get_block_transaction_count_by_number(BlockNumberOrTag::Number(999_999))
        .await
        .unwrap_err();

    assert!(matches!(result, Error::BlockNotFound));

    Ok(())
}

// ============================================================================
// eth_getCode
// ============================================================================

#[tokio::test]
async fn test_get_code_at_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let contract_address = *counter.address();

    let robust_code = robust.get_code_at(contract_address).await?;
    let alloy_code = alloy_provider.get_code_at(contract_address).await?;

    assert_eq!(robust_code, alloy_code);
    assert!(!robust_code.is_empty());

    Ok(())
}

// ============================================================================
// eth_getFilterLogs
// ============================================================================

#[tokio::test]
async fn test_get_filter_logs_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address());

    let filter_id = alloy_provider.new_filter(&filter).await?;

    let _ = counter.increase().send().await?.watch().await?;

    let robust_logs = robust.get_filter_logs(filter_id).await?;
    let alloy_logs = alloy_provider.get_filter_logs(filter_id).await?;

    assert_eq!(robust_logs.len(), alloy_logs.len());

    Ok(())
}

// ============================================================================
// eth_getFilterChanges
// ============================================================================

#[tokio::test]
async fn test_get_filter_changes_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;
    let filter = Filter::new().address(*counter.address()).event("CountIncreased(uint256)");

    let robust_filter_id = robust.new_filter(&filter).await?;
    let alloy_filter_id = alloy_provider.new_filter(&filter).await?;

    let robust_changes: Vec<Log> = robust.get_filter_changes(robust_filter_id).await?;
    let alloy_changes: Vec<Log> = alloy_provider.get_filter_changes(alloy_filter_id).await?;

    assert!(robust_changes.is_empty());
    assert!(alloy_changes.is_empty());

    let _ = counter.increase().send().await?.watch().await?;

    let robust_changes: Vec<Log> = robust.get_filter_changes(robust_filter_id).await?;
    let alloy_changes: Vec<Log> = alloy_provider.get_filter_changes(alloy_filter_id).await?;

    assert_eq!(robust_changes.len(), 1);
    assert_eq!(alloy_changes.len(), 1);

    Ok(())
}

// ============================================================================
// eth_newFilter
// ============================================================================

#[tokio::test]
async fn test_new_filter_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address());

    let robust_filter_id = robust.new_filter(&filter).await?;

    assert!(robust_filter_id > U256::ZERO);

    Ok(())
}

// ============================================================================
// eth_getProof
// ============================================================================

#[tokio::test]
async fn test_get_proof_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let contract_address = *counter.address();
    let storage_keys = vec![];

    let robust_proof = robust.get_proof(contract_address, storage_keys.clone()).await?;
    let alloy_proof = alloy_provider.get_proof(contract_address, storage_keys).await?;

    assert_eq!(robust_proof.address, alloy_proof.address);
    assert_eq!(robust_proof.balance, alloy_proof.balance);
    assert_eq!(robust_proof.nonce, alloy_proof.nonce);
    assert_eq!(robust_proof.code_hash, alloy_proof.code_hash);
    assert_eq!(robust_proof.storage_hash, alloy_proof.storage_hash);

    Ok(())
}

#[tokio::test]
async fn test_get_proof_with_storage_keys_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let contract_address = *counter.address();
    let storage_keys = vec![U256::ZERO.into()];

    let robust_proof = robust.get_proof(contract_address, storage_keys.clone()).await?;
    let alloy_proof = alloy_provider.get_proof(contract_address, storage_keys).await?;

    assert_eq!(robust_proof.address, alloy_proof.address);
    assert_eq!(robust_proof.storage_proof.len(), alloy_proof.storage_proof.len());
    assert_eq!(robust_proof.storage_proof.len(), 1);

    Ok(())
}

// ============================================================================
// eth_getStorageAt
// ============================================================================

#[tokio::test]
async fn test_get_storage_at_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;

    let contract_address = *counter.address();

    let robust_storage = robust.get_storage_at(contract_address, U256::ZERO).await?;
    let alloy_storage = alloy_provider.get_storage_at(contract_address, U256::ZERO).await?;

    assert_eq!(robust_storage, alloy_storage);
    assert_eq!(robust_storage, U256::from(3));

    Ok(())
}

#[tokio::test]
async fn test_get_storage_at_empty_slot() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let contract_address = *counter.address();

    let robust_storage = robust.get_storage_at(contract_address, U256::from(999)).await?;
    let alloy_storage = alloy_provider.get_storage_at(contract_address, U256::from(999)).await?;

    assert_eq!(robust_storage, alloy_storage);
    assert_eq!(robust_storage, U256::ZERO);

    Ok(())
}

// ============================================================================
// eth_getTransactionByBlockHashAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_block_hash_and_index_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_tx = robust.get_transaction_by_block_hash_and_index(block_hash, 0).await?;
    let alloy_tx = alloy_provider.get_transaction_by_block_hash_and_index(block_hash, 0).await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_block_hash_and_index_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_tx = robust.get_transaction_by_block_hash_and_index(block_hash, 999).await?;
    let alloy_tx = alloy_provider.get_transaction_by_block_hash_and_index(block_hash, 999).await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getTransactionByBlockNumberAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_block_number_and_index_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_tx = robust
        .get_transaction_by_block_number_and_index(BlockNumberOrTag::Number(block_number), 0)
        .await?;
    let alloy_tx = alloy_provider
        .get_transaction_by_block_number_and_index(BlockNumberOrTag::Number(block_number), 0)
        .await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_block_number_and_index_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_tx = robust
        .get_transaction_by_block_number_and_index(BlockNumberOrTag::Number(block_number), 999)
        .await?;
    let alloy_tx = alloy_provider
        .get_transaction_by_block_number_and_index(BlockNumberOrTag::Number(block_number), 999)
        .await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getTransactionByHash
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let receipt = counter.increase().send().await?.watch().await?;

    let robust_tx = robust.get_transaction_by_hash(receipt).await?;
    let alloy_tx = alloy_provider.get_transaction_by_hash(receipt).await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_hash_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let fake_hash = B256::ZERO;

    let robust_tx = robust.get_transaction_by_hash(fake_hash).await?;
    let alloy_tx = alloy_provider.get_transaction_by_hash(fake_hash).await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getRawTransactionByHash
// ============================================================================

#[tokio::test]
async fn test_get_raw_transaction_by_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let receipt = counter.increase().send().await?.watch().await?;

    let robust_raw = robust.get_raw_transaction_by_hash(receipt).await?;
    let alloy_raw = alloy_provider.get_raw_transaction_by_hash(receipt).await?;

    assert!(robust_raw.is_some());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}

#[tokio::test]
async fn test_get_raw_transaction_by_hash_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let fake_hash = B256::ZERO;

    let robust_raw = robust.get_raw_transaction_by_hash(fake_hash).await?;
    let alloy_raw = alloy_provider.get_raw_transaction_by_hash(fake_hash).await?;

    assert!(robust_raw.is_none());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}
