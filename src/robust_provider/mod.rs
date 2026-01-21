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

mod builder;
mod errors;
mod provider;
mod provider_conversion;
mod subscription;

pub use builder::*;
pub use errors::{CoreError, Error};
pub use provider::RobustProvider;
pub use provider_conversion::{IntoRobustProvider, IntoRootProvider};
pub use subscription::{
    DEFAULT_RECONNECT_INTERVAL, Error as SubscriptionError, RobustSubscription,
    RobustSubscriptionStream,
};
