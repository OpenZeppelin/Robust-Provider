//! Integration tests for `RobustProvider` HTTP endpoints against Kurtosis devnet.
//!
//! These tests verify that the RobustProvider methods work correctly against
//! real Ethereum execution clients (geth, nethermind, besu, reth) running in Kurtosis.
//!
//! Prerequisites:
//! ```bash
//! ./scripts/setup-kurtosis.sh local-eth-testnet
//! ```

#![cfg(feature = "integration")]

use std::time::Duration;

use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    primitives::BlockHash,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    transports::http::reqwest::Url,
};
use anyhow::Context;
use robust_provider::{Error, RobustProviderBuilder};

use crate::common::setup_kurtosis::{ElEndpoint, load_el_endpoints};

/// Adds client context to errors for better debugging in parameterized tests.
macro_rules! ctx {
    ($expr:expr, $client:expr) => {
        $expr.await.with_context(|| format!("client: {}", $client))
    };
}

/// Helper to create a RobustProvider from an endpoint
async fn setup_robust_provider(
    endpoint: &ElEndpoint,
) -> anyhow::Result<(robust_provider::RobustProvider, impl Provider)> {
    let alloy_provider = ProviderBuilder::new().connect_http(Url::parse(&endpoint.http)?);

    let robust = RobustProviderBuilder::new(alloy_provider.clone())
        .call_timeout(Duration::from_secs(30))
        .build()
        .await?;

    Ok((robust, alloy_provider))
}

// ============================================================================
// eth_blockNumber
// ============================================================================

#[tokio::test]
async fn test_get_block_number_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        let robust_block_num = ctx!(robust.get_block_number(), &endpoint.client)?;
        let alloy_block_num = ctx!(alloy_provider.get_block_number(), &endpoint.client)?;

        assert_eq!(
            robust_block_num, alloy_block_num,
            "Block number mismatch for client: {}",
            endpoint.client
        );
        assert!(robust_block_num >= 1, "Expected at least block 1 for client: {}", endpoint.client);
    }

    Ok(())
}

// ============================================================================
// eth_getBlockByNumber
// ============================================================================

#[tokio::test]
async fn test_get_block_by_number_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        let tags = [
            BlockNumberOrTag::Number(0),
            BlockNumberOrTag::Latest,
            BlockNumberOrTag::Earliest,
            BlockNumberOrTag::Safe,
            BlockNumberOrTag::Finalized,
        ];

        for tag in tags {
            let robust_block = ctx!(robust.get_block_by_number(tag), &endpoint.client)?;
            let alloy_block = ctx!(alloy_provider.get_block_by_number(tag), &endpoint.client)?
                .expect("block should exist");

            assert_eq!(
                robust_block.header.number, alloy_block.header.number,
                "Block number mismatch for client: {}, tag: {:?}",
                endpoint.client, tag
            );
            assert_eq!(
                robust_block.header.hash, alloy_block.header.hash,
                "Block hash mismatch for client: {}, tag: {:?}",
                endpoint.client, tag
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_get_block_by_number_future_block_fails() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, _) = setup_robust_provider(&endpoint).await?;

        let future_block = 999_999_999;
        let result = robust.get_block_by_number(BlockNumberOrTag::Number(future_block)).await;

        assert!(
            matches!(result, Err(Error::BlockNotFound(_))),
            "Expected BlockNotFound for client: {}, got: {:?}",
            endpoint.client,
            result
        );
    }

    Ok(())
}

// ============================================================================
// eth_getBlockByHash
// ============================================================================

#[tokio::test]
async fn test_get_block_by_hash_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        // Get genesis block hash
        let genesis =
            ctx!(alloy_provider.get_block_by_number(BlockNumberOrTag::Earliest), &endpoint.client)?
                .expect("genesis should exist");
        let genesis_hash = genesis.header.hash;

        let robust_block = ctx!(robust.get_block_by_hash(genesis_hash), &endpoint.client)?;

        assert_eq!(
            robust_block.header.number, 0,
            "Genesis block number should be 0 for client: {}",
            endpoint.client
        );
        assert_eq!(
            robust_block.header.hash, genesis_hash,
            "Genesis block hash mismatch for client: {}",
            endpoint.client
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_get_block_by_hash_fails() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, _) = setup_robust_provider(&endpoint).await?;

        let result = robust.get_block_by_hash(BlockHash::ZERO).await;

        assert!(
            matches!(result, Err(Error::BlockNotFound(_))),
            "Expected BlockNotFound for client: {}, got: {:?}",
            endpoint.client,
            result
        );
    }

    Ok(())
}

// ============================================================================
// eth_getBlock (by BlockId)
// ============================================================================

