mod common;

use std::time::Duration;

use alloy::{
    eips::BlockNumberOrTag,
    network::Ethereum,
    node_bindings::Anvil,
    providers::{Provider, ProviderBuilder, ext::AnvilApi},
};
use common::{BUFFER_TIME, SHORT_TIMEOUT};
use robust_provider::{Error, RobustProviderBuilder};

use crate::common::safe_drop_anvil;

macro_rules! assert_next {
    ($stream: expr, $expected: expr) => {
        let block_hashes = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio_stream::StreamExt::next(&mut $stream),
        )
        .await
        .expect("timed out")
        .expect("stream ended");
        assert_eq!(block_hashes, $expected);
    };
}

#[tokio::test]
async fn watch_blocks_returns_hashes_on_primary() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = common::setup_anvil().await?;

    let poller = robust.watch_blocks().await?;
    let mut stream = poller.with_poll_interval(Duration::from_millis(50)).into_stream();

    alloy_provider.anvil_mine(Some(1), None).await?;

    let block = alloy_provider.get_block_by_number(BlockNumberOrTag::Number(1)).await?.unwrap();

    assert_next!(stream, vec![block.header.hash]);

    Ok(())
}

#[tokio::test]
async fn watch_blocks_fails_over_when_primary_is_down() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let primary_provider = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());

    let anvil_fallback = Anvil::new().try_spawn()?;
    let fallback_provider = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    let robust = RobustProviderBuilder::<Ethereum, _>::fragile(primary_provider)
        .fallback(fallback_provider.clone())
        .call_timeout(SHORT_TIMEOUT)
        .min_delay(Duration::ZERO)
        .build()
        .await?;

    safe_drop_anvil(anvil_primary);

    let poller = robust.watch_blocks().await?;
    let mut stream = poller.with_poll_interval(Duration::from_millis(50)).into_stream();

    fallback_provider.anvil_mine(Some(1), None).await?;
    tokio::time::sleep(BUFFER_TIME).await;

    let block = fallback_provider.get_block_by_number(BlockNumberOrTag::Number(1)).await?.unwrap();

    assert_next!(stream, vec![block.header.hash]);

    Ok(())
}

#[tokio::test]
async fn watch_blocks_errors_when_all_providers_fail() -> anyhow::Result<()> {
    let anvil_primary = Anvil::new().try_spawn()?;
    let primary_provider = ProviderBuilder::new().connect_http(anvil_primary.endpoint_url());

    let anvil_fallback = Anvil::new().try_spawn()?;
    let fallback_provider = ProviderBuilder::new().connect_http(anvil_fallback.endpoint_url());

    let robust = RobustProviderBuilder::<Ethereum, _>::fragile(primary_provider)
        .fallback(fallback_provider)
        .call_timeout(SHORT_TIMEOUT)
        .min_delay(Duration::ZERO)
        .build()
        .await?;

    safe_drop_anvil(anvil_primary);
    safe_drop_anvil(anvil_fallback);

    let err = robust.watch_blocks().await.unwrap_err();
    assert!(matches!(err, Error::RpcError(_) | Error::Timeout));

    Ok(())
}
