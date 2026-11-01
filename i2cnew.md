# `i2cnew.md` — How I2C works now

This describes the I2C subsystem as implemented in the tree today. The three earlier documents
(`i2cfix.md` proposal, `i2creview.md` review, `i2cfinal.md` decision) are the argument; this is the
reference.

The one rule everything else follows: **an address is written exactly once, in the `.espf` file.**
Neither `app.rs` nor a driver re-types it.

```text
# peripherals
  <i2c id="i2c0" sda="6" scl="7" frequency="400kHz" />

# components
  <i2c_component id="display_bus" i2c="$i2c0" address="0x3C" />
  <i2c_component id="sensor_bus"  i2c="$i2c0" address="0x62" />

# devices
  <ssd1306 id="oled" i2c_component="$display_bus" />
  <scd4x   id="air"  i2c_component="$sensor_bus" />
```

One controller, two parts, two addresses: that is a legal project.

---

## 1. The two levels

| Element | Section | Means | Emitted as |
|---|---|---|---|
| `<i2c id="i2c0" sda scl frequency>` | `# peripherals` | the hardware controller and its two bus wires | a shared `let i2c0_bus = I2cBus::build(...)` in the `Components::new` prelude |
| `<i2c_component address="0x3C">` | `# components` | an **addressed endpoint** on that controller | field `I2cDevice<Rt>`, init `I2cDevice::new(i2c0_bus, 60)` |
| `<i2c_component>` (no `address`) | `# components` | the **raw bus** (a scanner) | field `I2cBus<Rt>`, init `i2c0_bus` |

`<i2c>` has no address: it is physical wiring and timing, not a device. `address` belongs on the
component because that is where the endpoint is selected — the same split SPI already had between
`<spi>` (SCLK/MOSI/MISO) and `<spi_component cs="..">`.

### Rules

- Any number of `<i2c_component>`s may share one `<i2c>`.
- Two **addressed** components on the same `<i2c>` with the same address: `E0708`, an error.
- The same address on **different** `<i2c>`s: fine.
- Unaddressed components never conflict — with anything, including each other. They are handles to
  the same controller.
- A controller with no component is never built (no `let` is emitted).
- Address range: `0x08..=0x77`. The I2C specification reserves `0x00..=0x07` (general call, CBUS /
  high-speed codes) and `0x78..=0x7F` (10-bit prefixes and reserved), so any other value is `E0603`.
- 7-bit addressing only; there is no 10-bit mode.

---

## 2. Diagnostics

| Code | Constant | When | Message |
|---|---|---|---|
| `E0708` (new) | `CONFLICT_I2C_ADDRESS` | two addressed components on one `<i2c>` share an address | `i2c address conflict` |
| `E0603` (reused) | `COMPONENT_INVALID_I2C` | `address` outside `0x08..=0x77`, or a value that is not a byte (`0xZZ`, `0x100`) | `invalid i2c_component` |
| `E0907` (reused) | `DEVICE_INVALID` | a device needs an address and its component has none, or the part is fixed and the component declares another | `<kind> needs an i2c address` / `<kind> address mismatch` |
| `E0611` | — | **retired.** Multi-drop is legal. The number is deleted and must never be reused. | |

Actual wording:

```text
error: i2c address conflict
  <i2c_component id="display"> and <i2c_component id="other"> both use address 0x3C on <i2c id="i2c0">

error: invalid i2c_component
  <i2c_component id="x"> address="0x7F" is reserved by the I2C specification; usable addresses are
  0x08..=0x77

error: display needs an i2c address
  <ssd1306 id="oled"> needs <i2c_component id="oled_bus"> to declare an `address`; it is a bus-only
  component

error: sensor needs an i2c address
  <scd4x id="air"> needs <i2c_component id="oled_bus"> to declare an `address` (this part is at
  0x62); it is a bus-only component

error: sensor address mismatch
  <scd4x id="air"> is fixed at address 0x62, but <i2c_component id="oled_bus"> declares 0x48
```

`address="0x100"` never reaches the range check: the generated `hex` reader fails first and reports
`invalid hex address`.

---

## 3. Generated code

Two endpoints on one controller:

```rust
pub struct Components {
    pub display_bus: espforge_runtime::I2cDevice<esp_hal::Blocking>,
    pub sensor_bus: espforge_runtime::I2cDevice<esp_hal::Blocking>,
}
impl Components {
    pub fn new(registry: &mut PeripheralRegistry) -> Self {
        // SAFETY: espforge checked that no other component uses this
        // peripheral or GPIO6/GPIO7.
        let i2c0_bus = espforge_runtime::I2cBus::build(
                unsafe { registry.peripherals.I2C0.clone_unchecked() },
                unsafe { registry.peripherals.GPIO6.clone_unchecked() },
                unsafe { registry.peripherals.GPIO7.clone_unchecked() },
                espforge_runtime::I2cConfig {
                    frequency: esp_hal::time::Rate::from_khz(400),
                },
            )
            .expect("I2C bus configuration failed");
        Self {
            display_bus: espforge_runtime::I2cDevice::new(i2c0_bus, 60),
            sensor_bus: espforge_runtime::I2cDevice::new(i2c0_bus, 98),
        }
    }
}
```

