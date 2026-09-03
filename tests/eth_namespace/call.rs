use crate::common::{setup_anvil, setup_anvil_with_contract};
use alloy::{
    network::TransactionBuilder,
    primitives::U256,
    providers::Provider,
    rpc::types::{
        Bundle, TransactionRequest,
        simulate::{SimBlock, SimulatePayload},
    },
};

// ============================================================================
// eth_call
// ============================================================================

#[tokio::test]
async fn test_call_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    counter.increase().send().await?.watch().await?;
    counter.increase().send().await?.watch().await?;
    counter.increase().send().await?.watch().await?;

    let get_count_call = counter.getCount();
    let tx = TransactionRequest::default()
        .with_to(*counter.address())
        .with_input(get_count_call.calldata().clone());

    let robust_result = robust.call(tx.clone()).await?;
    let alloy_result = alloy_provider.call(tx).await?;

    let count = U256::from_be_slice(&robust_result);
    assert_eq!(count, 3);

    assert_eq!(robust_result, alloy_result);

    Ok(())
}

// ============================================================================
// eth_callMany
// ============================================================================

#[tokio::test]
#[ignore = "Anvil does not support 'call many' therefore this fails with timeout/method not found"]
async fn test_call_many_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    counter.increase().send().await?.watch().await?;
    counter.increase().send().await?.watch().await?;

    let get_count_call = counter.getCount();
    let tx = TransactionRequest::default()
        .with_to(*counter.address())
        .with_input(get_count_call.calldata().clone());

    let bundle = Bundle { transactions: vec![tx], block_override: None };
    let bundles = vec![bundle];

    let robust_result = robust.call_many(&bundles).await?;
    let alloy_result = alloy_provider.call_many(&bundles).await?;

    assert_eq!(robust_result, alloy_result);
    assert!(!robust_result.is_empty());

    Ok(())
}

// ============================================================================
// eth_chainId
// ============================================================================

#[tokio::test]
async fn test_get_chain_id_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_chain_id = robust.get_chain_id().await?;
    let alloy_chain_id = alloy_provider.get_chain_id().await?;

    assert_eq!(robust_chain_id, alloy_chain_id);
    // Anvil default chain ID is 31337
    assert_eq!(robust_chain_id, 31337);

    Ok(())
}

// ============================================================================
// eth_createAccessList
// ============================================================================

#[tokio::test]
async fn test_create_access_list() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let tx = TransactionRequest::default();

    let robust_access_list = robust.create_access_list(&tx).await?;
    let alloy_access_list = alloy_provider.create_access_list(&tx).await?;

    assert_eq!(robust_access_list, alloy_access_list);

    Ok(())
}

// ============================================================================
// eth_estimate_eip1559Fees
// ============================================================================

#[tokio::test]
async fn test_estimate_gas_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let increase_call = counter.increase();
    let tx = TransactionRequest::default()
        .with_to(*counter.address())
        .with_input(increase_call.calldata().clone());

    let robust_gas = robust.estimate_gas(tx.clone()).await?;
    let alloy_gas = alloy_provider.estimate_gas(tx).await?;

    assert_eq!(robust_gas, alloy_gas);

    Ok(())
}

// ============================================================================
// eth_simulateV1
// ============================================================================

#[tokio::test]
async fn test_simulate_succeeds() -> anyhow::Result<()> {
    let (anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    // Anvil@1.8.1 sets `validation: true`, which requires a funded sender and fees
    // that cover the base fee.
    let fees = alloy_provider.estimate_eip1559_fees().await?;
    let get_count_call = counter.getCount();
    let tx = TransactionRequest::default()
        .with_from(anvil.addresses()[0])
        .with_to(*counter.address())
        .with_input(get_count_call.calldata().clone())
        .with_max_fee_per_gas(fees.max_fee_per_gas)
        .with_max_priority_fee_per_gas(fees.max_priority_fee_per_gas);

    let sim_block = SimBlock { calls: vec![tx], ..Default::default() };
    let request = SimulatePayload {
        block_state_calls: vec![sim_block],
        trace_transfers: false,
        validation: true,
        return_full_transactions: false,
    };

    let robust_result = robust.simulate(&request).await?;
    let alloy_result = alloy_provider.simulate(&request).await?;

    assert_eq!(robust_result.first().unwrap().inner, alloy_result.first().unwrap().inner);

    Ok(())
}
