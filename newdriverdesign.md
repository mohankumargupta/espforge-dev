# `newdriverdesign.md`: Single-Crate Device Driver Architecture

> **Design and Implementation Specification**
> **Status:** Implemented (see §4–§6 for what shipped; §1–§3 are the motivation,
> unchanged from the approved draft)
> **Goal:** Eliminate cross-crate coupling when adding device drivers. Make adding
> a new device driver a **single-crate task** isolated strictly to `espforge-drivers`.

---

## 1. Problem Statement & Motivation

The previous design detailed in `add-driver.md` required a contributor to touch **four separate crates** across ten distinct files to add a single device driver (such as a sensor or display):

```
1. espforge-drivers   --> Driver logic, Cargo.toml feature, lib.rs export
2. espforge-schema    --> espf.schema.kdl element block
3. espforge-elements  --> src/devices/<name>.rs, src/devices/mod.rs, src/model.rs (device_enum! & uses_* predicate)
4. espforge-emit      --> src/codegen/cargo.rs (struct Displays / Devices, feature list), src/codegen/mod.rs
```

### Root Causes of the Coupling
1. **Rigid Dependency Structures in `espforge-emit`**: `codegen/cargo.rs` hardcoded a manual struct (`struct Displays { pub ssd1306: bool, pub ili9341: bool }`) and manually mapped boolean flags to cargo features. Every new device forced code changes into the project emission layer.
2. **Exhaustive Enum Variants in `espforge-elements`**: `model.rs` required updating a closed `device_enum!` macro and defining per-device predicate methods (e.g., `Board::uses_scd4x()`) for every peripheral chip.
3. **Monolithic Central Schema**: All elements were centralized in `espforge-schema/src/espf.schema.kdl`, decoupling driver specifications from driver implementations.

---

## 2. Target Workflow: The 1-Crate Experience

Adding any new driver (e.g. Sensirion `scd4x`) touches **only `espforge-drivers`**:

```
espforge-drivers/
├── Cargo.toml            <-- 1. Declare optional feature: scd4x = []
├── drivers.schema.kdl    <-- 2. Declare the element and its device_recipe
├── src/
│   ├── lib.rs            <-- 3. Re-export module: #[cfg(feature = "scd4x")] pub mod scd4x;
│   └── scd4x.rs          <-- 4. Implement the driver
```

**Zero files modified in compiler crates:**
- 0 edits in `espforge-elements`
- 0 edits in `espforge-emit`
- 0 edits in `espforge-schema`
- 0 edits in `espforge-core`

This is true *after* the migration described below; the migration itself edited
all of them once, to install the generic tier.

---

## 3. Architectural Components

```
┌────────────────────────────────────────────────────────┐
│                   espforge-drivers                     │
│  - drivers.schema.kdl (device elements + recipes)      │
│  - DRIVERS_SCHEMA_KDL (the same file, embedded)        │
│  - Driver implementation (no_std, embedded-hal 1.0)    │
│  - Cargo.toml (Feature per driver)                     │
└──────────────────────────┬─────────────────────────────┘
                           │ composed at compile time
┌──────────────────────────▼─────────────────────────────┐
│                   espforge-schema                      │
│  - Owns espf.schema.kdl; `load_all` merges fragments   │
│  - Parses and validates `device_recipe`                │
└──────────────────────────┬─────────────────────────────┘
                           │ lowers via generic interpreter
┌──────────────────────────▼─────────────────────────────┐
│                  espforge-elements                     │
│  - src/schema.rs composes core + drivers schema        │
│  - Generic DeviceRecipeElement (one interpreter)       │
│  - Device struct (data, not an enum of variants)       │
│  - Collects driver features and deps dynamically       │
└──────────────────────────┬─────────────────────────────┘
                           │ emitted dynamically
┌──────────────────────────▼─────────────────────────────┐
│                    espforge-emit                       │
│  - Dynamic Cargo.toml feature generation               │
│  - Devices struct emitted from the Device data         │
└────────────────────────────────────────────────────────┘
```

---

## 4. Detailed Implementation Specification

### 4.1 Schema Co-Location (`espforge-drivers/drivers.schema.kdl`)

The `# devices` elements live beside the drivers they describe. The `devices`
*section* stays in `espf.schema.kdl` (a fragment names the section it lives in).

`espforge-drivers/src/lib.rs` embeds its own file:

```rust
pub const DRIVERS_SCHEMA_KDL: &str = include_str!("../drivers.schema.kdl");
```

The constant lives in the crate whose package contains the file, so
`cargo package`/`cargo publish` works for both crates and no crate reaches into
a sibling checkout's directory.

### 4.2 Schema Composition (`espforge-schema`)

`espforge-schema` gains `load_all`, which parses several sources into one
`schema` and runs the cross-source checks (tag uniqueness, `to=` and
`see_also` targets) once at the end:

```rust
pub fn load_all(sources: &[&str]) -> Result<Schema, Vec<SchemaError>>
```

