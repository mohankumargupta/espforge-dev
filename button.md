# button.md — button and button_async, end to end, in the new architecture

The phase-2 slice of `finalplan.md`, cut to the two examples that need an
element: `02.Digital/button` and `02.Digital/button_async`. It puts the second
real element through the framework, adds the second `Component` variant, and
widens the oracle to the `esp32s3` and `embassy` cells that `blink.md`
deliberately left out — which is a prerequisite, not an extra: five of button's
six declared cells are `esp32s3` or `embassy`, and `button_async` exists in no
other form.

Its job is to answer one question the framework cannot answer with a single
element: **is a second component its own file plus its own schema block, or does
it reach into the registry, the emitter or the runtime?** The measured answer is
the phase-1 buzzer-drill number, so it is written down in T5.

Everything else in phase 2 — the recipe tier, `check --format json`, the
contract test, tier enforcement — gets its own doc after this one. If a step
here does not change a byte of a generated project or prove that the machinery
is real, it is not in this slice.

---

## 1. The gate

Three checks. "Running end to end" means 1 and 2; 3 is what a user does. The
standing condition on all of them: **the two existing goldens
(`01.Basics.helloworld.esp32c3.blocking`, `01.Basics.blink.esp32c3.blocking`)
stay byte-identical.**

1. **Byte-identical.** `cargo test -p espforge --test golden_projects` generates
   every declared cell in-process and compares the whole tree — 13 files each —
   against `tests/golden/`. After this slice the matrix is:

   ```
   01.Basics.helloworld.{esp32c3,esp32s3}.{blocking,embassy}   4 cells (2 new)
   01.Basics.blink.{esp32c3,esp32s3}.{blocking,embassy}        4 cells (3 new)
   02.Digital.button.{esp32c3,esp32s3}.{blocking,embassy}      4 cells (all new)
   02.Digital.button_async.{esp32c3,esp32s3}.embassy           2 cells (all new)
   ```

   14 cells, 12 of them new. Every new tree is **moved** from
   `legacy/espforge/tests/golden/`, never produced by `UPDATE_GOLDENS=1`. The
   `embassy` cells of `button` are generation goldens only: their `src/app.rs`
   is the example's blocking app, exactly as the plan notes for `helloworld`.
2. **It compiles.** `cargo check` on the generated `button` (`esp32c3`,
   `blocking`) and `button_async` (`esp32c3`, `embassy`) projects, with
   `espforge-runtime = { path = ... }` pointing at this checkout. `button_async`
   is why this check matters: it is the first generated project that compiles
   the runtime's `embassy` feature and the generated `signal!` macro.
3. **The command a user types.**
   `espforge gen --example 02.Digital/button --out /tmp/button` (and the same
   for `button_async`) writes the tree, exit 0, and `diff -r` against the golden
   is empty.

The oracle is the **legacy tree**. Button's lowering, wording and emitted tokens
all exist there; port them, do not retype them.

---

## 2. What button's bytes actually depend on

Button is blink plus exactly three facts: a second component exists, it owns
GPIO9, and its pin is an input. The delta against
`01.Basics.blink.esp32c3.blocking` is the scope argument:

| File | Difference | Produced by |
|---|---|---|
| `src/generated.rs` | `pub button: espforge_runtime::Button`, `Button::new(unsafe { registry.peripherals.GPIO9.clone_unchecked() }, true)`, one more `// SAFETY` line | the existing `EmitCx` path, driven by the new element |
| `diagram.json` | the example's own overlay with two component pins (`{{ red_led.pin }}` and `{{ button.pin }}`) | `overlay.rs`, unchanged |
| `Cargo.toml`, `wokwi.toml`, `README.txt`, `src/bin/main.rs` | `button` instead of `blink` | existing emitters, driven by `Meta` |
| `src/app.rs` | the example's `app/rust/app.rs`, verbatim (`component!(ctx, button)`, `is_pressed()`) | existing copy |
| `.espforge/manifest.json` | new hashes | existing manifest |
| `build.rs`, `src/lib.rs`, `.cargo/*`, `rust-toolchain.toml` | **identical bytes** | unchanged code |

