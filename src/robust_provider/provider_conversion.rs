use alloy::{
    network::{Ethereum, Network},
    providers::{
        DynProvider, Provider, RootProvider,
        fillers::{FillProvider, TxFiller},
        layers::{CacheProvider, CallBatchProvider},
    },
    transports::http::reqwest::Url,
};

use crate::robust_provider::{Error, RobustProvider, RobustProviderBuilder};

/// Conversion trait for types that can be turned into an Alloy [`RootProvider`].
///
/// This is primarily used by [`RobustProviderBuilder`] to accept different provider types and
/// connection strings.
pub trait IntoRootProvider<N: Network = Ethereum> {
    /// Convert `self` into a [`RootProvider`].
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying provider cannot be constructed or connected.
    fn into_root_provider(self) -> impl Future<Output = Result<RootProvider<N>, Error>> + Send;
}

impl<N: Network> IntoRootProvider<N> for RootProvider<N> {
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(self)
    }
}

impl<N: Network> IntoRootProvider<N> for &str {
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(RootProvider::connect(self).await?)
    }
}

impl<N: Network> IntoRootProvider<N> for Url {
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(RootProvider::connect(self.as_str()).await?)
    }
}

impl<F, P, N> IntoRootProvider<N> for FillProvider<F, P, N>
where
    F: TxFiller<N>,
    P: Provider<N>,
    N: Network,
{
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(self.root().to_owned())
    }
}

impl<P, N> IntoRootProvider<N> for CacheProvider<P, N>
where
    P: Provider<N>,
    N: Network,
{
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(self.root().to_owned())
    }
}

impl<N> IntoRootProvider<N> for DynProvider<N>
where
    N: Network,
{
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(self.root().to_owned())
    }
}

impl<P, N> IntoRootProvider<N> for CallBatchProvider<P, N>
where
    P: Provider<N> + 'static,
    N: Network,
{
    async fn into_root_provider(self) -> Result<RootProvider<N>, Error> {
        Ok(self.root().to_owned())
    }
}

/// Conversion trait for types that can be turned into a [`RobustProvider`].
pub trait IntoRobustProvider<N: Network = Ethereum> {
    /// Convert `self` into a [`RobustProvider`].
    ///
    /// # Errors
    ///
    /// Returns an error if the primary or any fallback provider fails to connect.
    fn into_robust_provider(self) -> impl Future<Output = Result<RobustProvider<N>, Error>> + Send;
}

impl<N: Network> IntoRobustProvider<N> for RobustProvider<N> {
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        Ok(self)
    }
}

