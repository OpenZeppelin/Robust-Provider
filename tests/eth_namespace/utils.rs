use alloy::providers::Provider;

use crate::common::setup_anvil;

#[tokio::test]
async fn test_get_client_version_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_accounts = robust.get_client_version().await?;
    let alloy_accounts = alloy_provider.get_client_version().await?;

    assert_eq!(robust_accounts, alloy_accounts);

    Ok(())
}