The address is emitted as an unsuffixed decimal literal: `0x3C` prints as `60`, `0x62` as `98`.

A bus-level component emits the raw handle instead:

```rust
pub struct Components {
    pub scanner: espforge_runtime::I2cBus<esp_hal::Blocking>,
}
impl Components {
    pub fn new(registry: &mut PeripheralRegistry) -> Self {
        // SAFETY: ...
        let i2c0_bus = espforge_runtime::I2cBus::build(/* ... */)
            .expect("I2C bus configuration failed");
        Self { scanner: i2c0_bus }
    }
}
```

The controller is built **once** per `<i2c>`, by whichever component emits first; later components
reuse the same `let`. This is `EmitCx::once`, the same mechanism `<spi_component>` uses.

---

## 4. Runtime (`espforge-runtime/src/hw/i2c.rs`)

### `I2cBus<Dm>` — unchanged, raw

The address-taking bus. Public because the scanner needs it. Blocking methods are inherent
(`write(addr, ..)`, `read(addr, ..)`, `write_read(addr, ..)`, `transaction(addr, ..)`); async methods
are the `async` forms under the `embassy` feature. It also implements `embedded_hal::i2c::I2c` /
`embedded_hal_async::i2c::I2c` for its mode.

### `I2cDevice<Dm>` — new, addressed

```rust
pub struct I2cDevice<Dm: DriverMode + 'static> { bus: I2cBus<Dm>, address: u8 }

impl<Dm: DriverMode + 'static> I2cDevice<Dm> {
    pub fn new(bus: I2cBus<Dm>, address: u8) -> Self;
    pub fn address(&self) -> u8;        // the 7-bit address from .espf
    pub fn bus(&self) -> I2cBus<Dm>;    // escape hatch: the raw, unaddressed bus
}
```

`Copy`/`Clone` are hand-written, matching `I2cBus` (a derive would add an unwanted `Dm: Copy`
bound).

Both modes have inherent curried operations that take no address:

| Blocking | Async (`embassy`) |
|---|---|
| `write(&self, &[u8])` | `write(&self, &[u8]).await` |
| `read(&self, &mut [u8])` | `read(&self, &mut [u8]).await` |
| `write_read(&self, &[u8], &mut [u8])` | `write_read(..).await` |
| `transaction(&self, &mut [Operation])` | `transaction(..).await` |

They forward to the bus with `self.address`. Every one takes `&self`, so a `Copy` handle can be held
by an app and by a driver.

### The trait contract (important)

`I2cDevice` implements `embedded_hal::i2c::I2c` (blocking) and `embedded_hal_async::i2c::I2c`
(async) by overriding **only** `transaction`; the traits' provided `read`/`write`/`write_read` route
through it.

> **The address a driver passes is ignored.** This handle always talks to the address configured in
> `.espf`.

That is what lets a third-party driver take the handle unchanged — the `ssd1306` crate calls
`I2CDisplayInterface::new(handle)` and passes its own default `0x3C` into every transfer, and a panel
declared at `0x3D` still works because the configured address wins. The flip side: one `I2cDevice`
drives exactly one part; a driver that multiplexes several addresses through one handle will
silently talk to the configured one. Use `I2cBus` for that.

---

## 5. Drivers (`espforge-drivers`)

Both I2C drivers take `I2cDevice<Blocking>` and contain no address constant.

| Driver | Constructor | Address |
|---|---|---|
| `espforge_drivers::Scd4x` | `new(device: I2cDevice<Blocking>)` | fixed `0x62`, declared once in `drivers.schema.kdl` |
| `espforge_drivers::Ssd1306Display<SIZE>` | `new(device: I2cDevice<Blocking>, size, rotation)` | the component's — `0x3C` and `0x3D` both work |

```rust
let air = device!(ctx, air);
air.start().ok();                   // command to 0x62
let m = air.read().ok();            // write_read at 0x62
```

`scd4x` no longer exports `pub const ADDRESS`: the fixed address lives in the schema, which is the
layer that checks it. That is a net gain — a component wired to `0x48` is now a lowering error
instead of a bus timeout.

### How a device declares its need

A `device_recipe` in `drivers.schema.kdl` may carry `needs_address`:

```kdl
needs_address          // any usable address; the .espf decides   (ssd1306)
needs_address "0x62"   // the part's fixed address                 (scd4x)
// omitted              // the device takes whatever handle it is given (ili9341, SPI)
```

