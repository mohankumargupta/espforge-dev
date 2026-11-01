# `i2creview.md` — Review of `i2cfix.md`

**Reviewer:** Buffy · **Date:** 2026-11-01 · **Subject:** [i2cfix.md](i2cfix.md)

---

## 1. Verdict

**The diagnosis is accurate; the direction is right; the specification is inconsistent about what is
already built, and it would not compile as written.**

Every problem `i2cfix.md` claims about the current tree is real (checked below). The central idea —
`<i2c>` is the controller, `<i2c_component>` is an addressed endpoint, multi-drop is legal, and a
typed `I2cDevice` carries the address so neither `app.rs` nor a driver re-types it — is the correct
model, and it is the same shape SPI already has. Adopt it.

What needs fixing before anyone implements from this document:

1. It presents as *spec changes* several things that already exist (`address` is already an optional
   `Option<u8>` in the model, and already optional in the schema).
2. Two of its code snippets cannot compile as written (a borrow in the conflict check, an
   `Option`/`Copy` mismatch in the driver type alias it forgot to mention).
3. Three required pieces are missing entirely: a lower-time check that an *addressed* component is
   bound to a device that needs one, the new diagnostic numbers' stage-range bookkeeping, and the
   removal half of `used_by`.
4. Two diagnostics are filed under the wrong stage range, and one of the doc's "valid address"
   statements contradicts the I2C specification's reserved ranges.

Rough size: ~4 files of real edits plus golden re-baselining. The document is ~10x longer than the
change, which is why the gaps above are easy to miss in it.

---

## 2. Claims checked against the tree

| # | Claim in `i2cfix.md` | Verdict | Where |
|---|---|---|---|
| 1 | `address` is parsed and then discarded in `emit()` | **True** | `espforge-elements/src/components/i2c.rs:91-121` — `emit` destructures only `peripheral`/`sda`/`scl`/`khz`; `self.address` is never read |
| 2 | `app.rs` must re-type the address (`write_read(0x42, …)`) | **True** | `espforge/tests/golden/04.Communication.i2c_example.esp32c3.blocking/src/app.rs` |
| 3 | `scd4x` hardcodes `ADDRESS = 0x62` | **True** | `espforge-drivers/src/scd4x.rs:12`, used at `:66` and `:87` |
| 4 | A second `<i2c_component>` on one bus is fatal (`E0611`) | **True** | `components/i2c.rs:49-66`; `codes.rs:128`; pinned by `lower.rs:625` (`one_i2c_bus_takes_exactly_one_component`) |
| 5 | SPI already supports N components per bus via `cx.once` | **True** | `components/spi.rs` `emit`, `EmitCx::once` in `element.rs:437` |
| 6 | The schema must gain an optional `address` (§3.2) | **False — already there** | `espforge-schema/src/espf.schema.kdl:207-217`, with the scanner spelled out in the help text |
| 7 | `I2cComponent { address: Option<u8> }` must be introduced (§6.1) | **False — already there** | `components/i2c.rs:75-82`; `peripherals/i2c.rs:65` already records `used_by: None` |
| 8 | `E0708` is free | **True** | `codes.rs` tops out at `E0707` |
| 9 | `E0516` is free | **True, but wrong range** | `codes.rs` tops out at `E0515` — see §4.2 |

So the doc's "executive summary" is trustworthy and its "specification" sections are half
re-description of the existing code. That matters: a reader cannot tell which sections are work and
which are a changelog.

---

## 3. What the proposal gets right (worth keeping)

- **Controller vs endpoint split.** `address` belongs on the *component*, not the device and not the
  bus. That is the correct place: it is a property of the wiring, and it is what makes multi-drop
  typecheckable.
- **`I2cDevice` implements `embedded_hal::i2c::I2c`.** This is the lever that lets `ssd1306` take the
  handle unchanged, and it is the same trick `embedded-hal-bus` uses. Ignoring the caller-supplied
  address in favour of the configured one is unconventional but correct *if documented as a
  contract* (see §5.2).
- **Scanner = the absence of an address.** No new element, no new attribute, no `raw=true` flag. The
  schema's `Option` already means exactly this, and the scanner golden already relies on it.
