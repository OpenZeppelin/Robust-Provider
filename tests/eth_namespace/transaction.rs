use crate::common::{setup_anvil, setup_anvil_with_contract};
use alloy::{
    eips::BlockNumberOrTag,
    network::TransactionBuilder,
    primitives::{B256, U256},
    providers::{Provider, ext::AnvilApi},
    rpc::types::TransactionRequest,
};

// ============================================================================
// eth_getTransactionByBlockHashAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_block_hash_and_index_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let mine_blocks = 5;
    let _ = counter.increase().send().await?.watch().await?;

    // adds this redundancy to ensure transaction has been included
    alloy_provider.anvil_mine(Some(mine_blocks), None).await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number - mine_blocks - 1))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_tx = robust.get_transaction_by_block_hash_and_index(block_hash, 0).await?;
    let alloy_tx = alloy_provider.get_transaction_by_block_hash_and_index(block_hash, 0).await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_block_hash_and_index_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_tx = robust.get_transaction_by_block_hash_and_index(block_hash, 999).await?;
    let alloy_tx = alloy_provider.get_transaction_by_block_hash_and_index(block_hash, 999).await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getTransactionByBlockNumberAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_block_number_and_index_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let mine_blocks = 5;
    let _ = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(mine_blocks), None).await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_tx = robust
        .get_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            0,
        )
        .await?;
    let alloy_tx = alloy_provider
        .get_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            0,
        )
        .await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_block_number_and_index_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let mine_blocks = 5;
    let _ = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(mine_blocks), None).await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_tx = robust
        .get_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            999,
        )
        .await?;
    let alloy_tx = alloy_provider
        .get_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            999,
        )
        .await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getTransactionByHash
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let receipt = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(5), None).await?;

    let robust_tx = robust.get_transaction_by_hash(receipt).await?;
    let alloy_tx = alloy_provider.get_transaction_by_hash(receipt).await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_hash_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let fake_hash = B256::ZERO;

    let robust_tx = robust.get_transaction_by_hash(fake_hash).await?;
    let alloy_tx = alloy_provider.get_transaction_by_hash(fake_hash).await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getRawTransactionByHash
// ============================================================================

#[tokio::test]
async fn test_get_raw_transaction_by_hash_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let receipt = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(5), None).await?;

    let robust_raw = robust.get_raw_transaction_by_hash(receipt).await?;
    let alloy_raw = alloy_provider.get_raw_transaction_by_hash(receipt).await?;

    assert!(robust_raw.is_some());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}

#[tokio::test]
async fn test_get_raw_transaction_by_hash_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let fake_hash = B256::ZERO;

    let robust_raw = robust.get_raw_transaction_by_hash(fake_hash).await?;
    let alloy_raw = alloy_provider.get_raw_transaction_by_hash(fake_hash).await?;

    assert!(robust_raw.is_none());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}

// ============================================================================
// eth_getTransactionReceipt
// ============================================================================

#[tokio::test]
async fn test_get_transaction_receipt_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let tx_hash = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(5), None).await?;

    let robust_receipt = robust.get_transaction_receipt(tx_hash).await?;
    let alloy_receipt = alloy_provider.get_transaction_receipt(tx_hash).await?;

    assert!(robust_receipt.is_some());
    assert_eq!(robust_receipt, alloy_receipt);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_receipt_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let fake_hash = B256::ZERO;

    let robust_receipt = robust.get_transaction_receipt(fake_hash).await?;
    let alloy_receipt = alloy_provider.get_transaction_receipt(fake_hash).await?;

    assert!(robust_receipt.is_none());
    assert_eq!(robust_receipt, alloy_receipt);

    Ok(())
}

// ============================================================================
// eth_newPendingTransactionFilter
// ============================================================================

#[tokio::test]
async fn test_new_pending_transactions_filter_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let robust_filter_id = robust.new_pending_transactions_filter(false).await?;
    let alloy_filter_id = alloy_provider.new_pending_transactions_filter(false).await?;

    let robust_changes: Vec<B256> = robust.get_filter_changes(robust_filter_id).await?;
    let alloy_changes: Vec<B256> = alloy_provider.get_filter_changes(alloy_filter_id).await?;

    assert!(robust_changes.is_empty());
    assert!(alloy_changes.is_empty());

    let _ = counter.increase().send().await?.watch().await?;

    let robust_changes: Vec<B256> = robust.get_filter_changes(robust_filter_id).await?;
    let alloy_changes: Vec<B256> = alloy_provider.get_filter_changes(alloy_filter_id).await?;

    assert_eq!(robust_changes.first().unwrap(), alloy_changes.first().unwrap());
    assert_eq!(robust_changes.len(), 1);
    assert_eq!(alloy_changes.len(), 1);

    Ok(())
}

// ============================================================================
// eth_signTransaction
// ============================================================================

#[tokio::test]
async fn test_sign_transaction_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let from = accounts[0];

    let tx = TransactionRequest::default()
        .with_from(from)
        .with_to(from)
        .with_nonce(0)
        .with_gas_limit(1)
        .with_max_fee_per_gas(1)
        .with_max_priority_fee_per_gas(1);

    let robust_signed = robust.sign_transaction(tx.clone()).await?;
    let alloy_signed = alloy_provider.sign_transaction(tx).await?;

    assert!(!robust_signed.is_empty());
    assert_eq!(robust_signed, alloy_signed);
    Ok(())
}

// ============================================================================
// eth_sendRawTransaction
// ============================================================================

