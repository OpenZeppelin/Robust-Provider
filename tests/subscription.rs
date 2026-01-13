//! Tests for `RobustSubscription` and block subscription functionality.
//!
//! These tests cover WebSocket subscriptions, failover behavior,
//! reconnection logic, and stream handling.

mod common;

use std::time::{Duration, Instant};

use alloy::{
    network::Ethereum,
    providers::{ProviderBuilder, RootProvider, ext::AnvilApi},
};
use alloy_node_bindings::Anvil;
use common::{BUFFER_TIME, RECONNECT_INTERVAL, SHORT_TIMEOUT, spawn_ws_anvil};
use robust_provider::{
    DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY, RobustProviderBuilder, RobustSubscriptionStream,
    SubscriptionError,
};
use tokio::time::sleep;
use tokio_stream::StreamExt;

// ============================================================================
// Test Helpers
// ============================================================================

macro_rules! assert_next_block {
    ($stream: expr, $expected: expr) => {
        assert_next_block!($stream, $expected, timeout = 5)
    };
    ($stream: expr, $expected: expr, timeout = $secs: expr) => {
        let block = tokio::time::timeout(
            std::time::Duration::from_secs($secs),
            tokio_stream::StreamExt::next(&mut $stream),
        )
        .await
        .expect("timed out")
        .unwrap();
        let block = block.unwrap();
        assert_eq!(block.number, $expected);
    };
}

/// Waits for current provider to timeout, then mines on `next_provider` to trigger failover.
async fn trigger_failover_with_delay(
    stream: &mut RobustSubscriptionStream<Ethereum>,
    next_provider: RootProvider,
    expected_block: u64,
    extra_delay: Duration,
) -> anyhow::Result<()> {
    let task = tokio::spawn(async move {
        sleep(SHORT_TIMEOUT + extra_delay + BUFFER_TIME).await;
        next_provider.anvil_mine(Some(1), None).await.unwrap();
    });
    assert_next_block!(*stream, expected_block);
    task.await?;
    Ok(())
}

async fn trigger_failover(
    stream: &mut RobustSubscriptionStream<Ethereum>,
    next_provider: RootProvider,
    expected_block: u64,
) -> anyhow::Result<()> {
    trigger_failover_with_delay(stream, next_provider, expected_block, Duration::ZERO).await
}

// ============================================================================
// Basic Subscription Tests
// ============================================================================

#[tokio::test]
async fn test_successful_subscription_on_primary() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    // Subscription is created successfully - is_empty() returns true initially (no pending
    // messages)
    assert!(subscription.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_multiple_consecutive_recv_calls() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    for i in 1..=5 {
        provider.anvil_mine(Some(1), None).await?;
        let block = subscription.recv().await?;
        assert_eq!(block.number, i);
    }

    Ok(())
}

// ============================================================================
// Stream Tests
// ============================================================================

#[tokio::test]
async fn test_convert_subscription_to_stream() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;

    // Convert to stream
    let mut stream = subscription.into_stream();

    // Use the stream
    provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    Ok(())
}

#[tokio::test]
async fn test_stream_consuming_multiple_blocks() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    for i in 1..=5 {
        provider.anvil_mine(Some(1), None).await?;
        assert_next_block!(stream, i);
    }

    Ok(())
}

#[tokio::test]
async fn test_stream_consumes_multiple_blocks_in_sequence() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    provider.anvil_mine(Some(5), None).await?;
    assert_next_block!(stream, 1);
    assert_next_block!(stream, 2);
    assert_next_block!(stream, 3);
    assert_next_block!(stream, 4);
    assert_next_block!(stream, 5);

    Ok(())
}

#[tokio::test]
async fn test_stream_creation() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Stream should work normally
    provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    Ok(())
}

#[tokio::test]
async fn test_stream_continues_streaming_errors() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Get one block
    provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Trigger timeout error - the stream will continue to stream errors
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    // Without fallbacks, subsequent calls will continue to return errors
    // (not None, since only Error::Closed terminates the stream)
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

// ============================================================================
// Basic Failover Tests
// ============================================================================

