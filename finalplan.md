# plan.md: espforge, rebuilt from the ground up

> Final plan. It supersedes `better.md`, `better2.md` and `better3.md`, which are the audit,
> the contributor-UX plan and the "components as data" plan. Where they disagree, section 2
> records which one won.
>
> **Goal:** adding a component or device means writing a schema block and an example (for most
> elements), or one extra file (for the unusual ones). It never means editing nine crates.
>
> **How we get there:** a new workspace, built vertically one example at a time. Phase 1 gets
> `helloworld` and `blink` working end to end and nothing else.

---

## 1. Ground rules

1. **"Ground up" means new structure, not retyped logic.** The new workspace has new crate
   boundaries and new seams. Proven leaf logic is *ported with its tests*: the parser, the
   pin-claim graph, the options resolver, the Cargo/config/Wokwi/Rust emitters, the
   golden comparators. Re-deriving those would only create new bugs.
2. **The old tree is an oracle, not a source to copy blindly.** It moves to `legacy/`, outside the
   workspace. Its golden trees are the acceptance tests. The packed snapshot we planned from
   has comments stripped and at least two files that look truncated (`help_view.rs`
   `render_examples`, and `panic_audit.rs`, which uses an undefined `source`). Port from the real
   repo, verify each port compiles, and don't assume the snapshot is complete.
3. **One fact, one place.** Tag, attributes, kinds, ranges, defaults, `ref to=`, choices and
   examples live only in `espf.schema.kdl`. Everything else is derived from it.
4. **Delete before abstracting.** No committed generated code, no `build.rs` that writes into
   `src/`, one KDL walker, no title-matched diagnostic codes, no hand-kept lists of
   "supported" elements.
5. **The repo remembers so the human doesn't.** There is no hand-edited golden `MATRIX`, no
   `CELLS = 30`, no registry list and no per-element `match` outside the element's own file.
   Loops and contract tests do the remembering.
6. **Errors name the next step.** Every failure says what is missing and where to put it. A
   diagnostic always carries a typed code from the moment it is constructed.
7. **Out of scope for the whole project:** dynamic plugins, a proc-macro DSL, external element
   registries, `Box<dyn Lowered>`, new pin-validity checking smuggled into a refactor.

## 2. Decisions taken (where the three docs disagreed)

| Question | Decision | Why |
|---|---|---|
| Registry: one `ELEMENTS` list (better2) or distributed (better3)? | **Distributed registration** with `linkme` from day one. | The last hand-kept list is the one that gets forgotten. Risk (dead-stripped slice) is covered by a "registry is non-empty" test. |
| `Component` as `enum` or `Box<dyn>`? | **Closed enum** with a one-line-per-variant dispatch macro. | Keeps exhaustiveness, `Clone` and `PartialEq`. Revisit only if third-party elements become a goal. |
| When do declarative "recipe" elements (better3 Tier 0) arrive? | **Phase 2**, extracted from `led` + `button`. | Designing a recipe from one example guesses. Two real examples tell us the shape. Phase 1 writes `led` by hand as a full `Element`. |
| Typed diagnostic codes | **From the first line of code.** Old code numbers are kept for ported errors. | Codes are a public JSON contract. Deriving them from prose is what produced `E0999`. |
| Generated code | **`OUT_DIR` only.** Schema is loaded lazily with `LazyLock`. | Nothing generated is committed, so there is no CI diff guard. |
| Golden oracle | **Byte-identical** to the existing golden trees; workspace version stays `0.6.0` until cutover. | A version bump changes the manifest checksums and drowns real diffs in noise. |
| Auto-discovered golden matrix | **From phase 1**, with a small test-only `example.toml` per example. | With two examples it's cheap now and painful to retrofit. |
| Wi-Fi `status="planned"` | **Deleted.** An element is in the schema if and only if it is registered. | Today `wifi` is "planned" in the schema but actually builds. |

## 3. Target workspace

Nine crates, down from twelve. Dependency direction is strictly left to right:

```
espforge-diagnostics ─► espf ─► espforge-schema ─► espforge-elements ─► espforge-emit ─► espforge-core ─► espforge (CLI)
                                                         espforge-runtime   (standalone, target-only)
                                                         espforge-examples  (standalone, embedded data)
```

