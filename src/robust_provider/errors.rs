//! Error types and RPC error classification for robust provider operations.
//!
//! This module provides:
//! * Public error types ([`enum@Error`], [`CoreError`]) for provider operations
//! * RPC error classification logic to detect non-retryable errors from various Ethereum clients
//!
//! # Error Classification
//!
//! Ethereum clients return various error codes and messages for different failure conditions.
//! This module classifies these errors to determine whether an error should be retried. In general
//! failures related to invalid blocks are considered non-retryable.
//!
//! Some clients may use different error codes/messages; errors that don't match known
//! patterns will surface as [`Error::RpcError`] and will be retried by default.

use std::sync::Arc;

use alloy::transports::{RpcError, TransportErrorKind};
use thiserror::Error;
use tokio::time::error as TokioError;

use super::subscription;

/// Errors that can occur when using [`super::RobustProvider`].
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// The operation exceeded the configured timeout.
    #[error("Operation timed out")]
    Timeout,

    /// An RPC error occurred after exhausting all retry attempts.
    #[error("RPC call failed after exhausting all retry attempts: {0}")]
    RpcError(Arc<RpcError<TransportErrorKind>>),

    /// The requested block was not found.
    ///
    /// This error is returned when the underlying provider returns `None` for the requested
    /// block, or when detecting client-specific RPC error responses that indicate a missing block
    /// (e.g., Geth's error code `-32000` with a "block ... not found"-like message).
    ///
    /// **Note:** This classification has been verified on Anvil, Reth, and Geth. Other clients
    /// may use different error codes/messages; in those cases the error may surface as
    /// [`Error::RpcError`].
    #[error("Block not found")]
    BlockNotFound,
}

/// Low-level error related to RPC calls and failover logic.
///
/// This is an internal error type used during retry/failover operations.
/// It gets converted to [`enum@Error`] before being returned to users.
#[derive(Error, Debug)]
pub enum CoreError {
    /// The operation exceeded the configured timeout.
    #[error("Operation timed out")]
    Timeout,

    /// An RPC error occurred.
    #[error("RPC call failed after exhausting all retry attempts: {0}")]
    RpcError(RpcError<TransportErrorKind>),
}

impl From<RpcError<TransportErrorKind>> for CoreError {
    fn from(err: RpcError<TransportErrorKind>) -> Self {
        CoreError::RpcError(err)
    }
}

impl From<CoreError> for Error {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Timeout => Error::Timeout,
            CoreError::RpcError(RpcError::ErrorResp(ref err_resp))
                if is_block_not_found(err_resp.code, err_resp.message.as_ref()) =>
            {
                Error::BlockNotFound
            }
            CoreError::RpcError(e) => Error::RpcError(Arc::new(e)),
        }
    }
}

impl From<TokioError::Elapsed> for CoreError {
    fn from(_: TokioError::Elapsed) -> Self {
        CoreError::Timeout
    }
}

impl From<RpcError<TransportErrorKind>> for Error {
    fn from(err: RpcError<TransportErrorKind>) -> Self {
        Error::RpcError(Arc::new(err))
    }
}

impl From<TokioError::Elapsed> for Error {
    fn from(_: TokioError::Elapsed) -> Self {
        Error::Timeout
    }
}

impl From<subscription::Error> for Error {
    fn from(err: subscription::Error) -> Self {
        match err {
            subscription::Error::RpcError(e) => Error::RpcError(e),
            subscription::Error::Timeout |
            subscription::Error::Closed |
            subscription::Error::Lagged(_) => Error::Timeout,
        }
    }
}

/// Returns `true` if the error should be retried.
///
/// Non-retryable errors include:
/// * Block not found errors
/// * Invalid log filter errors
pub(crate) fn is_retryable_error(code: i64, message: &str) -> bool {
    let non_retryable = is_block_not_found(code, message) || is_invalid_log_filter(code, message);
    !non_retryable
}

pub(crate) fn is_block_not_found(code: i64, message: &str) -> bool {
    geth::is_block_not_found(code, message) ||
        besu::is_block_not_found(code, message) ||
        anvil::is_block_not_found(code, message)
}

pub(crate) fn is_invalid_log_filter(code: i64, message: &str) -> bool {
    geth::is_invalid_log_filter(code, message)
}

// Geth (go-ethereum) specific error detection.
mod geth {
    // Default error code used by Geth for various errors.
    // Reference: <https://github.com/ethereum/go-ethereum/blob/494908a8523af0e67d22d7930df15787ca5776b2/rpc/errors.go#L61>
    pub const DEFAULT_ERROR_CODE: i64 = -32000;

    pub fn is_block_not_found(code: i64, message: &str) -> bool {
        if code != DEFAULT_ERROR_CODE {
            return false;
        }

        matches!(
            message,
            // BlockByNumber
            // https://github.com/ethereum/go-ethereum/blob/e3e556b266ce0c645002f80195ac786dd5d9f2f8/eth/api_backend.go#L126
            "pending block is not available"
                | "finalized block not found"
                | "safe block not found"
                |
                // eth_getLogs and related filter APIs
                // https://github.com/ethereum/go-ethereum/blob/494908a8523af0e67d22d7930df15787ca5776b2/eth/filters/filter.go#L81
                // https://github.com/ethereum/go-ethereum/blob/494908a8523af0e67d22d7930df15787ca5776b2/eth/filters/api.go#L486
                "earliest header not found"
                | "finalized header not found"
                | "safe header not found"
                |
                // StateAndHeaderByNumberOrHash
                // https://github.com/ethereum/go-ethereum/blob/e3e556b266ce0c645002f80195ac786dd5d9f2f8/eth/api_backend.go#L259
                // https://github.com/ethereum/go-ethereum/blob/e3e556b266ce0c645002f80195ac786dd5d9f2f8/internal/ethapi/api.go#L321
                "header not found"
                | "header for hash not found"
        ) || (
            // Tracer pattern: "block {number} not found"
            // https://github.com/ethereum/go-ethereum/blob/e3e556b266ce0c645002f80195ac786dd5d9f2f8/eth/tracers/api.go#L133
            message.starts_with("block") && message.ends_with("not found")
        )
    }

