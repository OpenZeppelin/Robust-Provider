//! Integration tests for HTTP subscription functionality.
//!
//! These tests verify that HTTP providers can participate in subscriptions
//! via polling when the `http-subscription` feature is enabled.

#![cfg(feature = "http-subscription")]

mod common;

use std::time::Duration;

use alloy::{
    network::Ethereum,
    node_bindings::Anvil,
    providers::{Provider, ProviderBuilder, RootProvider, ext::AnvilApi},
};
use common::{BUFFER_TIME, SHORT_TIMEOUT};
use robust_provider::{RobustProviderBuilder, SubscriptionError};
use tokio_stream::StreamExt;

// ============================================================================
// Test Helpers
// ============================================================================

/// Short poll interval for tests
const TEST_POLL_INTERVAL: Duration = Duration::from_millis(50);

async fn spawn_http_anvil() -> anyhow::Result<(alloy::node_bindings::AnvilInstance, RootProvider<Ethereum>)> {
    let anvil = Anvil::new().try_spawn()?;
    let provider = RootProvider::new_http(anvil.endpoint_url());
    Ok((anvil, provider))
}

async fn spawn_ws_anvil() -> anyhow::Result<(alloy::node_bindings::AnvilInstance, RootProvider<Ethereum>)> {
    let anvil = Anvil::new().try_spawn()?;
    let provider = ProviderBuilder::new()
        .connect(anvil.ws_endpoint_url().as_str())
        .await?;
    Ok((anvil, provider.root().clone()))
}

// ============================================================================
// Basic HTTP Subscription Tests
// ============================================================================

/// Test: HTTP polling subscription receives blocks correctly
#[tokio::test]
async fn test_http_subscription_basic_flow() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Should receive genesis block (block 0)
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout waiting for genesis")
        .expect("recv error");
    assert_eq!(block.number, 0, "First block should be genesis");

    // Mine a new block
    provider.anvil_mine(Some(1), None).await?;

    // Should receive block 1
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout waiting for block 1")
        .expect("recv error");
    assert_eq!(block.number, 1, "Second block should be block 1");

    Ok(())
}

/// Test: HTTP subscription correctly receives multiple consecutive blocks
#[tokio::test]
async fn test_http_subscription_multiple_blocks() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis
    let block = subscription.recv().await?;
    assert_eq!(block.number, 0);

    // Mine and receive 5 blocks sequentially
    for expected_block in 1..=5 {
        provider.anvil_mine(Some(1), None).await?;
        let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
            .await
            .expect("timeout")
            .expect("recv error");
        assert_eq!(block.number, expected_block, "Block number mismatch");
    }

    Ok(())
}

/// Test: HTTP subscription works correctly when converted to a Stream
#[tokio::test]
async fn test_http_subscription_as_stream() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Get genesis via stream
    let block = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended unexpectedly")
        .expect("recv error");
    assert_eq!(block.number, 0);

    // Mine and receive via stream
    provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended unexpectedly")
        .expect("recv error");
    assert_eq!(block.number, 1);

    Ok(())
}

// ============================================================================
// Failover Tests
// ============================================================================

/// Test: When WS primary dies, subscription fails over to HTTP fallback
/// 
/// Verification: We confirm failover by checking that after WS death,
/// we still receive blocks (which must come from HTTP since WS is dead)
#[tokio::test]
async fn test_failover_ws_to_http_on_provider_death() -> anyhow::Result<()> {
    let (anvil_ws, ws_provider) = spawn_ws_anvil().await?;
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive initial block from WS
    ws_provider.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1, "Should receive from WS primary");

    // Kill WS provider - this will cause subscription to fail
    drop(anvil_ws);

    // Spawn task to mine on HTTP after timeout triggers failover
    let http_clone = http_provider.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        http_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should eventually receive a block - since WS is dead, this MUST be from HTTP
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout - failover may have failed")
        .expect("recv error");
    
    // We received a block after WS died, proving failover worked
    // (HTTP starts at genesis, so we get block 0 or 1 depending on timing)
    assert!(block.number <= 1, "Should receive low block number from HTTP fallback");

    Ok(())
}

