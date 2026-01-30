use crate::common::{setup_anvil, setup_anvil_with_contract};
use alloy::{primitives::U256, providers::Provider};

// ============================================================================
// eth_accounts
// ============================================================================

#[tokio::test]
async fn test_get_accounts_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let robust_accounts = robust.get_accounts().await?;
    let alloy_accounts = alloy_provider.get_accounts().await?;

    assert_eq!(robust_accounts, alloy_accounts);
    // Anvil provides 10 default accounts
    assert_eq!(robust_accounts.len(), 10);

    Ok(())
}

// ============================================================================
// eth_getAccount
// ============================================================================

#[tokio::test]
async fn test_get_account_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let address = accounts[0];

    let robust_account = robust.get_account(address).await?;
    let alloy_account = alloy_provider.get_account(address).await?;

    assert_eq!(robust_account.nonce, alloy_account.nonce);
    assert_eq!(robust_account.balance, alloy_account.balance);
    assert_eq!(robust_account.storage_root, alloy_account.storage_root);
    assert_eq!(robust_account.code_hash, alloy_account.code_hash);

    Ok(())
}

// ============================================================================
// eth_getBalance
// ============================================================================

#[tokio::test]
async fn test_get_balance_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let address = accounts[0];

    let robust_balance = robust.get_balance(address).await?;
    let alloy_balance = alloy_provider.get_balance(address).await?;

    assert_eq!(robust_balance, alloy_balance);

    Ok(())
}

// ============================================================================
// eth_getCode
// ============================================================================

#[tokio::test]
async fn test_get_code_at_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let contract_address = *counter.address();

    let robust_code = robust.get_code_at(contract_address).await?;
    let alloy_code = alloy_provider.get_code_at(contract_address).await?;

    assert_eq!(robust_code, alloy_code);
    assert!(!robust_code.is_empty());

    Ok(())
}

// ============================================================================
// eth_getProof
// ============================================================================

#[tokio::test]
async fn test_get_proof_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let contract_address = *counter.address();
    let storage_keys = vec![];

    let robust_proof = robust.get_proof(contract_address, storage_keys.clone()).await?;
    let alloy_proof = alloy_provider.get_proof(contract_address, storage_keys).await?;

    assert_eq!(robust_proof.address, alloy_proof.address);
    assert_eq!(robust_proof.balance, alloy_proof.balance);
    assert_eq!(robust_proof.nonce, alloy_proof.nonce);
    assert_eq!(robust_proof.code_hash, alloy_proof.code_hash);
    assert_eq!(robust_proof.storage_hash, alloy_proof.storage_hash);

    Ok(())
}

#[tokio::test]
async fn test_get_proof_with_storage_keys_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let contract_address = *counter.address();
    let storage_keys = vec![U256::ZERO.into()];

    let robust_proof = robust.get_proof(contract_address, storage_keys.clone()).await?;
    let alloy_proof = alloy_provider.get_proof(contract_address, storage_keys).await?;

    assert_eq!(robust_proof.address, alloy_proof.address);
    assert_eq!(robust_proof.storage_proof.len(), alloy_proof.storage_proof.len());
    assert_eq!(robust_proof.storage_proof.len(), 1);

    Ok(())
}

// ============================================================================
// eth_getStorageAt
// ============================================================================

#[tokio::test]
async fn test_get_storage_at_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;
    let _ = counter.increase().send().await?.watch().await?;

    let contract_address = *counter.address();

    let robust_storage = robust.get_storage_at(contract_address, U256::ZERO).await?;
    let alloy_storage = alloy_provider.get_storage_at(contract_address, U256::ZERO).await?;

    assert_eq!(robust_storage, alloy_storage);
    assert_eq!(robust_storage, U256::from(3));

    Ok(())
}

#[tokio::test]
async fn test_get_storage_at_empty_slot() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let contract_address = *counter.address();

    let robust_storage = robust.get_storage_at(contract_address, U256::from(999)).await?;
    let alloy_storage = alloy_provider.get_storage_at(contract_address, U256::from(999)).await?;

    assert_eq!(robust_storage, alloy_storage);
    assert_eq!(robust_storage, U256::ZERO);

    Ok(())
}
