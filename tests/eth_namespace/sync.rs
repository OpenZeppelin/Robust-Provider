use crate::common::setup_anvil;
use alloy::providers::Provider;

// ============================================================================
// eth_syncing
// ============================================================================

#[tokio::test]
async fn test_syncing_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_status = robust.syncing().await?;
    let alloy_status = alloy_provider.syncing().await?;

    assert_eq!(robust_status, alloy_status);

    Ok(())
}
