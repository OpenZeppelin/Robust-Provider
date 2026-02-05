use crate::common::{setup_anvil, setup_anvil_with_blocks, setup_anvil_with_contract};
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    primitives::BlockHash,
    providers::{Provider, ext::AnvilApi},
};
use robust_provider::Error;
use tokio_stream::StreamExt;

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
// eth_getUncleCountByBlockHash / eth_getUncleCountByBlockNumber
// ============================================================================

#[tokio::test]
async fn test_get_uncle_count_by_block_number_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(10).await?;

    let tags = [
        BlockId::number(5),
        BlockId::latest(),
        BlockId::earliest(),
        BlockId::safe(),
        BlockId::finalized(),
    ];

    for tag in tags {
        let robust_count = robust.get_uncle_count(tag).await?;
        let alloy_count = alloy_provider.get_uncle_count(tag).await?;

        assert_eq!(robust_count, alloy_count);
        assert_eq!(robust_count, 0);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_uncle_count_by_block_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(10).await?;

    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(5))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_count = robust.get_uncle_count(BlockId::hash(block_hash)).await?;
    let alloy_count = alloy_provider.get_uncle_count(BlockId::hash(block_hash)).await?;

    assert_eq!(robust_count, alloy_count);
    assert_eq!(robust_count, 0);

    Ok(())
}

// ============================================================================
// eth_getUncleByBlockHashAndIndex / eth_getUncleByBlockNumberAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_uncle_by_block_number_returns_none() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(10).await?;

    // Anvil doesn't produce uncles, so get_uncle should return None
    let robust_uncle = robust.get_uncle(BlockId::number(5), 0).await?;
    let alloy_uncle = alloy_provider.get_uncle(BlockId::number(5), 0).await?;

    assert!(robust_uncle.is_none());
    assert_eq!(robust_uncle, alloy_uncle);

    Ok(())
}

#[tokio::test]
async fn test_get_uncle_by_block_hash_returns_none() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(10).await?;

    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(5))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    // Anvil doesn't produce uncles, so get_uncle should return None
    let robust_uncle = robust.get_uncle(BlockId::hash(block_hash), 0).await?;
    let alloy_uncle = alloy_provider.get_uncle(BlockId::hash(block_hash), 0).await?;

    assert!(robust_uncle.is_none());
    assert_eq!(robust_uncle, alloy_uncle);

    Ok(())
}

#[tokio::test]
async fn test_get_uncle_with_various_block_tags() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(10).await?;

    let tags = [BlockId::latest(), BlockId::earliest(), BlockId::safe(), BlockId::finalized()];

    for tag in tags {
        let robust_uncle = robust.get_uncle(tag, 0).await?;
        let alloy_uncle = alloy_provider.get_uncle(tag, 0).await?;

        assert!(robust_uncle.is_none());
        assert_eq!(robust_uncle, alloy_uncle);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_uncle_with_various_indices() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(10).await?;

    for idx in [0, 1, 2, 10, 100] {
        let robust_uncle = robust.get_uncle(BlockId::number(5), idx).await?;
        let alloy_uncle = alloy_provider.get_uncle(BlockId::number(5), idx).await?;

        assert!(robust_uncle.is_none());
        assert_eq!(robust_uncle, alloy_uncle);
    }

    Ok(())
}

// ============================================================================
// watch_blocks
// ============================================================================

#[tokio::test]
async fn test_watch_blocks_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_poller = robust.watch_blocks().await?;
    let mut stream = robust_poller.into_stream();

    alloy_provider.anvil_mine(Some(3), None).await?;

    let block_hashes = stream.next().await.expect("should get block hashes");

    assert!(!block_hashes.is_empty());
    for hash in &block_hashes {
        let block = alloy_provider.get_block_by_hash(*hash).await?;
        assert!(block.is_some());
    }

    Ok(())
}

#[tokio::test]
async fn test_watch_blocks_returns_correct_hashes() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_poller = robust.watch_blocks().await?;
    let mut stream = robust_poller.into_stream();

    alloy_provider.anvil_mine(Some(1), None).await?;

    let watch_hashes = stream.next().await.expect("should get block hashes");
    assert!(!watch_hashes.is_empty());

    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(1))
        .await?
        .expect("block should exist");

    assert!(watch_hashes.contains(&block.header.hash));

    Ok(())
}

// ============================================================================
// watch_full_blocks
// ============================================================================

#[tokio::test]
async fn test_watch_full_blocks_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let watch = robust.watch_full_blocks().await?;
    let mut stream = watch.into_stream();

    alloy_provider.anvil_mine(Some(2), None).await?;

    let block1 = stream.next().await.expect("should get block")?;
    let block2 = stream.next().await.expect("should get block")?;

    assert_eq!(block1.header.number, 1);
    assert_eq!(block2.header.number, 2);

    Ok(())
}

#[tokio::test]
async fn test_watch_full_blocks_with_full_transactions() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider, counter) = setup_anvil_with_contract().await?;

    let watch = robust.watch_full_blocks().await?.full();
    let mut stream = watch.into_stream();

    let _ = counter.increase().send().await?.watch().await?;

    let mut found_tx = false;
    for _ in 0..5 {
        if let Some(Ok(block)) = stream.next().await &&
            !block.transactions.is_empty()
        {
            found_tx = true;
            break;
        }
    }

    assert!(found_tx, "Should find a block with transactions");

    Ok(())
}
