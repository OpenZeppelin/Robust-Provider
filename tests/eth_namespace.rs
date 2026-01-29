//! Tests for Ethereum JSON-RPC namespace methods exposed by `RobustProvider`.
//!
//! These tests verify the behavior of standard Ethereum RPC methods wrapped
//! by `RobustProvider` with retry and failover logic.

mod common;

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::TransactionBuilder,
    primitives::{BlockHash, U256},
    providers::{Provider, ext::AnvilApi},
    rpc::types::{Bundle, TransactionRequest},
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
