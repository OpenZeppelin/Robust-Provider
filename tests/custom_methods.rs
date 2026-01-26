//! Tests for `RobustProvider` custom methods.
//!
//! These tests cover methods that are unique to `RobustProvider` and not
//! direct wrappers of standard Ethereum JSON-RPC methods.

// ============================================================================
// get_latest_confirmed
// ============================================================================

use crate::common::setup_anvil::setup_anvil_with_blocks;

mod common;

#[tokio::test]
async fn test_get_latest_confirmed_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil_with_blocks(100).await?;

    // With confirmations
    let confirmed_block = robust.get_latest_confirmed(10).await?;
    assert_eq!(confirmed_block, 90);

    // Zero confirmations returns latest
    let confirmed_block = robust.get_latest_confirmed(0).await?;
    assert_eq!(confirmed_block, 100);

    // Single confirmation
    let confirmed_block = robust.get_latest_confirmed(1).await?;
    assert_eq!(confirmed_block, 99);

    // confirmations = latest - 1
    let confirmed_block = robust.get_latest_confirmed(99).await?;
    assert_eq!(confirmed_block, 1);

    // confirmations = latest (should return 0)
    let confirmed_block = robust.get_latest_confirmed(100).await?;
    assert_eq!(confirmed_block, 0);

    // confirmations = latest + 1 (saturates at zero)
    let confirmed_block = robust.get_latest_confirmed(101).await?;
    assert_eq!(confirmed_block, 0);

    // Saturates at zero when confirmations > latest
    let confirmed_block = robust.get_latest_confirmed(200).await?;
    assert_eq!(confirmed_block, 0);

    Ok(())
}
