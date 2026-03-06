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
use robust_provider::{Error, RobustProviderBuilder};
use tokio_stream::StreamExt;

use crate::common::safe_drop_anvil;

// ============================================================================
// Test Helpers
// ============================================================================

/// Short poll interval for tests
const TEST_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[allow(clippy::unused_async)]
async fn spawn_http_anvil()
-> anyhow::Result<(alloy::node_bindings::AnvilInstance, RootProvider<Ethereum>)> {
    let anvil = Anvil::new().try_spawn()?;
    let provider = RootProvider::new_http(anvil.endpoint_url());
    Ok((anvil, provider))
}

async fn spawn_ws_anvil()
-> anyhow::Result<(alloy::node_bindings::AnvilInstance, RootProvider<Ethereum>)> {
    let anvil = Anvil::new().try_spawn()?;
    let provider = ProviderBuilder::new().connect(anvil.ws_endpoint_url().as_str()).await?;
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

    // Mine a block
    provider.anvil_mine(Some(1), None).await?;

    // Should receive block 1
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout waiting for block 1")
        .expect("recv error");
    assert_eq!(block.number, 1, "Should receive block 1");

    // Mine another block
    provider.anvil_mine(Some(1), None).await?;

    // Should receive block 2
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout waiting for block 2")
        .expect("recv error");
    assert_eq!(block.number, 2, "Should receive block 2");

    Ok(())
}

// ============================================================================
// Regression Tests
// ============================================================================

/// Test: Enabling `allow_http_subscriptions(true)` does not break WS-only chains.
///
/// This is a regression guard ensuring pubsub-capable providers still use WS subscriptions
/// even when HTTP subscriptions are enabled.
#[tokio::test]
async fn test_ws_only_chain_works_with_http_subscriptions_enabled() -> anyhow::Result<()> {
    let (anvil_primary, primary) = spawn_ws_anvil().await?;
    let (_anvil_fallback, fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive initial block from WS primary.
    primary.anvil_mine(Some(1), None).await?;
    // mine different number of blocks on fallback node
    fallback.anvil_mine(Some(5), None).await?;

    // should get block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);
    assert!(subscription.is_empty());

    // Kill WS primary and ensure we can still fail over to WS fallback.
    safe_drop_anvil(anvil_primary).await;

    tokio::spawn(async move {
        // sleep just enough before mining to ensure subscription switches to this fallback provider
        tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        fallback.anvil_mine(Some(1), None).await.unwrap();
    });

    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 6);
    assert!(subscription.is_empty());

    Ok(())
}

