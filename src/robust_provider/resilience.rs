//! Resilience trait for providers with retry and fallback logic.

use std::{fmt::Debug, future::Future, time::Duration};

use alloy::{
    network::Network,
    providers::{Provider, RootProvider},
    transports::{RpcError, TransportErrorKind},
};
use backon::{ExponentialBuilder, Retryable};
use futures::TryFutureExt;
use tokio::time::timeout;

use super::errors::{CoreError, is_retryable_error};

/// Trait for the robust providers that support resilient operations i.e. ones with retry and
/// failover logic.
///
/// This trait provides methods for executing operations with exponential backoff,
/// timeouts, and automatic failover to backup providers.
pub trait Resilience<N: Network> {
    /// Get a reference to the primary provider.
    fn primary(&self) -> &RootProvider<N>;

    /// Get a reference to the fallback providers.
    fn fallback_providers(&self) -> &[RootProvider<N>];

    /// Get the call timeout duration.
    fn call_timeout(&self) -> Duration;

    /// Get the maximum number of retries.
    fn max_retries(&self) -> usize;

    /// Get the minimum delay between retries.
    fn min_delay(&self) -> Duration;

    /// Execute `operation` with exponential backoff and a total timeout.
    ///
    /// Wraps the retry logic with [`tokio::time::timeout`] so
    /// the entire operation (including time spent inside the RPC call) cannot exceed
    /// `call_timeout`.
    ///
    /// If the timeout is exceeded and fallback providers are available, it will
    /// attempt to use each fallback provider in sequence.
    ///
    /// If `require_pubsub` is true, providers that don't support pubsub will be skipped.
    ///
    /// # Errors
    ///
    /// * [`CoreError::RpcError`] - if no fallback providers succeeded; contains the last error
    ///   returned by the last provider attempted on the last retry.
    /// * [`CoreError::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
    ///   `call_timeout`).
    fn try_operation_with_failover<T: Debug, F, Fut>(
        &self,
        operation: F,
        require_pubsub: bool,
    ) -> impl Future<Output = Result<T, CoreError>>
    where
        F: Fn(RootProvider<N>) -> Fut,
        Fut: Future<Output = Result<T, RpcError<TransportErrorKind>>>,
    {
        async move {
            let primary = self.primary();
            self.try_provider_with_timeout(primary, &operation)
                .or_else(|last_error| {
                    self.try_fallback_providers_from(&operation, require_pubsub, last_error, 0)
                        .map_ok(|(value, _)| value)
                })
                .await
        }
    }

    /// Try fallback providers starting from a specific index.
    fn try_fallback_providers_from<T: Debug, F, Fut>(
        &self,
        operation: F,
        require_pubsub: bool,
        mut last_error: CoreError,
        start_index: usize,
    ) -> impl Future<Output = Result<(T, usize), CoreError>>
    where
        F: Fn(RootProvider<N>) -> Fut,
        Fut: Future<Output = Result<T, RpcError<TransportErrorKind>>>,
    {
        async move {
            let fallback_providers = self.fallback_providers();
            let num_fallbacks = fallback_providers.len();

            debug!(
                start_index = start_index,
                total_fallbacks = num_fallbacks,
                require_pubsub = require_pubsub,
                "Primary provider failed, attempting fallback providers"
            );

            let fallback_iter = fallback_providers.iter().enumerate().skip(start_index);
            for (fallback_idx, provider) in fallback_iter {
                if require_pubsub && !Self::supports_pubsub(provider) {
                    debug!(
                        provider_index = fallback_idx,
                        "Skipping fallback provider: pubsub not supported"
                    );
                    continue;
                }

                trace!(
                    fallback_index = fallback_idx,
                    total_fallbacks = num_fallbacks,
                    "Attempting fallback provider"
                );

                match self.try_provider_with_timeout(provider, &operation).await {
                    Ok(value) => {
                        info!(
                            fallback_index = fallback_idx,
                            total_fallbacks = num_fallbacks,
                            "Switched to fallback provider"
                        );
                        return Ok((value, fallback_idx));
                    }
                    Err(e) => {
                        warn!(
                            fallback_index = fallback_idx,
                            error = %e,
                            "Fallback provider failed"
                        );
                        last_error = e;
                    }
                }
            }

            error!(attempted_providers = num_fallbacks + 1, "All providers exhausted");

            Err(last_error)
        }
    }

    /// Try executing an operation with a specific provider with retry and timeout.
    fn try_provider_with_timeout<T, F, Fut>(
        &self,
        provider: &RootProvider<N>,
        operation: F,
    ) -> impl Future<Output = Result<T, CoreError>>
    where
        F: Fn(RootProvider<N>) -> Fut,
        Fut: Future<Output = Result<T, RpcError<TransportErrorKind>>>,
    {
        let retry_strategy = ExponentialBuilder::default()
            .with_max_times(self.max_retries())
            .with_min_delay(self.min_delay());

        let call_timeout = self.call_timeout();
        let provider = provider.clone();

        async move {
            timeout(
                call_timeout,
                (|| operation(provider.clone()))
                    .retry(retry_strategy)
                    .when(|e| match e {
                        // Check if the error is explicitly marked as retryable
                        RpcError::ErrorResp(err_resp) if err_resp.is_retry_err() => true,
                        // Check our custom non-retryable error classification
                        // TODO: check if this can be omitted once https://github.com/OpenZeppelin/Robust-Provider/issues/12 is implemented
                        RpcError::ErrorResp(err_resp) => {
                            is_retryable_error(err_resp.code, err_resp.message.as_ref())
                        }
                        // Transport errors have their own retry logic
                        RpcError::Transport(tr_err) => tr_err.is_retry_err(),
                        // Default to retrying unknown errors
                        _ => true,
                    })
                    .sleep(tokio::time::sleep),
            )
            .await
            .map_err(CoreError::from)?
            .map_err(CoreError::from)
        }
    }

    /// Check if a provider supports pubsub
    fn supports_pubsub(provider: &RootProvider<N>) -> bool {
        provider.client().pubsub_frontend().is_some()
    }
}
