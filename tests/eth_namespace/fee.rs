use crate::common::{setup_anvil, setup_anvil_with_blocks};
use alloy::{
    eips::{BlockNumberOrTag, eip1559::Eip1559Estimation},
    providers::{Provider, utils::Eip1559Estimator},
};

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

    assert_eq!(robust_fee_history, alloy_fee_history);

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

// ============================================================================
// estimate_eip1559_fees
// ============================================================================

#[tokio::test]
async fn test_estimate_eip1559_fees_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_gas = robust.estimate_eip1559_fees().await?;
    let alloy_gas = alloy_provider.estimate_eip1559_fees().await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

// ============================================================================
// estimate_eip1559_fees_with
// ============================================================================

#[tokio::test]
#[ignore = "Flaky, see: https://github.com/OpenZeppelin/Robust-Provider/issues/59"]
async fn test_estimate_eip1559_fees_with_default_estimator() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let robust_gas = robust.estimate_eip1559_fees_with(Eip1559Estimator::default()).await?;
    let alloy_gas = alloy_provider.estimate_eip1559_fees_with(Eip1559Estimator::default()).await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

#[tokio::test]
async fn test_estimate_eip1559_fees_with_custom_estimator() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let robust_gas = robust
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, _rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee * 2,
            max_priority_fee_per_gas: base_fee / 10,
        }))
        .await?;

    let alloy_gas = alloy_provider
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, _rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee * 2,
            max_priority_fee_per_gas: base_fee / 10,
        }))
        .await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

#[tokio::test]
async fn test_estimate_eip1559_fees_with_zero_priority_fee() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(50).await?;

    let robust_gas = robust
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, _rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee + 1_000_000_000,
            max_priority_fee_per_gas: 0,
        }))
        .await?;

    let alloy_gas = alloy_provider
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, _rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee + 1_000_000_000,
            max_priority_fee_per_gas: 0,
        }))
        .await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

#[tokio::test]
async fn test_estimate_eip1559_fees_with_high_priority_fee() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(50).await?;

    let robust_gas = robust
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, _rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee * 3,
            max_priority_fee_per_gas: base_fee,
        }))
        .await?;

    let alloy_gas = alloy_provider
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, _rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee * 3,
            max_priority_fee_per_gas: base_fee,
        }))
        .await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

#[tokio::test]
#[ignore = "Flaky, see: https://github.com/OpenZeppelin/Robust-Provider/issues/59"]
async fn test_estimate_eip1559_fees_with_reward_percentile_based() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil_with_blocks(100).await?;

    let robust_gas = robust
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee * 2,
            max_priority_fee_per_gas: if !rewards.is_empty() && !rewards[0].is_empty() {
                rewards[0][0]
            } else {
                base_fee / 100
            },
        }))
        .await?;

    let alloy_gas = alloy_provider
        .estimate_eip1559_fees_with(Eip1559Estimator::new(|base_fee, rewards| Eip1559Estimation {
            max_fee_per_gas: base_fee * 2,
            max_priority_fee_per_gas: if !rewards.is_empty() && !rewards[0].is_empty() {
                rewards[0][0]
            } else {
                base_fee / 100
            },
        }))
        .await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}