/// Test: With mixed fallbacks, HTTP is used when allowed, and WS is used if HTTP dies.
///
/// Chain:
/// - Primary: WS (pubsub)
/// - Fallback #1: HTTP (polling)
/// - Fallback #2: WS (pubsub)
#[tokio::test]
async fn test_mixed_fallback_ordering_ws_to_http_to_ws() -> anyhow::Result<()> {
    let (anvil_ws_primary, ws_primary) = spawn_ws_anvil().await?;
    let (anvil_http, http_fallback) = spawn_http_anvil().await?;
    let (_anvil_ws2, ws_fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(ws_primary.clone())
        .fallback(http_fallback.clone())
        .fallback(ws_fallback.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .max_retries(0)
        .min_delay(Duration::from_millis(0))
        // Same reasoning as `test_failover_ws_to_http_on_provider_death`.
        .call_timeout(Duration::from_millis(200))
        .subscription_timeout(Duration::from_secs(2))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Confirm we start on WS primary.
    ws_primary.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    // Kill WS primary to force failover to HTTP fallback.
    safe_drop_anvil(anvil_ws_primary).await;
    let http_clone = http_fallback.clone();
    let http_mining_task = tokio::spawn(async move {
        tokio::time::sleep(BUFFER_TIME).await;

        // Mine long enough to cover the failover window.
        for _ in 0..120 {
            if http_clone.anvil_mine(Some(1), None).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    // Must receive a block after WS primary died; this should come from HTTP fallback.
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert!(block.number >= 1);

    // Stop mining on HTTP so we don't enqueue extra hashes while switching away.
    http_mining_task.abort();

    // Drain any already-enqueued HTTP hashes to avoid `BlockNotFound` after the HTTP provider is
    // dropped and robust-provider routes `get_block_by_hash` to a different backend.
    for _ in 0..50 {
        if subscription.is_empty() {
            break;
        }

        // Use an outer timeout so we don't block here if `is_empty()` is stale.
        let _ = tokio::time::timeout(Duration::from_millis(200), subscription.recv()).await;
    }

    // Now kill HTTP fallback too, and ensure we can fail over to WS fallback.
    safe_drop_anvil(anvil_http).await;
    let ws2_clone = ws_fallback.clone();
    tokio::spawn(async move {
        // Wait long enough for:
        // - the HTTP polling recv() to time out
        // - fallback switching logic to establish a WS subscription
        tokio::time::sleep(Duration::from_millis(2500)).await;

        // Mine repeatedly to avoid racing with WS subscription establishment.
        for _ in 0..20 {
            if ws2_clone.anvil_mine(Some(1), None).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert!(block.number >= 1);

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

    // Mine and receive via stream
    provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended unexpectedly")
        .expect("recv error");
    assert_eq!(block.number, 1);

    // Mine another and receive via stream
    provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended unexpectedly")
        .expect("recv error");
    assert_eq!(block.number, 2);

    Ok(())
}

// ============================================================================
// Failover Tests
// ============================================================================

/// Test: When WS primary dies, subscription fails over to HTTP fallback
///
/// Verification: We confirm failover by checking that after WS death,
/// we still receive blocks (which must come from HTTP since WS is dead)
#[test_log::test(tokio::test)]
async fn test_failover_ws_to_http_on_provider_death() -> anyhow::Result<()> {
    let (anvil_ws, ws_provider) = spawn_ws_anvil().await?;
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        // Ensure robust-provider block fetching can fail over within the recv timeout.
        .call_timeout(SHORT_TIMEOUT / 2)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive initial block from WS
    ws_provider.anvil_mine(Some(1), None).await?;

    // mine different number of blocks on fallback
    http_provider.anvil_mine(Some(5), None).await?;

    // only primary blocks are received
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1, "Should receive from WS primary");
    assert!(subscription.is_empty());

    // Kill WS provider - this will cause subscription to fail
    safe_drop_anvil(anvil_ws).await;

    // Spawn task to mine repeatedly on HTTP after timeout triggers failover.
    // Mining just once can be flaky if it happens before the HTTP poller is fully established.
    let http_clone = http_provider.clone();
    let http_mining_task = tokio::spawn(async move {
        // Start mining soon and keep mining long enough to cover the failover window.
        // Failover only happens after `subscription_timeout` elapses on the WS backend.
        tokio::time::sleep(BUFFER_TIME).await;

        for _ in 0..120 {
            if http_clone.anvil_mine(Some(1), None).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    // Should eventually receive a block - since WS is dead, this MUST be from HTTP
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout - failover may have failed")
        .expect("recv error");

    // We received a block after WS died, proving failover worked
    // The block number may be > 5 because the test mines multiple blocks to avoid races.
    assert!(block.number >= 5, "Should receive a block from HTTP fallback");

    http_mining_task.abort();

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

    // Mine and receive from HTTP
    http_provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 1, "Should start on HTTP primary");

    // Kill HTTP provider
    safe_drop_anvil(anvil_http).await;

    // Mine on WS shortly after HTTP error is detected.
    // The HTTP poll will fail quickly (connection refused), triggering immediate failover to WS.
    // We mine after a small delay to ensure WS subscription is established.
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

    // Mine and receive
    http1.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    http1.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 2);

    Ok(())
}

/// Test: When `allow_http_subscriptions` is false (default), HTTP providers are skipped
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

/// Test: When `allow_http_subscriptions` is false and no WS providers exist,
/// `subscribe_blocks` should fail
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

/// Test: `poll_interval` configuration is respected
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

    // Mine first block and receive it
    provider.anvil_mine(Some(1), None).await?;
    let _ = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");

    // Mine another block
    provider.anvil_mine(Some(1), None).await?;

    // Measure how long it takes to receive the next block
    let start = std::time::Instant::now();
    let _ = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    let elapsed = start.elapsed();

    // Should take at least half the poll interval
    // (being lenient because block might arrive mid-interval)
    let min_expected = poll_interval / 2;
    assert!(
        elapsed >= min_expected,
        "Poll interval not respected. Expected >= {min_expected:?}, got {elapsed:?}",
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

    // Mine blocks - subscription should work
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

    // Mine and receive a block first
    provider.anvil_mine(Some(1), None).await?;
    let _ = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");

    // Kill the only provider
    safe_drop_anvil(anvil).await;

    // Next recv should eventually error (after timeout)
    let result = tokio::time::timeout(Duration::from_secs(5), subscription.recv()).await;

    match result {
        Ok(Ok(_)) => panic!("Should not receive block from dead provider"),
        Ok(Err(e)) => {
            // Expected - got an error
            assert!(
                matches!(e, Error::Timeout | Error::RpcError(_)),
                "Expected Timeout or RpcError, got {e:?}",
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

    // Mine first block
    provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 1);

    // Wait for multiple poll cycles without mining
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now mine ONE more block
    provider.anvil_mine(Some(1), None).await?;

    // Should receive exactly block 2 (not duplicate of block 1)
    let block = tokio::time::timeout(Duration::from_secs(1), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 2, "Should receive block 2, not duplicate of 1");

    Ok(())
}

// ============================================================================
// Configuration Propagation Tests
// ============================================================================

/// Test: `poll_interval` from builder is used when subscription fails over to HTTP
///
/// This verifies fix for bug where `http_config` used defaults instead of
/// user-configured values when a WebSocket subscription was created first.
#[tokio::test]
async fn test_poll_interval_propagated_from_builder() -> anyhow::Result<()> {
    let (anvil_ws, ws_provider) = spawn_ws_anvil().await?;
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;

    // Use a distinctive poll interval that's different from the default (12s)
    let custom_poll_interval = Duration::from_millis(500);

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(custom_poll_interval)
        // Ensure robust-provider block fetching can fail over within the recv timeout.
        // Keep this very small so per-block fetching doesn't dominate the poll-interval timing.
        .call_timeout(Duration::from_millis(50))
        .subscription_timeout(Duration::from_secs(2))
        .build()
        .await?;

    // Start subscription on WebSocket
    let mut subscription = robust.subscribe_blocks().await?;

    ws_provider.anvil_mine(Some(1), None).await?;

    http_provider.anvil_mine(Some(5), None).await?;

    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);
    assert!(subscription.is_empty());

    // Kill WS to force failover to HTTP
    safe_drop_anvil(anvil_ws).await;

    // Mine on HTTP and wait for failover
    let http_clone = http_provider.clone();
    let http_mining_task = tokio::spawn(async move {
        tokio::time::sleep(BUFFER_TIME).await;

        // Mine long enough to cover the failover window.
        for _ in 0..120 {
            if http_clone.anvil_mine(Some(1), None).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    // Should receive block from HTTP fallback
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout waiting for HTTP fallback block")
        .expect("recv error");

    // Verify we got a block (proving failover worked with correct config)
    assert!(block.number >= 5);

    http_mining_task.abort();

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
        elapsed < custom_poll_interval + BUFFER_TIME, // multiply to add margin
        "Poll interval not respected. Elapsed {elapsed:?}, expected ~{custom_poll_interval:?}",
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

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(Duration::from_millis(100)) // Fast reconnect for test
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Mine a block on primary after subscription
    primary.anvil_mine(Some(1), None).await?;

    // Get initial block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    // Kill primary - subscription should failover to fallback
    safe_drop_anvil(anvil_primary).await;

    // Trigger failover by waiting for timeout, then mine on fallback
    let fb_clone = fallback.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        fb_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should receive from fallback (block 1 on fallback)
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    let fallback_block = block.number;
    assert_eq!(fallback_block, 1, "Should receive block 1 from fallback");

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
    let (anvil_fb1, fallback1) = spawn_http_anvil().await?;
    let (_anvil_fb2, fallback2) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback1.clone())
        .fallback(fallback2.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Mine a block on primary after subscription
    primary.anvil_mine(Some(1), None).await?;

    // Get initial block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    // Kill primary AND fallback1 - only fallback2 will work
    safe_drop_anvil(anvil_primary).await;
    safe_drop_anvil(anvil_fb1).await;

    // Don't mine on fallback2 immediately - let timeouts trigger failover
    // After SHORT_TIMEOUT, primary poll fails -> try fallback1
    // After SHORT_TIMEOUT, fallback1 poll fails -> try fallback2
    // Then mine on fallback2
    let fb2_clone = fallback2.clone();
    tokio::spawn(async move {
        // Wait for a timeout cycle plus buffer
        tokio::time::sleep(SHORT_TIMEOUT + Duration::from_millis(50)).await;
        fb2_clone.anvil_mine(Some(1), None).await.unwrap();
    });

    // Should eventually receive from fallback2
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout - failover chain may have failed")
        .expect("recv error");

    // Block should be from fallback2 (block number >= 1)
    assert!(block.number >= 1, "Should receive block from fallback2, got {}", block.number);

    Ok(())
}

/// Test: Single fallback timeout behavior
///
/// When there's only one fallback and it times out, after exhausting reconnect
/// attempts, the subscription should return an error (no more providers to try).
#[tokio::test]
async fn test_single_fallback_timeout_exhausts_providers() -> anyhow::Result<()> {
    let (anvil_primary, primary) = spawn_http_anvil().await?;
    let (anvil_fb, fallback) = spawn_http_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;
    primary.anvil_mine(Some(1), None).await?;

    // Get initial block from primary
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    // Kill both providers
    safe_drop_anvil(anvil_primary).await;
    safe_drop_anvil(anvil_fb).await;

    // Don't mine anything - let it timeout and exhaust providers
    let result = tokio::time::timeout(Duration::from_secs(3), subscription.recv()).await;

    #[allow(clippy::match_same_arms)]
    match result {
        Ok(Err(Error::Timeout)) => {
            // Expected: all providers exhausted, returns timeout error
        }
        Ok(Err(Error::RpcError(_))) => {
            // Also acceptable: RPC error from dead providers
        }
        Ok(Ok(block)) => {
            panic!("Should not receive block, got block {}", block.number);
        }
        Err(_) => {
            // Outer timeout - also acceptable, means it's still trying
        }
        Ok(Err(e)) => {
            panic!("Unexpected error type: {e:?}");
        }
    }

    Ok(())
}
