//! Tests for RPC call retry and failover functionality.

mod common;

use std::time::{Duration, Instant};

use alloy::{
    eips::BlockNumberOrTag,
    node_bindings::Anvil,
    providers::{Provider, ProviderBuilder, ext::AnvilApi},
};
use robust_provider::{Error, RobustProviderBuilder};

// ============================================================================
// RPC Failover Tests
// ============================================================================

#[tokio::test]
async fn test_rpc_failover_when_primary_dead() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fallback = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fallback = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    // Mine different number of blocks on each to distinguish them
    primary.anvil_mine(Some(10), None).await?;
    fallback.anvil_mine(Some(20), None).await?;

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fallback)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Verify primary is used initially
    let block_num = robust.get_block_number().await?;
    assert_eq!(block_num, 10);

    // Kill primary
    drop(anvil_primary);

    // Should failover to fallback
    let block_num = robust.get_block_number().await?;
    assert_eq!(block_num, 20);

    Ok(())
}

#[tokio::test]
async fn test_rpc_cycles_through_multiple_fallbacks() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fb1 = Anvil::new().try_spawn()?;
    let anvil_fb2 = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fb1 = ProviderBuilder::new().connect_http(anvil_fb1.endpoint_url());
    let fb2 = ProviderBuilder::new().connect_http(anvil_fb2.endpoint_url());

    // Mine different blocks to identify each provider
    primary.anvil_mine(Some(10), None).await?;
    fb1.anvil_mine(Some(20), None).await?;
    fb2.anvil_mine(Some(30), None).await?;

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fb1)
        .fallback(fb2)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Kill primary and first fallback
    drop(anvil_primary);
    drop(anvil_fb1);

    // Should cycle through to fb2
    let block_num = robust.get_block_number().await?;
    assert_eq!(block_num, 30);

    Ok(())
}

#[tokio::test]
async fn test_rpc_all_providers_fail() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fallback = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fallback = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fallback)
        .call_timeout(Duration::from_secs(1))
        .build()
        .await?;

    // Kill all providers
    drop(anvil_primary);
    drop(anvil_fallback);

    // Should fail after trying all providers
    let result = robust.get_block_number().await;
    assert!(result.is_err());

    Ok(())
}

// ============================================================================
// Non-Retryable Error Tests
// ============================================================================

#[tokio::test]
async fn test_block_not_found_does_not_retry() -> anyhow::Result<()> {
    let anvil = Anvil::new().try_spawn()?;
    let provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

    let robust = RobustProviderBuilder::new(provider)
        .call_timeout(Duration::from_secs(5))
        .max_retries(3)
        .min_delay(Duration::from_millis(100))
        .build()
        .await?;

    let start = Instant::now();
    
    // Request future block - should be BlockNotFound, not retried
    let result = robust.get_block(alloy::eips::BlockId::number(999_999)).await;
    
    let elapsed = start.elapsed();

    assert!(matches!(result, Err(Error::BlockNotFound)));
    // With retries, this would take 300ms+ due to backoff
    assert!(elapsed < Duration::from_millis(200));

    Ok(())
}

// ============================================================================
// Timeout Tests
// ============================================================================

#[tokio::test]
async fn test_operation_completes_when_provider_unavailable() -> anyhow::Result<()> {
    // Create and immediately kill provider so endpoint doesn't exist
    let anvil = Anvil::new().try_spawn()?;
    let endpoint = anvil.endpoint_url();
    drop(anvil);
    
    let provider = ProviderBuilder::new().connect_http(endpoint);
    
    let robust = RobustProviderBuilder::fragile(provider)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    let start = Instant::now();
    let result = robust.get_block_number().await;
    let elapsed = start.elapsed();

    // Should fail (connection refused) and not hang
    assert!(result.is_err());
    assert!(elapsed < Duration::from_secs(5));

    Ok(())
}

// ============================================================================
// Failover with Different Operations
// ============================================================================

#[tokio::test]
async fn test_get_accounts_failover() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fallback = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fallback = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fallback)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Kill primary
    drop(anvil_primary);

    let accounts = robust.get_accounts().await?;
    assert!(!accounts.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_get_balance_failover() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fallback = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fallback = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    let accounts = fallback.get_accounts().await?;
    let address = accounts[0];

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fallback)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Kill primary
    drop(anvil_primary);

    let balance = robust.get_balance(address).await?;
    assert!(balance > alloy::primitives::U256::ZERO);

    Ok(())
}

#[tokio::test]
async fn test_get_block_failover() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fallback = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fallback = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    fallback.anvil_mine(Some(5), None).await?;

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fallback)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Kill primary
    drop(anvil_primary);

    let block = robust.get_block_by_number(BlockNumberOrTag::Number(3)).await?;
    assert_eq!(block.header.number, 3);

    Ok(())
}

// ============================================================================
// Primary Provider Preference
// ============================================================================

#[tokio::test]
async fn test_primary_provider_tried_first() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let anvil_fallback = Anvil::new().try_spawn()?;

    let primary = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());
    let fallback = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    primary.anvil_mine(Some(100), None).await?;
    fallback.anvil_mine(Some(200), None).await?;

    let robust = RobustProviderBuilder::fragile(primary)
        .fallback(fallback)
        .call_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Multiple calls should all use primary (it's healthy)
    for _ in 0..5 {
        let block_num = robust.get_block_number().await?;
        assert_eq!(block_num, 100);
    }

    Ok(())
}
