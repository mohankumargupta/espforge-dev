# buses.md — the bus tier, ported from `04.Communication`

Phase 3 of `finalplan.md`. The recipe tier is done (`recipe.md`), so a GPIO component is a schema
block plus an example. Buses are the first elements that are *not* that: three peripherals, three
components, three runtime types, one shared-resource rule, and the first thing `EmitCx` cannot
express with `field` + `init` alone.

That makes this slice a test of the seams phases 1–2 built:

| Phase-2 seam | What buses ask of it |
|---|---|
| `ResourceGraph` owns pins | bus pins and a UART's physical peripheral are claims too |
| `EmitCx` has `field` and `init` | an SPI bus is a `let` *before* `Self { … }`, emitted once |
| `LowerCx` lowers element by element | two diagnostics need the whole board (`E0701`, `E0607`) |
| the recipe tier covers `# components` | bus components carry an address/CS and stay hand-written |
| the schema is cut to what is implemented | new attribute kinds, `range`, and one option |

The oracle is the legacy tree, which is in this checkout: `legacy/espforge/tests/golden/` has all
14 bus cells, `legacy/espforge-examples/src/examples/04.Communication/` has the seven examples and
their Wokwi chips, and `legacy/espforge/tests/diagnostics/` has the specs. Nothing about the bus
protocol is being redesigned; the job is to reproduce it with the new structure.

The hard constraint is the same as every slice: **the 14 existing cells stay byte-identical**, and
the 14 new ones are **copied from the legacy oracle, never regenerated**. The new tree's
`02.Digital.button` golden is `diff -r`-identical to the legacy one today, so the oracle and the
harness speak the same language: the bus trees can be committed as-is and the emitter is fixed
until they match.

---

## 1. The gate

Six checks. 1–3 are "it still works and the port is faithful"; 4–6 are "the seams held".

1. **28/28 byte-identical.** `cargo test -p espforge --test golden_projects` green: the 14 current
   cells plus 14 copied from `legacy/espforge/tests/golden/04.Communication.*`. No `UPDATE_GOLDENS`
   run, no cell edited by hand. The 14 split as `i2c_example` 2, `i2c_example_async` 2,
   `i2c_scanner` 2, `spi_example` 2, `spi_example_async` 2, `uart_example` 2,
   `uart_example_async` 2 (`esp32c3` + `esp32s3`, the runtime each example declares).
2. **It still compiles.** `cargo test -p espforge --test generated_projects_compile -- --ignored`
   for the existing 4 cases plus 6 bus cells: `i2c_example`, `spi_example`, `uart_example`
   (`esp32c3`/`blocking`) and their `_async` twins (`esp32c3`/`embassy`), plus one inline spec
   using `i2c1` to prove the runtime is no longer hardcoded to `I2C0`. `i2c_scanner` is not a
   case: it generates the same `I2cBus` init as `i2c_example`. Xtensa is still out of scope.
3. **The diagnostics corpus.** The eight bus specs from `legacy/espforge/tests/diagnostics/` render
   byte-identically: `conflict-i2c-vs-gpio`, `conflict-spi-vs-gpio`, `conflict-spi-cs-vs-bus-pin`,
   `conflict-uart-vs-i2c`, `i2c-bad-id`, `i2c-bad-frequency`, `i2c-component-bad-address`,
   `uart-tx-equals-rx`. `component-missing-controller-link` is deliberately *not* ported: its
   wording ("needs an i2c=/spi=/gpio=/uart= attribute") was replaced by the recipe's
   `needs gpio="$<gpio id>"` in phase 2, which `recipe.md` pinned.
4. **The tier boundary holds.** The buzzer drill is still **two sites**. A bus element is *not* two
   sites and this doc says so rather than pretending: a new bus is a schema block, an element file
   and a runtime type (three), because bus behaviour is not a pin, a direction and booleans. The
   recipe tier is untouched: `led` and `button` still declare `recipe`, and the shape predicate in
   `registry.rs` still classifies all three new components as out of shape.
5. **Nothing hand-kept.** `known_tags` already derives from schema ∩ registry, so the legacy
   `generated_vocab.rs` / `bus_component=#true` list has no successor and must not reappear. After
   this slice, `grep -rn '"i2c"\|"spi"\|"uart"' espforge-elements/src` returns the three peripheral
   elements, the three components, `model.rs`'s `OwnerKind::tag`, and tests — no `match` on a bus
   tag anywhere outside the element's own file.
6. **The seams did not leak.** `EmitCx::once` exists for exactly one caller (the SPI prelude) and
   has a unit test that two devices on one bus emit one `let`; `ResourceGraph` grows buses and
   peripheral claims, not a general "resource" abstraction.