impl<N: Network> IntoRobustProvider<N> for RootProvider<N> {
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

impl<N: Network> IntoRobustProvider<N> for &str {
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

impl<N: Network> IntoRobustProvider<N> for Url {
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

impl<F, P, N> IntoRobustProvider<N> for FillProvider<F, P, N>
where
    F: TxFiller<N> + Send + 'static,
    P: Provider<N> + Send + 'static,
    N: Network,
{
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

impl<P, N> IntoRobustProvider<N> for CacheProvider<P, N>
where
    P: Provider<N> + Send + 'static,
    N: Network,
{
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

impl<N> IntoRobustProvider<N> for DynProvider<N>
where
    N: Network,
{
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

impl<P, N> IntoRobustProvider<N> for CallBatchProvider<P, N>
where
    P: Provider<N> + Send + 'static,
    N: Network,
{
    async fn into_robust_provider(self) -> Result<RobustProvider<N>, Error> {
        RobustProviderBuilder::new(self).build().await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use alloy::{
        network::Ethereum,
        node_bindings::Anvil,
        providers::{ProviderBuilder, WsConnect},
    };

    use crate::{
        DEFAULT_CALL_TIMEOUT, DEFAULT_MAX_RETRIES, DEFAULT_MIN_DELAY, DEFAULT_RECONNECT_INTERVAL,
        DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY, DEFAULT_SUBSCRIPTION_TIMEOUT, RobustProvider,
        RobustProviderBuilder,
    };

    use super::*;

    #[tokio::test]
    async fn robust_provider_into_robust_preserves_all_config() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let provider = ProviderBuilder::new()
            .connect_ws(WsConnect::new(anvil.ws_endpoint_url().as_str()))
            .await?;

        let call_timeout = Duration::from_secs(99);
        let subscription_timeout = Duration::from_secs(200);
        let max_retries = 42;
        let min_delay = Duration::from_millis(123);
        let reconnect_interval = Duration::from_secs(77);
        let subscription_buffer_capacity = 256;

        let original = RobustProviderBuilder::new(provider)
            .call_timeout(call_timeout)
            .subscription_timeout(subscription_timeout)
            .max_retries(max_retries)
            .min_delay(min_delay)
            .reconnect_interval(reconnect_interval)
            .subscription_buffer_capacity(subscription_buffer_capacity)
            .build()
            .await?;

        let converted: RobustProvider<Ethereum> = original.into_robust_provider().await?;

        assert_eq!(converted.call_timeout, call_timeout);
        assert_eq!(converted.subscription_timeout, subscription_timeout);
        assert_eq!(converted.max_retries, max_retries);
        assert_eq!(converted.min_delay, min_delay);
        assert_eq!(converted.reconnect_interval, reconnect_interval);
        assert_eq!(converted.subscription_buffer_capacity, subscription_buffer_capacity);

        Ok(())
    }

    #[cfg(feature = "http-subscription")]
    #[tokio::test]
    async fn robust_provider_into_robust_preserves_http_subscription_config() -> anyhow::Result<()>
    {
        let anvil = Anvil::new().try_spawn()?;
        let provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

        let poll_interval = Duration::from_secs(3);

        let original = RobustProviderBuilder::new(provider)
            .allow_http_subscriptions(true)
            .poll_interval(poll_interval)
            .build()
            .await?;

        let converted: RobustProvider<Ethereum> = original.into_robust_provider().await?;

        assert!(converted.allow_http_subscriptions);
        assert_eq!(converted.poll_interval, poll_interval);

        Ok(())
    }

    #[tokio::test]
    async fn robust_provider_into_robust_preserves_fallbacks() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let primary = ProviderBuilder::new()
            .connect_ws(WsConnect::new(anvil.ws_endpoint_url().as_str()))
            .await?;
        let fallback = ProviderBuilder::new().connect_http(anvil.endpoint_url());

        let original = RobustProviderBuilder::new(primary).fallback(fallback).build().await?;

        assert_eq!(original.fallback_providers.len(), 1);

        let converted: RobustProvider<Ethereum> = original.into_robust_provider().await?;

        assert_eq!(converted.fallback_providers.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn root_provider_into_robust_uses_defaults() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let fill_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());
        let root_provider = fill_provider.root().to_owned();

        let converted: RobustProvider<Ethereum> =
            root_provider.to_owned().into_robust_provider().await?;
        root_provider.into_robust_provider().await?;

        assert_eq!(converted.call_timeout, DEFAULT_CALL_TIMEOUT);
        assert_eq!(converted.subscription_timeout, DEFAULT_SUBSCRIPTION_TIMEOUT);
        assert_eq!(converted.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(converted.min_delay, DEFAULT_MIN_DELAY);
        assert_eq!(converted.reconnect_interval, DEFAULT_RECONNECT_INTERVAL);
        assert_eq!(converted.subscription_buffer_capacity, DEFAULT_SUBSCRIPTION_BUFFER_CAPACITY);
        assert!(converted.fallback_providers.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn fill_provider_into_robust_uses_defaults() -> anyhow::Result<()> {
        let anvil = Anvil::new().try_spawn()?;

        let fill_provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

        let converted: RobustProvider<Ethereum> = fill_provider.into_robust_provider().await?;

        assert_eq!(converted.call_timeout, DEFAULT_CALL_TIMEOUT);
        assert_eq!(converted.max_retries, DEFAULT_MAX_RETRIES);
        assert!(converted.fallback_providers.is_empty());

        Ok(())
    }
}
