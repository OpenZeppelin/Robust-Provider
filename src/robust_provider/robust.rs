//! Robustness trait for providers with retry and fallback logic.

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

/// Trait for the robust providers that support robust operations i.e. ones with retry and
/// failover logic.
///
/// This trait provides methods for executing operations with exponential backoff,
/// timeouts, and automatic failover to backup providers.
pub trait Robustness<N: Network> {
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

            debug!(
                start_index = start_index,
                total_fallbacks = fallback_providers.len(),
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
                    total_fallbacks = fallback_providers.len(),
                    "Attempting fallback provider"
                );

                match self.try_provider_with_timeout(provider, &operation).await {
                    Ok(value) => {
                        info!(
                            fallback_index = fallback_idx,
                            total_fallbacks = fallback_providers.len(),
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

            error!(attempted_providers = fallback_providers.len() + 1, "All providers exhausted");

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
    #[must_use]
    fn supports_pubsub(provider: &RootProvider<N>) -> bool {
        provider.client().pubsub_frontend().is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use alloy::{
        node_bindings::Anvil,
        providers::{ProviderBuilder, WsConnect},
    };
    use tokio::time::sleep;

    use crate::{
        DEFAULT_RECONNECT_INTERVAL, DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY,
        DEFAULT_SUBSCRIPTION_TIMEOUT, Error, RobustProvider, RobustProviderBuilder,
    };

    use super::*;

    fn test_provider(timeout: u64, max_retries: usize, min_delay: u64) -> RobustProvider {
        RobustProvider {
            primary_provider: RootProvider::new_http("http://localhost:8545".parse().unwrap()),
            fallback_providers: vec![],
            call_timeout: Duration::from_millis(timeout),
            subscription_timeout: DEFAULT_SUBSCRIPTION_TIMEOUT,
            max_retries,
            min_delay: Duration::from_millis(min_delay),
            reconnect_interval: DEFAULT_RECONNECT_INTERVAL,
            subscription_buffer_capacity: DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY,
        }
    }

    #[tokio::test]
    async fn test_retry_with_timeout_succeeds_on_first_attempt() {
        let provider = test_provider(100, 3, 10);

        let call_count = AtomicUsize::new(0);

        let result = provider
            .try_operation_with_failover(
                |_| async {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let count = call_count.load(Ordering::SeqCst);
                    Ok(count)
                },
                false,
            )
            .await;

        assert!(matches!(result, Ok(1)));
    }

    #[tokio::test]
    async fn test_retry_with_timeout_retries_on_error() {
        let provider = test_provider(100, 3, 10);

        let call_count = AtomicUsize::new(0);

        let result = provider
            .try_operation_with_failover(
                |_| async {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let count = call_count.load(Ordering::SeqCst);
                    match count {
                        3 => Ok(count),
                        // retriable error
                        _ => Err(TransportErrorKind::custom_str("429 Too Many Requests")),
                    }
                },
                false,
            )
            .await;

        assert!(matches!(result, Ok(3)));
    }

    #[tokio::test]
    async fn test_retry_with_timeout_fails_after_max_retries() {
        let provider = test_provider(100, 2, 10);

        let call_count = AtomicUsize::new(0);

        let result: Result<(), CoreError> = provider
            .try_operation_with_failover(
                |_| async {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    // retriable error
                    Err(TransportErrorKind::custom_str("429 Too Many Requests"))
                },
                false,
            )
            .await;

        assert!(matches!(result, Err(CoreError::RpcError(_))));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_timeout_respects_call_timeout() {
        let call_timeout = 50;
        let provider = test_provider(call_timeout, 10, 1);

        let result = provider
            .try_operation_with_failover(
                move |_provider| async move {
                    sleep(Duration::from_millis(call_timeout + 10)).await;
                    Ok(42)
                },
                false,
            )
            .await;

        assert!(matches!(result, Err(CoreError::Timeout)));
    }

    #[tokio::test]
    async fn test_subscribe_fails_when_all_providers_lack_pubsub() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let http_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

        let robust = RobustProviderBuilder::new(http_provider.clone())
            .fallback(http_provider)
            .call_timeout(Duration::from_secs(5))
            .min_delay(Duration::from_millis(100))
            .build()
            .await?;

        let result = robust.subscribe_blocks().await.unwrap_err();

        match result {
            Error::RpcError(e) => {
                assert!(matches!(
                    e.as_ref(),
                    RpcError::Transport(TransportErrorKind::PubsubUnavailable)
                ));
            }
            other => panic!("Expected PubsubUnavailable error type, got: {other:?}"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_subscribe_succeeds_if_primary_provider_lacks_pubsub_but_fallback_supports_it()
    -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let http_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
        let ws_provider = ProviderBuilder::new()
            .connect_ws(WsConnect::new(anvil.ws_endpoint_url().as_str()))
            .await?;

        let robust = RobustProviderBuilder::fragile(http_provider)
            .fallback(ws_provider)
            .call_timeout(Duration::from_secs(5))
            .build()
            .await?;

        let result = robust.subscribe_blocks().await;
        assert!(result.is_ok());

        Ok(())
    }
}