| New crate | Absorbs (legacy) | Owns |
|---|---|---|
| `espforge-diagnostics` | `espforge-diagnostics` | `Diagnostic`, `DiagCode`, typed `codes`, human renderer (JSON renderer arrives in phase 2). **No `for_message`.** |
| `espf` | `espf` (minus `context::Refs`, minus `writer`) | Parser, `ParsedNode`, dotted keys, `+=`, value assembly. Errors are `Diagnostic`s with a code at construction. |
| `espforge-schema` | `espforge-espgenerate-template`, `catalog.rs`, `schema.rs`, both KDL `build.rs` walkers | `espf.schema.kdl`, the **single** KDL walker, `Schema { sections, elements, options }`, `LazyLock` embed, the generator for typed `Attrs`. |
| `espforge-elements` | `espforge-model`, `espforge-hardware`, `espforge-bindings`, `espforge-options` | Board IR and pin-claim graph, `Element`/`Lowered` traits, `LowerCx`, `EmitCx`, registry, core section (`project`/`chip`/`runtime`/`console`/`options`), options resolver, one file per element. |
| `espforge-emit` | `espforge-emit` | Cargo/config/toolchain/Wokwi/diagram/overlay/Rust emitters, file plan, ownership and drift manifest, comparators. |
| `espforge-core` | `espforge-core` | Pipeline (parse → lower → emit → write), source resolution, flag overrides. |
| `espforge-runtime` | `espforge-runtime` | `Logger`, `Delay`, `Led`, and later the rest. |
| `espforge-examples` | `espforge_examples` | Embedded examples. |
| `espforge` | `espforge` | CLI. |

Crates that disappear: `espforge-espgenerate-template`, `espforge-bindings`, `espforge-options`,
`espforge-hardware`, `espforge-model`.

## 4. Phases at a glance

| Phase | Theme | New examples that must work | Architecture it proves |
|---|---|---|---|
| **1** | Foundation + walking skeleton | `helloworld`, `blink` | Schema as single source, typed codes, element trait, distributed registry, auto goldens |
| **2** | Digital I/O and the recipe tier | `button`, `button_async` | Recipe elements (Tier 0), generated runtime types, JSON output, tier enforcement |
| **3** | Buses | `i2c_example`, `i2c_scanner`, `spi_example`, `uart_example` + async variants | `EmitCx::once`, shared resources, generic peripheral instances, Wokwi custom chips |
| **4** | Services and Wi-Fi | `wifi_station` | Cargo-feature and heap collection, services, chip-compat options |
| **5** | Devices | `ili9341_example`, `ssd1306_example` (or delete it), driver devices | `binds`, `cargo_dep`, devices as real mechanism |
| **6** | CLI parity and contributor tooling | (none new) | `new`, `examples`, `espf`, `doctor`, `dev new element` scaffolder, release |
| **7** | Cutover and hardening | (none new) | Retire `legacy/`, more chips in goldens, bump version |

Every phase gate has three standing conditions on top of its own: all earlier goldens stay
byte-identical, `cargo test` names any missing step by name (never `E0999`), and the
"buzzer drill" (section 7) is re-measured.

Phases 2–7 are intentionally short here. Each gets its own detailed plan when the previous one
ships, using what phase 1 teaches us.

---

# Phase 1: foundation, `helloworld` and `blink`

## 5.1 Scope

**In scope**

- Examples: `01.Basics/helloworld` and `01.Basics/blink`.
- Chips: `esp32c3` and `esp32s3` (the two with golden coverage). Other chips resolve through
  `esp-metadata-generated` automatically but are untested until phase 7.
- Runtimes: `blocking` **and** `embassy`. Both have existing golden cells, and the difference
  (a few emitter branches) is cheap to carry from the start.
- Elements: `project`, `chip`, `runtime`, `console`, `options` (the `# espforge` section),
  `gpio` (peripheral), `led` (component).
- Options: `unstable-hal`, `alloc`, `embassy` (derived), `probe-rs`,
  `stack-smashing-protection`, `wokwi`.
- Wokwi output: default `diagram.json` and `wokwi.toml`, plus overlay of an example's own
  `diagram.json` with `{{ board }}`, `{{ serialMonitor }}` and `{{ <component>.pin }}`.
- CLI: `check`, `gen`, `build`, `version`, and `-C`.

