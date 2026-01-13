//! Tests for `RobustProvider` custom methods.
//!
//! These tests cover methods that are unique to `RobustProvider` and not
//! direct wrappers of standard Ethereum JSON-RPC methods.

mod common;

use common::setup_anvil_with_blocks;

// ============================================================================
// get_latest_confirmed
// ============================================================================

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

// ============================================================================
// get_safe_finalized_block
// ============================================================================

#[tokio::test]
#[ignore = "This currently passes but does not test against a 'real' node - see issue https://github.com/OpenZeppelin/Robust-Provider/issues/7 "]
async fn test_get_safe_finalized_block_with_small_chain_height() -> anyhow::Result<()> {
    // With only genesis block, finalized block may not be available on real nodes
    // but Anvil returns block 0. Either way, get_safe_finalized_block should succeed.
    let (_anvil, robust, _alloy_provider) = setup_anvil_with_blocks(0).await?;

    let safe_finalized = robust.get_safe_finalized_block().await?;

    // Should return genesis or block 0 (Anvil behavior)
    assert_eq!(safe_finalized.header.number, 0);

    Ok(())
}

// ============================================================================
// get_safe_finalized_block_number
// ============================================================================

#[tokio::test]
#[ignore = "This currently passes but does not test against a 'real' node - see issue https://github.com/OpenZeppelin/Robust-Provider/issues/7 "]
async fn test_get_safe_finalized_block_number_with_small_chain_height() -> anyhow::Result<()> {
    // With only genesis block, finalized block number may not be available on real nodes
    // but Anvil returns 0. Either way, get_safe_finalized_block_number should succeed.
    let (_anvil, robust, _alloy_provider) = setup_anvil_with_blocks(0).await?;

    let safe_finalized_num = robust.get_safe_finalized_block_number().await?;

    // Should return 0 (either directly from Anvil or as fallback)
    assert_eq!(safe_finalized_num, 0);

    Ok(())
}