- **Address-collision detection instead of a singleton.** Right error, right scope (same bus only),
  right severity. Two `0x3C` panels on `i2c0` is a wiring bug that cannot be caught anywhere else;
  `0x3C` on `i2c0` and `0x3C` on `i2c1` is fine.
- **Deleting `ADDRESS` from `scd4x`.** Correct as a default; see §5.1 for the trade-off you lose.

---

## 4. Defects in the proposal

### 4.1 The conflict check does not compile (borrow)

```rust
for component in cx.components() {          // borrows cx immutably
    if let Component::I2c(existing) = component {
        ...
        return Err(cx.report(...));          // needs &mut cx
```

`LowerCx::report` takes `&mut self`, so the loop body cannot call it while `cx.components()` is
alive. Collect the clash first, report after:

```rust
let clash = cx.components().iter().find_map(|component| match component {
    Component::I2c(existing) if existing.bus == bus && existing.address == address && address.is_some() => {
        Some(existing.id.clone())
    }
    _ => None,
});
if let Some(other) = clash {
    return Err(cx.report(codes::…, node_span(node), …));
}
```

Note also that `existing.address == Some(addr)` is unnecessary if you compare `Option<u8>` values
directly; and two *unaddressed* components on one bus must stay legal (both are just `I2cBus`
handles to the same controller), so the check must be skipped when `address` is `None` — the doc's
snippet does this, but only by accident of the `if let Some(addr)` wrapper.

### 4.2 The diagnostic numbers are in the wrong stage range

`codes.rs` opens with "The ranges are reserved by stage"; the guard test
`the_slice_only_defines_the_codes_it_can_reach` enforces `501..=515` and `701..=707` today.

- `E0708 CONFLICT_I2C_ADDRESS` — correct range (`E07xx` is conflicts). Keep it.
- `E0516 I2C_INVALID_ADDRESS` — **wrong range.** `E05xx` is *peripheral declaration* problems
  (`I2C_INVALID_ID`, `I2C_INVALID_PIN`, `I2C_INVALID_FREQUENCY`). An invalid address is a component
  problem, and `E0603 COMPONENT_INVALID_I2C` already exists for "an `<i2c_component>` that cannot be
  built". Either **reuse `E0603`** (cheapest, and the message already says
  `❌ [i2c_component] '…' invalid …`) or allocate **`E0623`**, the next free slot in the component
  range. Allocating `E0516` will read as a peripheral error forever.
- Whichever you choose, that guard test's range literal must be widened in the same commit. The doc
  does not mention it, which is the kind of thing that turns a 20-minute change into a failing CI run.

### 4.3 Retiring `E0611` is a four-site change, not one

The doc says "delete the singleton check". In practice:

| Site | What must change |
|---|---|
| `components/i2c.rs:49-66` | the check itself |
| `components/i2c.rs:70` + `:133-149` | `set_used_by` / `used_by` become dead for I2C |
| `components/i2c.rs:1-6` | module doc still reads "The schema allows exactly one component per bus" |
| `lower.rs:625` | `one_i2c_bus_takes_exactly_one_component` asserts `E0611` and its exact wording |
| `codes.rs:128` | the constant, if it is retired rather than repurposed |

Note `used_by` is **shared with `Bus::Uart`** (`model.rs:254`, `:275`). It cannot be deleted from the
model; for I2C it must either stay `Option<Ident>` and always be `None`, or be dropped from the I2C
variant only — which ripples into `peripherals/i2c.rs:65`, `board_json.rs`, and every pattern match
that says `Bus::I2c { used_by, .. }`. **Recommendation:** leave the field in place for now, stop
setting it for I2C, and note the dead field as a follow-up. Removing it in the same change doubles
the diff for no behavioural gain. The doc should state this choice; silence here is how a "simple"
edit turns into a 15-file refactor.

### 4.4 Missing: a device that needs an address can bind to an unaddressed component

This is the real functional hole. After the change, `<scd4x i2c_component="$i2c0">` where
`$i2c0` has no `address` still lowers cleanly, and then `Scd4x::new(components.i2c0)` receives an
`I2cBus` where an `I2cDevice` is expected — **generated code that does not compile**, which is
exactly the failure mode the project's design rules exist to prevent (the "source of truth" banner
promises the compiler catches this, not `rustc`).