**Explicitly out of scope (do not start these)**

`button`, any bus (`i2c`/`spi`/`uart`), `wifi`/`ntp`/services, devices, `# devices` content,
`check --format json`, `new`/`examples`/`espf`/`doctor` commands, `--write-spec` and the
spec writer, custom Wokwi chips (`chip.json`/`chip.wasm`/`keep-zig-source`), `wifi`/`ble-trouble`
options, recipes and tiers, the scaffolder, and any chip beyond c3/s3 in tests.

Anything in the schema file in phase 1 must be registered. That is the coherence rule from day
one.

## 5.2 Oracle: what "working" means

Phase 1 is done when the new workspace produces **byte-identical** output to the legacy golden
trees for these 8 cells:

```
01.Basics.helloworld.{esp32c3,esp32s3}.{blocking,embassy}
01.Basics.blink.{esp32c3,esp32s3}.{blocking,embassy}
```

(Note: in the `embassy` cells the app text is the example's blocking `app.rs`. Those cells
are generation goldens only and are not compile-checked.)

Plus: generated `helloworld` and `blink` for `esp32c3`/`blocking` pass `cargo check` for
`riscv32imc-unknown-none-elf`.

## 5.3 Build order: a walking skeleton first

Do **not** build layer by layer. Build a vertical slice that runs `helloworld` end to end with
zero components, then widen. Each milestone ends green and committed.

### M0: Bootstrap and oracle

- New branch `rewrite`. Move the old tree to `legacy/` (excluded from `[workspace] members`).
- New root `Cargo.toml`: edition 2024, `rust-version = "1.95"`, version `0.6.0`, shared
  `[workspace.package]`, shared `[workspace.dependencies]` (including `linkme`).
- Create the nine empty crates with correct dependency edges. `cargo build` must pass on an
  empty workspace.
- Copy the **8 golden trees** and the two embedded examples (`helloworld`, `blink`) into the new
  layout. Add a test-only `example.toml` to each example:

  ```toml
  chips    = ["esp32c3", "esp32s3"]
  runtimes = ["blocking", "embassy"]   # default: the runtime named in the .espf
  ```

  It is read by the test harness only. `materialize` ignores it.
- One file, `espforge-emit/src/versions.rs`, holds every dependency version that appears in
  generated `Cargo.toml` files (esp-hal `~1.2.2`, esp-rtos `0.4.0`, esp-bootloader-esp-idf `0.6.0`,
  esp-println `0.18.0`, esp-backtrace `0.20.0`, esp-alloc `0.11.0`, embassy-executor `0.10.0`,
  embassy-time `0.5.0`, embassy-sync `0.8.0`, static_cell `2.1.1`, critical-section `1.2.0`,
  log `0.4.27`, runtime `0.6.0`). `RUNTIME_VERSION` becomes `env!("CARGO_PKG_VERSION")`.
- CI: `cargo fmt --check`, `cargo clippy`, `cargo test --workspace`, plus a separate job running the
  ignored compile test. **No `git diff --exit-code` step**, because nothing generated is committed.

**Done when:** empty workspace builds in CI; golden trees and examples are in place; goldens test is
present but red.

### M1: `espforge-diagnostics` and `espf` (ported)

- Port `Diagnostic`, `Severity`, `Label`, `Edit`, `Fix`, `DiagCode`, and the human renderer
  (`annotate-snippets 0.12`).
- `codes.rs` is **plain constants** with a numeric block per area (E01xx syntax and structure,
  E02xx core, E03xx options, E04xx references, E05xx peripherals, E06xx components, E07xx
  conflicts). Only define codes that phase 1 actually uses; keep the old numbers (E0101–E0104,
  E0201–E0209, E0301–E0315, E0401, E0502, E0503, E0601, E0608, E0609, E0701–E0703).
- **Delete** `for_message`, `dynamic_code`, the title table and `E0999`.
- Every diagnostic constructor takes a `DiagCode` as its first argument. There is no way to build one
  without it.
- Port `espf`: `parse_espf`, `ParsedNode`, `TagAttribute`, block comments, `// ` trailing
  comments, junk spans, structure errors (missing/duplicate sections), dotted keys, `+=`
  operators, object assembly (`Value`/`Object`). Port its unit tests with it. Parser errors
  become `Diagnostic`s directly (no `EspfError` intermediate). Drop `writer.rs` and `context::Refs`
  for now (reference resolution is rewritten schema-driven in M4).