Standing conditions: `cargo fmt --all --check`, `cargo clippy --workspace`, `cargo test --workspace`
green, and `git status` clean except the plan docs.

---

## 2. What the legacy buses say the new model must express

Facts are read from `legacy/espforge-hardware/src/{peripherals/*.rs,board.rs}`,
`legacy/espforge-runtime/src/components/*.rs` and the 14 golden `src/generated.rs`. Wording is read
from `legacy/espforge/tests/diagnostics/expected/*.txt`.

### 2.1 The three peripherals

| Fact | `<i2c>` | `<spi>` | `<uart>` |
|---|---|---|---|
| id rule | `i2c0`/`i2c1`/`i2c2` (else `E0504`) | `spi2` only (else `E0507`) | `uart0..uart3`, or any id with `uart="UARTn"` (else `E0511`) |
| physical peripheral | uppercase id | `SPI2` | `uart="UARTn"`, else uppercase id (`E0512` if the attr is not `UART0..3`) |
| pins claimed | `sda`, `scl` | `sclk`, `mosi`, `miso` if present | `tx`, `rx` |
| `PinUse` | `Bidirectional` | `Bidirectional` | `Unspecified` |
| config | `frequency`, default `100kHz` | `mode` 0..=3, `frequency` optional | — |
| other claims | none | none | the peripheral name (`E0704`) |
| bus record | `{ id, peripheral, sda, scl, khz, used_by }` | `{ id, peripheral, sclk, mosi, miso, khz, mode }` | `{ id, peripheral, tx, rx, used_by }` |
| owner kind | `OwnerKind::I2c` | `OwnerKind::Spi` | `OwnerKind::Uart` |