`recipe.rs` already has the machinery: `recipe.takes_bus` plus `cx.devices()` (see the
`DEVICE_INVALID` "already taken" check at `recipe.rs:110`). Extend it with one rule:

> A device whose recipe declares it binds an `i2c_component` requires that component to declare an
> `address`; otherwise report at the `<i2c_component>`'s span that the component is bus-only.

Without this, the multi-drop feature ships with a whole class of projects that generate unbuildable
Rust. The doc's verification checklist has no entry for it.

### 4.5 The `ssd1306` driver change is incomplete

§7.2 changes the constructor to take `I2cDevice<Blocking>` but forgets the type it is stored in:

```rust
// espforge-drivers/src/ssd1306.rs:17-22 — still says I2cBus
type Oled<SIZE> = ssd1306::Ssd1306<
    ssd1306::prelude::I2CInterface<I2cBus<Blocking>>,   // must become I2cDevice<Blocking>
    SIZE,
    ssd1306::mode::BufferedGraphicsMode<SIZE>,
>;
```

`I2CDisplayInterface::new(device)` will not typecheck against the old alias, so the doc's snippet is
wrong until this line changes too. Same class of omission: nothing in §7 mentions that
`espforge-drivers/src/ssd1306.rs` header imports `I2cBus` and must import `I2cDevice`.

Also worth stating explicitly, because it looks like a bug and is not: `ssd1306` passes its own
address (`0x3C` by default) into `I2cDevice`'s ignored-address `I2c` impl. The panel therefore talks
to the `.espf` address, and a `address="0x3D"` panel works — but only because the address argument
is silently overridden. Prefer `I2CDisplayInterface::new_custom_address(device, addr_from_espf)` if
the driver can see it, or at minimum write the contract down in the driver's doc comment.

### 4.6 `#[derive(Clone, Copy)]` on `I2cDevice` will not compile as intended

```rust
#[derive(Clone, Copy)]
pub struct I2cDevice<Dm: DriverMode + 'static> { bus: I2cBus<Dm>, address: u8 }
```

`derive` adds a `Dm: Copy` bound, which is not what you mean and can fail (or silently constrain
driver-mode types). `I2cBus` in the same file deliberately avoids this with hand-written impls:

```rust
impl<Dm: DriverMode + 'static> Copy for I2cBus<Dm> {}
```

Do the same for `I2cDevice` — three lines, same file, matching the existing convention. The doc sets
a style precedent that contradicts its own cited file.

### 4.7 "Valid 7-bit address" is stated too loosely

`0x01..=0x7F` is not the set of usable 7-bit addresses. The specification reserves `0x00..=0x07`
(general call and CBUS/high-speed codes) and `0x78..=0x7F` (10-bit prefixes and reserved). If the
validator advertises "must be a valid 7-bit address" it should be at least `0x08..=0x77`, or it
should say "must be a 7-bit address (the reserved range is accepted)". Pick one and make the message
match; a diagnostic that claims more than it checks erodes trust in the rest.

Separately: check what type `attrs.address.value` is. If the generated `hex` reader yields a `u8`,
then `address="0x100"` fails at **parse** time (`COMPONENT_INVALID_I2C`, "invalid hex address") and
never reaches the new `> 0x7F` branch — so the doc's table row for "address > 0x7F" is only reachable
for `0x80..=0xFF`. That is fine, but the diagnostic catalog must describe what is actually reachable.

---

## 5. Design trade-offs the doc should decide explicitly

### 5.1 Deleting `ADDRESS` from drivers gives up a cross-check

The doc frames `pub const ADDRESS: u8 = 0x62` as pure duplication to delete. It is also the only
place that knows what address the part *actually* has. Keep it as a *default/expectation* rather than
deleting it:

- schema: `<scd4x>`'s `i2c_component` may omit `address` and inherit `0x62`;
- lowering: if both are present and disagree, warn (`W09xx` "component address 0x48 differs from
  SCD4x's fixed 0x62"). That is a real wiring bug today, and it is invisible in every layer of the
  proposed design.

If you would rather not carry it, say so and record the consequence. Right now the doc presents a
loss as a pure win.

### 5.2 `I2cDevice` silently discards the address its caller passed

Any third-party driver that addresses two devices through one handle will silently talk to the
configured one. That is the accepted convention (`embedded-hal-bus::I2cDevice` does the same), but it
must be documented on the impl, not left for a reader to discover:

```rust
/// The address passed by a driver is ignored: this handle talks to the
/// address configured in `.espf`.
```

### 5.3 The prelude changes every I2C project's generated code

Routing the bus build through `cx.once` moves `I2cBus::build` from the `Self { .. }` initializer into
a `let` above it — for *all* I2C projects, scanner included, even though nothing needed changing
there. That is unavoidable and fine, but the doc's §8 should show the scanner case (`Components::new`
with a prelude) so the golden churn is reviewed against a spec rather than discovered in a diff.

