#[cfg(feature = "timer")]
pub mod debounce;
pub mod future;
pub mod stream;
#[cfg(feature = "timer")]
/// Throttle combinator that rate-limits how often a signal emits updates.
pub mod throttle;
