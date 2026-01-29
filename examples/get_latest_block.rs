use alloy::network::Ethereum;
use robust_provider::RobustProviderBuilder;

const RPC_URL: &str = "https://ethereum-rpc.publicnode.com";

#[tokio::main]
async fn main() {
    let provider = RobustProviderBuilder::<Ethereum, &str>::new(RPC_URL)
        .build()
        .await
        .expect("Should to build provider");
    let latest_block = provider.get_block_number().await.expect("should fetch latest block number");
    println!("Latest block: {}", latest_block);
}
