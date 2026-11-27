//! Monotonic and mock clocks for deterministic protocol timing and simulation.

#[cfg(any(test, feature = "test-doubles"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Clock abstraction for timestamping telemetry frames and protocol packets.
pub trait Clock: Send + Sync {
    /// Returns the current monotonic timestamp in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Real-time clock backed by the system monotonic clock (`std::time::Instant`).
#[derive(Debug)]
pub struct SystemMonotonicClock {
    start: Instant,
}

impl Default for SystemMonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemMonotonicClock {
    /// Creates a new system monotonic clock starting at current process instant.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Clock for SystemMonotonicClock {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// Controllable mock clock for deterministic unit and integration testing.
#[cfg(any(test, feature = "test-doubles"))]
#[derive(Debug)]
pub struct MockClock {
    current_ms: AtomicU64,
}

#[cfg(any(test, feature = "test-doubles"))]
impl Default for MockClock {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(any(test, feature = "test-doubles"))]
impl MockClock {
    /// Creates a mock clock initialized to the given timestamp in milliseconds.
    pub fn new(initial_ms: u64) -> Self {
        Self {
            current_ms: AtomicU64::new(initial_ms),
        }
    }

    /// Advances the mock clock by the specified duration in milliseconds.
    pub fn advance_ms(&self, ms: u64) {
        self.current_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

#[cfg(any(test, feature = "test-doubles"))]
impl Clock for MockClock {
    fn now_ms(&self) -> u64 {
        self.current_ms.load(Ordering::SeqCst)
    }
}