    pub fn is_invalid_log_filter(code: i64, message: &str) -> bool {
        matches!(
            (code, message),
            (
                DEFAULT_ERROR_CODE,
                // https://github.com/ethereum/go-ethereum/blob/ef815c59a207d50668afb343811ed7ff02cc640b/eth/filters/api.go#L39-L46
                "invalid block range params" |
                    "block range extends beyond current head block" |
                    "can't specify fromBlock/toBlock with blockHash" |
                    "pending logs are not supported" |
                    "unknown block" |
                    "exceed max topics" |
                    "exceed max addresses or topics per search position" |
                    "filter not found"
            )
        )
    }
}

/// Besu specific error detection.
mod besu {

    /// Reference: <https://github.com/hyperledger/besu/blob/1dfd8ed9269ef33fdbda520ef8906c3dc059e713/ethereum/api/src/main/java/org/hyperledger/besu/ethereum/api/jsonrpc/internal/response/RpcErrorType.java#L126>
    pub const UNKNOWN_BLOCK_ERROR_CODE: i64 = -39001;

    /// Reference: <https://github.com/hyperledger/besu/blob/1dfd8ed9269ef33fdbda520ef8906c3dc059e713/ethereum/api/src/main/java/org/hyperledger/besu/ethereum/api/jsonrpc/internal/response/RpcErrorType.java#L126>
    pub fn is_block_not_found(code: i64, message: &str) -> bool {
        matches!((code, message), (UNKNOWN_BLOCK_ERROR_CODE, "Unknown block"))
    }
}

mod anvil {

    /// Reference: <https://github.com/foundry-rs/foundry/blob/2b85d1fbd3647865efdae4c0e17b994638ff722c/crates/anvil/rpc/src/error.rs#L102>
    pub const INVALID_PARAMS_ERROR_CODE: i64 = -32602;

    /// Reference: <https://github.com/foundry-rs/foundry/blob/2b85d1fbd3647865efdae4c0e17b994638ff722c/crates/anvil/src/eth/error.rs#L72>
    pub fn is_block_not_found(code: i64, message: &str) -> bool {
        if code != INVALID_PARAMS_ERROR_CODE {
            return false;
        }
        message.contains("BlockOutOfRangeError")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geth_block_not_found() {
        // Standard block not found messages
        assert!(geth::is_block_not_found(-32000, "pending block is not available"));
        assert!(geth::is_block_not_found(-32000, "finalized block not found"));
        assert!(geth::is_block_not_found(-32000, "safe block not found"));
        assert!(geth::is_block_not_found(-32000, "header not found"));
        assert!(geth::is_block_not_found(-32000, "header for hash not found"));

        assert!(geth::is_block_not_found(-32000, "block 12345 not found"));
        assert!(geth::is_block_not_found(-32000, "block 0x1234 not found"));

        // Non-matching
        assert!(!geth::is_block_not_found(-32000, "some other error"));
        assert!(!geth::is_block_not_found(-32001, "header not found"));
    }

    #[test]
    fn test_geth_invalid_log_filter() {
        assert!(geth::is_invalid_log_filter(-32000, "invalid block range params"));
        assert!(geth::is_invalid_log_filter(
            -32000,
            "block range extends beyond current head block"
        ));
        assert!(geth::is_invalid_log_filter(
            -32000,
            "can't specify fromBlock/toBlock with blockHash"
        ));
        assert!(geth::is_invalid_log_filter(-32000, "pending logs are not supported"));
        assert!(geth::is_invalid_log_filter(-32000, "unknown block"));
        assert!(geth::is_invalid_log_filter(-32000, "exceed max topics"));
        assert!(geth::is_invalid_log_filter(
            -32000,
            "exceed max addresses or topics per search position"
        ));
        assert!(geth::is_invalid_log_filter(-32000, "filter not found"));

        // Non-matching
        assert!(!geth::is_invalid_log_filter(-32000, "some other error"));
        assert!(!geth::is_invalid_log_filter(-32001, "invalid block range params"));
    }

    #[test]
    fn test_besu_block_not_found() {
        assert!(besu::is_block_not_found(-39001, "Unknown block"));

        // Non-matching
        assert!(!besu::is_block_not_found(-39001, "some other error"));
        assert!(!besu::is_block_not_found(-32000, "Unknown block"));
    }

    #[test]
    fn test_should_retry_rpc_error() {
        // Should NOT retry these
        assert!(!is_retryable_error(-32000, "header not found"));
        assert!(!is_retryable_error(-32000, "invalid block range params"));
        assert!(!is_retryable_error(-39001, "Unknown block"));
        assert!(!is_retryable_error(-32000, "pending logs are not supported"));
        assert!(!is_retryable_error(-32000, "unknown block"));
        assert!(!is_retryable_error(-32000, "exceed max topics"));
        assert!(!is_retryable_error(-32000, "exceed max addresses or topics per search position"));
        assert!(!is_retryable_error(-32000, "filter not found"));

        // Should retry these (unknown errors)
        assert!(is_retryable_error(-32000, "some transient error"));
        assert!(is_retryable_error(-32603, "internal error"));
    }
}