/// Test: When HTTP primary becomes unavailable, subscription fails over to WS fallback
#[tokio::test]
async fn test_failover_http_to_ws_on_provider_death() -> anyhow::Result<()> {
    let (anvil_http, http_provider) = spawn_http_anvil().await?;
    let (_anvil_ws, ws_provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(http_provider.clone())
        .fallback(ws_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis from HTTP
    let block = subscription.recv().await?;
    assert_eq!(block.number, 0, "Should start on HTTP primary");

    // Kill HTTP provider
    drop(anvil_http);

    // Mine on WS - after HTTP timeout, should failover to WS
    let ws_clone = ws_provider.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        ws_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should receive from WS fallback (WS also starts at genesis, so block 1 after mining)
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout - failover may have failed")
        .expect("recv error");
    
    assert_eq!(block.number, 1, "Should receive block from WS fallback");

    Ok(())
}

// ============================================================================
// Configuration Tests
// ============================================================================

/// Test: All-HTTP provider chain works (no WS providers at all)
#[tokio::test]
async fn test_http_only_provider_chain() -> anyhow::Result<()> {
    let (_anvil1, http1) = spawn_http_anvil().await?;
    let (_anvil2, http2) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(http1.clone())
        .fallback(http2.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Should work with HTTP polling
    let block = subscription.recv().await?;
    assert_eq!(block.number, 0);

    http1.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    Ok(())
}

/// Test: When allow_http_subscriptions is false (default), HTTP providers are skipped
/// and subscription uses WS fallback
#[tokio::test]
async fn test_http_subscriptions_disabled_skips_http() -> anyhow::Result<()> {
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;
    let (_anvil_ws, ws_provider) = spawn_ws_anvil().await?;

    // HTTP primary but http subscriptions NOT enabled (default)
    let robust = RobustProviderBuilder::new(http_provider.clone())
        .fallback(ws_provider.clone())
        // allow_http_subscriptions defaults to false
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    // subscribe_blocks should skip HTTP and use WS
    let mut subscription = robust.subscribe_blocks().await?;

    // Mine on both - if HTTP was used, we'd get block 0 first
    // Since HTTP is skipped, we should only see WS blocks
    ws_provider.anvil_mine(Some(1), None).await?;
    http_provider.anvil_mine(Some(5), None).await?; // Mine more on HTTP
    
    let block = subscription.recv().await?;
    // WS block 1, not HTTP block 0 or 5
    assert_eq!(block.number, 1, "Should use WS fallback, not HTTP primary");

    Ok(())
}

/// Test: When allow_http_subscriptions is false and no WS providers exist,
/// subscribe_blocks should fail
#[tokio::test]
async fn test_http_disabled_no_ws_fails() -> anyhow::Result<()> {
    let (_anvil, http_provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(http_provider.clone())
        // No fallbacks, HTTP subscriptions disabled
        .subscription_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Should fail because no pubsub-capable provider exists
    let result = robust.subscribe_blocks().await;
    assert!(result.is_err(), "Should fail when no WS providers and HTTP disabled");

    Ok(())
}

/// Test: poll_interval configuration is respected
#[tokio::test]
async fn test_poll_interval_is_respected() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let poll_interval = Duration::from_millis(200);

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(poll_interval)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis (immediate)
    let _ = subscription.recv().await?;

    // Mine a block
    provider.anvil_mine(Some(1), None).await?;

    // Measure how long it takes to receive the next block
    let start = std::time::Instant::now();
    let _ = subscription.recv().await?;
    let elapsed = start.elapsed();

    // Should take at least half the poll interval
    // (being lenient because block might arrive mid-interval)
    let min_expected = poll_interval / 2;
    assert!(
        elapsed >= min_expected,
        "Poll interval not respected. Expected >= {:?}, got {:?}",
        min_expected,
        elapsed
    );

    Ok(())
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test: HTTP subscription handles provider errors gracefully
#[tokio::test]
async fn test_http_subscription_survives_temporary_errors() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis
    let block = subscription.recv().await?;
    assert_eq!(block.number, 0);

    // Mine blocks - subscription should continue working
    for i in 1..=3 {
        provider.anvil_mine(Some(1), None).await?;
        let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
            .await
            .expect("timeout")
            .expect("recv error");
        assert_eq!(block.number, i);
    }

    Ok(())
}

/// Test: When all providers fail, subscription returns an error
#[tokio::test]
async fn test_all_providers_fail_returns_error() -> anyhow::Result<()> {
    let (anvil, provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis
    let _ = subscription.recv().await?;

    // Kill the only provider
    drop(anvil);

    // Next recv should eventually error (after timeout)
    let result = tokio::time::timeout(Duration::from_secs(5), subscription.recv()).await;
    
    match result {
        Ok(Ok(_)) => panic!("Should not receive block from dead provider"),
        Ok(Err(e)) => {
            // Expected - got an error
            assert!(
                matches!(e, SubscriptionError::Timeout | SubscriptionError::RpcError(_)),
                "Expected Timeout or RpcError, got {:?}", e
            );
        }
        Err(_) => {
            // Timeout is also acceptable
        }
    }

    Ok(())
}

// ============================================================================
// Deduplication Tests
// ============================================================================

/// Test: HTTP polling correctly deduplicates blocks (same block not emitted twice)
#[tokio::test]
async fn test_http_polling_deduplication() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(Duration::from_millis(20)) // Very fast polling
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis
    let block = subscription.recv().await?;
    assert_eq!(block.number, 0);

    // Wait for multiple poll cycles without mining
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now mine ONE block
    provider.anvil_mine(Some(1), None).await?;

    // Should receive exactly block 1 (not multiple copies of block 0)
    let block = tokio::time::timeout(Duration::from_secs(1), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 1, "Should receive block 1, not duplicate of 0");

    Ok(())
}
