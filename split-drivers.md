Here is the complete flat file restructuring for `espforge-drivers/src/`.

### File Operations:
1. **Create** `espforge-drivers/src/ssd1306.rs`
2. **Create** `espforge-drivers/src/ili9341.rs`
3. **Update** `espforge-drivers/src/lib.rs`
4. **Delete** `espforge-drivers/src/display.rs`

---

### 1. `espforge-drivers/src/ssd1306.rs`

```rust
//! SSD1306 monochrome I2C OLED display driver.

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::{
    Pixel, draw_target::DrawTarget, geometry::Size, prelude::OriginDimensions,
};
use esp_hal::Blocking;
use espforge_runtime::I2cBus;
use ssd1306::mode::DisplayConfig;

pub use ssd1306::prelude::{
    DisplayRotation, DisplaySize128x32 as Oled128x32, DisplaySize128x64 as Oled128x64,
};

/// The rotation of an OLED panel, re-exported so generated code names only
/// this crate.
type Oled<SIZE> = ssd1306::Ssd1306<
    ssd1306::prelude::I2CInterface<I2cBus<Blocking>>,
    SIZE,
    ssd1306::mode::BufferedGraphicsMode<SIZE>,
>;

/// An SSD1306 OLED panel on an I2C bus.
///
/// `SIZE` is one of [`Oled128x64`] or [`Oled128x32`]; the constructor takes the
/// size marker by value, in the same way `ssd1306` does.
pub struct Ssd1306Display<SIZE: ssd1306::prelude::DisplaySize> {
    display: Oled<SIZE>,
}

impl<SIZE: ssd1306::prelude::DisplaySize> Ssd1306Display<SIZE> {
    /// Build the panel on `bus` and run its initialization sequence.
    pub fn new(bus: I2cBus<Blocking>, size: SIZE, rotation: DisplayRotation) -> Self {
        let interface = ssd1306::I2CDisplayInterface::new(bus);
        let mut display =
            ssd1306::Ssd1306::new(interface, size, rotation).into_buffered_graphics_mode();
        display.init().expect("SSD1306 OLED initialization failed");
        Self { display }
    }

    /// Push the framebuffer to the panel. Draw calls only change the buffer.
    pub fn flush(&mut self) -> Result<(), ()> {
        self.display.flush().map_err(|_| ())
    }

    /// Clear the framebuffer (call [`flush`](Self::flush) to show it).
    pub fn clear(&mut self) {
        self.display.clear_buffer();
    }
}

impl<SIZE: ssd1306::prelude::DisplaySize> DrawTarget for Ssd1306Display<SIZE> {
    type Color = BinaryColor;
    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.display.draw_iter(pixels).map_err(|_| ())
    }
}

impl<SIZE: ssd1306::prelude::DisplaySize> OriginDimensions for Ssd1306Display<SIZE> {
    fn size(&self) -> Size {
        self.display.size()
    }
}
```

---

### 2. `espforge-drivers/src/ili9341.rs`

```rust
//! ILI9341 SPI color TFT display driver.

use embedded_graphics::{
    Pixel, draw_target::DrawTarget, geometry::Size, prelude::OriginDimensions,
};
use esp_hal::Blocking;
use esp_hal::gpio::{Level, Output};
use espforge_runtime::SpiDevice;

pub use sky_ili9341::{ColorOrder, Orientation};

/// An ILI9341 color TFT panel on an SPI device.
///
/// The display owns its SPI device: the `<spi_component>` it was declared with
/// is taken out of `Components` when `Devices::new` runs.
pub struct Ili9341Display {
    display: sky_ili9341::Display<sky_ili9341::SpiInterface<SpiDevice<Blocking>, Output<'static>>>,
    backlight: Option<Output<'static>>,
}

impl Ili9341Display {
    /// Build the panel and run its initialization sequence.
    ///
    /// The driver always pulses a hardware reset line, so `reset` is required
    /// (the schema makes `reset="$<gpio>"` mandatory for `<ili9341>`). The
    /// optional `backlight` is switched on after init.
    pub fn new(
        spi_device: SpiDevice<Blocking>,
        dc: Output<'static>,
        mut reset: Output<'static>,
        backlight: Option<Output<'static>>,
        orientation: Orientation,
        color_order: ColorOrder,
    ) -> Self {
        let interface = sky_ili9341::SpiInterface::new(spi_device, dc);
        let mut delay = esp_hal::delay::Delay::new();
        let display = sky_ili9341::Builder::new(interface)
            .orientation(orientation)
            .color_order(color_order)
            .init(&mut reset, &mut delay)
            .expect("ILI9341 TFT initialization failed");

        let mut backlight = backlight;
        if let Some(pin) = backlight.as_mut() {
            pin.set_high();
        }
        Self { display, backlight }
    }

    /// Turn the backlight on or off, when the panel has a controllable one.
    pub fn set_backlight(&mut self, on: bool) {
        if let Some(pin) = self.backlight.as_mut() {
            pin.set_level(if on { Level::High } else { Level::Low });
        }
    }
}

impl DrawTarget for Ili9341Display {
    type Color = embedded_graphics::pixelcolor::Rgb565;
    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.display.draw_iter(pixels).map_err(|_| ())
    }

    fn fill_solid(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        self.display.fill_solid(area, color).map_err(|_| ())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.display.clear(color).map_err(|_| ())
    }
}

impl OriginDimensions for Ili9341Display {
    fn size(&self) -> Size {
        self.display.size()
    }
}
```

---

### 3. `espforge-drivers/src/lib.rs`

```rust
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
```

---

### 4. Remove `espforge-drivers/src/display.rs`

Delete `src/display.rs`.

Each driver now has its own standalone file without internal `#[cfg(feature = "...")]` noise, and the crate root exports remain 100% backward-compatible with generated code (`espforge_drivers::Ssd1306Display`, `espforge_drivers::Ili9341Display`, etc.).