- Tests: every `codes::*` constant is well formed (`E`/`W` + 4 digits) and unique.

**Done when:** all ported unit tests pass; `espf` and `diagnostics` have no dependency on any other
workspace crate except each other.

### M2: `espforge-schema`

- Create `espf.schema.kdl` **inside this crate** (it replaces the `-espgenerate-template` crate).
  Phase 1 content only:
  - sections: `espforge`, `peripherals`, `components`, `devices`, `services` (the last two exist
    because the file format requires all five sections; they contain no elements yet).
  - elements: `project`, `chip`, `runtime`, `console`, `options`, `gpio`, `led`.
  - options: `unstable-hal`, `alloc`, `embassy` (marked `derived=#true`), `probe-rs`,
    `stack-smashing-protection`, `wokwi`.
  - every element has at least one `example`. Examples for elements with references are written
    as **self-contained fragments** that include their prerequisites (for example `led` carries its
    own `gpio`), so the contract test can wrap them without inventing fixtures.
- One loader, `espforge_schema::load(&str) -> Result<Schema, Vec<SchemaError>>`, the only code in
  the workspace that walks KDL. It reports all problems with spans and checks: reserved names
  (`help`, `examples`, `overview`), unique tags across sections, `ref to=` targets exist,
  `see_also` targets exist, a summary and an example are present.
- `pub fn schema() -> &'static Schema` via `LazyLock::new(|| load(SCHEMA_KDL).expect(...))`.
- **Generator for typed `Attrs`:** a `build.rs` in `espforge-elements` that depends on
  `espforge-schema` as a *build-dependency* and reuses its model. It writes to `OUT_DIR` and is
  `include!`d. Port the coercion helpers (`attrs.rs`: `ident`, `text`, `flag_or`, `pin`,
  `parsed_or`, `reference`, ...) as ordinary source. The generator is the only code that interprets
  `kind`, `range`, `default` and `choices`.
- Tests: schema loads; every example fragment parses with no junk; every fragment element exists
  in the schema and uses only declared attributes and all required ones.

**Done when:** `cargo test -p espforge-schema` is green and `git status` shows no generated files
after a build.

### M3: Core section and options (in `espforge-elements::core`)

- Port `core_from_section` (project/chip/runtime/console/options) but build it on the generated
  `Attrs` types instead of hand-written attribute fetching. Output: `Meta` (name, description,
  chip, runtime, console) plus the raw `<options>` node.
- Port options: `Answers`, `Origin`, flag application, `resolve` (defaults, derived flags,
  `requires`, chip `compatible`, selection groups, jtag-serial check). Derived options come from
  the schema (`derived=#true`); the `runtime` element supplies the value of `embassy`. No
  `DERIVED` constant.
- One `chips.rs`: one `NOT_YET_SUPPORTED`, one `has_usb_serial_jtag` (prefer a metadata query;
  fall back to a constant with a comment saying why).
- Port the resolver's unit tests, translated to the new schema. Wi-Fi and BLE options are not in
  the phase-1 schema, so those tests move to phase 4.

**Done when:** a `# espforge` section plus flags resolves to `(Meta, ResolvedOptions)`, with the
same error locations as legacy for chip/runtime/console/option mistakes.

### M4: Element framework with **zero** elements

Build the seams before any element uses them, and prove them with test doubles.

- **Model.** Port `Ident`, `Pin`, `PinUse`, `OwnerKind`, `Claim`, `ClaimedPin`, `Conflict`,
  `ResourceGraph` (pin claims, direction clash, self clash) and `Meta`. `Board { meta, resources,
  components, services }` with an empty `services` list. Port the existing board unit tests.
- **Traits.**

  ```rust
  pub trait Element: Sync {
      fn tag(&self) -> &'static str;                       // must equal a schema element tag
      fn lower(&self, node: &ParsedNode, cx: &mut LowerCx) -> Result<(), Reported>;
  }

  pub trait Lowered {
      fn id(&self) -> &Ident;
      fn emit(&self, board: &Board, cx: &mut EmitCx);
      fn overlay_attr(&self, attr: &str) -> Option<String> { None }   // {{ id.attr }}
      fn runtime_items(&self) -> &'static [&'static str];             // checked against the runtime
  }
  ```

  `Reported` is a zero-sized marker proving a diagnostic was pushed into `cx`.
