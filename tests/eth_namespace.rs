//! Tests for Ethereum JSON-RPC namespace methods exposed by RobustProvider.
//!
//! These tests verify the behavior of standard Ethereum RPC methods wrapped
//! by RobustProvider with retry and failover logic.

mod common;

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    primitives::BlockHash,
    providers::{Provider, ext::AnvilApi},
    rpc::types::Filter,
};
use common::{setup_anvil, setup_anvil_with_blocks};
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

    assert!(matches!(result, Err(Error::BlockNotFound(_))));

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
    assert!(matches!(result, Err(Error::BlockNotFound(_))));

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
    assert!(matches!(result, Err(Error::BlockNotFound(_))));

    // Non-existent hash
    let result = robust.get_block(BlockId::hash(BlockHash::ZERO)).await;
    assert!(matches!(result, Err(Error::BlockNotFound(_))));

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
    assert!(matches!(result, Err(Error::BlockNotFound(_))));

    Ok(())
}
