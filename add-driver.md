Here is a comprehensive, production-ready developer guide for `add-device-driver.md`. You can commit this directly to your repository documentation.

---

# `add-device-driver.md`

```markdown
# Adding a New Device Driver to espforge

This guide details the complete end-to-end workflow for adding a new device driver to `espforge`.

A device in `espforge` represents a specific chip or peripheral module (e.g., OLED displays, TFT screens, environmental sensors, IMUs, RTCs) declared in the `# devices` section of an `.espf` file.

---

## 1. Architectural Overview

Adding a device touches four workspace crates in sequence:

```
1. espforge-drivers   --> Implements the driver runtime (registers, bus I/O, blocking/async).
2. espforge-schema    --> Defines the KDL schema, attributes, and examples for <device>.
3. espforge-elements  --> Validates AST attributes and lowers them into the Board IR.
4. espforge-emit      --> Emits the driver feature in the generated Cargo.toml.
```

---

## 2. Directory Structure in `espforge-drivers`

Drivers in `espforge-drivers` follow a **Modern Hybrid Layout** (using Rust's 2018+ module system without `mod.rs` files).

Drivers are divided into two implementation tiers:

### Tier 1: Thin Facade (Wrapping a crates.io driver)
Use this when a robust, well-maintained `no_std` driver already exists on crates.io (e.g., `ssd1306`). The driver is a single flat file directly under `src/`:

```
espforge-drivers/src/
├── lib.rs
└── ssd1306.rs           <-- Single flat wrapper (~60-100 lines)
```

### Tier 2: Native Driver (Multi-file module)
Use this when crates.io lacks a driver, the existing crate lacks `embedded-hal` 1.0 / `embedded-hal-async` support, or you want full control over registers and commands. The driver has a root entry file and a sibling folder for internal submodules:

```
espforge-drivers/src/
├── lib.rs
├── scd4x.rs             <-- Module entry point: re-exports public types
└── scd4x/               <-- Private implementation submodules
    ├── registers.rs     <-- Register addresses, masks, bitfields
    ├── commands.rs      <-- Command opcodes, pure CRC/math, packet encoders
    ├── blocking.rs      <-- Blocking implementation (I2cBus<Blocking>)
    └── asynchronous.rs  <-- Async implementation (I2cBus<Async> + .await)
```

> **Naming Rule:** Always name the async implementation file `asynchronous.rs` rather than `async.rs`, as `async` is a reserved Rust keyword requiring awkward `r#async` escapes.

---

## 3. Step-by-Step Implementation Guide

Using an I2C sensor (`scd4x`) as an example, follow these five steps.

---

### Step 1: Implement the Driver in `espforge-drivers`

#### 1.1 `Cargo.toml`
Add an optional Cargo feature for your device so that projects only compile the code and dependencies they need:

```toml
# espforge-drivers/Cargo.toml
[features]
default = []
scd4x = [] # add external dependencies with "dep:..." if wrapping a crate
```

#### 1.2 `src/scd4x/registers.rs` (Pure Constants & Bitfields)
Keep all register addresses and raw bit definitions here. No I/O or HAL traits.

```rust
// espforge-drivers/src/scd4x/registers.rs
pub const CMD_START_PERIODIC: [u8; 2] = [0x21, 0xb1];
pub const CMD_READ_MEASUREMENT: [u8; 2] = [0xec, 0x05];
pub const CMD_STOP_PERIODIC: [u8; 2] = [0x3f, 0x86];
```

#### 1.3 `src/scd4x/commands.rs` (Pure Protocol & Conversions)
Put all checksum calculations, payload serialization, and integer-to-engineering-unit conversions here. **This file should have 0% I/O dependencies and must be unit-testable on your host machine.**

```rust
// espforge-drivers/src/scd4x/commands.rs
use super::registers::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub co2_ppm: u16,
    pub temperature_c: f32,
    pub relative_humidity_percent: f32,
}

pub fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0xff;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ 0x31;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

pub fn parse_measurement(buf: &[u8; 9]) -> Result<Measurement, ()> {
    if crc8(&buf[0..2]) != buf[2] 
        || crc8(&buf[3..5]) != buf[5] 
        || crc8(&buf[6..8]) != buf[8] 
    {
        return Err(());
    }

    let raw_co2 = u16::from_be_bytes([buf[0], buf[1]]);
    let raw_temp = u16::from_be_bytes([buf[3], buf[4]]);
    let raw_humid = u16::from_be_bytes([buf[6], buf[7]]);

    Ok(Measurement {
        co2_ppm: raw_co2,
        temperature_c: -45.0 + 175.0 * (raw_temp as f32 / 65535.0),
        relative_humidity_percent: 100.0 * (raw_humid as f32 / 65535.0),
    })
}
```

#### 1.4 `src/scd4x/blocking.rs`
Handles synchronous communication using `espforge_runtime::I2cBus<Blocking>`:

```rust
// espforge-drivers/src/scd4x/blocking.rs
use esp_hal::Blocking;
use espforge_runtime::I2cBus;
use super::commands::{parse_measurement, Measurement};
use super::registers::*;

pub struct Scd4x {
    bus: I2cBus<Blocking>,
    address: u8,
}

impl Scd4x {
    pub fn new(bus: I2cBus<Blocking>, address: u8) -> Self {
        Self { bus, address }
    }

    pub fn start_periodic_measurement(&mut self) -> Result<(), ()> {
        self.bus.write(self.address, &CMD_START_PERIODIC).map_err(|_| ())
    }

    pub fn read_measurement(&mut self) -> Result<Measurement, ()> {
        let mut buf = [0u8; 9];
        self.bus.write_read(self.address, &CMD_READ_MEASUREMENT, &mut buf).map_err(|_| ())?;
        parse_measurement(&buf)
    }
}
```

#### 1.5 `src/scd4x/asynchronous.rs`
Handles asynchronous communication using `espforge_runtime::I2cBus<Async>` and `.await`:

```rust
// espforge-drivers/src/scd4x/asynchronous.rs
use esp_hal::Async;
use espforge_runtime::I2cBus;
use super::commands::{parse_measurement, Measurement};
use super::registers::*;

pub struct Scd4xAsync {
    bus: I2cBus<Async>,
    address: u8,
}

impl Scd4xAsync {
    pub fn new(bus: I2cBus<Async>, address: u8) -> Self {
        Self { bus, address }
    }

    pub async fn start_periodic_measurement(&mut self) -> Result<(), ()> {
        self.bus.write(self.address, &CMD_START_PERIODIC).await.map_err(|_| ())
    }

    pub async fn read_measurement(&mut self) -> Result<Measurement, ()> {
        let mut buf = [0u8; 9];
        self.bus.write_read(self.address, &CMD_READ_MEASUREMENT, &mut buf).await.map_err(|_| ())?;
        parse_measurement(&buf)
    }
}
```

#### 1.6 `src/scd4x.rs` (Module Root)
Ties the submodules together and exposes types:

```rust
// espforge-drivers/src/scd4x.rs
pub mod commands;
pub mod registers;

mod blocking;
pub use blocking::Scd4x;

#[cfg(feature = "embassy")]
mod asynchronous;
#[cfg(feature = "embassy")]
pub use asynchronous::Scd4xAsync;

pub use commands::Measurement;
```

#### 1.7 `src/lib.rs`
Declare and re-export the driver in the crate root:

```rust
// espforge-drivers/src/lib.rs
#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "scd4x"
))]
pub mod scd4x;

#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "scd4x"
))]
pub use scd4x::*;
```

---

### Step 2: Declare Schema in `espforge-schema`

Edit `espforge-schema/src/espf.schema.kdl` to define the element in the `devices` section:

```kdl
element "scd4x" section="devices" {
    summary "Sensirion SCD40/SCD41 CO2, temperature, and humidity sensor"
    description "Connects to an I2C component bus and provides environmental readings."
    attr "id" kind="ident" required=#true
    attr "i2c_component" kind="ref" to="i2c_component" required=#true {
        help "The I2C bus component this sensor is connected to."
    }
    example "minimal" #"<scd4x id="air_sensor" i2c_component="$i2c_bus" />"#
    see_also "components/i2c_component"
}
```

Running `cargo build` in `espforge-elements` will run its `build.rs`, automatically generating `espforge_elements::generated::scd4x::Attrs`.

---

### Step 3: Implement AST Lowering in `espforge-elements`

#### 3.1 Create `espforge-elements/src/devices/scd4x.rs`
Implement the `Element` trait and register it into the distributed slice:

```rust
// espforge-elements/src/devices/scd4x.rs
use espf::context::ParsedNode;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::devices::{invalid_element, invalid_id, unknown_component};
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::scd4x::Attrs;
use crate::model::{Board, Component, Device, Ident, Runtime};
use crate::registry::ELEMENTS;

pub struct Scd4xElement;

#[distributed_slice(ELEMENTS)]
static SCD4X: &dyn Element = &Scd4xElement;

impl Element for Scd4xElement {
    fn tag(&self) -> &'static str {
        "scd4x"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| invalid_element(cx, node, "scd4x", issue))?;
        let id = cx.ident(node);

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "scd4x", &id));
        }

        let component_name = attrs.i2c_component.name;
        let Some(component) = cx.components().iter().find(|c| c.id().raw == component_name) else {
            return Err(unknown_component(cx, node, "scd4x", &id, "i2c_component", "i2c_component"));
        };

        let Component::I2c(_) = component else {
            return Err(unknown_component(cx, node, "scd4x", &id, "i2c_component", "i2c_component"));
        };

        let i2c_component = Ident::new(component_name, crate::devices::span_of(node, "i2c_component"));

        cx.push_device(Device::Scd4x(Scd4xDevice {
            id,
            i2c_component,
        }));
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scd4xDevice {
    pub id: Ident,
    pub i2c_component: Ident,
}

impl Lowered for Scd4xDevice {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let component = format_ident!("{}", self.i2c_component.rust_name());
        
        let driver_ty = match board.meta.runtime {
            Runtime::Blocking => quote! { espforge_drivers::Scd4x },
            Runtime::Embassy => quote! { espforge_drivers::Scd4xAsync },
        };

        cx.device_field(quote! {
            pub #name: #driver_ty,
        });

        cx.device_init(quote! {
            #name: #driver_ty::new(
                components.#component,
                0x62, // SCD4x fixed standard I2C address
            ),
        });
    }
}
```

#### 3.2 Update `espforge-elements/src/devices/mod.rs`
Register the module:
```rust
// espforge-elements/src/devices/mod.rs
pub mod ili9341;
pub mod scd4x;      // <-- Add this
pub mod ssd1306;
```

#### 3.3 Update `espforge-elements/src/model.rs`
Add `Scd4x` to `device_enum!`:

```rust
// espforge-elements/src/model.rs
device_enum! {
    Ssd1306(crate::devices::ssd1306::Ssd1306Device),
    Ili9341(crate::devices::ili9341::Ili9341Device),
    Scd4x(crate::devices::scd4x::Scd4xDevice), // <-- Add this
}

impl Board {
    pub fn uses_scd4x(&self) -> bool {
        self.devices.iter().any(|d| matches!(d, Device::Scd4x(_)))
    }
}
```

---

### Step 4: Emit Cargo Dependencies in `espforge-emit`

Update `espforge-emit/src/codegen/cargo.rs` to propagate the device's feature flag into generated firmware projects.

```rust
// espforge-emit/src/codegen/cargo.rs

// In struct Displays or a unified Devices struct:
pub struct Devices {
    pub ssd1306: bool,
    pub ili9341: bool,
    pub scd4x: bool, // <-- Add this
}

// In drivers_dependency():
fn drivers_dependency(source: &RuntimeSource, devices: Devices) -> Option<Value> {
    if !devices.ssd1306 && !devices.ili9341 && !devices.scd4x {
        return None;
    }
    ...
    let mut features: Vec<&str> = Vec::new();
    if devices.ssd1306 { features.push("ssd1306"); }
    if devices.ili9341 { features.push("ili9341"); }
    if devices.scd4x   { features.push("scd4x"); }
    table.insert("features", Value::Array(array(&features)));
    Some(Value::InlineTable(table))
}
```

And in `espforge-emit/src/codegen/mod.rs`:
```rust
// In generate():
cargo::Devices {
    ssd1306: board.uses_ssd1306(),
    ili9341: board.uses_ili9341(),
    scd4x: board.uses_scd4x(), // <-- Add this
}
```

---

### Step 5: Create an Example in `espforge-examples`

Create `espforge-examples/src/examples/07.Sensors/scd4x/`:

#### `example.espf.txt`
```
==================
SCD4X SENSOR EXAMPLE
==================

# espforge
  <project name="scd4x_example" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <i2c id="i2c0" sda="6" scl="7" frequency="100kHz" />

# components
  <i2c_component id="sensor_bus" i2c="$i2c0" address="0x62" />

# devices
  <scd4x id="air_sensor" i2c_component="$sensor_bus" />

# services
```

#### `app/rust/app.rs`
```rust
use crate::Context;

pub fn setup(ctx: &mut Context) {
    let sensor = device!(ctx, air_sensor);
    sensor.start_periodic_measurement().ok();
    ctx.logger.info("SCD4x measurement started.");
}

pub fn forever(ctx: &mut Context) {
    ctx.delay.delay_ms(5000);
    let sensor = device!(ctx, air_sensor);
    if let Ok(m) = sensor.read_measurement() {
        ctx.logger.info(format_args!(
            "CO2: {} ppm | Temp: {:.1} °C | Humidity: {:.1} %",
            m.co2_ppm, m.temperature_c, m.relative_humidity_percent
        ));
    }
}
```

#### `example.toml`
```toml
chips    = ["esp32c3", "esp32s3"]
runtimes = ["blocking", "embassy"]
```

---

## 4. Verification Checklist

Run these commands to verify the integration:

```bash
# 1. Format and check all workspace crates
cargo fmt --check
cargo check --workspace

# 2. Run schema and unit tests
cargo test -p espforge-schema
cargo test -p espforge-elements
cargo test -p espforge-drivers

# 3. Test code generation from the new example
cargo run -p espforge -- gen --example 07.Sensors/scd4x --out /tmp/scd4x-test

# 4. Verify that the generated project compiles for RISC-V target
cd /tmp/scd4x-test
cargo check --target riscv32imc-unknown-none-elf
```
```
