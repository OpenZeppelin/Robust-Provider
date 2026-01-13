//! Common test utilities and helpers for integration tests.

#![allow(dead_code)]
#![allow(clippy::missing_errors_doc)]

use std::time::Duration;

use alloy::providers::{Provider, ProviderBuilder, RootProvider, ext::AnvilApi};
use alloy_node_bindings::{Anvil, AnvilInstance};
use robust_provider::{RobustProvider, RobustProviderBuilder};

/// Short timeout for tests.
pub const SHORT_TIMEOUT: Duration = Duration::from_millis(300);

/// Reconnect interval for tests.
pub const RECONNECT_INTERVAL: Duration = Duration::from_millis(500);

/// Buffer time for async operations.
pub const BUFFER_TIME: Duration = Duration::from_millis(100);

// Setup a basic Anvil instance with a `RobustProvider`.
pub async fn setup_anvil() -> anyhow::Result<(AnvilInstance, RobustProvider, impl Provider)> {
    let anvil = Anvil::new().try_spawn()?;
    let alloy_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

    let robust = RobustProviderBuilder::new(alloy_provider.clone())
        .call_timeout(Duration::from_secs(5))
        .build()
        .await?;

    Ok((anvil, robust, alloy_provider))
}

/// Setup an Anvil instance with pre-mined blocks.
pub async fn setup_anvil_with_blocks(
    num_blocks: u64,
) -> anyhow::Result<(AnvilInstance, RobustProvider, impl Provider)> {
    let (anvil, robust, alloy_provider) = setup_anvil().await?;
    alloy_provider.anvil_mine(Some(num_blocks), None).await?;
    Ok((anvil, robust, alloy_provider))
}

/// Spawn a WebSocket-enabled Anvil instance.
pub async fn spawn_ws_anvil() -> anyhow::Result<(AnvilInstance, RootProvider)> {
    let anvil = Anvil::new().try_spawn()?;
    let provider = ProviderBuilder::new().connect(anvil.ws_endpoint_url().as_str()).await?;
    Ok((anvil, provider.root().to_owned()))
}
