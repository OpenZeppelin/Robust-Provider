use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
    time::{Duration, Instant},
};

use alloy::{
    network::Network,
    providers::{Provider, RootProvider},
    pubsub::Subscription,
    transports::{RpcError, TransportErrorKind},
};
use thiserror::Error;
use tokio::{sync::broadcast::error::RecvError, time::timeout};
use tokio_stream::Stream;
use tokio_util::sync::ReusableBoxFuture;

use crate::robust_provider::{CoreError, RobustProvider, Robustness};

#[cfg(feature = "http-subscription")]
use crate::robust_provider::http_subscription::{
    HttpPollingSubscription, HttpSubscriptionConfig, HttpSubscriptionError,
};

/// Errors that can occur when using [`RobustSubscription`].
#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("Operation timed out")]
    Timeout,
    #[error("RPC call failed after exhausting all retry attempts: {0}")]
    RpcError(Arc<RpcError<TransportErrorKind>>),
    #[error("Subscription closed")]
    Closed,
    #[error("Subscription lagged behind by: {0}")]
    Lagged(u64),
}

impl From<CoreError> for Error {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Timeout => Error::Timeout,
            CoreError::RpcError(e) => Error::RpcError(Arc::new(e)),
        }
    }
}

impl From<RecvError> for Error {
    fn from(err: RecvError) -> Self {
        match err {
            RecvError::Closed => Error::Closed,
            RecvError::Lagged(count) => Error::Lagged(count),
        }
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Error::Timeout
    }
}

#[cfg(feature = "http-subscription")]
impl From<HttpSubscriptionError> for Error {
    fn from(err: HttpSubscriptionError) -> Self {
        match err {
            HttpSubscriptionError::Timeout => Error::Timeout,
            HttpSubscriptionError::RpcError(e) => Error::RpcError(e),
            HttpSubscriptionError::Closed => Error::Closed,
            HttpSubscriptionError::BlockFetchFailed(msg) => {
                // Use custom_str which returns RpcError directly
                Error::RpcError(Arc::new(TransportErrorKind::custom_str(&msg)))
            }
        }
    }
}

/// Default time interval between primary provider reconnection attempts
pub const DEFAULT_RECONNECT_INTERVAL: Duration = Duration::from_secs(30);

/// Timeout for validating HTTP provider reachability during reconnection
const HTTP_RECONNECT_VALIDATION_TIMEOUT: Duration = Duration::from_millis(150);

/// Backend for subscriptions - either native WebSocket or HTTP polling.
///
/// This enum allows `RobustSubscription` to transparently handle both
/// WebSocket-based and HTTP polling-based subscriptions.
#[derive(Debug)]
pub(crate) enum SubscriptionBackend<N: Network> {
    /// Native WebSocket subscription using pubsub
    WebSocket(Subscription<N::HeaderResponse>),
    /// HTTP polling-based subscription (requires `http-subscription` feature)
    #[cfg(feature = "http-subscription")]
    HttpPolling(HttpPollingSubscription<N>),
}

/// A robust subscription wrapper that automatically handles provider failover
/// and periodic reconnection attempts to the primary provider.
#[derive(Debug)]
pub struct RobustSubscription<N: Network> {
    backend: SubscriptionBackend<N>,
    robust_provider: RobustProvider<N>,
    last_reconnect_attempt: Option<Instant>,
    current_fallback_index: Option<usize>,
    /// Configuration for HTTP polling (stored for failover to HTTP providers)
    #[cfg(feature = "http-subscription")]
    http_config: HttpSubscriptionConfig,
}

impl<N: Network> RobustSubscription<N> {
    /// Create a new [`RobustSubscription`] with a WebSocket backend.
    pub(crate) fn new(
        subscription: Subscription<N::HeaderResponse>,
        robust_provider: RobustProvider<N>,
    ) -> Self {
        #[cfg(feature = "http-subscription")]
        let http_config = HttpSubscriptionConfig {
            poll_interval: robust_provider.poll_interval,
            call_timeout: robust_provider.call_timeout,
            buffer_capacity: robust_provider.subscription_buffer_capacity,
        };

        Self {
            backend: SubscriptionBackend::WebSocket(subscription),
            robust_provider,
            last_reconnect_attempt: None,
            current_fallback_index: None,
            #[cfg(feature = "http-subscription")]
            http_config,
        }
    }

    /// Create a new [`RobustSubscription`] with an HTTP polling backend.
    #[cfg(feature = "http-subscription")]
    pub(crate) fn new_http(
        subscription: HttpPollingSubscription<N>,
        robust_provider: RobustProvider<N>,
        config: HttpSubscriptionConfig,
    ) -> Self {
        Self {
            backend: SubscriptionBackend::HttpPolling(subscription),
            robust_provider,
            last_reconnect_attempt: None,
            current_fallback_index: None,
            http_config: config,
        }
    }