- **`LowerCx`:** typed `attrs::<T>()` that turns a generic `Issue` into a `Diagnostic` through
  one function (`issue_to_diagnostic`, with per-element codes declared next to the element), a
  schema-driven `resolve_ref(attr)` that uses `ref to=` to check the target id exists and has the
  right tag (this replaces `Refs::resolve` and `PeripheralKind`), `claim_pin`, `ident`, `push`.
  Elements are lowered section by section: peripherals, then components.
- **`EmitCx`** is a plain collector with no knowledge of any element: `field`, `init`,
  `safety_comment`, `runtime_item`. (`once`, `cargo_feature`, `needs_heap` arrive in the phases
  that need them.)
- **Registry:** `#[distributed_slice] static ELEMENTS: [&dyn Element]`. `espforge-core` references
  `espforge_elements` explicitly so the slice cannot be linked away.
- **`Component` enum + `component_enum!`:** one line per variant generates the `Lowered`
  forwarding. Phase 1 has one variant (`Led`); phase 2 (button) is the test of one-line growth.
- **Unknown-element diagnostics are derived.** "`<pwm>` is not a known element in
  `# peripherals`; known: `<gpio>`" comes from the schema and registry. There is no hand-written
  "(only `<led>`, ...)" string anywhere.
- **Coherence test:** every schema element has exactly one registered `Element` and the reverse; every
  `ref to=` target is a schema tag; registry is non-empty. Initially exercised with a test-only
  element so the framework is proven without any real one.

**Done when:** framework compiles and its tests pass with a test-double element, and the
coherence test fails with a message naming the exact tag and file to touch when you add a schema
element with no implementation.

### M5: Emit crate, walking skeleton: `helloworld`, blocking, `esp32c3`

This is the first end-to-end milestone. No component, no embassy, one chip.

- Port from `legacy/espforge-emit/src/codegen`: `chip.rs`, `cargo.rs`, `config.rs`, `wokwi.rs`,
  `diagram.rs`, `plan.rs`, `comments.rs`, `equiv.rs`, plus `manifest.rs`, `project.rs`
  (README and overlay layering), and the ownership/drift `write`. Port their unit tests.
- Cut the code that phase 1 doesn't need and mark each cut in a one-line "returns in phase N"
  comment: all of `wifi`/`ble` branches in `cargo.rs` and `rust.rs`, the SPI/I2C/UART arms,
  `wifi()` and the `Services::new` impl, the `chip` overlay seeding.
- Rework `overlay.rs`: `{{ board }}` and `{{ serialMonitor }}` stay built in; `{{ x.attr }}` becomes
  "find the component by id, ask `overlay_attr(attr)`". No per-element `match`.
- Rework `rust.rs` `generated()` and `safety_comments()`: both are **loops over
  `board.components`** that call `Lowered::emit` and collect from `EmitCx`. With zero components
  they emit the empty `Components`, `Devices {}` and `Services {}` seen in the helloworld golden.
- Golden harness, **auto-discovered**: iterate `espforge_examples::categories()` ×
  `example.toml` chips × runtimes, generate into a scratch dir via the library (not a subprocess),
  compare the whole tree byte for byte against `tests/golden/<slug>/`. Keep `UPDATE_GOLDENS=1`,
  the "golden tree without a matrix cell" check, and the readable diff output. There is no
  `MATRIX` and no `CELLS`.
- Port the `codegen_equiv` parse sweep (every golden Rust/TOML/JSON file parses through the
  comparators).

**Done when:** `helloworld`/`esp32c3`/`blocking` is byte-identical to its golden through the
library API.

### M6: Pipeline and CLI

- `espforge-core`: port `source.rs` (FILE, `--example`, or the single `.espf` in the current
  directory; app logic from `src/app.rs` or `app/rust/app.rs`; overlay discovery limited to
  `diagram.json` and `wokwi.toml` for now), `answers.rs` (override flags with "note:" lines),
  and a `pipeline` module: parse → load core → options → lower → emit → write. It returns
  structured results (`Invalid`, `ProjectError`), never prints.