Composing is the *consumer's* job, in one place —
`espforge-elements/src/schema.rs`:

```rust
pub fn schema() -> &'static Schema {
    static SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
        espforge_schema::load_all(&[
            espforge_schema::SCHEMA_KDL,
            espforge_drivers::DRIVERS_SCHEMA_KDL,
        ])
        .unwrap()
    });
    &SCHEMA
}
```

`espforge-elements/build.rs` composes the same pair, so the generated `Attrs`
structs cover device elements too — and a `validate(tag, node)` dispatcher
generated next to them lets the generic tier use those typed readers without
knowing any tag at compile time.

### 4.3 `device_recipe`: typed slots, not a constructor template

A recipe is data. Each `arg` either maps an ESPF spelling to the Rust
expression it emits, or templates a claimed pin, so the lowerer never parses a
Rust expression to fill a placeholder.

```kdl
element "ssd1306" section="devices" {
    summary "SSD1306 monochrome OLED display"
    device_recipe {
        bus "i2c_component"      # the attribute naming the component it binds to
        feature "ssd1306"        # the espforge-drivers feature the project names
        kind "display"           # the word the blocking diagnostic uses
        type "espforge_drivers::Ssd1306Display<{size}>"
        new "espforge_drivers::Ssd1306Display::new"
        deps "embedded-graphics" # crates the project must name because of it
        arg "size" {
            map "128x64" "espforge_drivers::Oled128x64"
            map "128x32" "espforge_drivers::Oled128x32"
        }
        arg "rotation" {
            map "0" "espforge_drivers::DisplayRotation::Rotate0"
            map "90" "espforge_drivers::DisplayRotation::Rotate90"
        }
    }
    attr "id" kind="ident" required=#true
    attr "i2c_component" kind="ref" to="i2c_component" required=#true
    attr "size" kind="choice" default="128x64" { choices "128x64" "128x32" }
    attr "rotation" kind="choice" default="0" { choices "0" "90" "180" "270" }
    example "minimal" #"<ssd1306 id="oled" i2c_component="$display_bus" />"#
}
```

A driver with auxiliary lines declares `pin` slots and, when it consumes the
bus handle, two more facts:

```kdl
device_recipe {
    bus "spi_component"
    feature "ili9341"
    type "espforge_drivers::Ili9341Display"
    new "espforge_drivers::Ili9341Display::new"
    takes_bus
    taken "the display's SPI bus was already taken"
    deps "embedded-graphics"
    arg "dc" pin="esp_hal::gpio::Output::new(unsafe { registry.peripherals.GPIO{n}.clone_unchecked() }, esp_hal::gpio::Level::Low, esp_hal::gpio::OutputConfig::default())" direction="output"
    arg "backlight" pin="…" direction="output"
}
```