`button_async` differs from `button` on `esp32c3` in the files the runtime
selects (plus the name-derived `README.txt`, `wokwi.toml` and its own diagram
positions):
`src/lib.rs` (the `signal!` macro, emitted only for `Runtime::Embassy`),
`src/bin/main.rs` (`#[esp_rtos::main]`, the spawner, `esp_rtos::start`),
`Cargo.toml` (`esp-rtos`, `embassy-executor`, `embassy-time`, `embassy-sync`,
`espforge-runtime` with `features = ["embassy"]`), plus its own `app.rs`. All of
those branches are already ported into `espforge-emit`; what this slice adds is
a second project that exercises them and a runtime type that compiles under
them.

A crate is in this slice only if it changes one of those bytes, or if check 2
cannot run without it.

| Crate | Why it is here now | What it contains at the end of the slice |
|---|---|---|
| `espforge-diagnostics` | Two new failures need their legacy numbers | `COMPONENT_INVALID_BUTTON` (E0602), `CONFLICT_DIRECTION` (E0703) |
| `espf` | Already reads everything button writes | unchanged |
| `espforge-schema` | One schema block and one vocabulary assertion | a `button` element; the phase-1 vocabulary test counts eight |
| `espforge-elements` | The element, the second variant, `OwnerKind::Button` | `components/button.rs`, one `component_enum!` line, `OwnerKind::Button`; the generated `Attrs` pick the schema block up with no code change |
| `espforge-emit` | Only if T1 finds drift | expected: unchanged |
| `espforge-core` | Already lowers through the registry | unchanged |
| `espforge-runtime` | Only for check 2 | `Button` |
| `espforge-examples` | The two examples | `02.Digital/button`, `02.Digital/button_async` |
| `espforge` | Only for check 3 | unchanged |

No new dependencies. `linkme`, `quote`, the generated `Attrs` and the
`embassy` feature of `espforge-runtime` all already exist.

Notably absent even though phase 2 needs them eventually: the recipe block and
its generator, `Lowered::json`, `check`, `--format json`, `RUNTIME_EXPORTS` and
`runtime_items`, the per-element contract test, tier enforcement, the
diagnostics corpus and its snapshots, the no-panic audit, buses, devices, Wi-Fi,
and chip-validity checks for pins. None of them can change a byte of button, and
the first two would be designed against a single example if built now.

---

## 3. Steps

Each step ends green and is one commit. "Verify" is the command that has to pass
before the next step starts.

### T0 — branch

`git switch -c button` from `blink`. Nothing else.

### T1 — widen the oracle: `esp32s3` and `embassy` on the phase-1 examples

**Do**

- Copy the six missing phase-1 golden trees from
  `legacy/espforge/tests/golden/`:
  `01.Basics.helloworld.{esp32c3.embassy,esp32s3.blocking,esp32s3.embassy}` and
  the same three for `01.Basics.blink`.
- Update both embedded `example.toml` files:

  ```toml
  chips    = ["esp32c3", "esp32s3"]
  runtimes = ["blocking", "embassy"]
  ```

- Run the harness. Expected: **zero code changes** — `chip.rs`, `config.rs`,
  `cargo.rs`, `wokwi.rs` and the `embassy` branches of `rust.rs` were ported in
  H5 and are exercised here for the first time. If a cell is red, fix that cell's
  emitter branch in its own commit-sized edit, with the diff in the message;
  do not touch the golden to make it pass.
- Nothing in the two existing c3/blocking trees may change.

**Verify:** `cargo test -p espforge --test golden_projects` green for 8 cells;
`git status` clean.

If T1 turns out to need more than drift-fixing (say, the Xtensa `build.rs` or
`rust-toolchain.toml` path was cut, not ported), stop and report before
continuing: it changes the size of this slice, and it belongs in a separate
commit series that can be reverted without touching button.

### T2 — schema block, codes, the second variant, the runtime type

**Do**

- `espforge-schema/src/espf.schema.kdl`, after `led`, worded like the legacy
  block and with the same self-contained-document examples `led` uses:

  ```kdl
  element "button" section="components" {
      summary "A push button on a GPIO input"
      description #"A component over <gpio>: gpio="$<gpio id>" must name an input pin."#
      attr "id" kind="ident" required=#true
      attr "gpio" kind="ref" to="gpio" required=#true
      attr "pull_up" kind="bool" default="false" {
          help "true when the pin has an external pull-up (button to GND)."
      }
      example "esp32c3, blocking" "…complete document, one <gpio>, one <button>…"
      example "pull-up" "…same, pull_up=\"true\"…"
      see_also "peripherals/gpio"
  }
  ```

  Update the file's header comment (it currently says `gpio` and `led` are the
  only elements outside `# espforge`, and that `button` arrives later).