    /// Receive the next item from the subscription with automatic failover.
    ///
    /// This method will:
    /// * Attempt to receive from the current subscription (WebSocket or HTTP polling)
    /// * Handle errors by switching to fallback providers
    /// * Periodically attempt to reconnect to the primary provider
    /// * Will switch to fallback providers if subscription timeout is exhausted
    ///
    /// # Primary Provider Reconnection
    ///
    /// The primary provider is retried in two scenarios:
    /// 1. **Periodic reconnection**: Every `reconnect_interval` (default: 30 seconds) while on a
    ///    fallback provider and successfully receiving blocks. Note: The actual reconnection
    ///    attempt occurs when a new block is received, so if blocks arrive slower than the
    ///    reconnect interval, reconnection will be delayed until the next block.
    /// 2. **Fallback failure**: Immediately when a fallback provider fails, before attempting the
    ///    next fallback provider
    ///
    /// # Errors
    ///
    /// * Propagates any underlying subscription errors.
    /// * If all providers have been exhausted and failed, returns the last attempt's error.
    pub async fn recv(&mut self) -> Result<N::HeaderResponse, Error> {
        let subscription_timeout = self.robust_provider.subscription_timeout;

        loop {
            // Receive from the appropriate backend
            let result = match &mut self.backend {
                SubscriptionBackend::WebSocket(sub) => {
                    match timeout(subscription_timeout, sub.recv()).await {
                        Ok(Ok(header)) => Ok(header),
                        Ok(Err(recv_error)) => Err(Error::from(recv_error)),
                        Err(_elapsed) => Err(Error::Timeout),
                    }
                }
                #[cfg(feature = "http-subscription")]
                SubscriptionBackend::HttpPolling(sub) => {
                    match timeout(subscription_timeout, sub.recv()).await {
                        Ok(Ok(header)) => Ok(header),
                        Ok(Err(e)) => Err(Error::from(e)),
                        Err(_elapsed) => Err(Error::Timeout),
                    }
                }
            };

            match result {
                Ok(header) => {
                    if self.is_on_fallback() {
                        self.try_reconnect_to_primary(false).await;
                    }
                    return Ok(header);
                }
                Err(Error::Timeout) => {
                    warn!(
                        timeout_secs = subscription_timeout.as_secs(),
                        "Subscription timeout - no block received, switching provider"
                    );
                    self.switch_to_fallback(CoreError::Timeout).await?;
                }
                // Propagate these errors directly without failover
                Err(Error::Closed) => return Err(Error::Closed),
                Err(Error::Lagged(count)) => return Err(Error::Lagged(count)),
                // RPC errors trigger failover
                Err(Error::RpcError(_e)) => {
                    warn!("Subscription RPC error, switching provider");
                    self.switch_to_fallback(CoreError::Timeout).await?;
                }
            }
        }
    }

    /// Try to reconnect to the primary provider if enough time has elapsed.
    /// Returns true if reconnection was successful, false if it's not time yet or if it failed.
    async fn try_reconnect_to_primary(&mut self, force: bool) -> bool {
        // Check if we should attempt reconnection
        let should_reconnect = force ||
            match self.last_reconnect_attempt {
                None => false,
                Some(last_attempt) => {
                    last_attempt.elapsed() >= self.robust_provider.reconnect_interval
                }
            };

        if !should_reconnect {
            return false;
        }

        let primary = self.robust_provider.primary();

        // Try WebSocket subscription first if supported
        if Self::supports_pubsub(primary) {
            let operation =
                move |provider: RootProvider<N>| async move { provider.subscribe_blocks().await };

            let subscription =
                self.robust_provider.try_provider_with_timeout(primary, &operation).await;

            if let Ok(sub) = subscription {
                info!("Reconnected to primary provider (WebSocket)");
                self.backend = SubscriptionBackend::WebSocket(sub);
                self.current_fallback_index = None;
                self.last_reconnect_attempt = None;
                return true;
            }
        }

        // Try HTTP polling if enabled and WebSocket not available/failed
        #[cfg(feature = "http-subscription")]
        if self.robust_provider.allow_http_subscriptions {
            let validation = tokio::time::timeout(
                HTTP_RECONNECT_VALIDATION_TIMEOUT,
                primary.get_block_number(),
            )
            .await;

            if matches!(validation, Ok(Ok(_))) {
                let http_sub = HttpPollingSubscription::new(
                    primary.clone(),
                    self.http_config.clone(),
                );
                info!("Reconnected to primary provider (HTTP polling)");
                self.backend = SubscriptionBackend::HttpPolling(http_sub);
                self.current_fallback_index = None;
                self.last_reconnect_attempt = None;
                return true;
            }
        }

        self.last_reconnect_attempt = Some(Instant::now());
        false
    }

