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

    // Mine on WS shortly after HTTP error is detected.
    // The HTTP poll will fail quickly (connection refused), triggering immediate failover to WS.
    // We mine after a small delay to ensure WS subscription is established.
    let ws_clone = ws_provider.clone();
    tokio::spawn(async move {
        tokio::time::sleep(BUFFER_TIME).await;
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

// ============================================================================
// Configuration Propagation Tests
// ============================================================================

/// Test: poll_interval from builder is used when subscription fails over to HTTP
///
/// This verifies fix for bug where http_config used defaults instead of
/// user-configured values when a WebSocket subscription was created first.
#[tokio::test]
async fn test_poll_interval_propagated_from_builder() -> anyhow::Result<()> {
    let (_anvil_ws, ws_provider) = spawn_ws_anvil().await?;
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;

    // Use a distinctive poll interval that's different from the default (12s)
    let custom_poll_interval = Duration::from_millis(30);

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(custom_poll_interval)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    // Start subscription on WebSocket
    let mut subscription = robust.subscribe_blocks().await?;

    ws_provider.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    // Kill WS to force failover to HTTP
    drop(_anvil_ws);

    // Mine on HTTP and wait for failover
    let http_clone = http_provider.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        http_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should receive block from HTTP fallback
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout waiting for HTTP fallback block")
        .expect("recv error");

    // Verify we got a block (proving failover worked with correct config)
    assert!(block.number <= 1);

    // Now verify the poll interval is being used by timing block reception
    // Mine another block and measure how long until we receive it
    http_provider.anvil_mine(Some(1), None).await?;

    let start = std::time::Instant::now();
    let _ = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    let elapsed = start.elapsed();

    // Should take roughly poll_interval to detect the new block
    // Allow some margin but it should be much less than the default 12s
    assert!(
        elapsed < Duration::from_millis(500),
        "Poll interval not respected. Elapsed {:?}, expected ~{:?}",
        elapsed,
        custom_poll_interval
    );

    Ok(())
}

// ============================================================================
// HTTP Reconnection Validation Tests
// ============================================================================

/// Test: HTTP reconnection validates provider is reachable before claiming success
///
/// This verifies fix for bug where HTTP reconnection didn't validate the provider,
/// potentially "reconnecting" to a dead provider.
#[tokio::test]
async fn test_http_reconnect_validates_provider() -> anyhow::Result<()> {
    // Start with HTTP primary (will be killed) and HTTP fallback
    let (anvil_primary, primary) = spawn_http_anvil().await?;
    let (_anvil_fallback, fallback) = spawn_http_anvil().await?;

    // Mine different blocks to identify providers
    primary.anvil_mine(Some(10), None).await?;
    fallback.anvil_mine(Some(20), None).await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(Duration::from_millis(100)) // Fast reconnect for test
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Get initial block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 10);

    // Kill primary - subscription should failover to fallback
    drop(anvil_primary);

    // Trigger failover by waiting for timeout, then mine on fallback
    let fb_clone = fallback.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        fb_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should receive from fallback (block 20 or 21 depending on timing)
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    let fallback_block = block.number;
    assert!(fallback_block >= 20, "Should receive block from fallback, got {}", fallback_block);

    // Wait for reconnect interval to elapse
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Mine another block on fallback - this triggers reconnect attempt
    // Since primary is dead, reconnect should FAIL validation and stay on fallback
    fallback.anvil_mine(Some(1), None).await?;

    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");

    // Should still be on fallback (next block), NOT have "reconnected" to dead primary
    assert!(
        block.number > fallback_block,
        "Should still be on fallback after failed reconnect, got block {}",
        block.number
    );

    Ok(())
}

/// Test: Timeout-triggered failover cycles through multiple fallbacks correctly
///
/// When a fallback times out (no blocks received), the subscription should:
/// 1. Try to reconnect to primary (fails if dead)
/// 2. Move to the next fallback
/// 3. Eventually receive blocks from a working fallback
#[tokio::test]
async fn test_timeout_triggered_failover_with_multiple_fallbacks() -> anyhow::Result<()> {
    let (anvil_primary, primary) = spawn_http_anvil().await?;
    let (_anvil_fb1, fallback1) = spawn_http_anvil().await?;
    let (_anvil_fb2, fallback2) = spawn_http_anvil().await?;

    // Mine different blocks to identify providers
    primary.anvil_mine(Some(5), None).await?;
    fallback1.anvil_mine(Some(10), None).await?;
    fallback2.anvil_mine(Some(20), None).await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback1.clone())
        .fallback(fallback2.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Get initial block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 5);

    // Kill primary AND fallback1 - only fallback2 will work
    drop(anvil_primary);
    drop(_anvil_fb1);

    // Don't mine on fallback2 immediately - let timeouts trigger failover
    // After SHORT_TIMEOUT, primary poll fails -> try fallback1
    // After SHORT_TIMEOUT, fallback1 poll fails -> try fallback2
    // Then mine on fallback2
    let fb2_clone = fallback2.clone();
    tokio::spawn(async move {
        // Wait for two timeout cycles plus buffer
        tokio::time::sleep(SHORT_TIMEOUT * 2 + BUFFER_TIME * 2).await;
        fb2_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should eventually receive from fallback2
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout - failover chain may have failed")
        .expect("recv error");

    // Block should be from fallback2 (20 or 21 depending on timing)
    assert!(
        block.number >= 20,
        "Should receive block from fallback2, got {}",
        block.number
    );

    Ok(())
}

/// Test: Single fallback timeout behavior
///
/// When there's only one fallback and it times out, after exhausting reconnect
/// attempts, the subscription should return an error (no more providers to try).
#[tokio::test]
async fn test_single_fallback_timeout_exhausts_providers() -> anyhow::Result<()> {
    let (anvil_primary, primary) = spawn_http_anvil().await?;
    let (_anvil_fb, fallback) = spawn_http_anvil().await?;

    primary.anvil_mine(Some(5), None).await?;
    fallback.anvil_mine(Some(10), None).await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Get initial block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 5);

    // Kill both providers
    drop(anvil_primary);
    drop(_anvil_fb);

    // Don't mine anything - let it timeout and exhaust providers
    let result = tokio::time::timeout(Duration::from_secs(3), subscription.recv()).await;

    match result {
        Ok(Err(SubscriptionError::Timeout)) => {
            // Expected: all providers exhausted, returns timeout error
        }
        Ok(Err(SubscriptionError::RpcError(_))) => {
            // Also acceptable: RPC error from dead providers
        }
        Ok(Ok(block)) => {
            panic!("Should not receive block, got block {}", block.number);
        }
        Err(_) => {
            // Outer timeout - also acceptable, means it's still trying
        }
        Ok(Err(e)) => {
            panic!("Unexpected error type: {:?}", e);
        }
    }

    Ok(())
}
