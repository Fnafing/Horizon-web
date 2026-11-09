//! Horizon web: Instant from `performance.timeOrigin + performance.now()` (the same clock
//! in every Web Worker), SystemTime from `Date.now()`.
use crate::time::Duration;

#[link(wasm_import_module = "hzweb")]
unsafe extern "C" {
    /// Milliseconds on a clock shared by all threads (fractional).
    fn hz_now_ms() -> f64;
    /// Milliseconds since the Unix epoch.
    fn hz_date_ms() -> f64;
}

fn ms(v: f64) -> Duration {
    Duration::from_nanos((v.max(0.0) * 1_000_000.0) as u64)
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime(Duration);

pub const UNIX_EPOCH: SystemTime = SystemTime(Duration::from_secs(0));

impl Instant {
    pub fn now() -> Instant {
        Instant(ms(unsafe { hz_now_ms() }))
    }
    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        self.0.checked_sub(other.0)
    }
    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_add(*other)?))
    }
    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_sub(*other)?))
    }
}

impl SystemTime {
    pub const MAX: SystemTime = SystemTime(Duration::MAX);
    pub const MIN: SystemTime = SystemTime(Duration::ZERO);
    pub fn now() -> SystemTime {
        SystemTime(ms(unsafe { hz_date_ms() }))
    }
    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        self.0.checked_sub(other.0).ok_or_else(|| other.0 - self.0)
    }
    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_add(*other)?))
    }
    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_sub(*other)?))
    }
}
