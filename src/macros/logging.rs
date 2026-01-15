//! internal logging macros that wrap `tracing` when the feature is enabled.
//!
//! when the `tracing` feature is disabled, all logging calls compile to no-ops,
//! ensuring zero runtime cost for users who don't need observability.

#[allow(unused_macros)]
macro_rules! error {
    ($($arg:tt)*) => {
        if cfg!(feature = "tracing") {
            tracing::error!(target: "robust_provider", $($arg)*)
        }
    };
}

#[allow(unused_macros)]
macro_rules! warn {
    ($($arg:tt)*) => {
        if cfg!(feature = "tracing") {
            tracing::warn!(target: "robust_provider", $($arg)*)
        }
    };
}

#[allow(unused_macros)]
macro_rules! info {
    ($($arg:tt)*) => {
        if cfg!(feature = "tracing") {
            tracing::info!(target: "robust_provider", $($arg)*)
        }
    };
}

#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {
        if cfg!(feature = "tracing") {
            tracing::debug!(target: "robust_provider", $($arg)*)
        }
    };
}

#[allow(unused_macros)]
macro_rules! trace {
    ($($arg:tt)*) => {
        if cfg!(feature = "tracing") {
            tracing::trace!(target: "robust_provider", $($arg)*)
        }
    };
}