- `espforge-diagnostics/src/codes.rs`: add, in their legacy blocks with legacy
  comments, `COMPONENT_INVALID_BUTTON = "E0602"` and
  `CONFLICT_DIRECTION = "E0703"`. `ALL` picks them up; the well-formedness and
  uniqueness tests must stay green without edits.
- `espforge-elements/src/model.rs`: `OwnerKind::Button` and its `tag()`. The
  doc comment that says `Button` returns with `<button>` goes.
- `espforge-elements/src/model.rs`: one line in `component_enum!`:
  `Button(crate::components::button::Button),`. Nothing else in the enum macro,
  `Component::id`, `Lowered` forwarding or any emitter changes.
- `espforge-elements/src/components/mod.rs`: `pub mod button;`.
- `espforge-runtime/src/lib.rs`, inside the existing single `mod hw`: port
  `legacy/espforge-runtime/src/components/button.rs` verbatim — `Button` holding
  `RefCell<Input<'static>>` and `pull_up`, `new(pin, pull_up)` selecting
  `Pull::Up`/`Pull::None`, `is_pressed()` inverting on `pull_up`, and
  `#[cfg(feature = "embassy")] async fn wait_for_pressed()` waiting for low or
  high. `pub use hw::*` picks it up; no new `cfg` and no new feature.
- `espforge-schema/src/lib.rs`: `the_embedded_schema_loads_and_has_the_phase_one_vocabulary`
  becomes eight elements, two in `components`, and asserts `button`'s `gpio`
  `ref to="gpio"` and `pull_up` default `false`. Do not add a second vocabulary
  test; this is the same one growing.

**Verify:** `cargo test -p espforge-schema`, `cargo test -p espforge-diagnostics`,
`cargo build -p espforge-runtime` (host build: `Button` is target-only, so
nothing host-side changes), and `cargo test -p espforge-elements` — the
coherence tests must still pass, which proves the schema block alone registered
the element and that `espforge-elements` builds with the generated
`button::Attrs` in `OUT_DIR`. `git status` clean.

### T3 — `<button>`, and the direction-clash wording the framework got wrong

**Do**

- `espforge-elements/src/components/button.rs`, mirroring `led.rs`: read the
  generated `Attrs`, take the referenced `<gpio>`, refuse a declared direction
  that is not `input` (`E0609`, message `wrong gpio direction`,  legacy label
  `"<button id=\"x\"> needs an input pin, but `g` has direction=\"output\""`),
  refuse a non-identifier id (`E0602`, `invalid button`, the legacy
  `require_slug` wording), claim the pin as `PinUse::Input` with
  `OwnerKind::Button`, and push `Button { id, pin, pull_up }`.
  `Lowered::emit` prints the field, the `Button::new(...)` initialiser and the
  `SAFETY` line; `overlay_attr("pin")` returns the number. `Attrs` failures map
  through a `describe` function with the legacy `button_issue` labels
  (`needs gpio="$<gpio id>"`, `Missing required attribute '…'`,
  `pull_up="…" must be "true" or "false"`).
- **Fix the conflict wording at the same time.** `element.rs::conflict_diagnostic`
  currently reports every conflict as `pin already used`/`E0702`. The first
  reachable direction clash in the project is `led` (output) vs `button`
  (input), and legacy reports it as code `E0703` title `pin direction clash`,
  with the existing owner slugged and the incoming one as written:

  ```
  error: pin direction clash
  <led id="red_led"> and <button id="btn">: one pin cannot be both input and output
  ```

  Change it to return `(DiagCode, &'static str, String)`; `Direction` →
  `CONFLICT_DIRECTION`, `Pin`/`SelfClash` → `CONFLICT_PIN_USED`. Update the
  single call site in `led.rs`. This is a real bug `blink` could not reach,
  found by the second element — it belongs in this commit, with the legacy
  diagnostics snapshot quoted in the message, not in a later "wording" sweep.