- `espforge` CLI (`clap`): `check`, `gen`, `build`, `version`, `-C/--directory`, the answer flags
  (`--name --chip --runtime --console --defaults --option`), `--out`, `--release`,
  `--runtime-path`. Exit codes: `0` ok, `1` user error, `2` internal.
- Human diagnostic output only. Footer line keeps the existing wording
  ("Found N problem(s) in '…'; fix them and re-run.").
- Port transcript tests for: `version`, `no-command`, `help`, `usage-unknown-subcommand`,
  `directory-missing`, `check-example`, `check-valid-file`, `check-missing-sections`,
  `check-syntax-error`, `check-unknown-example`, `check-no-spec-no-example`, `gen-valid-file`,
  `build-without-spec`. Help text now lists only the commands that exist.

**Done when:** `espforge gen --example 01.Basics/helloworld` writes a project whose files match the
golden tree.

### M7: Widen helloworld, `esp32s3` and `embassy`

- Remaining three `helloworld` cells go green: Xtensa differences (`build.rs` linker check,
  `rust-toolchain.toml` channel `esp`, link args), embassy differences (`esp-rtos`, executor,
  `signal!` macro in `lib.rs`, async `main.rs`, `Cargo.toml` features, runtime feature).
- These are emitter branches already ported in M5, so this milestone is mostly confirming and fixing
  drift, not designing.

**Done when:** all 4 `helloworld` cells are byte-identical.

### M8: `gpio` and `led`, the first real element