### 5.4 Pre-existing hazard the change makes easier to hit

`I2cBus::build` keeps its `static CELL: StaticCell<…>` **inside a generic function**
(`espforge-runtime/src/hw/i2c.rs`). A `static` in a generic fn is shared across monomorphizations, so
`i2c0` + `i2c1` in one project panics at the second `.init()` ("already initialized") at runtime,
despite lowering allowing both ids. Multi-drop does not cause this, but a document about making I2C
first-class should at least record it as a known limitation instead of implying multi-bus is safe.
Fixing it (per-instance cells via a trait, or `static_cell` per generated bus) is out of scope here.

---

## 6. Revised execution plan

Smaller than the doc's, in dependency order. Steps 1-3 are the whole feature; 4-5 are the fallout.

1. **`espforge-runtime/src/hw/i2c.rs`** — add `I2cDevice { bus, address }` with hand-written
   `Clone`/`Copy`, inherent curried `write`/`read`/`write_read`/`transaction` for `Blocking` and
   `Async`, and `embedded_hal::i2c::I2c` / `embedded_hal_async::i2c::I2c` impls that ignore the
   caller's address (documented). Re-export from `lib.rs` next to `I2cBus`.
2. **`espforge-elements/src/components/i2c.rs`** —
   - delete the singleton check (leave `Bus::I2c::used_by` in the model, always `None`, with a TODO);
   - add the address range check; add the same-bus address-collision check (collect-then-report);
   - emit via `cx.once(format!("{handle}"))`: addressed → `I2cDevice::new(handle, addr)`,
     unaddressed → `handle`;
   - update the module doc.
3. **`espforge-diagnostics/src/codes.rs`** — add `CONFLICT_I2C_ADDRESS = "E0708"` and either reuse
   `COMPONENT_INVALID_I2C` or add `I2C_INVALID_ADDRESS = "E0623"`; widen the range literal in
   `the_slice_only_defines_the_codes_it_can_reach`; retire `E0611` and fix the four sites in §4.3.
4. **`espforge-drivers`** — `scd4x::new(I2cDevice<Blocking>)` (keep `ADDRESS` as documented
   expectation or delete it deliberately), `ssd1306::new(I2cDevice<Blocking>)` **plus** the `Oled`
   alias and imports. No schema or codegen change is needed: the recipes pass the component field
   positionally, so the constructor's argument type is transparent to lowering.
5. **`recipe.rs`** — the new check from §4.4: a device binding an `i2c_component` needs an addressed
   component. New diagnostic (e.g. `E09xx`/`DEVICE_NEEDS_I2C_ADDRESS`).
6. **Examples + goldens** — drop the addresses from `i2c_example{, _async}` apps, re-baseline
   `04.Communication.i2c_*` and `06.Displays.*`, add a two-devices-one-bus golden (display `0x3C` +
   sensor `0x62`) and an address-conflict failure fixture.

---

## 7. Open questions to settle before implementing

1. `E0623` vs reusing `E0603` for an invalid address — the doc should not leave a code-number choice
   to the implementer, since numbers are permanent once shipped.
2. Is a bus-level (unaddressed) component allowed to coexist with addressed ones on the same bus,
   e.g. a scanner plus a sensor? The proposed model says yes (two `I2cBus`/`I2cDevice` handles to one
   controller, both `Copy`). Confirm that is intended; it is harmless but it is a new state.
3. Does `address` on the component ever *override* a driver's fixed address, or is it the source of
   truth with the driver merely expecting it? The doc implies the former; §5.1 argues for the latter.
4. Should two unaddressed components on one bus be deduplicated into one `Components` field? Not
   needed (they are the same controller), but the answer determines whether the prelude key is the
   bus id or the component id.
