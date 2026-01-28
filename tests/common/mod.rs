//! Common test utilities and helpers for integration tests.

#![allow(dead_code)]
#![allow(clippy::missing_errors_doc)]

use std::time::Duration;

use alloy::{
    network::Ethereum,
    providers::{Provider, ProviderBuilder, RootProvider, ext::AnvilApi},
};
use alloy_node_bindings::{Anvil, AnvilInstance};
use robust_provider::{RobustProvider, RobustProviderBuilder};

// Shared test contract used across integration tests
alloy::sol! {
    // Built directly with solc 0.8.30+commit.73712a01.Darwin.appleclang
    #[sol(rpc, bytecode="608080604052346015576101b0908161001a8239f35b5f80fdfe6080806040526004361015610012575f80fd5b5f3560e01c90816306661abd1461016157508063a87d942c14610145578063d732d955146100ad5763e8927fbc14610048575f80fd5b346100a9575f3660031901126100a9575f5460018101809111610095576020817f7ca2ca9527391044455246730762df008a6b47bbdb5d37a890ef78394535c040925f55604051908152a1005b634e487b7160e01b5f52601160045260245ffd5b5f80fd5b346100a9575f3660031901126100a9575f548015610100575f198101908111610095576020817f53a71f16f53e57416424d0d18ccbd98504d42a6f98fe47b09772d8f357c620ce925f55604051908152a1005b60405162461bcd60e51b815260206004820152601860248201527f436f756e742063616e6e6f74206265206e6567617469766500000000000000006044820152606490fd5b346100a9575f3660031901126100a95760205f54604051908152f35b346100a9575f3660031901126100a9576020905f548152f3fea2646970667358221220471585b420a1ad0093820ff10129ec863f6df4bec186546249391fbc3cdbaa7c64736f6c634300081e0033")]
    contract TestCounter {
        uint256 public count;

        #[derive(Debug)]
        event CountIncreased(uint256 newCount);
        #[derive(Debug)]
        event CountDecreased(uint256 newCount);

        function increase() public {
            count += 1;
            emit CountIncreased(count);
        }

        function decrease() public {
            require(count > 0, "Count cannot be negative");
            count -= 1;
            emit CountDecreased(count);
        }

        function getCount() public view returns (uint256) {
            return count;
        }
    }
}

pub async fn deploy_counter<P>(provider: P) -> anyhow::Result<TestCounter::TestCounterInstance<P>>
where
    P: Provider<Ethereum>,
{
    let contract = TestCounter::deploy(provider).await?;
    Ok(contract)
}

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

/// Setup an Anvil instance with a deployed TestCounter contract.
/// Returns the contract address and a provider that can send transactions.
pub async fn setup_anvil_with_contract() -> anyhow::Result<(
    AnvilInstance,
    RobustProvider,
    impl Provider<Ethereum>,
    TestCounter::TestCounterInstance<impl Provider>,
)> {
    let anvil = Anvil::new().try_spawn()?;
    let wallet = anvil.wallet().expect("anvil should have wallet");
    let alloy_provider = ProviderBuilder::new().wallet(wallet).connect_http(anvil.endpoint_url());

    let counter = deploy_counter(alloy_provider.clone()).await?;
    let contract_address = *counter.address();

    // Create contract instance to interact with it
    let counter = TestCounter::new(contract_address, alloy_provider.clone());

    let robust = RobustProviderBuilder::new(alloy_provider.clone())
        .call_timeout(Duration::from_secs(5))
        .build()
        .await?;

    Ok((anvil, robust, alloy_provider, counter))
}
