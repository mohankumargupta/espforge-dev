# better2: Adding a device, component, or peripheral in 15 minutes

> Companion to `better.md`. Read that for the full audit. Read this to decide what to build.
>
> Ruthless goal: **a contributor who has never touched this repo can add a real `<buzzer>`, `<sensor>`, or `<display>` in one sitting, touching one schema block + one file + one example, and the repo tells them what to do next when they get it wrong.**

## 1. Who cares, and what job are they doing?

Three jobs, nothing else:

1. **"I have a GPIO thing"** — LED, button, buzzer, relay. Wires to one pin. Wants: declare tag + pin attr, get validation, JSON, `{{ x.pin }}`, and Rust emit for free.
2. **"I have a bus thing"** — new peripheral (`can`, `adc`) or bus-attached component. Wants: claim shared resource once, not fight cross-element matches.
3. **"I have a driver chip"** — `tmp102`, `ili9341`, `ssd1306`, any sensor/display. Wants: bind to an already-validated `i2c_component` / `spi_component`, add Cargo dep + constructor, done. This path is broken today (`reject_if_present` rejects all devices) — fixing it is P0.

If your change doesn't make one of these three jobs shorter, don't make it.

## 2. Today's experience, in contributor words

> "I copied `<led>` to make `<buzzer>`. I edited the KDL, then chased compiler errors through 9 crates and 20+ sites. I fixed the `(only <led>, <button>, ...)` string in two places, then snapshots failed in a third place I never touched. I added a diagnostic and got `E0999` because I didn't know titles are matched as strings. I gave up on devices because `check` rejects them unconditionally."

That's not a missing abstraction. That's hostile UX. The fix is deletion + derivation, not more docs.

## 3. Ruthless principles (say no to everything else)

1. **One fact lives in one place.** Tag, attrs, `ref to=`, examples live in `espf.schema.kdl`. Everything else is derived. If you type `<buzzer>` anywhere except KDL + `buzzer.rs` + example, that's a bug.
2. **Delete before you abstract.** Two `build.rs` that write into `src/`, committed generated files, four KDL walkers, `route_driver` stub registry, title-matched diagnostics — delete first, then design what's left. No new crate until old code is gone.
3. **The repo remembers so the human doesn't.** No `MATRIX`, no `CELLS = 30`, no hand-listed transcripts, no "(only ...)" strings to update by hand. Loops + contract tests remember.
4. **Errors teach the next step.** Every failure names the missing piece: "no registry entry for `<buzzer>` — add `espforge-elements/src/components/buzzer.rs` + one line in `lib.rs`". Never `E0999`, never a bare `Result<(), String>`.
5. **No plugin system, no proc-macro DSL.** Goal is "one file per element in this repo", not third-party extensibility. `Box<dyn Lowered>` and dynamic loading are out. Keep `enum Component` + one macro dispatch line until it actually hurts.

## 4. Target: what "done" looks like for each job

### Job 1: GPIO component (`<buzzer>`)

Touch exactly this:

```kdl
// espforge-espgenerate-template/espf.schema.kdl
element "buzzer" section="components" {
  attr "gpio" kind="reference" ref to="gpio" required
  attr "active_low" kind="bool" default="false"
  example "<buzzer gpio=\"$gpio0\" />"
}
```

```rust
// espforge-elements/src/components/buzzer.rs
pub struct Buzzer;
impl Element for Buzzer {
  const TAG: &'static str = "buzzer";
  fn lower(node: &ParsedNode, cx: &mut LowerCx) -> Result<(), ()> { /* claim pin, push BuzzerLowered */ }
}
impl Lowered for BuzzerLowered {
  fn json(&self, ...) -> Value { /* kind + pin */ }
  fn emit(&self, ..., cx: &mut EmitCx) { /* field + init + safety comment */ }
  fn overlay_attr(&self, attr: &str) -> Option<String> { (attr == "pin").then(...) }
}
```

```rust
// espforge-elements/src/lib.rs — ONE line
&Buzzer,
```

Plus: one runtime file + one `pub use`, one example dir. That's it. `board::build`, `board_json`, `rust.rs`, `overlay.rs`, cargo features, supported-elements text, goldens, diagnostics corpus all update themselves via loops.

Time budget: 15 min if you copy `led.rs`. If it takes longer, we failed.

### Job 2: Peripheral / bus component (`<can>`, `<i2c_component>`)

Same as above, plus only shared-state helpers on `EmitCx` — no cross-element matches:

```rust
cx.once("spi2_bus", || quote!{ ... });  // emitted once, in first-use order
cx.cargo_feature("esp-radio", "wifi");
cx.needs_heap();
```

Rule: if you write `match other_element` inside your element file, stop. Add a helper to `LowerCx`/`EmitCx` instead so the next contributor doesn't copy your hack.