Rules (all checked at schema load time, with the offending node's span):

1. `bus` must name a `ref` attribute of the same element; the component tag it
   accepts is that attribute's own `to=` — there is no separate `expect` field
   to drift.
2. Every attribute except `id` and `bus` must have exactly one `arg`; an `arg`
   must name a declared attribute.
3. A `map` slot reads a `choice` or `bool` attribute and maps **every** value
   the schema allows, and nothing else. A spelling without a mapping is a load
   error, so no unmapped value can reach emission.
4. A `pin` slot reads a `ref` attribute (normally `to="gpio"`) and its template
   must contain `{n}`; the claimed pin number fills it. Whether the slot is
   optional comes from the attribute's own `required=`.
5. Every `{slot}` in `type` names an `arg`. Slot order is `arg` order, which is
   the constructor's argument order.
6. `new` must be a Rust path (`a::b::new`).

### 4.4 Generic Device Lowering (`espforge-elements`)

`src/devices/recipe.rs` is one interpreter for every recipe, with no tag in it.
It runs the same steps the two hand-written elements did:

1. `generated::validate(tag, node)` — the typed, schema-generated `Attrs`
   readers still own presence and kind.
2. The id must become a Rust identifier (`DEVICE_INVALID_ID`).
3. The runtime must be blocking (`DEVICE_REQUIRES_BLOCKING`, with the recipe's
   `kind` word).
4. The bus reference must name a component whose `Component::tag()` equals the
   recipe's `expect`, so a display on an SPI component is refused instead of
   emitting code that does not compile.
5. When `takes_bus`, a second device on the same component is refused.
6. Auxiliary pins are resolved through `<gpio>`, direction-checked, and claimed
   as `OwnerKind::Device(tag)`; an absent optional pin is not claimed.
7. Arguments are resolved in `arg` order: a mapped slot is the expression its
   spelling carries, a pin slot its filled template (wrapped in `Some(..)` when
   the attribute is optional, `None` when it is absent).
8. The field type is the recipe's `type` with its slots filled.

The result is the `Device` struct in `src/model.rs` — data, not an enum of
hand-written variants:

```rust
pub struct Device {
    pub id: Ident,
    pub tag: &'static str,
    pub bus_component: Ident,
    pub feature: String,
    pub deps: Vec<String>,
    pub ty: String,
    pub new: String,
    pub takes_bus: bool,
    pub taken: Option<String>,
    pub pins: Vec<(String, ClaimedPin)>,
    pub args: Vec<String>,
}
```

`Lowered for Device` emits the `Devices` field, the initialiser (taking the bus
out of `Components` when `takes_bus`) and a `SAFETY:` note naming the pins it
claimed — the same note the hand-written ILI9341 emitted.

### 4.5 Dynamic Features and Dependencies (`espforge-emit`)

`struct Displays` is gone. The board answers for itself, in declaration order
and without duplicates:

```rust
impl Board {
    pub fn driver_features(&self) -> Vec<&str>;
    pub fn driver_deps(&self) -> Vec<&str>;
    pub fn takes_component(&self, id: &Ident) -> bool;
}
```

`codegen::cargo::manifest` takes `&Board`; `drivers_dependency` asks it for the
features, and `embedded-graphics` is added only when a driver's recipe names it
— a sensor project never pulls the graphics crate.

### 4.6 Registry: a third door, not a second mechanism

`registry.rs` gains `device_recipes()` beside `recipes()`, and `element()` falls
through to it. The existing coherence test is extended by one predicate
(`element.recipe.is_some() || element.device_recipe.is_some()`), so an element
still has exactly one implementation: a hand-written `Element`, a component
`recipe`, or a `device_recipe` — never two, never none.

### 4.7 Escape hatch

`#[distributed_slice(ELEMENTS)]` remains the path for a driver a recipe cannot
express (async init, DMA, multi-bus parts). It builds the same `Device` struct
itself, so its `Devices` wiring is identical. The honest end state is "simple
drivers are schema-only; complex ones are one Rust file plus a schema block" —
both are one crate.

---

## 5. Walkthrough: Adding a New Device (`scd4x`, as shipped)

### Step 1: feature and code in `espforge-drivers`

1. `Cargo.toml`: `scd4x = []`.
2. `src/scd4x.rs`: the driver, over `espforge_runtime::I2cBus<Blocking>`.
3. `src/lib.rs`:

```rust
#[cfg(all(any(target_arch = "riscv32", target_arch = "xtensa"), feature = "scd4x"))]
pub mod scd4x;
#[cfg(all(any(target_arch = "riscv32", target_arch = "xtensa"), feature = "scd4x"))]
pub use scd4x::*;
```

### Step 2: the element in `drivers.schema.kdl`

```kdl
element "scd4x" section="devices" {
    summary "Sensirion SCD40/SCD41 CO2, temperature and humidity sensor"
    device_recipe {
        bus "i2c_component"
        feature "scd4x"
        kind "sensor"
        type "espforge_drivers::Scd4x"
        new "espforge_drivers::Scd4x::new"
    }
    attr "id" kind="ident" required=#true
    attr "i2c_component" kind="ref" to="i2c_component" required=#true
    example "minimal" #"<scd4x id="air" i2c_component="$sensor_bus" />"#
}
```

### Done

```bash
cargo test --workspace
```

The pipeline discovers the element, generates its typed `Attrs`, validates and
lowers it, registers it, and emits the `espforge-drivers` feature into the
project's `Cargo.toml`. `<scd4x id="air" i2c_component="$sensor_bus" />` now
works in any project, and its manifest names no `embedded-graphics`.

---

## 6. Verification Checklist

- [x] `espforge-drivers/drivers.schema.kdl` is parsed and composed into the
      pipeline's schema (`espforge-elements/src/schema.rs`).
- [x] `DeviceRecipeElement` validates `id`, resolves the bus component by kind,
      claims auxiliary GPIO lines, and fills typed value slots.
- [x] `espforge-emit` collects driver features and dependencies dynamically;
      `struct Displays` is gone from `cargo.rs`.
- [x] Adding a device requires zero code changes to `espforge-elements`,
      `espforge-emit`, `espforge-schema` or `espforge-core`.
- [x] `cargo test --workspace` green, and the four `06.Displays.*` goldens
      **byte-identical** (including `SAFETY:` comments and feature order).
- [x] `<ssd1306 i2c_component="$spi_bus">` (wrong component kind) is refused
      with the existing diagnostic.
- [x] A second `<ili9341>` on the same `<spi_component>` is refused.
- [x] `rotation="45"` is refused, quoting the allowed values.
- [x] A `<scd4x>` project's `Cargo.toml` contains no `embedded-graphics`.
- [x] `cargo package -p espforge-schema` and `-p espforge-drivers` both carry
      the schema file they embed.
- [x] `espforge-elements/build.rs` generates `Attrs` for device elements too.
- [x] The registry coherence test covers the device-recipe door (a
      `device_recipe` element with a hand-written `.rs` is refused).
- [x] Diagnostic wording stayed byte-identical: `invalid_element` derives its
      "must be …" list from the schema's own `choices`/`bool` kind.
