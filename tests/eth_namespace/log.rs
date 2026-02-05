use crate::common::{setup_anvil, setup_anvil_with_contract};
use alloy::{
    primitives::U256,
    providers::Provider,
    rpc::types::{Filter, Log},
};
use tokio_stream::StreamExt;

// ============================================================================
// eth_getFilterLogs
// ============================================================================

#[tokio::test]
async fn test_get_filter_logs_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address());

    let filter_id = alloy_provider.new_filter(&filter).await?;

    let _ = counter.increase().send().await?.watch().await?;

    let robust_logs = robust.get_filter_logs(filter_id).await?;
    let alloy_logs = alloy_provider.get_filter_logs(filter_id).await?;

    assert_eq!(robust_logs.len(), alloy_logs.len());

    Ok(())
}

// ============================================================================
// eth_getFilterChanges
// ============================================================================

#[tokio::test]
async fn test_get_filter_changes_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;
    let filter = Filter::new().address(*counter.address()).event("CountIncreased(uint256)");

    let robust_filter_id = robust.new_filter(&filter).await?;
    let alloy_filter_id = alloy_provider.new_filter(&filter).await?;

    let robust_changes: Vec<Log> = robust.get_filter_changes(robust_filter_id).await?;
    let alloy_changes: Vec<Log> = alloy_provider.get_filter_changes(alloy_filter_id).await?;

    assert!(robust_changes.is_empty());
    assert!(alloy_changes.is_empty());

    let _ = counter.increase().send().await?.watch().await?;

    let robust_changes: Vec<Log> = robust.get_filter_changes(robust_filter_id).await?;
    let alloy_changes: Vec<Log> = alloy_provider.get_filter_changes(alloy_filter_id).await?;

    assert_eq!(robust_changes.len(), 1);
    assert_eq!(alloy_changes.len(), 1);

    Ok(())
}

// ============================================================================
// eth_newFilter
// ============================================================================

#[tokio::test]
async fn test_new_filter_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address());

    let robust_filter_id = robust.new_filter(&filter).await?;

    assert!(robust_filter_id > U256::ZERO);

    Ok(())
}

// ============================================================================
// eth_newBlockFilter
// ============================================================================

#[tokio::test]
async fn test_new_block_filter_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_filter_id = robust.new_block_filter().await?;
    let alloy_filter_id = alloy_provider.new_block_filter().await?;

    assert!(robust_filter_id > U256::ZERO);
    assert!(alloy_filter_id > U256::ZERO);

    Ok(())
}

// ============================================================================
// eth_uninstallFilter
// ============================================================================

#[tokio::test]
async fn test_uninstall_filter_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let filter = Filter::new();
    let robust_filter_id = robust.new_filter(&filter).await?;
    let alloy_filter_id = alloy_provider.new_filter(&filter).await?;

    let robust_result = robust.uninstall_filter(robust_filter_id).await?;
    let alloy_result = alloy_provider.uninstall_filter(alloy_filter_id).await?;

    assert!(robust_result);
    assert_eq!(robust_result, alloy_result);

    Ok(())
}

#[tokio::test]
async fn test_uninstall_filter_returns_false_for_invalid_id() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let invalid_filter_id = U256::from(999);

    let robust_result = robust.uninstall_filter(invalid_filter_id).await?;
    let alloy_result = alloy_provider.uninstall_filter(invalid_filter_id).await?;

    assert!(!robust_result);
    assert_eq!(robust_result, alloy_result);

    Ok(())
}

// ============================================================================
// watch_logs
// ============================================================================

#[tokio::test]
async fn test_watch_logs_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address());
    let poller = robust.watch_logs(&filter).await?;
    let mut stream = poller.into_stream();

    let _ = counter.increase().send().await?.watch().await?;

    let logs: Vec<Log> = stream.next().await.expect("should get logs");

    assert!(!logs.is_empty());
    assert_eq!(logs[0].address(), *counter.address());

    Ok(())
}

#[tokio::test]
async fn test_watch_logs_with_event_filter() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address()).event("CountIncreased(uint256)");

    let poller = robust.watch_logs(&filter).await?;
    let mut stream = poller.into_stream();

    let _ = counter.increase().send().await?.watch().await?;

    let logs: Vec<Log> = stream.next().await.expect("should get logs");

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].address(), *counter.address());

    Ok(())
}

#[tokio::test]
async fn test_watch_logs_multiple_events() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider, counter) = setup_anvil_with_contract().await?;

    let filter = Filter::new().address(*counter.address());
    let poller = robust.watch_logs(&filter).await?;
    let mut stream = poller.into_stream();

    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;

    let mut all_logs = Vec::new();
    for _ in 0..3 {
        if let Some(logs) = stream.next().await {
            all_logs.extend(logs);
            if all_logs.len() >= 3 {
                break;
            }
        }
    }

    assert!(all_logs.len() >= 3);

    Ok(())
}

#[tokio::test]
async fn test_watch_logs_empty_filter() -> anyhow::Result<()> {
    let (_anvil, robust, _alloy_provider) = setup_anvil().await?;

    let filter = Filter::new();
    let result = robust.watch_logs(&filter).await;

    assert!(result.is_ok());

    Ok(())
}
