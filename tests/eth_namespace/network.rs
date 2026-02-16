use crate::common::setup_anvil;
use alloy::{
    primitives::{U64, U256},
    providers::Provider,
};
use serde_json::value::RawValue;
use std::borrow::Cow;

// ============================================================================
// net_version
// ============================================================================

#[tokio::test]
async fn test_get_net_version_succeeds() -> anyhow::Result<()> {
    let (anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_net_version = robust.get_net_version().await?;
    let alloy_net_version = alloy_provider.get_net_version().await?;

    assert_eq!(robust_net_version, alloy_net_version);
    assert_eq!(robust_net_version, anvil.chain_id());

    Ok(())
}

// ============================================================================
// raw_request
// ============================================================================

#[tokio::test]
async fn test_raw_request_succeeds() -> anyhow::Result<()> {
    let (anvil, robust, _alloy_provider) = setup_anvil().await?;

    let robust_chain_id: U64 = robust.raw_request(Cow::Borrowed("eth_chainId"), ()).await?;

    assert_eq!(robust_chain_id.to::<u64>(), anvil.chain_id());

    Ok(())
}

#[tokio::test]
async fn test_raw_request_with_params() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let address = accounts[0];

    let robust_balance: U256 =
        robust.raw_request(Cow::Borrowed("eth_getBalance"), (address, "latest")).await?;
    let alloy_balance = alloy_provider.get_balance(address).await?;

    assert_eq!(robust_balance, alloy_balance);

    Ok(())
}

// ============================================================================
// raw_request_dyn
// ============================================================================

#[tokio::test]
async fn test_raw_request_dyn_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let params = RawValue::from_string("[]".to_string())?;
    let result = robust.raw_request_dyn(Cow::Borrowed("eth_blockNumber"), &params).await?;

    let block_number: U64 = serde_json::from_str(result.get())?;
    let alloy_block_number = alloy_provider.get_block_number().await?;

    assert_eq!(block_number.to::<u64>(), alloy_block_number);

    Ok(())
}

#[tokio::test]
async fn test_raw_request_dyn_with_params() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let address = accounts[0];

    let params_json = format!(r#"["{address:?}", "latest"]"#);
    let params = RawValue::from_string(params_json)?;
    let result = robust.raw_request_dyn(Cow::Borrowed("eth_getBalance"), &params).await?;

    let balance: U256 = serde_json::from_str(result.get())?;
    let alloy_balance = alloy_provider.get_balance(address).await?;

    assert_eq!(balance, alloy_balance);

    Ok(())
}

#[tokio::test]
async fn test_get_sha3_empty_data() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let data = b"";

    let robust_hash = robust.get_sha3(data).await?;
    let alloy_hash = alloy_provider.get_sha3(data).await?;

    assert_eq!(robust_hash, alloy_hash);

    Ok(())
}

#[tokio::test]
async fn test_get_sha3_with_various_inputs() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let test_cases = vec![
        b"test".as_slice(),
        b"0x1234".as_slice(),
        &[0u8, 1, 2, 3, 4, 5],
        b"The quick brown fox jumps over the lazy dog".as_slice(),
    ];

    for data in test_cases {
        let robust_hash = robust.get_sha3(data).await?;
        let alloy_hash = alloy_provider.get_sha3(data).await?;

        assert_eq!(robust_hash, alloy_hash);
    }

    Ok(())
}
