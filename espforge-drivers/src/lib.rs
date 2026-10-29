//! espforge-drivers: device drivers for generated firmware projects.
//!
//! Drivers wrap third-party crates and expose the types and operations app
//! code uses, so a generated project never names the vendor crate directly.
//! They depend on [`espforge_runtime`] for the bus handles and delays the
//! generated `Components` struct hands out.
//!
//! Target-only in practice, but the crate compiles on the host too (the
//! esp-hal parts live behind one cfg), so `cargo build`/`cargo test` on the
//! workspace work.

#![cfg_attr(not(test), no_std)]

#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "ssd1306"
))]
pub mod ssd1306;

#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "ssd1306"
))]
pub use ssd1306::*;

#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "ili9341"
))]
pub mod ili9341;

#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "ili9341"
))]
pub use ili9341::*;