`needs_address` becomes `espforge_schema::AddressNeed` (`None` / `Any` / `Fixed(u8)`) on
`DeviceRecipe`, validated at schema load (hex byte in `0x08..=0x77`, given once). At lowering,
`device_recipe.rs` compares it against `Component::address()` and reports `E0907` for a missing
address or a disagreeing fixed one. This is the check that stops the compiler emitting `Scd4x::new`
with an `I2cBus` — otherwise `rustc` would be the first to notice.

---

## 6. Application code

Addressed component — no address in Rust, it is already in `.espf`:

```rust
let i2c = component!(ctx, i2c_tester);        // &I2cDevice<Blocking>
i2c.write_read(&[sent_byte], &mut buffer);    // to 0x42
```

Async is the same with `.await` on the handle's own methods.

Scanner — the component has no address, so the field is the raw bus and the caller addresses each
slave:

```rust
let i2c = component!(ctx, i2c0);              // &I2cBus<Blocking>
let logger = ctx.logger;
for address in 1..127 {
    if i2c.write(address, &[]).is_ok() {
        logger.info(format_args!("Found device at 0x{address:02x}"));
    }
}
```

Rare cases: `I2cDevice::address()` reads the configured address; `I2cDevice::bus()` drops back to
the raw handle.

---

## 7. Examples in the tree

| Example | `.espf` | App |
|---|---|---|
| `04.Communication/i2c_example` | `address="0x42"` on `i2c_tester` | `i2c.write_read(&[sent_byte], &mut buffer)` |
| `04.Communication/i2c_example_async` | `address="0x42"` | the same, `.await` |
| `04.Communication/i2c_scanner` | no `address` | `i2c.write(address, &[])` |
| `06.Displays/ssd1306` | `address="0x3C"` | `device!(ctx, oled)` |

---

## 8. Tests that pin this

- `espforge-elements/src/lower.rs`: two endpoints on one bus lower; one address twice is `E0708`
  (label names both ids and the bus); one address on two controllers lowers; `0x00`/`0x07`/`0x78`/
  `0x7F` are `E0603` while `0x08`/`0x77` lower; an unaddressed endpoint beside an addressed one
  lowers; display or sensor on a bus-only component is `E0907`; `scd4x` on `0x48` is `E0907`
  naming `0x62`; `ssd1306` on `0x3D` lowers; display + sensor on one bus both lower.
- `espforge-elements/src/components/i2c.rs`: two endpoints emit exactly one `let ..._bus` and one
  `SAFETY` block; addressed emits `I2cDevice`, unaddressed emits `I2cBus`.
- `espforge-schema/src/lib.rs`: `needs_address` parses bare and with a value, rejects a reserved
  address, rejects a duplicate; `espforge-elements/src/schema.rs` pins the shipped recipes.
- `espforge/tests/golden/`: 8 cells re-baselined (`i2c_example`, `i2c_example_async`, `i2c_scanner`,
  `ssd1306` × esp32c3/esp32s3). Their diffs are only the prelude move, the field type, the
  initialiser, and the two example files.
- `espforge/tests/generated_projects_compile.rs` (`--ignored`): compiles the scanner (raw `I2cBus`),
  the blocking and embassy examples, and an inline multi-drop spec — a display at `0x3C` and a
  sensor at `0x62` on one controller — for `riscv32imc-unknown-none-elf`. It is the only check that
  takes two `I2cDevice`s on one bus all the way through `rustc`.

---

## 9. Migrating old `.espf` projects

1. If the project talked to a device, add `address=".."` to its `<i2c_component>`. Without one the
   field is still an `I2cBus`, and existing address-passing code keeps compiling unchanged.
2. With the address added, drop the address argument from the app (`write_read(0x42, ..)` →
   `write_read(..)`). Anything still passing one gets a `rustc` arity error, not a silent wrong
   device.
3. A project that needs the raw bus as well (a scanner next to a sensor) declares a second,
   unaddressed component on the same `<i2c>`; both are `Copy` handles to one controller.
4. Error code `E0611` is gone. Nothing needs to be written to replace it: several components on one
   bus are now simply legal.

---

## 10. Known limitations

- **Two I2C controllers in one project panic at runtime.** `I2cBus::build` keeps its
  `static CELL: StaticCell<..>` inside a generic function, and a `static` in a generic function
  exists once for all monomorphizations, so `i2c0` + `i2c1` hits "already initialized" at the second
  `.init()`. Lowering allows both ids; the compile tests cannot see it. Not introduced by this
  change (it was already true), and not fixed here — the fix is for the generated code to own the
  cell and pass a `&'static` in.
- 10-bit addressing is not supported; the model is 7-bit end to end.
- There is no bus multiplexer or address-translation story, which is why a fixed-address mismatch is
  an error rather than a remap.
- `I2cDevice` ignoring the caller-supplied address is deliberate and documented on the impl; it is
  the price of handing a third-party driver a pre-addressed handle.