#[tokio::test]
async fn test_send_raw_transaction_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let from = accounts[0];

    let tx = TransactionRequest::default()
        .with_from(from)
        .with_to(from)
        .with_nonce(0)
        .with_gas_limit(21000)
        .with_max_fee_per_gas(1_000_000_000)
        .with_max_priority_fee_per_gas(1_000_000_000);

    let signed_tx = alloy_provider.sign_transaction(tx).await?;

    let robust_tx_hash = robust.send_raw_transaction(&signed_tx).await?;

    alloy_provider.anvil_mine(Some(5), None).await?;

    let alloy_pending =
        alloy_provider.get_transaction_by_hash(robust_tx_hash.tx_hash().to_owned()).await?;

    assert!(alloy_pending.is_some());
    Ok(())
}

// ============================================================================
// eth_sendRawTransactionSync
// ============================================================================

#[tokio::test]
async fn test_send_raw_transaction_sync_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let from = accounts[0];

    let tx = TransactionRequest::default()
        .with_from(from)
        .with_to(from)
        .with_nonce(0)
        .with_gas_limit(21000)
        .with_max_fee_per_gas(1_000_000_000)
        .with_max_priority_fee_per_gas(1_000_000_000);

    let signed_tx = alloy_provider.sign_transaction(tx).await?;

    let receipt = robust.send_raw_transaction_sync(&signed_tx).await?;

    assert!(receipt.status());
    Ok(())
}

// ============================================================================
// eth_sendTransaction
// ============================================================================

#[tokio::test]
async fn test_send_transaction_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, _counter) = setup_anvil_with_contract().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let from = accounts[0];
    let to = accounts[1];

    let tx = TransactionRequest::default().with_from(from).with_to(to).with_value(U256::from(1000));

    let robust_pending = robust.send_transaction(tx.clone()).await?;
    let robust_receipt = robust_pending.get_receipt().await?;

    assert!(robust_receipt.status());
    Ok(())
}

// ============================================================================
// eth_sendTransactionSync
// ============================================================================

#[tokio::test]
async fn test_send_transaction_sync_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, _counter) = setup_anvil_with_contract().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let from = accounts[0];
    let to = accounts[1];

    let tx = TransactionRequest::default().with_from(from).with_to(to).with_value(U256::from(1000));

    let receipt = robust.send_transaction_sync(tx).await?;

    assert!(receipt.status());
    Ok(())
}

// ============================================================================
// eth_getTransactionBySenderNonce
// ============================================================================

#[tokio::test]
async fn test_get_transaction_by_sender_nonce_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(5), None).await?;

    let accounts = alloy_provider.get_accounts().await?;
    let sender = accounts[0];

    // Nonce 0 was used by the contract deployment, nonce 1 by the increase call
    let robust_tx = robust.get_transaction_by_sender_nonce(sender, 1).await?;
    let alloy_tx = alloy_provider.get_transaction_by_sender_nonce(sender, 1).await?;

    assert!(robust_tx.is_some());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_sender_nonce_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider) = setup_anvil().await?;

    let accounts = alloy_provider.get_accounts().await?;
    let sender = accounts[0];

    let robust_tx = robust.get_transaction_by_sender_nonce(sender, 999).await?;
    let alloy_tx = alloy_provider.get_transaction_by_sender_nonce(sender, 999).await?;

    assert!(robust_tx.is_none());
    assert_eq!(robust_tx, alloy_tx);

    Ok(())
}

// ============================================================================
// eth_getRawTransactionByBlockHashAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_raw_transaction_by_block_hash_and_index_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let mine_blocks = 5;
    let _ = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(mine_blocks), None).await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number - mine_blocks - 1))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_raw = robust.get_raw_transaction_by_block_hash_and_index(block_hash, 0).await?;
    let alloy_raw =
        alloy_provider.get_raw_transaction_by_block_hash_and_index(block_hash, 0).await?;

    assert!(robust_raw.is_some());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}

#[tokio::test]
async fn test_get_raw_transaction_by_block_hash_and_index_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let _ = counter.increase().send().await?.watch().await?;

    let block_number = alloy_provider.get_block_number().await?;
    let block = alloy_provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .expect("block should exist");
    let block_hash = block.header.hash;

    let robust_raw = robust.get_raw_transaction_by_block_hash_and_index(block_hash, 999).await?;
    let alloy_raw =
        alloy_provider.get_raw_transaction_by_block_hash_and_index(block_hash, 999).await?;

    assert!(robust_raw.is_none());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}

// ============================================================================
// eth_getRawTransactionByBlockNumberAndIndex
// ============================================================================

#[tokio::test]
async fn test_get_raw_transaction_by_block_number_and_index_succeeds() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let mine_blocks = 5;
    let _ = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(mine_blocks), None).await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_raw = robust
        .get_raw_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            0,
        )
        .await?;
    let alloy_raw = alloy_provider
        .get_raw_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            0,
        )
        .await?;

    assert!(robust_raw.is_some());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}

#[tokio::test]
async fn test_get_raw_transaction_by_block_number_and_index_not_found() -> anyhow::Result<()> {
    let (_anvil, robust, alloy_provider, counter) = setup_anvil_with_contract().await?;

    let mine_blocks = 5;
    let _ = counter.increase().send().await?.watch().await?;

    alloy_provider.anvil_mine(Some(mine_blocks), None).await?;

    let block_number = alloy_provider.get_block_number().await?;

    let robust_raw = robust
        .get_raw_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            999,
        )
        .await?;
    let alloy_raw = alloy_provider
        .get_raw_transaction_by_block_number_and_index(
            BlockNumberOrTag::Number(block_number - mine_blocks - 1),
            999,
        )
        .await?;

    assert!(robust_raw.is_none());
    assert_eq!(robust_raw, alloy_raw);

    Ok(())
}