- Tests with the button snippet (five sections, two gpios, `led` + `button`):
  - lowers to two components, `red_led` on pin 7 output and `button` on pin 9
    input, `pull_up == true`, both pins claimed by their component;
  - `direction` absent on the button's gpio is allowed (legacy behaviour);
  - a declared `output` gpio for a button is `E0609`;
  - `pull_up` defaults to `false` and `pull_up="sideways"` is `E0602`;
  - `$gpio99` is `COMPONENT_UNKNOWN_GPIO`; an id like `9btn` is `E0602`;
  - two buttons on one pin are `E0702`; `led` + `button` on one pin is `E0703`
    with the exact legacy label;
  - `overlay_attr("pin")` is `"9"`, and `button` in `# peripherals` is still
    `PERIPHERAL_UNSUPPORTED`, still naming only `gpio` as known there.
- Do **not** touch `lower.rs`, the registry, `EmitCx` or any emitter: if either
  needs a change, the framework claim failed and that is worth stopping for.

**Verify:** `cargo test -p espforge-elements`; the golden tests still green (this
step emits nothing new because no example declares a button yet).

### T4 — the two examples, six cells

**Do**

- Copy `legacy/espforge-examples/src/examples/02.Digital/button/` and
  `button_async/` into `espforge-examples/src/examples/02.Digital/`:
  `example.espf.txt`, `app/rust/app.rs`, `diagram.json`, plus the test-only
  manifests:

  ```toml
  # button
  chips    = ["esp32c3", "esp32s3"]
  runtimes = ["blocking", "embassy"]

  # button_async
  chips    = ["esp32c3", "esp32s3"]
  runtimes = ["embassy"]
  ```

  `button_async`'s only runtime is `embassy`: its `<runtime type="embassy" />`
  is the example, and a `blocking` cell would be a project whose app cannot
  compile and whose .espf says otherwise.
- Copy the six golden trees from `legacy/` under the same slugs. Do not produce
  them with `UPDATE_GOLDENS=1`; a golden the code under test wrote proves
  nothing, and the harness already refuses a declared cell with no golden.
- Run the harness. The only expected failures are the ones T3 already fixed; if
  `src/generated.rs` differs, the diff is the element's tokens, not the emitter.
- Extend `espforge/tests/generated_projects_compile.rs`: one list of
  `(example, chip, runtime)` cases, now `button`/`esp32c3`/`blocking` and
  `button_async`/`esp32c3`/`embassy` in addition to the two phase-1 cases. One
  shared `CARGO_TARGET_DIR`. Do not add the `esp32s3` cases: that is an Xtensa
  toolchain, not this slice.

**Verify:** the three checks in §1. Check 2 needs stable rustc 1.99.0, the
`riscv32imc-unknown-none-elf` target and network access for `esp-hal`,
`esp-rtos` and the embassy crates; record what the run needed in the commit
message. The `button_async` compile is the first proof that the runtime's
`embassy` feature and the generated `signal!` macro are real.

### T5 — close out and measure

**Do**

- `cargo fmt --check`, `cargo clippy --workspace`, `cargo test --workspace`
  from a clean `target/`.
- **Walk the buzzer drill** with `<button>` itself as the measurement: list every
  file this slice touched to add an element, minus the widening and the two
  examples' harness edits. Expected:

  | Site | File |
  |---|---|
  | schema block | `espforge-schema/src/espf.schema.kdl` |
  | diagnostics | `espforge-diagnostics/src/codes.rs` (E0602, E0703) |
  | element | `espforge-elements/src/components/button.rs` |
  | module line | `espforge-elements/src/components/mod.rs` |
  | variant + owner kind | `espforge-elements/src/model.rs` (one line each) |
  | runtime type | `espforge-runtime/src/lib.rs` |
  | example | `espforge-examples/src/examples/02.Digital/button/…` |

  Seven files, zero registry edits, zero `match` outside an element's own file,
  zero emitter edits. That is not the plan's four: the plan's number assumes a
  recipe element, and this slice deliberately writes the hand-written tier.
  Record it as the baseline `recipe.md` has to beat — the bookkeeping rows
  (diagnostics, module line, variant + owner kind) are exactly what the recipe
  removes for a pin component.
- Update the phase-2 line in the plan docs only if the slice changes what it
  promises; otherwise leave `finalplan.md` alone.

**Verify:** everything green, `git status` clean, and the count recorded.

---

## 4. What button proves (raw material for `recipe.md`)

Written down now so the recipe is designed from evidence, not memory. `led` and
`button` differ on exactly:

| Fact | `led` | `button` |
|---|---|---|
| schema tag / section | `led` / components | `button` / components |
| referenced peripheral | `gpio`, `ref to="gpio"` | same |
| required direction | `output` | `input` |
| `PinUse` claimed | `Output` | `Input` |
| bool attribute | `active_low`, default `false` | `pull_up`, default `false` |
| runtime type | `espforge_runtime::Led` | `espforge_runtime::Button` |
| constructor args | pin, `active_low` | pin, `pull_up` |
| overlay attribute | `pin` | `pin` |
| diagnostics | E0601, E0608, E0609 | E0602, E0608, E0609 |

That table is the recipe grammar: pin source, direction, emitted type, bool
passthrough, and the per-tag code for "invalid <tag>". Nothing else differs, so
a recipe that cannot express every row above is the wrong recipe.

It also shows what is **not** recipe-shaped yet: `button_async` adds no element
behaviour at all — its async method comes from the runtime type and the
`embassy` feature. Generated runtime types (phase 2's "generated runtime types")
are therefore a separate mechanism from the recipe, and `recipe.md` must say so
rather than fold them together.

---

## 5. Deliberately absent

The recipe block, its generator, migration of `led`/`button`, tier enforcement
and the two-site buzzer gate (all `recipe.md`); `Lowered::json`, `board_json`,
`check`, `check --format json` and the JSON half of the no-panic audit
(`check.md`, because `check` itself does not exist yet — the CLI is still only
`gen`); `RUNTIME_EXPORTS`, `runtime_items` and the per-element contract test
(`contracts.md`); the diagnostics corpus and snapshots (`E0602`/`E0703` are
tested at the element, not through the CLI); `build`, `version`, transcripts,
`new`, `examples`, `espf`; buses, devices, services, Wi-Fi; custom Wokwi chips;
duplicate-id handling; pin validity per chip; and the `esp32s3` **compile**
check, which needs the Xtensa toolchain.

Also untouched: `prettyplease`/`syn`/`quote`/`toml_edit` versions, and every byte
of helloworld and blink.

---

## 6. Decisions to make before T1

My recommendation is bold. Say which you want differently and I will fold the
answer back into this file before writing code.

**Q1 — Does the widening (the six missing `esp32s3`/`embassy` phase-1 cells) land
in this slice?**
(a) **yes, as T1, before any button work.** (b) its own `widening.md`, and button
only on `esp32c3`/`blocking` for now.
Five of button's six cells are `esp32s3` or `embassy` (only `esp32c3`/`blocking`
needs neither), and `button_async` is embassy-only, so (b) splits phase 2 in
half and leaves the async example — the whole point of the async button API —
undone. (a) is expected to be a copy-and-declare step with zero code; if it is
not, T1's verify stops the slice before button muddies the diff.

**Q2 — How many button cells does the gate cover?**
(a) **all six declared cells (`button` ×4, `button_async` ×2).** (b) only
`esp32c3` (`blocking` + `embassy`).
(b) is two goldens cheaper and keeps the first async project to `button_async`,
but `esp32s3` is then first exercised by phase 3's buses with two elements in
flight. The trees already exist; copying them is the cheap part.

**Q3 — Hand-written element now, or the recipe tier in this doc?**
(a) **hand-written `button`, `recipe.md` next.** (b) define the recipe block
here and ship `led` + `button` through it.
The plan's own order is "with `led` and `button` as two data points, define the
recipe tier" — you cannot extract a recipe before the second element exists and
its goldens pass, and §4 exists to make that extraction evidence-based. (b)
would also delay the compile proof of `button_async` behind a generator rewrite.
The cost of (a): the phase-2 buzzer gate (two sites) is not met at the end of
this doc; it is met by `recipe.md`, and T5 records the honest count (seven
files, three of them bookkeeping).

**Q4 — Does `check --format json` belong here?**
(a) **no, `check.md`.** (b) yes, with `Lowered::json` and `board_json`.
`check` does not exist in the new CLI yet — only `gen`. (b) drags source
resolution, rendering, transcripts, the JSON schema and the no-panic audit into
an element slice, and `Lowered::json` would be a framework method with exactly
one consumer. Port `board_json` with the command that prints it, as
`helloworld.md` did for `gen`.

**Q5 — Is an undeclared `<gpio direction>` still allowed for a button?**
(a) **yes: only a declared non-`input` is `E0609`** (legacy behaviour, matches
`led`). (b) require `direction="input"` on the referenced gpio.
(b) is stricter and arguably better, but it changes behaviour for every existing
ESPF file and every golden that omits `direction` — a diagnostics decision with
a corpus, not a side effect of adding an element.