    async fn switch_to_fallback(&mut self, last_error: CoreError) -> Result<(), Error> {
        // If we're on a fallback, try primary first before moving to next fallback
        if self.is_on_fallback() && self.try_reconnect_to_primary(true).await {
            return Ok(());
        }

        if self.last_reconnect_attempt.is_none() {
            self.last_reconnect_attempt = Some(Instant::now());
        }

        // Start searching from the next provider after the current one
        let start_index = self.current_fallback_index.map_or(0, |idx| idx + 1);
        let fallback_providers = self.robust_provider.fallback_providers();

        // Try each fallback provider
        for (idx, provider) in fallback_providers.iter().enumerate().skip(start_index) {
            // Try WebSocket subscription first if provider supports pubsub
            if Self::supports_pubsub(provider) {
                let operation =
                    move |p: RootProvider<N>| async move { p.subscribe_blocks().await };

                if let Ok(sub) = self
                    .robust_provider
                    .try_provider_with_timeout(provider, &operation)
                    .await
                {
                    info!(
                        fallback_index = idx,
                        "Subscription switched to fallback provider (WebSocket)"
                    );
                    self.backend = SubscriptionBackend::WebSocket(sub);
                    self.current_fallback_index = Some(idx);
                    return Ok(());
                }
            }

            // Try HTTP polling if enabled
            #[cfg(feature = "http-subscription")]
            if self.robust_provider.allow_http_subscriptions {
                let http_sub = HttpPollingSubscription::new(
                    provider.clone(),
                    self.http_config.clone(),
                );
                info!(
                    fallback_index = idx,
                    "Subscription switched to fallback provider (HTTP polling)"
                );
                self.backend = SubscriptionBackend::HttpPolling(http_sub);
                self.current_fallback_index = Some(idx);
                return Ok(());
            }
        }

        // All fallbacks exhausted
        error!(
            attempted_providers = fallback_providers.len() + 1,
            "All providers exhausted for subscription"
        );
        Err(last_error.into())
    }

    /// Returns true if currently using a fallback provider
    fn is_on_fallback(&self) -> bool {
        self.current_fallback_index.is_some()
    }

    /// Check if a provider supports native pubsub (WebSocket)
    fn supports_pubsub(provider: &RootProvider<N>) -> bool {
        provider.client().pubsub_frontend().is_some()
    }

    /// Check if the subscription channel is empty (no pending messages)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match &self.backend {
            SubscriptionBackend::WebSocket(sub) => sub.is_empty(),
            #[cfg(feature = "http-subscription")]
            SubscriptionBackend::HttpPolling(sub) => sub.is_empty(),
        }
    }

    /// Convert the subscription into a stream.
    #[must_use]
    pub fn into_stream(self) -> RobustSubscriptionStream<N> {
        RobustSubscriptionStream::from(self)
    }
}

type SubscriptionResult<N> = (Result<<N as Network>::HeaderResponse, Error>, RobustSubscription<N>);

pub struct RobustSubscriptionStream<N: Network> {
    inner: ReusableBoxFuture<'static, SubscriptionResult<N>>,
}

async fn make_future<N: Network>(mut rx: RobustSubscription<N>) -> SubscriptionResult<N> {
    let result = rx.recv().await;
    (result, rx)
}

impl<N: 'static + Clone + Send + Network> RobustSubscriptionStream<N> {
    /// Create a new `RobustSubscriptionStream`.
    #[must_use]
    pub fn new(rx: RobustSubscription<N>) -> Self {
        Self { inner: ReusableBoxFuture::new(make_future(rx)) }
    }
}

impl<N: 'static + Clone + Send + Network> Stream for RobustSubscriptionStream<N> {
    type Item = Result<N::HeaderResponse, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (result, rx) = ready!(self.inner.poll(cx));
        self.inner.set(make_future(rx));
        match result {
            Ok(item) => Poll::Ready(Some(Ok(item))),
            Err(Error::Closed) => Poll::Ready(None),
            Err(e) => Poll::Ready(Some(Err(e))),
        }
    }
}

impl<N: 'static + Clone + Send + Network> From<RobustSubscription<N>>
    for RobustSubscriptionStream<N>
{
    fn from(recv: RobustSubscription<N>) -> Self {
        Self::new(recv)
    }
}