#[tokio::test]
async fn test_get_block_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        let block_ids = [
            BlockId::number(0),
            BlockId::latest(),
            BlockId::earliest(),
            BlockId::safe(),
            BlockId::finalized(),
        ];

        for block_id in block_ids {
            let robust_block = ctx!(robust.get_block(block_id), &endpoint.client)?;
            let alloy_block = ctx!(alloy_provider.get_block(block_id), &endpoint.client)?
                .expect("block should exist");

            assert_eq!(
                robust_block.header.number, alloy_block.header.number,
                "Block number mismatch for client: {}, block_id: {:?}",
                endpoint.client, block_id
            );
            assert_eq!(
                robust_block.header.hash, alloy_block.header.hash,
                "Block hash mismatch for client: {}, block_id: {:?}",
                endpoint.client, block_id
            );
        }

        // Test with block hash
        let genesis =
            ctx!(alloy_provider.get_block_by_number(BlockNumberOrTag::Earliest), &endpoint.client)?
                .expect("genesis should exist");
        let block_id = BlockId::hash(genesis.header.hash);
        let robust_block = ctx!(robust.get_block(block_id), &endpoint.client)?;

        assert_eq!(
            robust_block.header.hash, genesis.header.hash,
            "Block hash mismatch for client: {}",
            endpoint.client
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_get_block_fails() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, _) = setup_robust_provider(&endpoint).await?;

        // Future block number
        let result = robust.get_block(BlockId::number(999_999_999)).await;
        assert!(
            matches!(result, Err(Error::BlockNotFound(_))),
            "Expected BlockNotFound for future block, client: {}, got: {:?}",
            endpoint.client,
            result
        );

        // Non-existent hash
        let result = robust.get_block(BlockId::hash(BlockHash::ZERO)).await;
        assert!(
            matches!(result, Err(Error::BlockNotFound(_))),
            "Expected BlockNotFound for zero hash, client: {}, got: {:?}",
            endpoint.client,
            result
        );
    }

    Ok(())
}

// ============================================================================
// get_block_number_by_id (custom helper)
// ============================================================================

#[tokio::test]
async fn test_get_block_number_by_id_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        // By number
        let block_num = ctx!(robust.get_block_number_by_id(BlockId::number(0)), &endpoint.client)?;
        assert_eq!(block_num, 0, "Block number should be 0 for client: {}", endpoint.client);

        // By hash
        let genesis =
            ctx!(alloy_provider.get_block_by_number(BlockNumberOrTag::Earliest), &endpoint.client)?
                .expect("genesis should exist");
        let block_num = ctx!(
            robust.get_block_number_by_id(BlockId::hash(genesis.header.hash)),
            &endpoint.client
        )?;
        assert_eq!(
            block_num, 0,
            "Genesis block number should be 0 for client: {}",
            endpoint.client
        );

        // Latest
        let robust_latest =
            ctx!(robust.get_block_number_by_id(BlockId::latest()), &endpoint.client)?;
        let alloy_latest = ctx!(alloy_provider.get_block_number(), &endpoint.client)?;
        assert_eq!(
            robust_latest, alloy_latest,
            "Latest block number mismatch for client: {}",
            endpoint.client
        );

        // Earliest
        let block_num = ctx!(robust.get_block_number_by_id(BlockId::earliest()), &endpoint.client)?;
        assert_eq!(
            block_num, 0,
            "Earliest block number should be 0 for client: {}",
            endpoint.client
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_get_block_number_by_id_fails() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, _) = setup_robust_provider(&endpoint).await?;

        let result = robust.get_block_number_by_id(BlockId::hash(BlockHash::ZERO)).await;

        assert!(
            matches!(result, Err(Error::BlockNotFound(_))),
            "Expected BlockNotFound for client: {}, got: {:?}",
            endpoint.client,
            result
        );
    }

    Ok(())
}

// ============================================================================
// get_latest_confirmed
// ============================================================================

#[tokio::test]
async fn test_get_latest_confirmed_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        let latest = ctx!(alloy_provider.get_block_number(), &endpoint.client)?;

        // Zero confirmations returns latest
        let confirmed = ctx!(robust.get_latest_confirmed(0), &endpoint.client)?;
        assert_eq!(
            confirmed, latest,
            "Zero confirmations should return latest for client: {}",
            endpoint.client
        );

        // With confirmations
        if latest >= 10 {
            let confirmed = ctx!(robust.get_latest_confirmed(10), &endpoint.client)?;
            assert_eq!(
                confirmed,
                latest - 10,
                "Confirmed block mismatch for client: {}",
                endpoint.client
            );
        }

        // Confirmations exceeding latest should saturate at 0
        let confirmed = ctx!(robust.get_latest_confirmed(latest + 100), &endpoint.client)?;
        assert_eq!(confirmed, 0, "Should saturate at 0 for client: {}", endpoint.client);
    }

    Ok(())
}

// ============================================================================
// get_logs
// ============================================================================

#[tokio::test]
async fn test_get_logs_succeeds() -> anyhow::Result<()> {
    let endpoints = load_el_endpoints()?;

    for endpoint in endpoints {
        let (robust, alloy_provider) = setup_robust_provider(&endpoint).await?;

        // Query logs for the first few blocks (may be empty, but should not error)
        let filter = Filter::new().from_block(0).to_block(100);

        let robust_logs = ctx!(robust.get_logs(&filter), &endpoint.client)?;
        let alloy_logs = ctx!(alloy_provider.get_logs(&filter), &endpoint.client)?;

        assert_eq!(
            robust_logs.len(),
            alloy_logs.len(),
            "Logs count mismatch for client: {}",
            endpoint.client
        );
    }

    Ok(())
}
