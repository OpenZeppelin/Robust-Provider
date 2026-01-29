//! Robust provider implementation with automatic reconnection and failover.
//!
//! # Components
//!
//! * [`RobustProvider`] - The main provider type that wraps underlying providers with reconnection
//!   logic
//! * [`RobustProviderBuilder`] - Builder for constructing a `RobustProvider` with custom
//!   configuration
//! * [`RobustSubscription`] / [`RobustSubscriptionStream`] - Subscription types that automatically
//!   reconnect on failure
//!
//! # Traits
//!
//! * [`IntoRobustProvider`] - Convert types into a `RobustProvider`
//! * [`IntoRootProvider`] - Convert types into an underlying root provider
//!
//! # Feature Flags
//!
//! * `http-subscription` - Enable HTTP-based polling subscriptions for providers without
//!   native pubsub support

mod builder;
mod errors;
#[cfg(feature = "http-subscription")]
mod http_subscription;
mod provider;
mod provider_conversion;
mod robust;
mod subscription;

pub use builder::*;
pub use errors::{CoreError, Error};
#[cfg(feature = "http-subscription")]
pub use http_subscription::{
    HttpPollingStream, HttpPollingSubscription, HttpSubscriptionConfig, HttpSubscriptionError,
    DEFAULT_POLL_INTERVAL,
};
pub use provider::RobustProvider;
pub use provider_conversion::{IntoRobustProvider, IntoRootProvider};
pub use robust::Robustness;
pub use subscription::{
    DEFAULT_RECONNECT_INTERVAL, Error as SubscriptionError, RobustSubscription,
    RobustSubscriptionStream,
};