#[tokio::test]
async fn robust_subscription_stream_with_failover() -> anyhow::Result<()> {
    let (_anvil_1, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Test: Primary works initially
    primary.anvil_mine(Some(1), None).await?;
    assert_eq!(subscription.recv().await?.number, 1);

    primary.anvil_mine(Some(1), None).await?;
    assert_eq!(subscription.recv().await?.number, 2);

    // After timeout, should failover to fallback provider
    let fb = fallback.clone();
    tokio::spawn(async move {
        sleep(SHORT_TIMEOUT + BUFFER_TIME).await;
        fb.anvil_mine(Some(1), None).await.unwrap();
    });
    assert_eq!(subscription.recv().await?.number, 1);

    // PP is not used after failover
    primary.anvil_mine(Some(1), None).await?;
    fallback.anvil_mine(Some(1), None).await?;

    // From fallback, not primary's block 3
    assert_eq!(subscription.recv().await?.number, 2);

    Ok(())
}

#[tokio::test]
async fn subscription_fails_with_no_fallbacks() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // No fallback available - should error after timeout
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

#[tokio::test]
async fn ws_fails_http_fallback_returns_primary_error() -> anyhow::Result<()> {
    // Setup: Create WS primary and HTTP fallback
    let anvil_1 = Anvil::new().try_spawn()?;
    let ws_provider = ProviderBuilder::new().connect(anvil_1.ws_endpoint_url().as_str()).await?;

    let anvil_2 = Anvil::new().try_spawn()?;
    let http_provider = ProviderBuilder::new().connect_http(anvil_2.endpoint_url());

    let robust = RobustProviderBuilder::fragile(ws_provider.clone())
        .fallback(http_provider.clone())
        .subscription_timeout(Duration::from_secs(1))
        .build()
        .await?;

    // Test: Verify subscription works on primary
    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    ws_provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    ws_provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    // Verify: HTTP fallback can't provide subscription, so we get an error
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

// ============================================================================
// Fallback Cycling Tests
// ============================================================================

#[tokio::test]
async fn test_single_fallback_provider() -> anyhow::Result<()> {
    let (anvil_pp, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .call_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Kill primary so reconnect attempts fail
    drop(anvil_pp);

    // PP -> FB
    trigger_failover(&mut stream, fallback.clone(), 1).await?;

    // FB -> try PP (fails) -> no more fallbacks -> error
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

#[tokio::test]
async fn subscription_cycles_through_multiple_fallbacks() -> anyhow::Result<()> {
    let (anvil_pp, primary) = spawn_ws_anvil().await?;
    let (_anvil_1, fb_1) = spawn_ws_anvil().await?;
    let (_anvil_2, fb_2) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fb_1.clone())
        .fallback(fb_2.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .call_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Kill primary - all future PP reconnection attempts will fail
    drop(anvil_pp);

    // PP times out -> FP1
    trigger_failover(&mut stream, fb_1.clone(), 1).await?;

    // FP1 times out -> tries PP (fails, takes call_timeout) -> FP2
    trigger_failover_with_delay(&mut stream, fb_2.clone(), 1, SHORT_TIMEOUT).await?;

    fb_2.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    // FP2 times out -> tries PP (fails) -> no more fallbacks -> error
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

#[tokio::test]
async fn test_many_fallback_providers() -> anyhow::Result<()> {
    let (anvil_pp, primary) = spawn_ws_anvil().await?;
    let (_anvil_1, fb_1) = spawn_ws_anvil().await?;
    let (_anvil_2, fb_2) = spawn_ws_anvil().await?;
    let (_anvil_3, fb_3) = spawn_ws_anvil().await?;
    let (_anvil_4, fb_4) = spawn_ws_anvil().await?;
    let (_anvil_5, fb_5) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fb_1.clone())
        .fallback(fb_2.clone())
        .fallback(fb_3.clone())
        .fallback(fb_4.clone())
        .fallback(fb_5.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .call_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Kill primary
    drop(anvil_pp);

    // Cycle through all fallbacks
    trigger_failover(&mut stream, fb_1.clone(), 1).await?;
    trigger_failover_with_delay(&mut stream, fb_2.clone(), 1, SHORT_TIMEOUT).await?;
    trigger_failover_with_delay(&mut stream, fb_3.clone(), 1, SHORT_TIMEOUT).await?;
    trigger_failover_with_delay(&mut stream, fb_4.clone(), 1, SHORT_TIMEOUT).await?;
    trigger_failover_with_delay(&mut stream, fb_5.clone(), 1, SHORT_TIMEOUT).await?;

    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

// ============================================================================
// Reconnection Tests
// ============================================================================

#[tokio::test]
async fn subscription_reconnects_to_primary() -> anyhow::Result<()> {
    let (_anvil_1, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(RECONNECT_INTERVAL)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // PP times out -> FP1
    trigger_failover(&mut stream, fallback.clone(), 1).await?;

    fallback.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    // FP1 times out -> PP (reconnect succeeds)
    trigger_failover(&mut stream, primary.clone(), 2).await?;

    // PP times out -> FP1 (fallback index was reset)
    trigger_failover(&mut stream, fallback.clone(), 3).await?;

    fallback.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 4);

    Ok(())
}

#[tokio::test]
async fn subscription_periodically_reconnects_to_primary_while_on_fallback() -> anyhow::Result<()> {
    // Use a longer reconnect interval to make timing more predictable
    let reconnect_interval = Duration::from_millis(800);

    let (_anvil_1, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(reconnect_interval)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // PP times out -> FP (this sets last_reconnect_attempt)
    trigger_failover(&mut stream, fallback.clone(), 1).await?;
    let failover_time = Instant::now();

    // Now on fallback - mine blocks before reconnect_interval elapses
    // These should stay on fallback (no reconnect attempt)
    fallback.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    fallback.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 3);

    // Ensure reconnect_interval has fully elapsed since failover
    let elapsed = failover_time.elapsed();
    if elapsed < reconnect_interval + BUFFER_TIME {
        sleep(reconnect_interval + BUFFER_TIME - elapsed).await;
    }

    // Mine on fallback - receiving this block triggers try_reconnect_to_primary
    fallback.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 4);

    // Now we should be back on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 3);

    Ok(())
}

#[tokio::test]
async fn test_reconnection_skipped_before_interval_elapsed() -> anyhow::Result<()> {
    let (_anvil_1, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fallback) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fallback.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(Duration::from_secs(10)) // Long interval
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    // Failover to fallback
    trigger_failover(&mut stream, fallback.clone(), 1).await?;

    // Immediately try another recv - should stay on fallback (no reconnect attempt)
    fallback.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    Ok(())
}

#[tokio::test]
async fn test_successful_reconnection_resets_state() -> anyhow::Result<()> {
    let (_anvil_1, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fb_1) = spawn_ws_anvil().await?;
    let (_anvil_3, fb_2) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fb_1.clone())
        .fallback(fb_2.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(RECONNECT_INTERVAL)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Failover to fallback
    trigger_failover(&mut stream, fb_1.clone(), 1).await?;

    fb_1.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    // Wait for reconnect interval, then timeout - reconnect to primary
    sleep(RECONNECT_INTERVAL).await;
    trigger_failover(&mut stream, primary.clone(), 2).await?;

    // After reconnection, next failover should go to fallback[0] again (not fallback[1])
    trigger_failover(&mut stream, fb_1.clone(), 3).await?;

    Ok(())
}

#[tokio::test]
async fn test_multiple_failed_reconnection_attempts() -> anyhow::Result<()> {
    let (anvil_pp, primary) = spawn_ws_anvil().await?;
    let (_anvil_1, fb_1) = spawn_ws_anvil().await?;
    let (_anvil_2, fb_2) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fb_1.clone())
        .fallback(fb_2.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(RECONNECT_INTERVAL)
        .call_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Kill primary
    drop(anvil_pp);

    // Failover to fb_1 (primary is dead)
    trigger_failover(&mut stream, fb_1.clone(), 1).await?;

    // Stay on fb_1 for a bit
    fb_1.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    // Wait for reconnect interval, then timeout - try primary (fails), go to fb_2
    sleep(RECONNECT_INTERVAL).await;
    trigger_failover_with_delay(&mut stream, fb_2.clone(), 1, SHORT_TIMEOUT).await?;

    // fb_2 continues to work
    fb_2.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 2);

    Ok(())
}

#[tokio::test]
async fn test_primary_reconnect_attempt_before_next_fallback() -> anyhow::Result<()> {
    let (_anvil_1, primary) = spawn_ws_anvil().await?;
    let (_anvil_2, fb_1) = spawn_ws_anvil().await?;
    let (_anvil_3, fb_2) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(primary.clone())
        .fallback(fb_1.clone())
        .fallback(fb_2.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .reconnect_interval(RECONNECT_INTERVAL)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Start on primary
    primary.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // PP -> FB1
    trigger_failover(&mut stream, fb_1.clone(), 1).await?;

    // FB1 -> PP (reconnect succeeds, not FB2)
    trigger_failover(&mut stream, primary.clone(), 2).await?;

    Ok(())
}

// ============================================================================
// Error Propagation Tests
// ============================================================================

#[tokio::test]
async fn test_backend_gone_error_propagation() -> anyhow::Result<()> {
    let (anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Get one block
    provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Kill the provider
    drop(anvil);

    // Should get BackendGone or Timeout error
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

#[tokio::test]
async fn test_immediate_consecutive_failures() -> anyhow::Result<()> {
    let (anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(SHORT_TIMEOUT)
        .build()
        .await?;

    let subscription = robust.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    // Get one block
    provider.anvil_mine(Some(1), None).await?;
    assert_next_block!(stream, 1);

    // Kill provider immediately
    drop(anvil);

    // First failure
    assert!(matches!(stream.next().await.unwrap(), Err(SubscriptionError::Timeout)));

    Ok(())
}

#[tokio::test]
async fn test_subscription_lagged_error() -> anyhow::Result<()> {
    let (_anvil, provider) = spawn_ws_anvil().await?;

    let robust = RobustProviderBuilder::fragile(provider.clone())
        .subscription_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let mut subscription = robust.subscribe_blocks().await?;

    // Mine more blocks than channel can hold without consuming
    provider.anvil_mine(Some(DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY as u64 + 1), None).await?;

    // Allow time for block notifications to propagate through WebSocket
    // and fill the subscription channel
    sleep(BUFFER_TIME).await;

    // First recv should return Lagged error (skipped some blocks)
    let result = subscription.recv().await;
    assert!(matches!(result, Err(SubscriptionError::Lagged(_))));

    Ok(())
}

// ============================================================================
// Pubsub Support Tests
// ============================================================================

#[tokio::test]
async fn test_subscribe_fails_when_all_providers_lack_pubsub() -> anyhow::Result<()> {
    let anvil = Anvil::new().try_spawn()?;

    let http_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

    let robust = RobustProviderBuilder::new(http_provider.clone())
        .fallback(http_provider)
        .call_timeout(Duration::from_secs(5))
        .min_delay(Duration::from_millis(100))
        .build()
        .await?;

    let result = robust.subscribe_blocks().await.unwrap_err();

    match result {
        robust_provider::Error::RpcError(e) => {
            assert!(matches!(
                e.as_ref(),
                alloy::transports::RpcError::Transport(
                    alloy::transports::TransportErrorKind::PubsubUnavailable
                )
            ));
        }
        other => panic!("Expected PubsubUnavailable error type, got: {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn test_subscribe_succeeds_if_primary_provider_lacks_pubsub_but_fallback_supports_it()
-> anyhow::Result<()> {
    let anvil = Anvil::new().try_spawn()?;

    let http_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
    let ws_provider = ProviderBuilder::new()
        .connect_ws(alloy::providers::WsConnect::new(anvil.ws_endpoint_url().as_str()))
        .await?;

    let robust = RobustProviderBuilder::fragile(http_provider)
        .fallback(ws_provider)
        .call_timeout(Duration::from_secs(5))
        .build()
        .await?;

    let result = robust.subscribe_blocks().await;
    assert!(result.is_ok());

    Ok(())
}