Two asymmetries are real and must be ported exactly: `<spi>`'s `frequency` has **no schema
default** (the element falls back to 1000 kHz, which is `SpiConfig`'s default), while `<i2c>`'s
defaults to `100kHz` in the schema; and only `<uart>` claims a physical peripheral name, because
`spi2`/`i2cN` are already encoded in the id rule.

### 2.2 The three components

| Fact | `<i2c_component>` | `<spi_component>` | `<uart_component>` |
|---|---|---|---|
| reference | `i2c="$id"` | `spi="$id"` | `uart="$id"` |
| own attributes | optional hex `address` | `cs` (pin number or `<gpio>` id), `mode` | `baud_rate`, `parity`, `stop_bits` |
| components per bus | exactly one (`E0611`) | any number | exactly one (`E0612`) |
| emitted field type | `I2cBus<runtime>` | `SpiDevice<runtime>` | `UartDevice<runtime>` |
| emitted construction | inline in the init | bus `let` **before** `Self`, then the device | inline in the init |
| id rule | Rust identifier (`E0614`) | same | same |
| extra checks | `address` must be hex (`E0603` wording) | `cs` ref must exist (`E0607`) | `baud_rate > 0` (`E0610`) |

`<spi_component mode>` is read and stored but **not** used for the bus configuration (the `<spi>`
mode wins); it survives because `board_json` emits it (`legacy/espforge-core/src/board_json.rs`) and
`check.md` owns that contract. `<spi_component frequency>` is read by nothing at all: not the
emitter, not the IR, not the JSON (see Q4).

### 2.3 The emitted Rust

Measured from the goldens. I2C and UART build inside the initialiser:

```rust
pub struct Components {
    pub i2c_tester: espforge_runtime::I2cBus<esp_hal::Blocking>,
}
impl Components {
    pub fn new(registry: &mut PeripheralRegistry) -> Self {
        Self {
            // SAFETY: espforge checked that no other component uses this
            // peripheral or GPIO6/GPIO7.
            i2c_tester: espforge_runtime::I2cBus::build(
                    unsafe { registry.peripherals.I2C0.clone_unchecked() },
                    unsafe { registry.peripherals.GPIO6.clone_unchecked() },
                    unsafe { registry.peripherals.GPIO7.clone_unchecked() },
                    espforge_runtime::I2cConfig {
                        frequency: esp_hal::time::Rate::from_khz(100),
                    },
                )
                .expect("I2C bus configuration failed"),
        }
    }
}
```

UART is the same shape with `UART1`/`GPIO6`/`GPIO5`, `UartConfig { baudrate, data_bits:
esp_hal::uart::DataBits::_8, parity, stop_bits }` and `.expect("UART configuration failed")`.

SPI is the one that needs a prelude, because `SpiDevice::new` takes the bus by value and the bus is
shared:

```rust
pub fn new(registry: &mut PeripheralRegistry) -> Self {
    // SAFETY: espforge checked that no other component uses this
    // peripheral or GPIO8/GPIO7/GPIO6.
    let spi2_bus = espforge_runtime::SpiBus::build(
            unsafe { registry.peripherals.SPI2.clone_unchecked() },
            unsafe { registry.peripherals.GPIO8.clone_unchecked() },
            unsafe { registry.peripherals.GPIO7.clone_unchecked() },
            unsafe { registry.peripherals.GPIO6.clone_unchecked() },
            espforge_runtime::SpiConfig {
                frequency: esp_hal::time::Rate::from_khz(100),
                mode: esp_hal::spi::Mode::_0,
            },
        )
        .expect("SPI bus configuration failed");
    Self {
        // SAFETY: espforge checked that no other component uses GPIO9.
        // Chip-select idles high (deasserted).
        spi_tester: espforge_runtime::SpiDevice::new(
            spi2_bus,
            esp_hal::gpio::Output::new(
                unsafe { registry.peripherals.GPIO9.clone_unchecked() },
                esp_hal::gpio::Level::High,
                esp_hal::gpio::OutputConfig::default(),
            ),
            espforge_runtime::Delay::new(),
        ),
    }
}
```

A `<spi>` with no `miso` emits `SpiBus::build_write_only` and an extra
`// No miso: a write-only bus (displays).` line on the same anchor. The `SAFETY` notes are
`(anchor, line)` pairs already; the prelude's anchor is `let spi2_bus =`, the device's is
`spi_tester: espforge_runtime::SpiDevice::new(`.

### 2.4 What is deliberately *not* bus-shaped

**Chips are example data, not elements.** `chip.json` (text) and `chip.wasm` (binary) are seeded
once at the project root, `chip/` (the Zig sources) only when `keep-zig-source` is on, and the
`[[chip]]` section is appended to `wokwi.toml` with the name parsed from `chip.json`. None of that
belongs in `espforge-elements`; it is `espforge-emit`'s overlay work plus `espforge-core` passing
the example's files through.

**Bus components are Tier 1, not recipes.** `i2c_component` has an integer `address`,
`uart_component` has three more attributes, `spi_component` has a `pin_or_ref` and a mode. The
phase-2 predicate already says they are out of recipe shape, so each gets one file, and the tier
enforcement test keeps passing without an exception list. No `bus_component=#true` property is
added: `ref to="i2c"` already says which peripheral kind a reference must name, and `known_tags`
already answers "what may appear here" from schema ∩ registry.

**The runtime bodies are ported, not redesigned.** `I2cBus`, `SpiBus`/`SpiDevice` and `UartDevice`
come from `legacy/espforge-runtime/src/components/` with their tests. The one deliberate change is
genericity (Q2). JSON output (`check.md`), `RUNTIME_EXPORTS` and the per-element contract test
(`contracts.md`) are separate slices: the bus components carry the fields those need (`address`,
`cs`, `mode`, `baud`, framing) but this doc adds no `json()` arm.

---

## 3. Scope

| Crate | Why it is here | What changes |
|---|---|---|
| `espforge-diagnostics` | 24 new codes the corpus asserts | `E0504`–`E0513`, `E0603`–`E0605`, `E0607`, `E0610`–`E0614`, `E0701`, `E0704`–`E0707`; the reachable-range test in `codes.rs` grows |
| `espforge-schema` | the elements, and one attribute property | six element blocks; `range=` on `AttrDef` and its validation; `spi_component` loses `frequency` (Q4); `keep-zig-source` option (Q5) |
| `espforge-elements` | the model, six elements, `EmitCx::once` | `attrs.rs` + `build.rs` learn `freq`, `hex`, `u32`+range, `pin_or_ref`; `model.rs` gains `Bus`/`BusId`/`PeripheralClaim`, the six `OwnerKind` variants and the conflict matrix; `LowerCx` gains `add_bus`/`bus`/`bus_mut`; `peripherals/{i2c,spi,uart}.rs` and `components/{i2c,spi,uart}.rs`; `component_enum!` +3 lines; `lower.rs` gains the bus-vs-gpio post-pass; `element.rs` gains `once` and `EmitParts.prelude` |
| `espforge-emit` | the prelude slot and the Wokwi chip | `rust.rs` prints `#(#prelude)*` before `Self`; `wokwi.rs` gains `chip_section`; `project.rs` seeds `chip.json`/`chip.wasm`/`chip/**` and appends `[[chip]]` |
| `espforge-core` | the example's chip files must reach the render | `source.rs`: `overlay_files` returns chip files too, and `chip/**` only under `keep-zig-source` |
| `espforge-runtime` | three bus types | `hw` gains `i2c.rs`, `spi.rs`, `uart.rs`; `static_cell`, `embassy-sync`, `embedded-hal-async` (feature-gated) |
| `espforge-examples` | seven examples, embedded | seven directories with `example.espf.txt`, `example.toml`, `app/rust/app.rs`, `diagram.json`, `chip.json`, `chip.wasm`, `chip/**` |
| `espforge` | the corpus, and the one format string it shares | `tests/diagnostics_corpus.rs` + the eight specs and expected files copied from `legacy/`; `commands/report.rs` calls the shared footer |

New dependencies: `static_cell`, `embassy-sync`, `embedded-hal-async` in `espforge-runtime` —
all three are already in `legacy/espforge-runtime/Cargo.toml` at the versions the port needs
(`2.1`, `0.7`, `1.0`), and none is added to any other crate.

Explicitly out of scope, each with its own doc or phase: `Lowered::json`, `check --format json`,
`board_json` (`check.md`); `RUNTIME_EXPORTS` and the per-element contract test (`contracts.md`);
Wi-Fi and services (phase 4); devices, `binds`, `cargo_dep` (phase 5); `gpio_component` and
`status_led` (phase 5's cleanup); relaxing `<spi>`/`<i2c>` ids to arbitrary numbers (Q3); per-chip
pin/instance validity (phase 7); the Xtensa compile check.

---

## 4. The bus model

```rust
pub struct BusId(pub usize);           // index into ResourceGraph::buses

pub enum Bus {
    I2c { id: Ident, peripheral: String, sda: ClaimedPin, scl: ClaimedPin, khz: u32, used_by: Option<Ident> },
    Spi { id: Ident, peripheral: String, sclk: ClaimedPin, mosi: ClaimedPin, miso: Option<ClaimedPin>, khz: u32, mode: u8 },
    Uart { id: Ident, peripheral: String, tx: ClaimedPin, rx: ClaimedPin, used_by: Option<Ident> },
}
```

`ResourceGraph` grows `buses: Vec<Bus>`, `add_bus`, `bus(BusId)`, `bus_mut(BusId)`, `buses()`,
`bus_by_name(&str)`, and `peripherals: BTreeMap<String, PeripheralClaim>` with `claim_peripheral`,
returning a `PeripheralClash` the way `claim` returns a `Conflict`. `Bus::id()` gives the `Ident`, so
a component can look the human id up for a message. Buses are recorded in `<peripherals>` order,
which is what makes the SPI prelude order stable.

`LowerCx` grows the same accessors plus `claim_peripheral`. The lookup a component does is
`cx.bus(name)` after trimming `$` from the reference — the graph is the only place a bus id lives,
so "declared but not lowered" and "referenced but not declared" are one check, not two.

`OwnerKind` gains `I2c`, `Spi`, `Uart`, `I2cComponent`, `SpiComponent`, `UartComponent`. That is six
variants, and `tag()` maps each to its schema spelling. The enum stays closed: these are the kinds
the conflict matrix needs to word, nothing else.

**The conflict matrix is the part that is not obvious.** Today `conflict_diagnostic` handles a
direction clash and a generic pin conflict. The corpus pins seven more wordings, and they depend on
which side is a bus, a component or a declared `<gpio>`:

| Situation | Title / code | Points at |
|---|---|---|
| `<i2c>`/`<spi>`/`<uart>` pin where a `<gpio>` is declared | `pin conflict` / `E0701` | the `<gpio>` node |
| `<spi_component cs>` pin where a `<gpio>` is declared (unless the CS named that gpio) | `pin conflict` / `E0701` | the `<gpio>` node |
| `<spi_component>` CS vs its own bus pin | `pin conflict` / `E0701` | the component |
| two `<spi_component>` CS on one pin | `cs pin already used` / `E0702` | the second component |
| `<i2c>` vs `<uart>` sharing a pin | `pin conflict` / `E0701`, i2c named first | the i2c |
| a bus claiming two of its own pins | `i2c`/`spi`/`uart wiring` / `E0705`/`E0706`/`E0707` | the bus |
| two peripherals naming one `UART1` | `peripheral clash` / `E0704` | the first |

The first two need the **whole board**: a declared `<gpio>` may appear after the bus that uses its
pin, so they cannot be decided while lowering one node. `lower.rs` gains a post-pass that runs after
both sections and receives the filed `GpioDecl`s plus the graph, exactly as
`check_bus_gpio_declarations`/`check_spi_cs_gpio_declarations` did. The rest fire during lowering,
through `conflict_diagnostic`, whose arms grow with each bus step.

`EmitCx` gains:

```rust
/// A statement that must appear before `Self { … }`, emitted at most once per
/// key, in first-use order.
pub fn once(&mut self, key: &str, statement: TokenStream);
```

`EmitParts` gains `prelude: Vec<TokenStream>`, and `rust.rs` prints it between the `SAFETY`
statements and `Self`:

```rust
pub fn new(registry: &mut PeripheralRegistry) -> Self {
    #(#prelude)*
    Self {
        #(#inits)*
    }
}
```

Legacy emitted every SPI bus with a device, in graph order, before every init. First-use order is
the same thing for every example here (one bus, or buses in declaration order), and it is the only
rule that keeps `once` honest when a bus is declared but unused: no device, no `let`, no orphan in
the file. No golden distinguishes the two orders; the unit test in T3 pins first-use.

---

## 5. Schema additions

Six blocks, ported from `legacy/espforge-espgenerate-template/src/espf.schema.kdl` with the
phase-3 surface only (no `bus_component=#true`, no `spi_component.frequency`):

```kdl
element "i2c" section="peripherals" {
    summary "I2C bus controller"
    attr "id" kind="ident" required=#true
    attr "sda" kind="pin" required=#true { help "GPIO number; must be valid for the chosen chip." }
    attr "scl" kind="pin" required=#true
    attr "frequency" kind="freq" default="100kHz"
    example "minimal" #"<i2c id="i2c0" sda="6" scl="7" />"#
    see_also "components/i2c_component"
}

element "spi" section="peripherals" {
    summary "SPI bus controller"
    attr "id" kind="ident" required=#true
    attr "sclk" kind="pin" required=#true
    attr "mosi" kind="pin" required=#true
    attr "miso" kind="pin" { help "Displays generally omit it." }
    attr "mode" kind="u32" default="0" range="0..=3"
    attr "frequency" kind="freq"
    …
}

element "uart" section="peripherals" {
    attr "id" kind="ident" required=#true
    attr "uart" kind="str" { help "Physical peripheral, e.g. UART1." }
    attr "tx" kind="pin" required=#true
    attr "rx" kind="pin" required=#true
    …
}

element "i2c_component" section="components" {
    attr "id" kind="ident" required=#true
    attr "i2c" kind="ref" to="i2c" required=#true
    attr "address" kind="hex" { help "Omit for a bus-only component (for example an I2C scanner)." }
    …
}

element "spi_component" section="components" {
    attr "id" kind="ident" required=#true
    attr "spi" kind="ref" to="spi" required=#true
    attr "mode" kind="u32" default="0" range="0..=3"
    attr "cs" kind="pin_or_ref" required=#true
    …
}

element "uart_component" section="components" {
    attr "id" kind="ident" required=#true
    attr "uart" kind="ref" to="uart" required=#true
    attr "baud_rate" kind="u32" required=#true
    attr "parity" kind="choice" default="none" { choices "none" "even" "odd" }
    attr "stop_bits" kind="choice" default="1" { choices "1" "2" }
    …
}
```

Every element needs at least one `example` that the schema test parses, and each block gets
`see_also` both ways (peripheral ↔ component) so `espforge espf` in phase 6 has the graph. The
`i2c` example with a component, and the `<spi_component>` "display bus" example, are ported too.

Two plumbing changes the blocks need:

- **`range=`** does not exist in `espforge-schema` yet (`AttrDef` has no field for it, and
  `parse_attr` ignores unknown properties). Add `range: Option<(u32, u32)>`, parse `"0..=3"` with a
  schema error for a bad bound, and thread it into `espforge-elements/build.rs`, where
  `AttrKind::U32` becomes `number_in_or(node, name, low, high, default)`. Without this, `mode="7"`
  would lower.
- **The four unused kinds** panic in `field_type` today ("arrives with the element that first uses
  it"). `Freq` → `Spanned<&'a str>` via `frequency_or`/`opt_frequency`; `Hex` →
  `Option<Spanned<u8>>` via `opt_hex`; `U32` → `Spanned<u32>` via `number_in_or`; `PinOrRef` →
  `PinOrRef<'a>` via `pin_or_ref`, a new enum in `attrs.rs` (`Pin(Spanned<u8>)` |
  `Ref(Ref<'a>)`). `Ref::at` already strips `$`, so `cs="gpio5"`, `cs="$gpio5"` and `cs="9"` all
  work as the schema's help says.

---

## 6. Decisions to make before T1

**Q1 — How does the SPI prelude get into `Components::new`?**
(a) **`EmitCx::once(key, statement)`, first-use order (§4).** (b) a board-level pass in
`espforge-emit` that walks the graph's buses and emits their `let`s before asking components to
emit.
(b) re-derives "which components use which bus" in the emitter, which is the `match`-per-element
shape every slice so far has deleted, and it has to know how to build each bus. (a) keeps the
tokens with the element that owns them, costs twelve lines in `element.rs` + one `quote` slot, and
is testable without a board. Recommendation: **(a)**.

**Q2 — Do the runtime bus types go generic over the peripheral now?**
(a) **`I2cBus::build<I: Instance>` and `SpiBus::build<I: Instance>` (§4).** (b) keep `I2C0`/`SPI2`
hardcoded as legacy did.
`<i2c>` already accepts `i2c1`/`i2c2` and emits `registry.peripherals.I2C1`; with (b) that spec
generates code that cannot compile, which is the kind of lie this project keeps deleting. The
change is two bounds and does not touch a golden (the tokens are still `I2C0`/`SPI2`). Cost: the
`i2c1` proof is one inline compile case (T2); nothing else exercises a second instance.
Recommendation: **(a)**.

**Q3 — Keep `<spi>`'s `spi2`-only id rule, or relax it to `spiN` now?**
(a) **keep `spi2` only.** (b) accept `spi2`/`spi3` and derive the peripheral name like `<i2c>`.
(b) is a behaviour change the corpus does not require and no example uses, and nothing here knows
which chips expose `SPI3` (`esp-metadata` does, but wiring that in is phase 7's per-chip validity
work). Keeping the whitelist means a user who writes `spi3` gets `E0507` naming the one id that
works, which is a better error than generating a build failure. Recommendation: **(a)**, recorded
in the schema's help text.

**Q4 — `<spi_component frequency>`: delete or carry?**
(a) **delete.** (b) keep it declared and unread.
Legacy read it into nothing: not the IR's `SpiComponent`, not the emitter, not `board_json`. It is
the one bus attribute with no consumer anywhere. `mode` stays for the opposite reason: unread by
the emitter, but `board_json` emits it, so it is `check.md`'s fact. A spec that still writes
`frequency` on a component parses and is ignored exactly as before.
Recommendation: **(a)**, and drop it from the schema example.

**Q5 — Ship `keep-zig-source` in this slice?**
(a) **yes, one option and one guard.** (b) seed only `chip.json`/`chip.wasm` and defer the Zig
sources.
The golden trees are `keep-zig-source`-off, so both choices pass 28/28; the difference is whether
the seven `chip/` directories in the examples do anything. (a) is a schema option plus
`p.starts_with("chip/") && keep_zig` in `layer_overlay`, and it makes the examples' chip sources
reviewable as what they are. (b) leaves seventy files in the tree that no code path copies, which
the tests cannot tell apart from dead data. Recommendation: **(a)**.

**Q6 — Where does the diagnostics corpus live, and what does it compare?**
(a) **`espforge/tests/diagnostics_corpus.rs`, comparing the rendered human output to the legacy
expected files.** (b) structured assertions (`code`, `message`, `labels`) inside the six element
files, no specs.
(b) is where the existing tests are, but it re-types the oracle from memory and cannot catch a
wording drift the corpus was written for. (a) copies the eight spec/expected pairs verbatim from
`legacy/`, runs them through `pipeline::validate`, renders with `to_human`, and compares the
`--- stderr ---` block byte for byte. The `❌ Found N problem(s)` footer currently lives in
`espforge/src/commands/report.rs`, a module of the binary target that an integration test cannot
reach: move it to `espforge_diagnostics::render::summary` and have the CLI call it, so the test and
the binary share one string. Recommendation: **(a)**.

---

## 7. Steps

Each step ends green and is one commit. "Verify" is the command that has to pass before the next
step starts.

### T0 — branch

`git switch -c buses` from `recipe`. Nothing else.

### T1 — the plumbing no element uses yet

- `espforge-schema`: `AttrDef.range`, parsing and validation for `range="0..=3"`; the
  `keep-zig-source` option (off by default, `display_name` "Keep the Zig source of custom Wokwi
  chips."); tests for a good range, a reversed range and a non-numeric one.
- `espforge-elements/build.rs` + `attrs.rs`: `Freq`, `Hex`, `U32`+range, `PinOrRef` mappings and
  their helpers (`frequency_or`, `opt_frequency`, `opt_hex`, `number_in_or`, `pin_or_ref`);
  `PinOrRef` enum.
- `espforge-emit`: `wokwi::chip_section` ported with its tests; `project::layer_overlay` seeds
  `chip.json`/`chip.wasm` (text → artifact, anything `.wasm` → asset, both seed-once) and appends
  `[[chip]]` to whichever `wokwi.toml` is in the render (generated or the example's own), naming it
  from `chip.json`; `chip/**` only when `keep-zig-source` is on.
- `espforge-core`: `source::overlay_files` returns `chip.json`, `chip.wasm` and (conditionally) the
  `chip/` tree alongside `diagram.json`/`wokwi.toml`.

**Verify:** `cargo test --workspace` with **14/14** cells identical — no schema element uses the new
kinds yet, so a byte diff here is a bug in T1. New unit tests: seeding both files, `[[chip]]` name
from `chip.json` (and the `chip` fallback), `chip/` gated by the option, and an example's own
`wokwi.toml` getting the section too.

### T2 — `<i2c>` and `<i2c_component>`, end to end

- `espforge-diagnostics`: `E0504`, `E0505`, `E0506`, `E0603`, `E0611`, `E0613`, `E0614`, `E0701`,
  `E0705`, and the reachable-range test.
- `espforge-elements`: `Bus`/`BusId`/`PeripheralClaim`, the `I2c`/`I2cComponent` owner kinds,
  `LowerCx::{add_bus,bus,bus_mut}`, `conflict_diagnostic`'s i2c arms (`pin conflict`, `i2c wiring`),
  `peripherals/i2c.rs`, `components/i2c.rs`, `component_enum!` one line, the `lower.rs` post-pass
  (`bus_pins_vs_gpio_declarations`) and its unit test.
- `espforge-emit`: nothing. I2C needs no prelude.
- `espforge-runtime`: `hw/i2c.rs` ported, generic over `impl Instance`, with `static_cell` and
  `embassy-sync`; the `embassy` feature list grows.
- Examples: `04.Communication/i2c_example`, `i2c_example_async`, `i2c_scanner` with their
  `example.toml` (chips c3/s3, runtime per cell) and chip files.
- Goldens: copy the six legacy trees.
- `generated_projects_compile`: three cases (`i2c_example`, `i2c_example_async`, and an inline
  `i2c1` spec through a new `Case::Inline`).

**Verify:** 20/20 — the 14 current cells plus 6 new (`i2c_example` and `i2c_scanner` × c3/s3
blocking, `i2c_example_async` × c3/s3 embassy). Unit tests for a valid bus (pins claimed
`Bidirectional`, `khz` from `100kHz`), a bad id, a bad frequency, `sda == scl`
(`i2c wiring`), a bus pin on a declared `<gpio>` (`pin conflict`, pointed at the gpio), a second
`<i2c_component>` on one bus (`E0611`), an unknown `$i2c9`, and a `0xZZ` address. The inline
compile case is expected to tell us whether the chip has `I2C1`; if it does not, the case moves to
`esp32s3` and the reason is recorded in this file.

### T3 — `<spi>`, `<spi_component>` and `EmitCx::once`

- `espforge-diagnostics`: `E0507`–`E0510`, `E0604`, `E0607`, `E0706`, `E0702`'s `cs pin already
  used` wording.
- `espforge-elements`: `Spi`/`SpiComponent` kinds and matrix arms (`(Spi, SpiComponent)`, two
  `SpiComponent`s, `Spi` self-clash → `spi wiring`), `peripherals/spi.rs`, `components/spi.rs`
  (including the `cs` ref → pin resolution and the `cs_gpios` bookkeeping the post-pass needs),
  `EmitCx::once` + `EmitParts.prelude`, the `spi_cs_vs_gpio_declarations` half of the post-pass.
- `espforge-emit`: print `#(#prelude)*` before `Self`.
- `espforge-runtime`: `hw/spi.rs`, generic over `impl Instance`, `build` and `build_write_only`,
  `SpiDevice` with `RefCell` CS/delay and both trait impls.
- Examples + goldens: `spi_example`, `spi_example_async`; two compile cases.

**Verify:** 24/24. The SPI cells are the ones that prove the prelude: the byte oracle fixes the
`let`'s position, indentation and both safety notes. Unit tests: one bus + two devices emits one
`let` (called twice with the same key), first-use order with two buses, a missing `miso` selects
`build_write_only`, a `cs` ref that does not exist is `E0607`, and a `cs` pin number that collides
with a declared `<gpio>` is `pin conflict` at the gpio.

### T4 — `<uart>` and `<uart_component>`

- `espforge-diagnostics`: `E0511`–`E0513`, `E0605`, `E0610`, `E0612`, `E0704`, `E0707`.
- `espforge-elements`: `Uart`/`UartComponent` kinds, `claim_peripheral` and `peripheral clash`, the
  `uart wiring` self-clash arm, `peripherals/uart.rs` (id-vs-`uart=` peripheral naming),
  `components/uart.rs` (baud > 0, parity/stop_bits choices), `component_enum!` one line.
- `espforge-runtime`: `hw/uart.rs` with the line-buffer API.
- Examples + goldens: `uart_example`, `uart_example_async`; two compile cases.

**Verify:** 28/28. Unit tests: `tx == rx` is `uart wiring` at the node, two `<uart>` on `UART1` is
`peripheral clash` at the first, `id="gps" uart="UART1"` names the peripheral, `baud_rate="0"` is
`E0610`, and a second `<uart_component>` on one port is `E0612`.

### T5 — the corpus harness (no new element behaviour)

- Move the `❌ Found N problem(s)` footer to `espforge_diagnostics::render::summary` and call it
  from `espforge/src/commands/report.rs`.
- `espforge/tests/diagnostics_corpus.rs`: for each `specs/<name>.espf.txt`, validate, render, and
  compare against the `--- stderr ---` block of `expected/<name>.txt` (the header lines `$ …`,
  `exit: 1`, `--- stdout ---` and the empty stdout are asserted structurally).
- Copy the eight spec/expected pairs verbatim from `legacy/espforge/tests/diagnostics/`.

**Verify:** `cargo test -p espforge --test diagnostics_corpus` 8/8, and the suite still 28/28. If a
diff is in the renderer's gutter rather than in a diagnostic, fix the renderer once (it is a port);
if it is a *wording* difference in a bus message, stop: the corpus is the contract and the element
is wrong, not the expected file.

### T6 — measure, and record

- `cargo fmt --all --check`, `cargo clippy --workspace`, `cargo test --workspace`, `git status`.
- `generated_projects_compile -- --ignored`: 10 example cells + the inline `i2c1` case.
- **The buzzer drill**, unchanged as a test of the recipe tier: add a throwaway `<buzzer>` to the
  schema plus one example, generate, count two sites, revert. Then record the *bus* site count
  separately: schema block + element file + runtime type = three, which is the tier boundary this
  slice is built on.
- The gate greps of §1.5.
- Write §11 (measured) in this file: cells, corpus, compile results, drill counts, and the exact
  list of files a new bus touches versus a new GPIO component.
- `finalplan.md` §7 needs no edit ("Any later phase | 2" is about the buzzer), but its §4 phase-3
  row should be read against §11 and corrected if the measured work changed the shape.

---

## 8. Risks

| Risk | Signal | Action |
|---|---|---|
| The SPI prelude prints at the wrong level or in the wrong place | any of the 4 `spi_example`/`spi_example_async` cells differs | The oracle fixes the bytes: fix `rust.rs`/`once`, never the golden |
| `prettyplease` re-indents the prelude differently from legacy | `let spi2_bus` column shifts | Legacy printed the same token stream through the same printer; if it shifts, the tokens differ (a missing `.expect` or a different argument count), not the printer |
| esp-hal 1's bus APIs moved since the legacy tree compiled | runtime build fails in T2/T3/T4 | The goldens contain no runtime code, so the runtime can be fixed without touching them; record any signature change in §11 |
| The async bus code does not compile (`embedded_hal_async` traits, `Mutex` across `await`) | the `_async` compile cases fail | Port the legacy bodies verbatim; they were written against the same esp-hal 1 line. Do not "improve" the lock scope in this slice |
| The `i2c1` inline case fails because the chip has no `I2C1` | T2 compile case | Move the case to `esp32s3` or drop it and record why; the generic bounds still stand |
| The corpus disagrees with a message the new tree already ships | a spec fails on `led`/`button` wording | Those specs are out of this set by design (`component-missing-controller-link`); if a bus spec disagrees, the bus element is wrong |
| `[[chip]]` appended to an example's own `wokwi.toml` at the wrong place | no example ships a `wokwi.toml` today, so only a synthetic overlay reaches that path | Test both paths in T1; append as text, as legacy did, so an example's formatting would survive |
| Binary goldens bloat the tree | 7 × ~13 KB `chip.wasm` in examples + 14 copies in goldens | Expected and reviewable; say so in the commit that adds them |
| Two SPI buses make `once` ordering observable | no golden covers it | The T3 unit test pins first-use order; if an example ever needs two buses, add a golden then |
| `range` changes `AttrDef` and breaks fixtures | `registry.rs`'s `attr()` helper, schema tests | One mechanical update; no golden effect |

---

## 9. Commits

```
T0  (branch only; no commit)
T1  feat(schema): bus attribute kinds, ranges and Wokwi chip seeding
T2  feat(elements): i2c bus and component, with examples
T3  feat(elements): spi bus and component; EmitCx::once
T4  feat(elements): uart bus and component
T5  test(diagnostics): the bus corpus, byte for byte
T6  docs(buses): gate measured
```

Every commit is `cargo test --workspace` green with its cells identical: 14 after T1, 20 after T2,
24 after T3, 28 after T4–T6. T1 changes no generated byte. T5 adds tests and moves one format
string. T6 is documentation.

---

## 10. If you say "go"

I start at T1 and go in order. I stop after T2 if the i2c cells differ, so a regression is one
commit wide rather than buried under SPI's prelude. If you want a smaller first cut, **T1+T2 is a
shippable slice on its own** — i2c exercises every seam except `EmitCx::once`, and T3/T4 repeat its
shape. Questions I need answered first: Q1–Q6. On completion I record the measured numbers in §11,
in `recipe.md`'s format.

---

## 11. Measured (T6)

*(empty until the slice ships)*
