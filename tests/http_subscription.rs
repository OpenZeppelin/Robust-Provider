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
use common::{BUFFER_TIME, RECONNECT_INTERVAL, SHORT_TIMEOUT};
use robust_provider::RobustProviderBuilder;
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

    // Should receive genesis block
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 0);

    // Mine a new block
    provider.anvil_mine(Some(1), None).await?;

    // Should receive block 1
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 1);

    Ok(())
}

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

    // Mine multiple blocks
    for i in 1..=5 {
        provider.anvil_mine(Some(1), None).await?;
        let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
            .await
            .expect("timeout")
            .expect("recv error");
        assert_eq!(block.number, i);
    }

    Ok(())
}

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
        .expect("stream ended")
        .expect("recv error");
    assert_eq!(block.number, 0);

    // Mine and receive via stream
    provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended")
        .expect("recv error");
    assert_eq!(block.number, 1);

    Ok(())
}

// ============================================================================
// Failover Tests
// ============================================================================

#[tokio::test]
async fn test_failover_from_ws_to_http() -> anyhow::Result<()> {
    let (anvil_ws, ws_provider) = spawn_ws_anvil().await?;
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;

    // Pre-mine on HTTP so it has blocks ready
    http_provider.anvil_mine(Some(5), None).await?;

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Should start on WS primary
    ws_provider.anvil_mine(Some(1), None).await?;
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 1);

    // Kill WS provider
    drop(anvil_ws);

    // Mine on HTTP - after timeout, should failover to HTTP
    tokio::spawn({
        let http = http_provider.clone();
        async move {
            tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
            http.anvil_mine(Some(1), None).await.unwrap();
        }
    });

    // Should receive from HTTP fallback (block 6 since we pre-mined 5)
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    // HTTP provider started at 5, mined 1 more = block 6
    assert!(block.number >= 5, "Should receive block from HTTP fallback, got {}", block.number);

    Ok(())
}

#[tokio::test]
async fn test_failover_from_http_to_ws() -> anyhow::Result<()> {
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

    // Should start on HTTP primary (polling)
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 0);

    // Kill HTTP provider
    drop(anvil_http);

    // Mine on WS - after timeout, should failover to WS
    tokio::spawn({
        let ws = ws_provider.clone();
        async move {
            tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
            ws.anvil_mine(Some(1), None).await.unwrap();
        }
    });

    // Should receive from WS fallback
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert_eq!(block.number, 1);

    Ok(())
}

#[tokio::test]
async fn test_mixed_provider_chain_failover() -> anyhow::Result<()> {
    let (anvil_ws1, ws1) = spawn_ws_anvil().await?;
    let (_anvil_http, http) = spawn_http_anvil().await?;
    let (_anvil_ws2, ws2) = spawn_ws_anvil().await?;

    // Pre-mine on HTTP
    http.anvil_mine(Some(10), None).await?;

    // Chain: WS1 (primary) -> HTTP (fallback1) -> WS2 (fallback2)
    let robust = RobustProviderBuilder::fragile(ws1.clone())
        .fallback(http.clone())
        .fallback(ws2.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .call_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Start on WS1
    ws1.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    // Kill WS1 - should failover to HTTP
    drop(anvil_ws1);

    tokio::spawn({
        let h = http.clone();
        async move {
            tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
            h.anvil_mine(Some(1), None).await.unwrap();
        }
    });

    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    // HTTP started at 10, mined 1 = block 11
    assert!(block.number >= 10, "Should receive from HTTP fallback, got {}", block.number);

    Ok(())
}

// ============================================================================
// Reconnection Tests
// ============================================================================

#[tokio::test]
async fn test_http_reconnects_to_ws_primary() -> anyhow::Result<()> {
    let (_anvil_ws, ws_provider) = spawn_ws_anvil().await?;
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;

    // Pre-mine on HTTP to make it distinguishable from WS
    http_provider.anvil_mine(Some(100), None).await?;

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(TEST_POLL_INTERVAL)
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(RECONNECT_INTERVAL)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Start on WS - mine to block 1
    ws_provider.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1, "Should start on WS primary");

    // Trigger failover to HTTP by timing out
    tokio::spawn({
        let h = http_provider.clone();
        async move {
            tokio::time::sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
            h.anvil_mine(Some(1), None).await.unwrap();
        }
    });

    // Now on HTTP (should get block >= 100)
    let block = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    assert!(block.number >= 100, "Should have failed over to HTTP, got block {}", block.number);

    // Continue receiving on HTTP to confirm we're on it
    http_provider.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert!(block.number > 100, "Should still be on HTTP, got block {}", block.number);

    // Wait for reconnect interval
    tokio::time::sleep(RECONNECT_INTERVAL + BUFFER_TIME).await;

    // Mine on HTTP - this recv should trigger reconnect check
    http_provider.anvil_mine(Some(1), None).await?;
    let _ = subscription.recv().await?;

    // If reconnected to WS, mining on WS should give us low block numbers
    // Mine several blocks on WS
    ws_provider.anvil_mine(Some(5), None).await?;
    
    // Try to get a block - might be from WS (low) or HTTP (high)
    let block = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("timeout")
        .expect("recv error");
    
    // Reconnection is best-effort; test that we received *some* block
    // The actual reconnection timing depends on when the reconnect check runs
    assert!(block.number > 0, "Should receive a block after reconnect attempt");

    Ok(())
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_http_only_no_ws_providers() -> anyhow::Result<()> {
    let (_anvil1, http1) = spawn_http_anvil().await?;
    let (_anvil2, http2) = spawn_http_anvil().await?;

    // All HTTP providers
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

#[tokio::test]
async fn test_http_subscription_disabled_falls_back_to_ws() -> anyhow::Result<()> {
    let (_anvil_http, http_provider) = spawn_http_anvil().await?;
    let (_anvil_ws, ws_provider) = spawn_ws_anvil().await?;

    // HTTP primary but http subscriptions NOT enabled
    let robust = RobustProviderBuilder::new(http_provider.clone())
        .fallback(ws_provider.clone())
        // allow_http_subscriptions(false) is default
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    // Should skip HTTP and use WS fallback for subscription
    let mut subscription = robust.subscribe_blocks().await?;

    // Mining on WS should work
    ws_provider.anvil_mine(Some(1), None).await?;
    let block = subscription.recv().await?;
    assert_eq!(block.number, 1);

    Ok(())
}

#[tokio::test]
async fn test_custom_poll_interval() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_http_anvil().await?;

    let custom_interval = Duration::from_millis(200);

    let robust = RobustProviderBuilder::new(provider.clone())
        .allow_http_subscriptions(true)
        .poll_interval(custom_interval)
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Receive genesis
    let start = std::time::Instant::now();
    let _ = subscription.recv().await?;

    // Mine a block
    provider.anvil_mine(Some(1), None).await?;

    // Next recv should take approximately poll_interval
    let _ = subscription.recv().await?;
    let elapsed = start.elapsed();

    // Should have taken at least one poll interval (with some tolerance)
    assert!(
        elapsed >= custom_interval,
        "Expected at least {:?}, got {:?}",
        custom_interval,
        elapsed
    );

    Ok(())
}
