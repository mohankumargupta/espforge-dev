//! Display drivers: [`Ssd1306Display`] (I2C OLED) and [`Ili9341Display`] (SPI
//! TFT).
//!
//! Both wrap a third-party driver and implement
//! `embedded_graphics::DrawTarget`, so app code draws with the ordinary
//! primitives and never names the driver crate. They are blocking-only: the
//! bus components are shared through `RefCell` in the blocking build, and
//! `sky-ili9341`'s blocking interface is the one this runtime exposes.
//!
//! A display takes the bus component it is attached to by value (the OLED
//! copies the `Copy` I2C handle; the TFT takes the SPI device out of
//! `Components`), so nothing else can drive the same display bus.

#[cfg(feature = "ssd1306")]
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::{
    Pixel, draw_target::DrawTarget, geometry::Size, prelude::OriginDimensions,
};
use esp_hal::Blocking;
#[cfg(feature = "ili9341")]
use esp_hal::gpio::{Level, Output};
#[cfg(feature = "ssd1306")]
use ssd1306::mode::DisplayConfig;

#[cfg(feature = "ssd1306")]
use crate::I2cBus;
#[cfg(feature = "ili9341")]
use crate::SpiDevice;

#[cfg(feature = "ssd1306")]
pub use ssd1306::prelude::{
    DisplayRotation, DisplaySize128x32 as Oled128x32, DisplaySize128x64 as Oled128x64,
};

#[cfg(feature = "ili9341")]
pub use sky_ili9341::{ColorOrder, Orientation};

/// The rotation of an OLED panel, re-exported so generated code names only
/// this crate.
#[cfg(feature = "ssd1306")]
type Oled<SIZE> = ssd1306::Ssd1306<
    ssd1306::prelude::I2CInterface<I2cBus<Blocking>>,
    SIZE,
    ssd1306::mode::BufferedGraphicsMode<SIZE>,
>;

/// An SSD1306 OLED panel on an I2C bus.
///
/// `SIZE` is one of [`Oled128x64`] or [`Oled128x32`]; the constructor takes the
/// size marker by value, in the same way `ssd1306` does.
#[cfg(feature = "ssd1306")]
pub struct Ssd1306Display<SIZE: ssd1306::prelude::DisplaySize> {
    display: Oled<SIZE>,
}

#[cfg(feature = "ssd1306")]
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

#[cfg(feature = "ssd1306")]
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

#[cfg(feature = "ssd1306")]
impl<SIZE: ssd1306::prelude::DisplaySize> OriginDimensions for Ssd1306Display<SIZE> {
    fn size(&self) -> Size {
        self.display.size()
    }
}

/// An ILI9341 color TFT panel on an SPI device.
///
/// The display owns its SPI device: the `<spi_component>` it was declared with
/// is taken out of `Components` when `Devices::new` runs.
#[cfg(feature = "ili9341")]
pub struct Ili9341Display {
    display: sky_ili9341::Display<sky_ili9341::SpiInterface<SpiDevice<Blocking>, Output<'static>>>,
    backlight: Option<Output<'static>>,
}

#[cfg(feature = "ili9341")]
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

#[cfg(feature = "ili9341")]
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

#[cfg(feature = "ili9341")]
impl OriginDimensions for Ili9341Display {
    fn size(&self) -> Size {
        self.display.size()
    }
}
