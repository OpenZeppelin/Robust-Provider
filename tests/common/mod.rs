//! Common test utilities and helpers for integration tests.

#![allow(dead_code)]

use std::time::Duration;

/// Short timeout for tests.
pub const SHORT_TIMEOUT: Duration = Duration::from_millis(300);

/// Reconnect interval for tests.
pub const RECONNECT_INTERVAL: Duration = Duration::from_millis(500);

/// Buffer time for async operations.
pub const BUFFER_TIME: Duration = Duration::from_millis(100);

pub mod setup_anvil;
pub mod setup_kurtosis;