Implement `led` as a **hand-written full `Element`** (this is deliberately Tier 2; extraction into
a recipe is phase 2's job).

- `espforge-elements/src/peripherals/gpio.rs`: lowers to a `GpioDecl { id, pin, direction }` in
  `LowerCx`. It does not claim the pin. The first component to use it does, as in legacy.
- `espforge-elements/src/components/led.rs`: attrs (`gpio` ref to `gpio`, `active_low`), checks the
  gpio direction (`wrong gpio direction` if the gpio is declared `input`), claims the pin as
  `PinUse::Output`, requires the id to be a valid Rust identifier after `-` → `_`, and pushes
  `Led`. `Lowered for Led`: `emit` adds the field, the initialiser and the SAFETY comment;
  `overlay_attr("pin")` returns the pin number; `runtime_items` is `["Led"]`.
- `espforge-runtime`: port `Logger`, `Delay` (blocking and `embassy` feature), and `Led`. Put every
  `#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]` behind **one** `mod hw` and one
  `pub use hw::*`, instead of repeating the cfg on every item. Publish `RUNTIME_EXPORTS`.
- Contract test checks every `runtime_item` a `Lowered` declares is in `RUNTIME_EXPORTS`.
- Goldens: all 4 `blink` cells, including the overlaid `diagram.json` with
  `{{ board }}` / `{{ red_led.pin }}`.

**Done when:** all 8 cells are byte-identical, and the contract test below is green for `led`.

### M9: Diagnostics corpus, contract test, no-panic audit

- **Diagnostics corpus subset.** Port the cases that phase-1 elements can produce:
  `syntax-unclosed-quote`, `structure-missing-sections`, `structure-unknown-section`,
  `chip-unknown`, `runtime-missing`, `option-bad-value`, `ref-unknown-peripheral`,
  `ref-duplicate-id-silently-accepted` (decide: this passes silently today. Either make it an error
  with a new code now or record it as a known gap with a failing-expected test), `gpio-empty-pin`,
  `gpio-non-numeric-pin`, `component-missing-controller-link`,
  `component-wrong-controller-kind`, `conflict-led-vs-led`, `peripheral-unsupported`,
  `structure-element-outside-its-section`. Wording that came from hand-written "(only …)" strings
  changes once. Re-baseline with `UPDATE_DIAGNOSTICS=1`, **review the diff once**, and record in
  the commit message which snapshots changed and why. Error codes for ported errors must not change.
- **Per-element contract test.** For every registered element and every schema `example`: wrap the
  fragment in the five required sections, run `check`, lower, emit, parse every produced Rust file
  with `syn`, call `overlay_attr` for each declared attribute, and assert every emitted code is a
  registered constant. Adding an element with one example gets all of this for free.
- **No-panic audit.** Port the spirit of `panic_audit.rs` for phase-1 inputs (empty file,
  header-only, duplicate sections, CRLF, a 1 MiB line, 129 gpios and 129 leds, numbers too large for
  their type, empty and missing ids). Assert exit code is `0` or `1` and stderr never contains
  "panicked at". Fix the undefined-`source` bug while porting.
- **Compile check.** The ignored `generated_projects_compile` test runs `cargo check` on
  `helloworld` and `blink` for `esp32c3`/`blocking`.

**Done when:** full `cargo test --workspace` is green and the CI compile job is green.

### M10: Close-out and baselines

- Run the **buzzer drill** (section 7): add a throwaway `<buzzer>` the phase-1 way and write down
  every file touched. Expected result: 4 sites (schema block, `buzzer.rs` element, runtime file, one
  example), with **zero** registry or dispatch edits. This is the number phase 2 has to halve.
- Write `docs/ARCHITECTURE.md` (one page: crate graph, the lifecycle of an element, the rules in
  section 1).
- Record the baseline numbers: crate count, lines of non-test code, time for `cargo test`, number of
  hand-listed lists remaining (target: zero).
- Tag `phase-1`.

## 5.4 Phase 1 exit checklist

- [ ] 8 golden cells byte-identical; the matrix is discovered, not listed.
- [ ] `helloworld` and `blink` generate, and compile for `esp32c3`/`blocking`.
- [ ] Nothing generated is committed; the schema is `LazyLock`-loaded.
- [ ] Every diagnostic carries a typed code; `for_message` and `E0999` do not exist.
- [ ] The only KDL walker is in `espforge-schema`.
- [ ] No per-element `match` exists outside an element's own file; no hand-written "supported
      elements" text exists.
- [ ] Schema ⇔ registry coherence test and per-element contract test are green.
- [ ] Buzzer drill recorded.
- [ ] Out-of-scope list in 5.1 untouched: no stray `button`, bus, wifi or device code.

## 5.5 Phase 1 risks and how we watch them

| Risk | Signal | Mitigation |
|---|---|---|
| Porting drifts from golden bytes | Golden diff in M5 | Port emitters verbatim first, refactor only after green. Use `equiv` comparators to separate "different bytes" from "different meaning" while debugging. |
| `linkme` slice dead-stripped | Registry empty at runtime | Explicit `use espforge_elements as _` in `espforge-core`; "registry is non-empty" test. |
| Build-dependency cycle (`elements` build.rs needs `schema`) | Cargo error at M2 | Keep `espforge-schema` free of any workspace dependency except `diagnostics`. |
| Generic diagnostics read worse than hand-tuned ones | Snapshot diff in M9 | Per-element `message=` override in KDL is allowed. Otherwise accept one reviewed re-baseline. |
| Over-building for a future we haven't met | `EmitCx` grows methods nothing calls | Add `once`, `cargo_feature`, `needs_heap` only in the phase that first uses them. |
| Scope creep (someone "just adds button") | Out-of-scope list violated | Phase gate reviews the diff against 5.1. |

---

# Phases 2–7: high level

## Phase 2: digital I/O and the recipe tier

- **Examples:** `02.Digital/button`, `02.Digital/button_async` (c3/s3 × blocking/embassy as declared).
- **Build:** `button` element and runtime type; a second `Component` variant proves the one-line
  growth. `check --format json` and `board_json` through a `json()` method on `Lowered`; port
  `check_json` and the JSON half of the no-panic audit.
- **Architecture:** with `led` and `button` as two data points, define the **recipe tier**: a
  `recipe { pin from=…; field …; runtime "…" }` block in the schema synthesises `lower`, `json`,
  `emit`, `overlay_attr` and the runtime type. Migrate `led` and `button` to recipes. Add tier
  enforcement: an element file that adds nothing beyond its recipe is a test failure that says
  "delete this file and move the exception into the recipe".
- **Gate:** the buzzer drill is **schema block + example, zero Rust files**.

## Phase 3: buses

- **Examples:** `i2c_example`, `i2c_scanner`, `spi_example`, `uart_example` and their async
  variants (the whole `04.Communication` category).
- **Build:** `i2c`/`spi`/`uart` peripherals and `*_component` elements. Bus claims join
  `ResourceGraph`. Add `EmitCx::once` for the shared SPI bus prelude (first-use order, so goldens
  stay stable). Make `I2cBus::build` and `SpiBus::build` generic over `impl Instance`, so `i2c1` and
  other instances can actually compile. Wokwi custom chips: seed `chip.json`/`chip.wasm`, the
  `[[chip]]` section, and the `keep-zig-source` option.
- **Tests:** remaining diagnostics corpus (`conflict-*`, `i2c-*`, `uart-*`), bus golden cells,
  schema-driven `bus_component` replaces the hand-listed vocab.

## Phase 4: services and Wi-Fi

- **Examples:** `05.Networking/wifi_station`.
- **Build:** `Services` as a real emitted struct; `wifi` element; `EmitCx::cargo_feature` and
  `needs_heap` replace the `opts.is_on("wifi") || opts.is_on("ble-trouble")` checks; radio
  dependencies in `Cargo.toml`; `wifi` and `ble-trouble` options with chip compatibility and
  `requires`. `ntp` stays out until it does something.
- **Gate:** the `wifi_station` goldens, and no `status="planned"` concept left anywhere.

## Phase 5: devices

- **Examples:** `06.Displays/ili9341_example`, plus `ssd1306_example` either given a real schema
  element and driver or deleted.
- **Build:** `binds "i2c_component"` / `binds "spi_component"` in the schema, `runtime_item` and
  `cargo_dep` collected by `EmitCx`. Migrate `tmp102` (recipe + binds) then `ili9341`
  (protocol quirks, so a Tier 2 file). `status_led` either lands as a recipe or is removed.
- **Cleanup:** the `05.Networking/wifi/http_example` directory (nested too deep, stale syntax) is
  removed.
- **Gate:** adding a device that fits the recipe is one schema block plus one example.

## Phase 6: CLI parity and contributor tooling

- **Build:** `new`, `examples` (interactive and `list`/`categories`/`get`), `espf` (catalog browser,
  man pages and examples, generated from the schema), `doctor`, `--write-spec` with the ported
  writer, the dist release workflow. Fix `help_view::render_examples` (truncated in the snapshot
  we planned from).
- **Contributor tooling:** `espforge dev new element <tag> --section …` writes the schema block, an
  example and a **failing** contract test that names the next step. `dev new runtime-component`
  generates the runtime type from the recipe. Both are idempotent.
- **Docs:** a one-page "add an element" guide that is just the output of the scaffolder.

## Phase 7: cutover and hardening

- **Cutover:** delete `legacy/`, bump the workspace version (goldens re-baselined once, in one
  reviewed commit), publish.
- **Hardening:** extend golden coverage to the other chips (`esp32c6`, `esp32h2`, …) with their
  toolchain and `build.rs` paths; decide whether pin-validity per chip is worth building (schema
  claims "valid for the chosen chip" but nothing enforces it today); exercise `probe-rs` and
  `stack-smashing-protection` in goldens.

---

## 6. Standing quality gates (every phase)

1. All goldens from earlier phases remain byte-identical.
2. `cargo test --workspace` is green; ignored compile tests are green in CI.
3. Schema ⇔ registry coherence and the per-element contract test pass.
4. No new hand-kept list of elements, tags, codes or example cells.
5. Every new diagnostic has a typed code and a message that names the fix.
6. The buzzer drill is re-measured and does not get worse.

## 7. The buzzer drill (the metric that tells us this is working)

After each phase, add a throwaway `<buzzer>` (a GPIO component) and list every file touched.

| Point in time | Expected edit sites |
|---|---|
| Legacy | about 20 sites across 9 crates |
| End of phase 1 | 4: schema block, element file, runtime file, example |
| End of phase 2 | 2: schema block (with recipe), example |
| Any later phase | 2, or the phase has regressed |

## 8. Definition of done for the whole project

- [ ] A GPIO component or a simple device is a schema block plus an example, with no Rust files and
      no registry edits.
- [ ] Unusual elements are one file each, discovered automatically, and the test suite tells the
      contributor what is missing.
- [ ] No committed generated code. No `MATRIX`. No `for_message`. No `E0999`.
- [ ] `legacy/` is gone and every legacy golden is reproduced byte for byte or deliberately
      re-baselined in one reviewed commit.
- [ ] `espforge dev new element buzzer` takes a contributor from nothing to a green contract test
      without reading anything.
