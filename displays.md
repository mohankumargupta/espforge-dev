Here is `displays.md`, designed to match the architectural style, formatting, and rigorous standards of `http-https.md`, `websockets.md`, and `ntp.md`.

---

# displays.md: Display Device Elements in espforge (`ili9341` and `ssd1306`)

> **Design and Implementation Specification**  
> **Target:** Adding display device elements (`<ssd1306>`, `<ili9341>`) to `# devices` in `espforge` using the [`ssd1306`](https://crates.io/crates/ssd1306) and [`sky-ili9341`](https://crates.io/crates/sky-ili9341) crates with `embedded-graphics` support.  
> **Status:** Final Draft / Ready for Implementation  
> **Scope:** Hardware display controllers bound to `# components` (`i2c_component`, `spi_component`). Runtime drivers live in `espforge-drivers` (or conditionally feature-gated under `espforge-runtime`).

---

## 1. Executive Summary & Goals

`espforge` generates bare-metal `no_std` Rust firmware for ESP32 microcontrollers. With bus peripherals (`<i2c>`, `<spi>`) and bus components (`<i2c_component>`, `<spi_component>`) established in Phase 3, projects need standard visual output for status dashboards, sensor readings, and user interfaces.

This document specifies the complete design for adding **display devices** to `# devices`:
1. **`<ssd1306>`**: Monochrome 128x64 / 128x32 OLED display controller connected via an I2C bus component, driven by the [`ssd1306`](https://crates.io/crates/ssd1306) crate.
2. **`<ili9341>`**: Full-color 240x320 RGB565 TFT LCD controller connected via an SPI device bus component and auxiliary GPIO control pins (`dc`, `reset`, optional `backlight`), driven by the high-performance, async-ready [`sky-ili9341`](https://crates.io/crates/sky-ili9341) crate.
3. **`embedded-graphics` Integration**: Both display drivers implement `embedded_graphics::draw_target::DrawTarget`, giving applications access to geometric primitives, text, fonts, and images.

### Core Tenets
* **Devices Layer (Rank 3):** Displays live in `# devices`. They do not declare raw I2C/SPI pins; they bind to already-validated bus components in `# components` (`i2c_component="$bus"`, `spi_component="$bus"`).
* **Pin Claiming for Auxiliary Pins:** `<ili9341>` claims dedicated digital control lines (`dc`, `reset`, `backlight`) as `PinUse::Output` through `LowerCx`. Conflicts with bus pins or other components are flagged during validation.
* **First-Class Context Macro:** Display handles populate the generated `Devices` struct in `src/generated.rs` and are accessed in `app.rs` via the existing `device!(ctx, my_display)` macro.
* **Strictly Feature-Gated Dependencies:** Projects without displays incur zero dependency overhead. `ssd1306` and `sky-ili9341` are only injected into `Cargo.toml` when declared in the `.espf` spec.

---

## 2. Architecture & Layer Ranking

`espforge` enforces strict vertical layering across its five sections:

```
Rank 0: # espforge       (Project settings, chip, runtime)
Rank 1: # peripherals    (<i2c>, <spi>, <gpio>)
Rank 2: # components     (<i2c_component>, <spi_component>)
Rank 3: # devices        (<ssd1306>, <ili9341>)
Rank 4: # services       (<http_client>, <ws_client>, <ntp>)
```

### Layer Rules
1. **Downward Reference Constraint:** Elements in `# devices` reference bus interfaces in `# components` (`<i2c_component>`, `<spi_component>`), never `# peripherals` directly.
2. **Auxiliary Pin Ownership:** Auxiliary GPIOs (such as Data/Command `dc` and Reset `reset` on SPI displays) must reference declared `<gpio>` peripherals in `# peripherals`. They are claimed as `PinUse::Output`.
3. **Runtime Support:** Both `ssd1306` and `sky-ili9341` support standard `embedded-hal` 1.0 blocking interfaces, and `sky-ili9341` supports `embedded-hal-async` 1.0, enabling clean operation under both `blocking` and `embassy` runtimes.

---

## 3. Element Specifications (`espforge-schema/src/espf.schema.kdl`)

Add two element blocks to `# devices` in `espforge-schema/src/espf.schema.kdl`:

### 3.1 `<ssd1306>`
```kdl
element "ssd1306" section="devices" {
    summary "SSD1306 monochrome OLED display (0.96\" / 1.3\")"
    description "Drives an I2C SSD1306 OLED panel in buffered graphics mode using the ssd1306 crate."
    attr "id" kind="ident" required=#true
    attr "i2c_component" kind="ref" to="i2c_component" required=#true {
        help "The I2C bus component this display is connected to."
    }
    attr "size" kind="choice" default="128x64" {
        choices "128x64" "128x32"
        help "Pixel dimensions of the OLED panel."
    }
    attr "rotation" kind="choice" default="0" {
        choices "0" "90" "180" "270"
        help "Display rotation in degrees clockwise."
    }
    example "minimal" #"""
        # devices
          <ssd1306 id="oled" i2c_component="$display_bus" />
        """#
    example "custom size and rotation" #"""
        # devices
          <ssd1306 id="oled" i2c_component="$display_bus" size="128x32" rotation="180" />
        """#
    see_also "components/i2c_component"
}
```

### 3.2 `<ili9341>`
```kdl
element "ili9341" section="devices" {
    summary "ILI9341 240x320 RGB565 color TFT LCD display"
    description "Drives an SPI ILI9341 TFT display using the sky-ili9341 driver with hardware-accelerated drawing."
    attr "id" kind="ident" required=#true
    attr "spi_component" kind="ref" to="spi_component" required=#true {
        help "The SPI device component providing SCLK, MOSI, and CS lines."
    }
    attr "dc" kind="ref" to="gpio" required=#true {
        help "GPIO pin for Data/Command selection."
    }
    attr "reset" kind="ref" to="gpio" {
        help "Optional GPIO pin for hardware reset. If omitted, software reset is used."
    }
    attr "backlight" kind="ref" to="gpio" {
        help "Optional GPIO pin to enable display backlight (driven HIGH on init)."
    }
    attr "rotation" kind="choice" default="0" {
        choices "0" "90" "180" "270"
        help "Display orientation in degrees clockwise."
    }
    attr "rgb" kind="bool" default="false" {
        help "When true, uses RGB color order instead of BGR."
    }
    example "minimal" #"""
        # devices
          <ili9341 id="tft" spi_component="$display_spi" dc="$gpio4" />
        """#
    example "with reset and backlight" #"""
        # devices
          <ili9341 id="tft" spi_component="$display_spi" dc="$gpio4" reset="$gpio5" backlight="$gpio6" rotation="90" />
        """#
    see_also "components/spi_component"
    see_also "peripherals/gpio"
}
```

---

## 4. Diagnostics & Validation (`espforge-diagnostics`)

Define the dedicated `E09xx` diagnostic code range for `# devices`:

| Code | Constant Name | Condition | Severity | Message / Label |
|---|---|---|---|---|
| `E0901` | `DEVICE_UNKNOWN_COMPONENT` | `i2c_component` or `spi_component` references a component not in `# components` | Error | `unknown bus component: <tag id=".."> refers to '..', which is not in # components` |
| `E0902` | `DEVICE_UNKNOWN_GPIO` | Auxiliary pin (`dc`, `reset`, `backlight`) references a non-existent `<gpio>` | Error | `unknown gpio: <tag id=".."> refers to '..', which is not a <gpio> in # peripherals` |
| `E0903` | `DEVICE_WRONG_DIRECTION` | Auxiliary GPIO pin is declared with `direction="input"` | Error | `wrong gpio direction: <tag id=".."> needs an output pin, but '..' has direction="input"` |
| `E0904` | `DEVICE_INVALID_ID` | `id` is not a valid Rust identifier | Error | `invalid device id: <tag id=".."> id must be a valid Rust identifier` |
| `E0905` | `DEVICE_UNSUPPORTED` | Device tag is unrecognized | Error | `<tag> is not a known element in # devices` |

---

## 5. Board IR & Lowering (`espforge-elements`)

### 5.1 Extending the Board Model (`espforge-elements/src/model.rs`)
Add `Device` representations using a forwarding macro:

```rust
macro_rules! device_enum {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Device {
            $($variant($ty)),*
        }

        impl Device {
            pub fn id(&self) -> &Ident {
                match self {
                    $(Device::$variant(d) => crate::element::Lowered::id(d),)*
                }
            }
        }

        impl crate::element::Lowered for Device {
            fn id(&self) -> &Ident { self.id() }
            fn emit(&self, board: &Board, cx: &mut crate::element::EmitCx) {
                match self {
                    $(Device::$variant(d) => d.emit(board, cx),)*
                }
            }
        }
    };
}

device_enum! {
    Ssd1306(crate::devices::ssd1306::Ssd1306Device),
    Ili9341(crate::devices::ili9341::Ili9341Device),
}
```

Add `devices` to `Board`:
```rust
#[derive(Debug, Clone)]
pub struct Board {
    pub meta: Meta,
    pub resources: ResourceGraph,
    pub components: Vec<Component>,
    pub devices: Vec<Device>,
    pub services: Vec<Service>,
    pub warnings: Vec<Diagnostic>,
}

impl Board {
    pub fn uses_ssd1306(&self) -> bool {
        self.devices.iter().any(|d| matches!(d, Device::Ssd1306(_)))
    }

    pub fn uses_ili9341(&self) -> bool {
        self.devices.iter().any(|d| matches!(d, Device::Ili9341(_)))
    }
}
```

### 5.2 Device Elements
Implement `Element` in:
- `espforge-elements/src/devices/ssd1306.rs`
- `espforge-elements/src/devices/ili9341.rs`

#### Lowering `<ssd1306>`:
1. Verify `id` is a Rust identifier (`E0904`).
2. Resolve `i2c_component` reference against components (`E0901`).
3. Store size enum (`DisplaySize128x64` or `DisplaySize128x32`) and rotation.

#### Lowering `<ili9341>`:
1. Verify `id` is a Rust identifier (`E0904`).
2. Resolve `spi_component` reference against components (`E0901`).
3. Resolve `dc` reference against declared GPIOs (`E0902`). Check direction is not `input` (`E0903`). Claim pin as `PinUse::Output` under `OwnerKind::Device("ili9341")`.
4. Resolve and claim optional `reset` and `backlight` GPIO pins as `PinUse::Output`.
5. Store rotation (0, 90, 180, 270) and color order (RGB/BGR).

---

## 6. Code Generation (`espforge-emit`)

### 6.1 `Cargo.toml` Dependencies (`codegen/cargo.rs`)
Update `codegen/cargo.rs` to inspect `board.uses_ssd1306()` and `board.uses_ili9341()`:

```rust
if board.uses_ssd1306() || board.uses_ili9341() {
    deps["embedded-graphics"] = value("0.8.1");
}

if board.uses_ssd1306() {
    deps["ssd1306"] = value("0.9.0");
    deps["display-interface"] = value("0.5.0");
}

if board.uses_ili9341() {
    deps["sky-ili9341"] = value("0.1.1");
}
```

### 6.2 `src/generated.rs` (Devices Struct Emission)
In `codegen/rust.rs`, populate the `Devices` struct:

```rust
// When devices are declared:
#[doc = " Constructed devices."]
pub struct Devices {
    pub oled: espforge_drivers::Ssd1306Display,
    pub tft: espforge_drivers::Ili9341Display,
}

impl Devices {
    pub fn new(components: &Components) -> Self {
        let oled = espforge_drivers::Ssd1306Display::new(
            components.oled_bus,
            ssd1306::size::DisplaySize128x64,
            ssd1306::rotation::DisplayRotation::Rotate0,
        );
        let tft = espforge_drivers::Ili9341Display::new(
            components.tft_bus,
            unsafe { registry.peripherals.GPIO4.clone_unchecked() }, // DC
            Some(unsafe { registry.peripherals.GPIO5.clone_unchecked() }), // Reset
            Some(unsafe { registry.peripherals.GPIO6.clone_unchecked() }), // Backlight
            sky_ili9341::Orientation::Landscape,
        );
        Self { oled, tft }
    }
}
```

---

## 7. Driver Runtime Abstraction (`espforge-drivers`)

Create a clean driver wrapper crate (`espforge-drivers`) in the workspace, keeping `espforge-runtime` minimal and unbloated.

### 7.1 `Cargo.toml` (`espforge-drivers/Cargo.toml`)
```toml
[package]
name = "espforge-drivers"
version.workspace = true
edition.workspace = true

[dependencies]
espforge-runtime = { workspace = true }
esp-hal = { version = "1", features = ["unstable"] }
embedded-hal = "1.0"
embedded-graphics = "0.8.1"
ssd1306 = { version = "0.9.0", default-features = false, optional = true }
display-interface = { version = "0.5.0", optional = true }
sky-ili9341 = { version = "0.1.1", default-features = false, features = ["graphics"], optional = true }

[features]
default = []
ssd1306 = ["dep:ssd1306", "dep:display-interface"]
ili9341 = ["dep:sky-ili9341"]
```

### 7.2 SSD1306 Driver (`espforge-drivers/src/ssd1306.rs`)
```rust
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::geometry::Dimensions;
use ssd1306::prelude::*;
use ssd1306::mode::BufferedGraphicsMode;
use ssd1306::I2CDisplayInterface;
use espforge_runtime::I2cBus;

pub struct Ssd1306Display<SIZE: DisplaySize = DisplaySize128x64> {
    display: Ssd1306<I2CInterface<I2cBus<esp_hal::Blocking>>, SIZE, BufferedGraphicsMode<SIZE>>,
}

impl<SIZE: DisplaySize> Ssd1306Display<SIZE> {
    pub fn new(bus: I2cBus<esp_hal::Blocking>, _size: SIZE, rotation: DisplayRotation) -> Self {
        let interface = I2CDisplayInterface::new(bus);
        let mut display = Ssd1306::new(interface, SIZE::default(), rotation)
            .into_buffered_graphics_mode();
        display.init().expect("SSD1306 OLED initialization failed");
        Self { display }
    }

    pub fn flush(&mut self) -> Result<(), espforge_runtime::I2cError> {
        self.display.flush().map_err(|_| espforge_runtime::I2cError::Bus)
    }

    pub fn clear(&mut self) {
        self.display.clear();
    }
}

impl<SIZE: DisplaySize> DrawTarget for Ssd1306Display<SIZE> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        self.display.draw_iter(pixels)
    }
}

impl<SIZE: DisplaySize> Dimensions for Ssd1306Display<SIZE> {
    fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
        self.display.bounding_box()
    }
}
```

### 7.3 ILI9341 Driver (`espforge-drivers/src/ili9341.rs`)
```rust
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::geometry::Dimensions;
use esp_hal::gpio::{Output, Level, OutputConfig, OutputPin};
use sky_ili9341::{Ili9341, Orientation, ColorOrder};
use espforge_runtime::SpiDevice;

pub struct Ili9341Display {
    driver: Ili9341<SpiDevice<esp_hal::Blocking>, Output<'static>, Output<'static>>,
    backlight: Option<Output<'static>>,
}

impl Ili9341Display {
    pub fn new(
        spi_device: SpiDevice<esp_hal::Blocking>,
        dc_pin: impl OutputPin + 'static,
        reset_pin: Option<impl OutputPin + 'static>,
        backlight_pin: Option<impl OutputPin + 'static>,
        orientation: Orientation,
    ) -> Self {
        let dc = Output::new(dc_pin, Level::Low, OutputConfig::default());
        let rst = reset_pin.map(|p| Output::new(p, Level::High, OutputConfig::default()));

        let mut delay = espforge_runtime::Delay::new();
        let mut display = sky_ili9341::Builder::new(spi_device, dc)
            .with_orientation(orientation)
            .with_color_order(ColorOrder::Bgr)
            .build();

        if let Some(mut rst_pin) = rst {
            display.hard_reset(&mut rst_pin, &mut delay).expect("TFT reset failed");
        }
        display.init(&mut delay).expect("ILI9341 TFT initialization failed");

        let mut bl = None;
        if let Some(p) = backlight_pin {
            let mut bl_out = Output::new(p, Level::High, OutputConfig::default());
            bl_out.set_high();
            bl = Some(bl_out);
        }

        Self { driver: display, backlight: bl }
    }

    pub fn set_backlight(&mut self, on: bool) {
        if let Some(ref mut bl) = self.backlight {
            if on { bl.set_high(); } else { bl.set_low(); }
        }
    }
}

impl DrawTarget for Ili9341Display {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        self.driver.draw_iter(pixels)
    }
}

impl Dimensions for Ili9341Display {
    fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
        self.driver.bounding_box()
    }
}
```

---

## 8. Application Code Integration (`app.rs`)

### 8.1 Example 1: Drawing on the SSD1306 OLED Display
```rust
use crate::{Context, device};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
    text::Text,
};

pub fn setup(ctx: &'static Context) {
    ctx.logger.info("Initializing SSD1306 OLED Display...");
    let oled = device!(ctx, oled);

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    Circle::new(Point::new(10, 10), 20)
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(oled)
        .ok();

    Text::new("ESPFORGE OLED", Point::new(35, 25), text_style)
        .draw(oled)
        .ok();

    oled.flush().ok();
}

pub fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000);
}
```

### 8.2 Example 2: Full-Color Graphics on the ILI9341 TFT Display
```rust
use crate::{Context, device};
use embedded_graphics::{
    mono_font::{ascii::FONT_9X18_BOLD, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Rectangle, PrimitiveStyleBuilder},
    text::Text,
};

pub fn setup(ctx: &'static Context) {
    ctx.logger.info("Initializing ILI9341 Color TFT...");
    let tft = device!(ctx, tft);

    // Clear display to Dark Blue
    Rectangle::new(Point::zero(), Size::new(320, 240))
        .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::new(0, 0, 10)).build())
        .draw(tft)
        .ok();

    // Render Title
    let title_style = MonoTextStyle::new(&FONT_9X18_BOLD, Rgb565::YELLOW);
    Text::new("ESPFORGE ILI9341", Point::new(20, 40), title_style)
        .draw(tft)
        .ok();
}

pub fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000);
}
```

---

## 9. Implementation Roadmap & Milestones

```
T0: Branch & baseline test verification
T1: Create espforge-drivers workspace crate with ssd1306 and sky-ili9341
T2: Schema additions (<ssd1306>, <ili9341>) and Diagnostics (E0901..E0905)
T3: Lowering in espforge-elements (devices/ssd1306.rs, devices/ili9341.rs)
T4: Codegen in espforge-emit (Devices struct generation and Cargo.toml deps)
T5: Embedded examples & golden baseline (06.Displays/ssd1306, 06.Displays/ili9341)
T6: Hardware-free compilation check via generated_projects_compile
```

### Milestone Breakdown

#### T1 — `espforge-drivers` Crate Creation
1. Create `espforge-drivers` in workspace root.
2. Add dependencies: `espforge-runtime`, `esp-hal`, `embedded-hal`, `embedded-graphics`, `ssd1306`, `display-interface`, `sky-ili9341`.
3. Implement `Ssd1306Display` and `Ili9341Display` wrappers.
4. Add `espforge-drivers` to root `Cargo.toml`.
5. Verify: `cargo check -p espforge-drivers --target riscv32imc-unknown-none-elf`.

#### T2 — Schema & Diagnostics
1. In `espforge-diagnostics/src/codes.rs`: Add `E0901`–`E0905` to `codes!` macro.
2. In `espforge-schema/src/espf.schema.kdl`: Add `<ssd1306>` and `<ili9341>` to `# devices`.
3. Verify: `cargo test -p espforge-schema` and `cargo test -p espforge-diagnostics`.

#### T3 — Lowering in `espforge-elements`
1. Update `model.rs` with `Device::Ssd1306` and `Device::Ili9341`.
2. Implement `devices/ssd1306.rs` and `devices/ili9341.rs`.
3. In `lower.rs`: Wire lowering pass for `# devices` after `# components`.
4. Verify: Unit tests verifying unknown component (`E0901`), invalid auxiliary pin (`E0902`, `E0903`), and valid lowering.

#### T4 — Codegen in `espforge-emit`
1. In `codegen/cargo.rs`: Add `embedded-graphics`, `ssd1306`, and `sky-ili9341` when `board.uses_ssd1306()` or `board.uses_ili9341()` is true.
2. In `codegen/rust.rs`: Emit `pub struct Devices { ... }` and initializers in `Devices::new(&components)`.
3. Verify: `cargo test -p espforge-emit`.

#### T5 — Examples & Goldens
1. Create `espforge-examples/src/examples/06.Displays/ssd1306/` and `ili9341/`:
   - `example.espf.txt`
   - `example.toml` (`chips = ["esp32c3", "esp32s3"]`, `runtimes = ["blocking"]`)
   - `app/rust/app.rs`
   - Wokwi `diagram.json` (wiring I2C OLED and SPI TFT to ESP32 board).
2. Baseline golden project outputs in `espforge/tests/golden/`.
3. Verify: `cargo test -p espforge --test golden_projects`.

#### T6 — Compile Gate & Cleanliness
1. Add displays to `espforge/tests/generated_projects_compile.rs`.
2. Run target compile verification:
   ```bash
   cargo test -p espforge --test generated_projects_compile -- --ignored
   ```
3. Run workspace sanity checks:
   ```bash
   cargo fmt --check
   cargo clippy --workspace
   cargo test --workspace
   ```

---

## 10. Acceptance Criteria / Definition of Done

- [ ] `espf.schema.kdl` defines `<ssd1306>` and `<ili9341>` in `# devices` with complete attribute validation and self-contained examples.
- [ ] Dedicated `espforge-drivers` crate isolates display driver logic from core `espforge-runtime`.
- [ ] Display elements lower cleanly by referencing `# components` (`i2c_component`, `spi_component`).
- [ ] `<ili9341>` correctly claims auxiliary pins (`dc`, `reset`, `backlight`) as `PinUse::Output` and catches pin conflicts.
- [ ] Generated projects populate `Devices` in `generated.rs`, and `device!(ctx, name)` is callable from `app.rs`.
- [ ] Both display drivers implement `embedded_graphics::draw_target::DrawTarget`.
- [ ] Generated `Cargo.toml` selectively pulls in `ssd1306` or `sky-ili9341` only when used.
- [ ] `generated_projects_compile` passes `cargo check` for both displays on `riscv32imc-unknown-none-elf`.
- [ ] All 28+ existing golden projects remain green and byte-identical.