**Q6 — How much `Button` API?**
(a) **exactly the legacy three: `new`, `is_pressed`, `wait_for_pressed`.** (b)
add `is_released`/`wait_for_released`.
Nothing in the examples or goldens calls (b); the runtime grows one method per
call site that needs it, not per symmetry.

---

## 7. Risks

| Risk | Signal | Action |
|---|---|---|
| Widening is not a copy step (an emitter branch was cut, not ported) | T1 red on an `esp32s3`/`embassy` file | Stop and report; fix that branch alone before button, never edit the golden |
| First async project reveals the runtime's `embassy` feature is wrong | T4 check 2 | Fix `espforge-runtime`'s feature wiring; the generated `Cargo.toml` is the oracle |
| Direction clash keeps the wrong code or wording | T3 test / future corpus | Fix `conflict_diagnostic` now, quote the legacy snapshot in the commit |
| Element reaches into the registry or the emitter | Diff outside `button.rs`, `model.rs`, the schema and the runtime | The framework claim failed: stop and design, do not add a `match` |
| `button` golden produced instead of moved | A cell that always passes | Copy from `legacy/`; the harness refuses a declared cell with no golden |
| `button_async` compile needs network/target | T4 | Run by hand, record what it needed; do not assume CI has it |
| `esp32s3` compile attempted | T4 | Out of scope: Xtensa toolchain, phase 7 |

---

## 8. Commits

```
T0  (branch only; no commit)
T1  test(goldens): widen phase-1 oracle to esp32s3 and embassy
T2  feat(elements): button schema block, Button runtime type, Component::Button
T3  feat(elements): <button> element; direction clash is E0703
T4  feat(examples): button and button_async, six byte-identical cells
T5  test(cli): button and button_async compile; drill measured
```

Every commit is `cargo test --workspace` green and leaves helloworld and blink
byte-identical. T4 is the first proof that button is real; T1 is the first proof
that the widened emitters are.

---

## 9. If you say "go"

I start at T1 and go in order, and I stop after T4 with the six byte-identical
trees, the compile results, and the exact toolchain/target/network that run
needed. Questions I need answered first: Q1–Q6.

---

## 10. Measured (T5)

The slice shipped as five commits on `button`, all green. The gate:

- 14/14 golden cells byte-identical (helloworld 4, blink 4, button 4,
  button_async 2), every new tree moved from `legacy/`.
- `cargo check` passed for button/esp32c3/blocking and
  button_async/esp32c3/embassy (rustc 1.99.0, `riscv32imc-unknown-none-elf`,
  crates.io for esp-hal, esp-rtos and the embassy crates).
- `espforge gen --example 02.Digital/button` (and `button_async`) exits 0 and
  `diff -r` against the golden is empty.
- `cargo fmt --check`, `cargo clippy --workspace`, `cargo test --workspace`
  green; `git status` clean.

**The drill, measured.** `git diff T1..HEAD` for the element:

| Site | File |
|---|---|
| schema block | `espforge-schema/src/espf.schema.kdl` |
| diagnostics (E0602, E0703) | `espforge-diagnostics/src/codes.rs` |
| element | `espforge-elements/src/components/button.rs` |
| module line | `espforge-elements/src/components/mod.rs` |
| variant + owner kind | `espforge-elements/src/model.rs` |
| runtime type | `espforge-runtime/src/lib.rs` |
| example | `espforge-examples/src/examples/02.Digital/button/…` |

Seven files, three of them bookkeeping. Zero registry edits, zero emitter
edits, zero `match` outside an element's own file (the dispatch the enum
macro generates is not a site).

Three further files were touched, and none is element bookkeeping:

- `espforge-schema/src/lib.rs` — the phase-1 vocabulary test grows to eight
  elements (the plan's one test, not a new one).
- `espforge-elements/src/lower.rs` — one refutable `let` in an existing blink
  test became `let … else` when the enum grew a second variant, plus button's
  lowering tests.
- `espforge-elements/src/{element.rs,components/led.rs}` — the E0703
  direction-clash fix: a real bug `blink` could not reach, found by the second
  element.

`espforge/tests/generated_projects_compile.rs` gained the two new cases (the
harness, not the element). This seven-file / three-bookkeeping number is the
baseline `recipe.md` has to beat: the recipe is expected to remove all three
bookkeeping rows and leave schema block + example.
