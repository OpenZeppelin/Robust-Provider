use crate::common::{setup_anvil, setup_anvil_with_contract};
use alloy::{eips::BlockNumberOrTag, primitives::B256, providers::Provider};

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
