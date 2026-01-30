use crate::common::{setup_anvil, setup_anvil_with_blocks};
use alloy::{eips::BlockNumberOrTag, providers::Provider};

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
// eth_maxPriorityFeePerGas
// ============================================================================

#[tokio::test]
async fn test_get_max_priority_fee_per_gas_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_fee = robust.get_max_priority_fee_per_gas().await?;
    let alloy_fee = alloy_provider.get_max_priority_fee_per_gas().await?;

    assert_eq!(robust_fee, alloy_fee);

    Ok(())
}
