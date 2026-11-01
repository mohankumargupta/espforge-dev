# i2cfinal.md: addressed I2C, multi-drop, and a scanner that stays a scanner

Final recommendation after `i2cfix.md` (the proposal) and `i2creview.md` (the review).
Read this one; the other two are the argument, this is the decision.

Everything below is checked against the source snapshot in this conversation. Nothing was
compiled. Two claims are marked **verify** because they depend on esp-hal types or borrowck
behaviour I could not run.

---

## 1. Verdict

`i2cfix.md` has the right diagnosis and the right model. `i2creview.md` is mostly right about
what is wrong with the specification. Neither caught everything.

**The model, kept as proposed**

- `<i2c>` is the controller: pins, frequency, no address.
- `<i2c_component>` is an endpoint on it. With `address` it is an addressed `I2cDevice`; without
  one it is the raw `I2cBus` (the scanner). No new element or attribute.
- Several components may share one `<i2c>`. Two on the same `<i2c>` may not share an address.
- Neither `app.rs` nor a driver re-types an address that `.espf` already holds.

### How each review point came out

| # | Review point | Verdict | Decision |
|---|---|---|---|
| 1 | `address` and `Option<u8>` already exist; the doc presents them as new | Valid | Treat as already built (schema and `I2cComponent` need no change) |
| 2 | Conflict check "does not compile" (borrow) | **Overstated** | Two-phase borrows plausibly accept it (**verify**). Collect-then-report anyway: it is the pattern this file already uses |
| 3 | `E0516` is in the peripheral range | Valid | Don't allocate it. Reuse `COMPONENT_INVALID_I2C` (`E0603`) for address errors |
| 4 | Retiring `E0611` touches several sites | Valid | List below (section 5.3) |
| 5 | Keep `used_by` as a dead field | **Disagree** | Remove it from `Bus::I2c`: three small sites, and a dead field is how the next reader gets confused |
| 6 | A device on an unaddressed component generates unbuildable Rust | **Valid, and the most important** | New `needs_address` recipe key (section 6) |
| 7 | `ssd1306`'s `Oled` alias still names `I2cBus` | Valid | Fix with the constructor |
| 8 | `derive(Clone, Copy)` adds a `Dm: Copy` bound | Valid | Hand-written impls, as `I2cBus` does (**verify** whether esp-hal's `Blocking`/`Async` are `Copy`; the manual impls make it irrelevant) |
| 9 | "Valid 7-bit address" is `0x01..=0x7F` | Valid | Enforce `0x08..=0x77`; the rest is reserved by the I2C specification |
| 10 | Deleting `ADDRESS` from `scd4x` loses a cross-check | Valid | The schema declares the part's fixed address; a mismatch is an error (section 6) |
| 11 | `I2cDevice` silently ignores the caller's address | Valid | Document it on the impl; implement only `transaction` and let the trait defaults route |
| 12 | Prelude changes every I2C project | Valid | Re-baseline 8 cells once, with the diff described (section 8) |
| 13 | Shared `static` in generic `I2cBus::build` breaks `i2c0` + `i2c1` | Valid, pre-existing | Out of scope; recorded in section 10 |
| 14 | (cut off) dedupe unaddressed components? | n/a | **No.** One field per component, named by the user |

### What neither document caught

1. **The examples have no address.** `i2c_example` and `i2c_example_async` declare
   `<i2c_component ... />` with no `address`, even though the Wokwi chip answers at `0x42`.
   `i2cfix.md` section 8.1 assumes the `.espf` already says `address="0x42"`. It doesn't, so the
   new `app.rs` would not compile until the `.espf` is edited.
2. **The phase gate is deliberately broken once.** "All earlier goldens stay byte-identical"
   cannot hold here: moving the bus build into a prelude `let` changes generated code. Eight
   cells change, and the commit message must say which and why.
3. **A fixed-address part should say so in the schema.** See section 6.

---

## 2. Final model

| Thing | Meaning | Emitted as |
|---|---|---|
| `<i2c id="i2c0" sda=".." scl="..">` | controller | shared `let i2c0_bus = I2cBus::build(...)` in the `Components::new` prelude, once per used bus |
| `<i2c_component address="0x3C">` | addressed endpoint | field `I2cDevice<Rt>`, init `I2cDevice::new(i2c0_bus, 0x3C)` |
| `<i2c_component>` (no address) | raw bus handle | field `I2cBus<Rt>`, init `i2c0_bus` |

Rules:

- Any number of components per `<i2c>`.
- Two **addressed** components on the same `<i2c>` with the same address: error `E0708`.
- Same address on different `<i2c>`s: fine.
- Unaddressed components never conflict with anything, including each other.
- Address range: `0x08..=0x77`. Anything else is `E0603`.
- A bus with no component is never built (same as SPI today).

---

## 3. Diagnostics

| Code | Constant | When |
|---|---|---|
| `E0708` (new) | `CONFLICT_I2C_ADDRESS` | two addressed components on one `<i2c>` share an address |
| `E0603` (reused) | `COMPONENT_INVALID_I2C` | address outside `0x08..=0x77` |
| `E0907` (reused) | `DEVICE_INVALID` | device needs an address and the component has none, or the addresses disagree |
| `E0611` | retired | multi-drop is legal. **Delete the constant; never reuse the number** |

Edits in `codes.rs`: add `E0708`; widen the `701..=707` literal in
`the_slice_only_defines_the_codes_it_can_reach` to `701..=708`; remove `COMPONENT_I2C_BUS_USED`
(the `601..=622` range stays contiguous, so that test is unaffected).

Messages:

```
error: i2c address conflict
<i2c_component id="display_bus"> and <i2c_component id="other"> both use address 0x3C on <i2c id="i2c0">

error: invalid i2c_component
<i2c_component id="x"> address="0x7F" is reserved by the I2C specification; usable addresses are 0x08..=0x77
```

Note the generated `hex` reader returns `u8`, so `address="0x100"` fails at parse time with the
existing "invalid hex address" message and never reaches the range check. That is fine.

---

## 4. Runtime (`espforge-runtime/src/hw/i2c.rs`)

```rust
/// An addressed endpoint on a shared I2C bus.
///
/// The address comes from `<i2c_component address=..>`. When this handle is
/// used through the `embedded-hal` `I2c` traits, the address a driver passes
/// is ignored and the configured one is used instead.
pub struct I2cDevice<Dm: DriverMode + 'static> {
    bus: I2cBus<Dm>,
    address: u8,
}
impl<Dm: DriverMode + 'static> Clone for I2cDevice<Dm> { fn clone(&self) -> Self { *self } }
impl<Dm: DriverMode + 'static> Copy for I2cDevice<Dm> {}

impl<Dm: DriverMode + 'static> I2cDevice<Dm> {
    pub fn new(bus: I2cBus<Dm>, address: u8) -> Self { Self { bus, address } }
    pub fn address(&self) -> u8 { self.address }
    pub fn bus(&self) -> I2cBus<Dm> { self.bus }
}
```

- Blocking (`not(feature = "embassy")`): inherent `write`, `read`, `write_read`, `transaction`,
  each forwarding to the `I2cBus` method with `self.address`.
- Async (`feature = "embassy"`): the same, `async`.
- `embedded_hal::i2c::ErrorType` for both modes.
- `embedded_hal::i2c::I2c` (blocking) and `embedded_hal_async::i2c::I2c` (async): implement only
  `transaction(&mut self, _address, ops)`, forwarding with `self.address`. The trait's provided
  `write`/`read`/`write_read` already route through it. That is less code than `i2cfix.md`'s four
  overrides and cannot drift.
- `pub use i2c::*` in `hw` already exports it.

`I2cBus` stays unchanged and public: the scanner needs it.

---

## 5. Lowering (`espforge-elements`)

### 5.1 `components/i2c.rs`

Remove the `used_by` check, `used_by()`, `set_used_by()`, the `COMPONENT_I2C_BUS_USED` report, and
the "exactly one component per bus" module doc. Then, after the bus lookup and the identifier check:

```rust
let address = attrs.address.map(|a| a.value);

if let Some(a) = address && !(0x08..=0x77).contains(&a) {
    return Err(cx.report(
        codes::COMPONENT_INVALID_I2C,
        crate::devices::span_of(node, "address"),
        "invalid i2c_component",
        format!(
            "<i2c_component id=\"{}\"> address=\"0x{a:02X}\" is reserved by the I2C \
             specification; usable addresses are 0x08..=0x77",
            id.raw
        ),
    ));
}

let clash = address.and_then(|a| {
    cx.components().iter().find_map(|c| match c {
        Component::I2c(e) if e.bus == bus && e.address == Some(a) => Some(e.id.clone()),
        _ => None,
    })
});
if let (Some(other), Some(a)) = (clash, address) {
    let bus_id = cx.bus(bus).map_or(String::new(), |b| b.id().raw.clone());
    return Err(cx.report(
        codes::CONFLICT_I2C_ADDRESS,
        node_span(node),
        "i2c address conflict",
        format!(
            "<i2c_component id=\"{}\"> and <i2c_component id=\"{}\"> both use address \
             0x{a:02X} on <i2c id=\"{bus_id}\">",
            other.raw, id.raw
        ),
    ));
}
```

(`devices::span_of` is `pub(crate)`; if you would rather not reach across modules, point at
`node_span(node)` as the other checks in this file do.)

### 5.2 `Bus::I2c.used_by`

Drop the field. Three sites: `model.rs` (the variant and its doc), `peripherals/i2c.rs`
(`used_by: None`), and the `lower.rs` test that asserts `used_by == Some("i2c_tester")`.
`Bus::Uart.used_by` is untouched; it is not part of this change.

### 5.3 Everything `E0611` touches

`codes.rs` (constant), `components/i2c.rs` (check, helpers, module doc), `lower.rs`
(`one_i2c_bus_takes_exactly_one_component`, replaced by the tests in section 9).

### 5.4 Emit

Follow SPI's shape. Build the bus once through `cx.once`, anchor the `SAFETY` block on the
`let`, and only the first component on a bus emits it (`once` returns whether it inserted):

```rust
let handle = format_ident!("{}_bus", bus_id.rust_name());
let inserted = cx.once(&handle.to_string(), quote! {
    let #handle = espforge_runtime::I2cBus::build(
        #peripheral, #sda, #scl,
        espforge_runtime::I2cConfig { frequency: #rate },
    )
    .expect("I2C bus configuration failed");
});
if inserted {
    cx.safety_block(format!("let {handle} ="), &[
        "SAFETY: espforge checked that no other component uses this",
        &format!("peripheral or GPIO{sda_pin}/GPIO{scl_pin}."),
    ]);
}
match self.address {
    Some(a) => { /* field: I2cDevice<#runtime>;  init: #name: I2cDevice::new(#handle, #a) */ }
    None    => { /* field: I2cBus<#runtime>;     init: #name: #handle */ }
}
```

Use an unsuffixed literal for the address, as `components::int` does for integers
(`Literal::u8_unsuffixed`), so it prints `0x3C` as `60`. That is a golden byte, so confirm it
against the re-baselined output rather than assuming.

Add one accessor next to `Component::tag()`:

```rust
pub fn address(&self) -> Option<u8> {
    match self { Component::I2c(c) => c.address, _ => None }
}
```

---

## 6. Devices: addresses are declared once

The hole in `i2cfix.md`: `<scd4x i2c_component="$bus">` on an unaddressed component lowers fine,
then `Scd4x::new(components.bus)` receives an `I2cBus` where it wants an `I2cDevice`. The project
promises the compiler catches this, not `rustc`.

### 6.1 Schema: `needs_address`

A child of `device_recipe`, in `espforge-schema`:

```kdl
device_recipe { ... needs_address }           // any address: the user's .espf decides (ssd1306)
device_recipe { ... needs_address "0x62" }    // fixed by the part (scd4x)
```

```rust
pub enum AddressNeed { None, Any, Fixed(u8) }   // DeviceRecipe.address
```

Load-time rules, with the node's span: the value must be hex in `0x08..=0x77`; the node may be
given once. (It is meaningful only when `bus` resolves to an `i2c_component`; say so in the
element's help rather than putting a tag name in the schema crate.)

### 6.2 Interpreter (`devices/recipe.rs`)

After the component of the right kind is found, and before the pin claims:

```rust
match (recipe.address, component.address()) {
    (AddressNeed::None, _) => {}
    (_, None) => return Err(device_error(cx, node, bus_attr,
        format!("<{tag} id=\"{}\"> needs `{component_name}` to declare an address{}; \
                 it is a bus-only component",
                id.raw, fixed_hint))),
    (AddressNeed::Fixed(want), Some(got)) if got != want => return Err(device_error(cx, node, bus_attr,
        format!("<{tag} id=\"{}\"> is fixed at address 0x{want:02X}, but `{component_name}` \
                 declares 0x{got:02X}", id.raw))),
    _ => {}
}
```

Both use `DEVICE_INVALID`, the code this tier already uses for "cannot be built". The interpreter
still knows no tag.

### 6.3 Drivers

- `drivers.schema.kdl`: `scd4x` gains `needs_address "0x62"`; `ssd1306` gains `needs_address`.
- `scd4x.rs`: `Scd4x::new(I2cDevice<Blocking>)`; delete `pub const ADDRESS`; commands use
  `self.device.write(..)` and `self.device.write_read(..)`. The `0x62` now lives in exactly one
  place, the schema, which is what checks it.
- `ssd1306.rs`: import `I2cDevice` instead of `I2cBus`; change the alias to
  `I2CInterface<I2cDevice<Blocking>>`; `new(device: I2cDevice<Blocking>, ..)`. The crate passes its
  default `0x3C` into our ignored-address impl, so a panel declared `0x3D` works. State that in
  the driver's doc comment.
- `ili9341` is unaffected (SPI).

---

## 7. Examples

| File | Change |
|---|---|
| `04.Communication/i2c_example/example.espf.txt` | `<i2c_component id="i2c_tester" i2c="$i2c0" address="0x42" />` |
| `04.Communication/i2c_example/app/rust/app.rs` | `i2c.write_read(&[sent_byte], &mut buffer)` |
| `04.Communication/i2c_example_async/example.espf.txt` | add `address="0x42"` |
| `04.Communication/i2c_example_async/app/rust/app.rs` | `i2c.write_read(&[sent_byte], &mut buffer).await` |
| `04.Communication/i2c_scanner/*` | unchanged: stays unaddressed, uses `i2c.write(address, &[])` |
| `06.Displays/ssd1306/*` | unchanged: already declares `address="0x3C"`, now actually used |

The Wokwi chip files and `diagram.json` do not change.

---

## 8. Steps

Every commit is `cargo test --workspace` green. Goldens are byte-identical until I4.

| Step | Work | Goldens |
|---|---|---|
| **I1 runtime** | `I2cDevice` and its impls (section 4). Cross-check: `cargo check -p espforge-runtime --target riscv32imc-unknown-none-elf` with the chip features, blocking and `embassy` | unchanged |
| **I2 lowering rules** | section 3 codes; section 5.1 checks; remove `E0611`, `used_by`. Corpus specs `conflict-i2c-address` and `i2c-address-reserved`. Emit is **not** touched yet | unchanged |
| **I3 `needs_address`** | schema parse and validation, interpreter check, `Component::address()`, `drivers.schema.kdl`. Existing ssd1306 example already passes (it has `0x3C`) | unchanged |
| **I4 shape** | section 5.4 emit, section 6.3 drivers, section 7 examples | **re-baseline 8 cells, once** |
| **I5 prove it** | the multi-drop inline compile case; docs | unchanged |

I4 re-baselines exactly these cells and nothing else:

- `04.Communication.i2c_example.{esp32c3,esp32s3}.blocking`
- `04.Communication.i2c_example_async.{esp32c3,esp32s3}.embassy`
- `04.Communication.i2c_scanner.{esp32c3,esp32s3}.blocking`
- `06.Displays.ssd1306.{esp32c3,esp32s3}.blocking`

The review of those diffs should find only: the `let i2c0_bus = ...` prelude and its `SAFETY` block
moving out of the initialiser, the field type becoming `I2cDevice` (or staying `I2cBus` for the
scanner), the initialiser, and the two changed `app.rs`/`.espf` files. If any other line moves, stop.

---

## 9. Tests

Replace `one_i2c_bus_takes_exactly_one_component` with:

- two components on one `<i2c>` with different addresses lower, and the board has two components;
- the same address on one `<i2c>` is `E0708`, labelled with both ids and the bus id;
- the same address on two different `<i2c>`s lowers;
- `address="0x00"`, `"0x07"`, `"0x78"`, `"0x7F"` are `E0603`; `"0x08"` and `"0x77"` lower;
- an unaddressed component beside an addressed one lowers, and so do two unaddressed ones;
- emit: two components on one bus produce exactly one `let ..._bus` (like the SPI `once` test);
  addressed emits `I2cDevice`, unaddressed emits `I2cBus`;
- device: `<ssd1306>` or `<scd4x>` on an unaddressed component is `E0907`; `<scd4x>` on `0x48` is
  `E0907` naming `0x62`; `<ssd1306>` on `0x3D` lowers;
- schema: `needs_address` parses bare and with a value, rejects `0x7F`, rejects a duplicate;
- `generated_projects_compile -- --ignored`: add one inline spec with `<ssd1306>` and `<scd4x>` on
  one `<i2c>` (`0x3C` and `0x62`, blocking, `esp32c3`). It is the only test that exercises
  multi-drop through to `rustc`.

---

## 10. Out of scope, recorded so it isn't lost

- **Two I2C controllers in one project panic at runtime.** `I2cBus::build` keeps
  `static CELL: StaticCell<..>` inside a generic function. A `static` in a generic function exists
  once, not once per instance, so `i2c0` plus `i2c1` hits "already initialized" at the second
  `.init()`. The compile checks cannot see it. Fix separately, for example by having the generated
  code own the cell and pass `&'static` in. Until then, say so wherever multi-controller is
  advertised.
- Pin validity per chip (still phase 7).
- 10-bit addressing. The model is 7-bit throughout.
- A bus-clock or address-translation (mux) story; a fixed-address mismatch is an error because
  nothing here can express a legitimate translation.

---

## 11. Definition of done

- [ ] Two components share one `<i2c>`; one `let ..._bus` is emitted.
- [ ] Same address on one bus is `E0708`; reserved addresses are `E0603`; `E0611` is gone and its
      number is never reused.
- [ ] `i2c.write_read(&[..], &mut buf)` compiles in `app.rs` with no address, and the address is
      the one in `.espf`.
- [ ] The scanner still compiles unchanged and uses `I2cBus`.
- [ ] A device on an unaddressed component, or on the wrong address for a fixed-address part, is
      refused at lowering, not by `rustc`.
- [ ] `scd4x` has no address constant; `ssd1306` uses the component's address.
- [ ] Exactly the 8 cells in section 8 changed; every other golden is byte-identical.
- [ ] `cargo fmt --check`, `cargo clippy --workspace`, `cargo test --workspace`, and the ignored
      compile test (including the multi-drop spec) are green.
