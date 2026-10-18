//! A monotonic clock the rate limiter reads, so fixtures run without real sleeping.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Monotonic milliseconds and a sleep.
pub trait Clock {
    /// Milliseconds since an arbitrary fixed origin; never decreases.
    fn now_ms(&self) -> u64;
    /// Blocks (or, for a manual clock, advances) for `ms` milliseconds.
    fn sleep_ms(&self, ms: u64);
}

/// The real monotonic clock.
#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// Starts a clock at zero.
    pub fn new() -> Self {
        SystemClock {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// A clock that only moves when told to; `sleep_ms` advances it instantly.
#[derive(Debug, Default)]
pub struct ManualClock {
    now: Cell<u64>,
    slept: Cell<u64>,
}

impl ManualClock {
    /// Starts at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total milliseconds spent in `sleep_ms`.
    pub fn total_slept_ms(&self) -> u64 {
        self.slept.get()
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }

    fn sleep_ms(&self, ms: u64) {
        self.now.set(self.now.get().saturating_add(ms));
        self.slept.set(self.slept.get().saturating_add(ms));
    }
}
