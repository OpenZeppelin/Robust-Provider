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

use crate::robust_provider::{CoreError, RobustProvider};

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

/// Default time interval between primary provider reconnection attempts
pub const DEFAULT_RECONNECT_INTERVAL: Duration = Duration::from_secs(30);

/// A robust subscription wrapper that automatically handles provider failover
/// and periodic reconnection attempts to the primary provider.
#[derive(Debug)]
pub struct RobustSubscription<N: Network> {
    subscription: Subscription<N::HeaderResponse>,
    robust_provider: RobustProvider<N>,
    last_reconnect_attempt: Option<Instant>,
    current_fallback_index: Option<usize>,
}

impl<N: Network> RobustSubscription<N> {
    /// Create a new [`RobustSubscription`]
    pub(crate) fn new(
        subscription: Subscription<N::HeaderResponse>,
        robust_provider: RobustProvider<N>,
    ) -> Self {
        Self {
            subscription,
            robust_provider,
            last_reconnect_attempt: None,
            current_fallback_index: None,
        }
    }

    /// Receive the next item from the subscription with automatic failover.
    ///
    /// This method will:
    /// * Attempt to receive from the current subscription
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
            match timeout(subscription_timeout, self.subscription.recv()).await {
                Ok(Ok(header)) => {
                    if self.is_on_fallback() {
                        self.try_reconnect_to_primary(false).await;
                    }
                    return Ok(header);
                }
                Ok(Err(recv_error)) => return Err(recv_error.into()),
                Err(_elapsed) => {
                    warn!(
                        timeout_secs = subscription_timeout.as_secs(),
                        "Subscription timeout - no block received, switching provider"
                    );
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

        let operation =
            move |provider: RootProvider<N>| async move { provider.subscribe_blocks().await };

        let primary = self.robust_provider.primary();
        let subscription =
            self.robust_provider.try_provider_with_timeout(primary, &operation).await;

        if let Ok(sub) = subscription {
            info!("Reconnected to primary provider");
            self.subscription = sub;
            self.current_fallback_index = None;
            self.last_reconnect_attempt = None;
            true
        } else {
            self.last_reconnect_attempt = Some(Instant::now());
            false
        }
    }

    async fn switch_to_fallback(&mut self, last_error: CoreError) -> Result<(), Error> {
        // If we're on a fallback, try primary first before moving to next fallback
        if self.is_on_fallback() && self.try_reconnect_to_primary(true).await {
            return Ok(());
        }

        if self.last_reconnect_attempt.is_none() {
            self.last_reconnect_attempt = Some(Instant::now());
        }

        let operation =
            move |provider: RootProvider<N>| async move { provider.subscribe_blocks().await };

        // Start searching from the next provider after the current one
        let start_index = self.current_fallback_index.map_or(0, |idx| idx + 1);

        let (sub, fallback_idx) = self
            .robust_provider
            .try_fallback_providers_from(&operation, true, last_error, start_index)
            .await?;

        info!(fallback_index = fallback_idx, "Subscription switched to fallback provider");
        self.subscription = sub;
        self.current_fallback_index = Some(fallback_idx);
        Ok(())
    }

    /// Returns true if currently using a fallback provider
    fn is_on_fallback(&self) -> bool {
        self.current_fallback_index.is_some()
    }

    /// Check if the subscription channel is empty (no pending messages)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.subscription.is_empty()
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
