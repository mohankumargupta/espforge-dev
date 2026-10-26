//! espforge-runtime: no_std runtime support for generated firmware projects.
//!
//! The generated entry point builds a [`Logger`] and a [`Delay`], stores them
//! on the project's `Context`, and hands that to the user's `app`. Everything
//! the app touches comes from here, so the app never names `esp-hal` itself.
//!
//! Target-only in practice, but it compiles on the host too (the esp-hal parts
//! live behind one cfg), so `cargo build`/`cargo test` on the workspace work.

#![cfg_attr(not(test), no_std)]

/// Logging handle stored on the generated `Context`. Forwards to the `log`
/// facade; the generated project installs the sink (esp-println).
#[derive(Clone, Copy)]
pub struct Logger;

impl Logger {
    pub fn new() -> Self {
        Logger
    }
    pub fn info(&self, msg: impl core::fmt::Display) {
        log::info!("{}", msg);
    }
    pub fn warn(&self, msg: impl core::fmt::Display) {
        log::warn!("{}", msg);
    }
    pub fn error(&self, msg: impl core::fmt::Display) {
        log::error!("{}", msg);
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything that needs esp-hal. One module, one re-export: adding a runtime
/// type costs a `pub use` here instead of another repeated `cfg` on every
/// call site.
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
mod hw {
    /// Delay handle stored on the generated `Context`.
    ///
    /// - blocking (default): `delay_ms` blocks via `esp_hal::delay::Delay`.
    /// - `embassy` feature: `delay_ms` is `async` and yields via `embassy_time`.
    #[derive(Clone, Copy)]
    pub struct Delay {
        #[cfg(not(feature = "embassy"))]
        inner: esp_hal::delay::Delay,
    }

    impl Delay {
        pub fn new() -> Self {
            Delay {
                #[cfg(not(feature = "embassy"))]
                inner: esp_hal::delay::Delay::new(),
            }
        }

        #[cfg(not(feature = "embassy"))]
        pub fn delay_ms(&self, ms: u32) {
            self.inner.delay_millis(ms);
        }

        #[cfg(feature = "embassy")]
        pub async fn delay_ms(&self, ms: u32) {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(ms as u64)).await;
        }
    }

    impl Default for Delay {
        fn default() -> Self {
            Self::new()
        }
    }

    /// The embedded-hal traits, re-exported so generated projects can name
    /// `embedded_hal::...` through their own crate without adding the
    /// dependency themselves.
    pub use embedded_hal;
}

#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
pub use hw::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logger_is_a_cheap_handle() {
        let logger = Logger::new();
        let copy = logger;
        logger.info("hello");
        copy.warn("still usable");
        let _ = Logger::default();
    }
}