### Job 3: Driver device (`<tmp102>`, `<ili9341>`)

A device is **a driver bound to a component**. Declare the binding in KDL, resolve it in `lower`:

```kdl
element "tmp102" section="devices" {
  binds "i2c_component"
  runtime_item "espforge_drivers::Tmp102"
  cargo_dep "tmp102-driver" "0.1"
  example "<tmp102 i2c_component=\"$temp_bus\" />"
}
```

`lower` resolves the bound component, claims `sda/scl` via that component, `emit` adds a `Devices` field. No `route_driver`, no directory-scanning `build.rs`, no `reject_if_present`. If `binds` target doesn't exist, the error says so.

## 5. Ruthless cut list — delete this, in this order

| Delete | Why it's user-hostile | Replace with |
|---|---|---|
| `espforge-options/build.rs` + `generated_catalog.rs` + `generated_options.rs` | Precomputes what `catalog::load` already computes in microseconds; forces committed generated code + CI diff guard | `LazyLock::new(\|\| load(SCHEMA_KDL))` + one test that schema loads |
| Bindings `build.rs` writing into `src/` + committed `generated_*.rs` | Fights `cargo publish`, needs `git diff --exit-code` babysitting | Emit to `OUT_DIR` + `include!`, delete from git, drop CI step |
| 4 KDL walkers (`option_literal`, `element_literal`, ...) | 4 ways to misparse one file | One `espforge-schema` model (`ElementDef`, `AttrDef`) shared by loader + generator |
| `Refs::resolve` hard-coding `i2c`/`spi`/`gpio`/`uart` + `key.contains("component")` | Second source of truth, silently wrong for new buses | Iterate `ref to=` + `section=` from schema |
| `PeripheralKind` enum in `espf` | Third copy of section membership | `section="peripherals"` query |
| Per-element `*_issue` / `describe` fns + duplicated `(only ...)` strings in `board.rs` ×2 + `devices/mod.rs` + snapshots | Must remember 5 places or tests fail mysteriously | One generic `issue_to_diagnostic(&ElementDef, Issue)` + `ELEMENTS.iter().filter(section).map(tag)`; optional `message=` override in KDL |
| `for_message(title) -> code` + 70-entry title table + `E0999` fallback | Reword a title, silently break public JSON contract | `Diag::at_node(CODE, node, title, msg)`; test: every code unique, well-formed, referenced; reserve numeric block per section |
| `route_driver(tag) -> Result<(), String>` + `drivers/generated_registry.rs` + dir-scan `build.rs` + `reject_if_present` | Fake mechanism that rejects all real devices | `binds` in KDL + `Element::lower` resolving it |
| `MATRIX`, `CELLS = 30`, hand-listed transcripts, per-chip golden special-casing | Contributor must edit test harness to add hardware | Auto-discover from `examples::categories() × offered_chips() × example.chips/runtime`; keep committed goldens + `UPDATE_GOLDENS=1` |
| `RUNTIME_VERSION = "0.6.0"`, duplicated `NOT_YET_SUPPORTED` in `chips.rs` + `doctor.rs` | Rot by copy-paste | `env!("CARGO_PKG_VERSION")`, one constant |
| Concrete `I2cBus::build(I2C0)` / `SpiBus::build(SPI2)` | Lowering allows `i2c1`/`spi2` that can't compile | `impl Instance + 'static` like `UartDevice` already does; single `mod hw` + `pub use hw::*` for the `cfg(any(riscv32, xtensa))` wall |

If a refactor doesn't delete one of these, deprioritize it.

## 6. Schema is the only form to fill in

KDL already holds tag, attrs, kinds, ranges, defaults, `ref to=`, choices, examples. Add three optional keys and stop:

- `binds "i2c_component"` — for devices.
- `message="..."` — wording override when generic diagnostic text isn't good enough.
- `runtime_item` / `cargo_dep` — collected by `EmitCx`, rendered by `cargo.rs`.

Coherence is tested, not documented:

- Every schema element has a registry entry unless `status="planned"`.
- Every registry entry has a schema element.
- Every `ref to=` / `binds` target exists.
- `planned ⇔ unregistered` (fixes today's `wifi`-is-planned-but-builds lie).

New `ref` kinds (e.g. `can`, `adc`) require zero code changes — they flow through `ref to=`.

## 7. Minimal element API — copy `led.rs`, don't learn a framework

```rust
trait Element { const TAG: &'static str; fn lower(node: &ParsedNode, cx: &mut LowerCx) -> Result<(),()>; }
trait Lowered { fn id(&self) -> &Ident; fn json(&self, b: &Board) -> Value; fn emit(&self, b: &Board, cx: &mut EmitCx); fn overlay_attr(&self, a: &str) -> Option<String>; }
```

- Keep `enum Component`, add one macro dispatch line per variant (keeps `Clone`/`PartialEq`/exhaustiveness). `Box<dyn Lowered>` only if third-party elements become real.
- `LowerCx` owns: typed `attrs::<T>()`, `gpio()/i2c()/spi()/uart()/resolve_ref()`, `claim_pin()`, `ident()`, `push()`.
- `EmitCx` owns: `field()`, `init()`, `once()`, `safety_comment()`, `cargo_feature()`, `runtime_feature()`, `needs_heap()`, `declare_runtime_item()`.
- Each `emit` declares `runtime_items()`; test checks them against runtime's published `RUNTIME_EXPORTS`. Catches `Led::new` renames without running the ignored compile test.

New contributor docs = one page: "copy `led.rs` → rename → fill `lower/json/emit` → add example → run contract test". If it needs more, simplify the API.

## 8. Diagnostics: codes are typed, messages teach

```rust
// never again:
EspfError::at_node(n, "invalid led", msg)  // code guessed from prose → E0999
// always:
Diag::at_node(codes::COMPONENT_INVALID_BUZZER, n, "invalid buzzer", "gpio '99' doesn't exist. Use gpio='0'..'7' on esp32c3.")
```

Rules: code passed at construction, never derived from title. Message states what was wrong + valid range + where to look. Test asserts no unreferenced codes, no unregistered codes emitted.

## 9. Tests that remember for you

1. **Per-element contract test (the big win).** For every registry element × every schema `example`: wrap in minimal file → `check` → `lower` → codegen → parse with `syn` → assert `json()`/`overlay_attr` don't panic → assert codes registered. Adding `<buzzer>` with one example gets full coverage free.
2. **Schema ⇔ registry coherence** (see §6).
3. **Auto-discovered goldens.** No `MATRIX` edits. Goldens stay committed, `UPDATE_GOLDENS=1` workflow unchanged. `EmitCx::once` appends in first-use order so bus prelude order stays stable.
4. **Diagnostics corpus stays**, but `(only ...)` text is derived so it only changes on real support changes.
5. Keep ignored full-compile test; add cheap `runtime_items ⊆ RUNTIME_EXPORTS` check as aということ normal test.

## 10. Migration in pain order (goldens byte-identical each step)

- **P0 — Stop lying about devices.** Implement `binds`, migrate `tmp102` then `ili9341`, delete `reject_if_present` + stub registry. Fix or remove `ssd1306_example` (no schema element today) and un-nest `05.Networking/wifi/http_example` (undiscoverable, stale syntax). Until this ships, "add a device" has no path.
- **P1 — Pure deletions.** Options `build.rs` → `LazyLock`; bindings → `OUT_DIR`; one `NOT_YET_SUPPORTED`; version from Cargo; derive `(only ...)` strings (one reviewed snapshot update via `UPDATE_DIAGNOSTICS=1`).
- **P2 — Typed codes.** Thread `DiagCode` through, delete `for_message`. Mechanical.
- **P3 — Schema-driven refs.** `Refs::resolve` over `ref to=` + section; drop `PeripheralKind`; add coherence test against existing generated list.
- **P4 — Pathfinder: `led` only.** New `espforge-elements`, registry, `LowerCx`/`EmitCx`, enum dispatch. Migrate `led` end-to-end, `01.Basics.blink.*` goldens unchanged. Then `button`, then buses (exercise `once()`), then `wifi`. Delete each old `match` arm as you go; delete the `match` when the last arm goes.
- **P5 — Auto goldens + contract test + generic `Instance` runtime.**

Phase 0 from `better.md` stands: add throwaway `<buzzer>` on current code, count sites, keep as regression metric. Goal after: 1 schema block + 1 file + 1 registry line + runtime file + example.

## 11. What we will NOT do

- No dynamic plugins, no proc-macro DSL, no external element registry.
- No `Box<dyn Lowered>` until third-party elements are a stated goal.
- No new config keys, no new sections, no pin-validity checker in this pass (schema claims pins are chip-valid but nothing checks — file it separately, don't smuggle it in).
- No golden coverage expansion beyond c3/s3 in this pass (file it; don't block device work on it).

## 12. Definition of done

- [ ] Throwaway `<buzzer>` on new code touches ≤6 places across ≤3 crates (§4), vs ~20 sites / 9 crates today.
- [ ] New contributor checklist is 5 lines: KDL block with example → copy `led.rs` → registry line → runtime file + `pub use` → example dir. Codes only if new error kinds.
- [ ] `cargo test` tells you the missing step by name (no `E0999`, no silent snapshot-only failures).
- [ ] `route_driver`, `reject_if_present`, committed generated files, `for_message`, `MATRIX`/`CELLS` are gone.
- [ ] Goldens + diagnostics corpus green; `ili9341`/`tmp102` pass `check`; `ssd1306` either has an element or no example.
