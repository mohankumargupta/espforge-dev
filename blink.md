# blink.md — blink, end to end, in the new architecture

The second slice of `finalplan.md`: the `gpio`/`led` half of M8, cut to one
example, `01.Basics/blink` (`esp32c3`, `blocking`). The `esp32s3`/`embassy`
widening in M7 is deliberately left out — see Q1. This is the smallest slice that
puts a real element through the pipeline — one `<gpio>`, one `<led>`, one pin
claim, one overlay token — and its job is to prove the element framework on a
project whose bytes we already know, without touching a byte of `helloworld`.

Everything else in phase 1 (`button`, the recipe tier, buses, devices, the
diagnostics corpus, the widening to `esp32s3`/`embassy`) gets its own doc after
this one. If a step here does not change a byte of the blink project, or prove
that the machinery is real, it is not in this slice.

---

## 1. The gate

Three checks. "Running end to end" means 1 and 2; 3 is what a user does. The
standing condition on all of them: **`01.Basics/helloworld.esp32c3.blocking`
stays byte-identical.**

1. **Byte-identical.** `cargo test -p espforge --test golden_projects` generates
   both declared cells in-process and compares the whole tree — 13 files each —
   against `tests/golden/`. The blink tree is moved over from
   `legacy/espforge/tests/golden/01.Basics.blink.esp32c3.blocking`, not
   generated: a golden produced by the code under test proves nothing. Byte for
   byte, including `.espforge/manifest.json` (no trailing newline) and the
   overlaid `diagram.json` (which is the example's compact JSON, not the
   pretty-printed default).
2. **It compiles.** `cargo check` on the generated blink project, with
   `espforge-runtime = { path = ... }` pointing at this checkout, succeeds —
   `espforge-runtime::Led` is a real type with a real constructor, not a name
   the emitter prints. Same test file as helloworld's, one more case.
3. **The command a user types.**
   `espforge gen --example 01.Basics/blink --out /tmp/blink` writes the same
   tree (exit 0), and `diff -r /tmp/blink
   espforge/tests/golden/01.Basics.blink.esp32c3.blocking` is empty.

The oracle is the **legacy tree**. Everything ported here is moved with its
tests, not retyped; where a message or a pin is involved, the legacy text is the
spec.

---

## 2. What blink's bytes actually depend on

Blink is helloworld plus exactly three facts: a component exists, it owns
GPIO18, and the example's own `diagram.json` asks for that pin. The delta
against `01.Basics.helloworld.esp32c3.blocking` is the scope argument:

| File | Difference | Produced by |
|---|---|---|
| `src/generated.rs` | `Components { pub red_led: espforge_runtime::Led }`, `Led::new(unsafe { registry.peripherals.GPIO18.clone_unchecked() }, false)`, one `// SAFETY` line | the new `EmitCx` path |
| `diagram.json` | the example's overlay rendered: `board-esp32-c3-devkitm-1`, `"{{ red_led.pin }}"` → `18` | overlay lookup |
| `Cargo.toml`, `wokwi.toml`, `README.txt`, `src/bin/main.rs` | `blink` instead of `hello_world` | existing emitters, driven by `Meta` |
| `src/app.rs` | the example's `app/rust/app.rs`, verbatim (`component!(ctx, red_led)`, `led.toggle()`) | existing copy |
| `.espforge/manifest.json` | new hashes | existing manifest |
| `build.rs`, `src/lib.rs`, `.cargo/*`, `rust-toolchain.toml` | **identical bytes** | unchanged code |

Blink's `.espf` has no `<console>` and no `<options>` element; it must resolve to
the same console and the same option defaults as helloworld. That is a property
to verify, not code to add — if it does not hold, the resolver is wrong and the
fix belongs in its own commit, not behind a golden.

A crate is in this slice only if it changes one of those bytes, or if check 2
cannot run without it.

| Crate | Why it is here now | What it contains at the end of the slice |
|---|---|---|
| `espforge-diagnostics` | New failures need their old numbers | codes `E0501–E0503`, `E0601`, `E0606`, `E0608`, `E0609`, `E0702` |
| `espf` | Already reads everything blink writes | unchanged |
| `espforge-schema` | The two elements, and the source of the typed attribute structs | `gpio`, `led` blocks with self-contained examples |
| `espforge-elements` | The board IR, the pin graph, the framework, the two elements | `Pin`/`Claim`/`ResourceGraph`, `Element`/`Lowered`/`LowerCx`/`EmitCx`, `linkme` registry, `peripherals/gpio.rs`, `components/led.rs`, `build.rs`-generated `Attrs` |
| `espforge-emit` | Two files consume the board | `rust.rs` renders `Components` from `EmitCx`; `overlay.rs` resolves `<component>.pin` |
| `espforge-core` | The pipeline has a lowering stage now | `Checked.board`, `project::write_checked` writes it |
| `espforge-runtime` | Only for check 2 | `Led` |
| `espforge-examples` | The example | `01.Basics/blink` (espf, app, diagram, `example.toml`) |
| `espforge` | Only for check 3 | unchanged: `gen` already does the work |

New dependencies: `linkme 0.3.37` (registry), `quote`/`proc-macro2` in
`espforge-elements` (an element owns its emitted tokens), and `espforge-schema`
additionally as a build-dependency of `espforge-elements` (the `Attrs`
generator); it is already a normal dependency there for the schema lookup.

Notably absent even though phase 1 needs them eventually: the `Led` runtime
type's siblings (`Button`), `runtime_items`/`RUNTIME_EXPORTS`, the per-element
contract test, buses, `EmitCx::once`/`cargo_feature`/`needs_heap`, pin validity
per chip, and a second component variant. None of them can change a byte here.

---

## 3. Steps

Each step ends green and is one commit. "Verify" is the command that has to pass
before the next step starts.

### B0 — branch

`git switch -c blink` from `rewrite`. Nothing else.

### B1 — board IR: pins, claims, and a board that owns resources

**Do** — port from `legacy/espforge-model/src/board.rs`, with the claim tests:

- `Pin(u8)`, `PinUse { Input, Output, Bidirectional, Unspecified }`,
  `OwnerKind { Gpio, Led }`, `Owner`, `Claim`, `ClaimedPin`, `ConflictKind`,
  `Conflict`, `ResourceGraph` with `claim`.
- Cut the parts that need buses or peripherals: `add_bus`, `bus`, `used_by`,
  `claim_peripheral`, `PeripheralClash`, the `Button`/`I2c`/`Spi`/`Uart` owner
  kinds, and the bus-specific conflict wordings. Each cut is a one-line
  `// returns with buses (phase 3)` / `// returns with <button>` comment; the
  enum is closed and grows one variant per element.
- `Board { meta, resources: ResourceGraph, components: Vec<Component> }`.
  `Component` stays **uninhabited** in this step; the first variant lands with
  its element in B4.
- Update every `Board { .. }` construction (`espforge-core/src/project.rs`,
  `espforge-emit` tests, `codegen/test_support.rs`) with
  `resources: ResourceGraph::default()`.

**Verify:** `cargo test --workspace`; the helloworld golden is still green (this
step changes no emitter).

### B2 — schema: `gpio` and `led`

**Do**

- Add to `espforge-schema/src/espf.schema.kdl`, worded like the legacy blocks:

  ```kdl
  element "gpio" section="peripherals" {
      summary "A single GPIO pin"
      attr "id" kind="ident" required=#true
      attr "pin" kind="pin" required=#true
      attr "direction" kind="choice" {
          choices "output" "input"
      }
      example "minimal" #"<gpio id="gpio18" pin="18" />"#
      example "output" #"<gpio id="gpio18" pin="18" direction="output" />"#
  }

  element "led" section="components" {
      summary "An LED on a GPIO output"
      attr "id" kind="ident" required=#true
      attr "gpio" kind="ref" to="gpio" required=#true
      attr "active_low" kind="bool" default="false" {
          help "true when the LED lights on a LOW pin."
      }
      // Its examples are complete documents carrying their own <gpio>; below.
  }
  ```

- `led`'s examples are **self-contained documents**, because the schema test's
  `wrapped()` only wraps a fragment that does not start with `#` — anything
  starting with a header is parsed as-is and must carry all five sections
  (three of them may be empty). So the `led` examples are complete:

  ```
  # espforge
    <project name="blink" />
    <chip type="esp32c3" />
    <runtime type="blocking" />
  # peripherals
    <gpio id="gpio18" pin="18" direction="output" />
  # components
    <led id="red_led" gpio="$gpio18" />
  # devices
  # services
  ```

  `gpio`'s `see_also` from the legacy schema (`components/gpio_component`) is
  dropped — that element does not exist; `led`'s `see_also "peripherals/gpio"`
  stays.
- Update `the_embedded_schema_loads_and_has_the_phase_one_vocabulary`: seven
  elements, one in `peripherals`, one in `components`, `devices`/`services`
  still empty; assert `gpio`'s three attributes (with `direction` optional) and
  `led`'s `ref to="gpio"` on the embedded schema. The `SAMPLE` fixture stays as
  it is — it already has a `gpio` and a `led`.

**Verify:** `cargo test -p espforge-schema` (the existing
`every_example_fragment_parses_with_espf` covers the new blocks).

### B3 — generated `Attrs`, from the schema

**Do** — helloworld.md's Q3 deferred this to the slice where it has more than one
consumer; `gpio` and `led` are those consumers.

- `espforge-elements/build.rs`: read the schema through
  `espforge_schema::load(espforge_schema::SCHEMA_KDL)` (build-dependency on
  `espforge-schema`; it depends on nothing else, so there is no cycle), and
  write **one `pub mod <tag>` per schema element** to
  `$OUT_DIR/element_attrs.rs`: a `struct Attrs<'a>` whose fields follow each
  `AttrDef`'s `kind`/`required`/`default`, a span on every value, and
  `from_node`. Print `cargo:rerun-if-changed` for the schema file and the
  schema crate's source. No file under `src/` is written; a build leaves
  `git status` clean.
- `espforge-elements/src/attrs.rs`: port the coercion helpers and their tests
  (`Spanned`, `Ref`, `Issue`, `ident`, `text`, `text_or`, `opt_text`, `pin`,
  `opt_pin`, `flag`, `flag_or`, `reference`, `opt_reference`, `parsed`/`parsed_or`
  for `choice`). Only the kinds the phase-1 schema declares; `freq`, `hex`,
  `u32`, `list` and `pin_or_ref` arrive with the elements that use them, and the
  generator **panics** on a kind it has no mapping for rather than guessing.
- `lib.rs` includes the generated module in a module body (not as a
  `src/`-backed `mod` declaration):
  `pub mod generated { include!(concat!(env!("OUT_DIR"), "/element_attrs.rs")); }`.
- The core reader keeps its hand-written attribute fetching: its messages
  ("unknown chip", "# espforge is incomplete") are not the generic `Issue`
  wording, and helloworld's diagnostics are the regression test for that.

**Verify:** `cargo test -p espforge-elements` — a test parses a `<gpio>` node and
a `<led>` node through the generated structs (including a missing `pin`, a bad
`direction` and `active_low` defaulting to `false`); `git status` clean;
`cargo test -p espforge --test golden_projects` still green.

### B4 — the framework, `gpio`, and `led`

This is the first element, so the seams and the elements land together: a
framework with no consumers is unverified scaffolding.

**Do**

- `linkme = "0.3.37"` in `espforge-elements`; a registry in
  `src/registry.rs`:

  ```rust
  #[distributed_slice] pub static ELEMENTS: [&dyn Element];
  pub fn elements() -> &'static [&'static dyn Element];
  ```

  The element's section comes from the schema (`schema().element(tag).section`),
  not from a second method on the trait: one fact, one place.
- `src/element.rs`:

  ```rust
  pub trait Element: Sync {
      fn tag(&self) -> &'static str;                    // must equal a schema tag
      fn lower(&self, node: &ParsedNode, cx: &mut LowerCx) -> Result<(), Reported>;
  }
  pub trait Lowered {
      fn id(&self) -> &Ident;
      fn emit(&self, board: &Board, cx: &mut EmitCx);
      fn overlay_attr(&self, attr: &str) -> Option<String> { None }
  }
  ```

  `Reported` is a zero-sized marker proving a diagnostic was pushed into `cx`;
  `runtime_items` is left out until the contract test that reads it exists.
- `LowerCx`: `report(code, span, message, label) -> Reported`, `gpio(name)`,
  `add_gpio(decl)`, `claim_pin(...)`, `push(component)`, `ident(node)`, and the
  final `into_board(meta)`. Diagnostics are collected, not returned at the first
  one, like the core reader.
- `EmitCx` (also in `espforge-elements`, using `quote`/`proc-macro2`): collects
  `field` tokens, `init` tokens, and `safety_comment(anchor, line)` pairs. The
  emit crate turns the comments into its existing `comments::Comment`; no
  comment type crosses the crate boundary.
- `src/model.rs`: `component_enum! { Led(Led) }` — one line per variant
  generates `Component::id` and the `Lowered` forwarding (`id`, `emit`,
  `overlay_attr`). This replaces the empty `enum Component {}` and its
  "no variants" docs; a second component is a second line plus its file.
- `lower(meta, peripherals, components) -> Result<Board, Vec<Diagnostic>>`: walk
  `# peripherals` then `# components`, find the registered element whose tag
  matches **and** whose schema section is the one being walked, and call it.
  An unregistered or unknown tag is
  `` `<pwm>` is not a known element in `# peripherals`; known: `<gpio>` ``,
  with the known list derived from schema ∩ registry. `espforge-core` imports
  `espforge_elements` explicitly so the slice cannot be linked away.
- `src/peripherals/gpio.rs`: attrs → `GpioDecl { id, pin, direction }` in
  `LowerCx`; it claims nothing (the first component to use the pin does, as in
  legacy). `pin` failure → `E0502` ("invalid gpio pin", legacy wording);
  a bad `direction` → `E0503`.
- `src/components/led.rs`: attrs → the referenced `<gpio>` (missing → `E0608`,
  "unknown gpio", legacy wording), the direction check (`direction != "output"`
  → `E0609`, "wrong gpio direction"), the id-is-a-Rust-identifier check
  (`E0601`, "invalid led"), then claim the pin as `PinUse::Output` (a second
  owner → `E0702`, "pin already used") and push `Led { id, pin, active_low }`.
  `Lowered::emit` adds `pub <name>: espforge_runtime::Led,` and the initialiser
  with its SAFETY line; `overlay_attr("pin")` returns the pin number.
  The init token names `registry`, the argument of the generated
  `Components::new`; that name is part of the emitted-code contract, and the
  golden diff is what enforces it.
- Codes in `espforge-diagnostics/src/codes.rs`, legacy numbers and the existing
  `ALL` well-formedness test's range: `E0501` (unsupported peripheral), `E0502`,
  `E0503`, `E0601`, `E0606` (unsupported component), `E0608`, `E0609`, `E0702`.
  There is no title→code table: each element names the code at the point it
  reports.
- Coherence test: every schema element in `peripherals`/`components` has exactly
  one registered `Element`, and every registered element is a schema element;
  the failure names the tag and the file to touch. Plus "the registry is
  non-empty" (the `linkme` dead-strip risk).
- `espforge-core`: `pipeline::validate` calls `lower` after `resolve` and stores
  the result as `Checked.board`; `project::write_checked` writes that board
  instead of constructing an empty one. The existing `Invalid`/`ProjectError`
  split already carries located diagnostics to the CLI, so nothing prints in the
  pipeline.

**Verify:** `cargo test -p espforge-elements` (board from the blink snippet: one
`Led`, pin 18, `active_low == false`; plus the direction, ref, slug, conflict and
unknown-element cases); `cargo test -p espforge-core` (blink validates to a board
with one component); `cargo test -p espforge --test golden_projects` still
green.

### B5 — emit reads components

**Do**

- `espforge-emit/src/codegen/rust.rs`: `collect_components` becomes "build an
  `EmitCx`, `emit` every component, use its fields and inits"; `safety_comments`
  becomes "map `cx.safety_comment`s to `comments::Comment`s" in board order. The
  `match *component {}` placeholders and their "no variants" doc comments go.
- `espforge-emit/src/codegen/overlay.rs`: `resolve_token` keeps `{{ board }}`
  and `{{ serialMonitor }}`; `{{ <id>.attr }}` finds the component by raw or
  `rust_name` and asks `overlay_attr`, returning the existing
  `OverlayError::unknown` when the component or the attribute is not there.
- Tests: a blink board emits the field, the `GPIO18.clone_unchecked()` call and
  the SAFETY line; `{{ red_led.pin }}` and `{{ red-led.pin }}` both render `18`;
  `{{ red_led.color }}` is an error listing the supported tokens; an example
  without a `diagram.json` still gets the generated default diagram.

**Verify:** `cargo test -p espforge-emit`; helloworld's golden still green — this
is the proof that the loop refactor changed no bytes.

### B6 — runtime `Led`

**Do** — inside the existing single `mod hw` in `espforge-runtime/src/lib.rs`,
alongside `Delay`: `Led` holding `RefCell<Output<'static>>` and `active_low`,
with `new(pin: impl OutputPin + 'static, active_low: bool)` starting *off*
(everything the legacy type does, and nothing else). `pub use hw::*` picks it up,
so generated projects name `espforge_runtime::Led` with no new `cfg`.

**Verify:** `cargo build -p espforge-runtime`, `cargo test --workspace` (host
builds still pass: `Led` is target-only, so nothing host-side changes).

### B7 — the example, the golden, and check 2

**Do**

- Copy `legacy/espforge-examples/src/examples/01.Basics/blink/` to
  `espforge-examples/src/examples/01.Basics/blink/`: `example.espf.txt`,
  `app/rust/app.rs`, `diagram.json`, plus the test-only `example.toml`:

  ```toml
  chips = ["esp32c3"]
  runtimes = ["blocking"]
  ```

  (`example.toml` is read by the golden harness only; source resolution copies
  `app/rust/app.rs` and layers `diagram.json`/`wokwi.toml`, so it never leaks
  into a project.)
- Copy the golden tree
  `legacy/espforge/tests/golden/01.Basics.blink.esp32c3.blocking` to
  `espforge/tests/golden/` under the same slug. Do **not** produce it with
  `UPDATE_GOLDENS=1`; the harness already refuses a declared cell with no golden.
- Extend `espforge/tests/generated_projects_compile.rs` to check blink as well as
  helloworld (one shared target dir, one `RuntimeSource::Path`).

**Verify:** the three checks in §1. Check 2 needs what helloworld's run needed:
stable rustc 1.99.0, the `riscv32imc-unknown-none-elf` target, and network access
for `esp-hal`/`esp-bootloader-esp-idf`; record it in the commit message. Also
`cargo fmt --check` and `cargo test --workspace`.

---

## 4. Deliberately absent

`button` and every second `Component` variant, the recipe tier, buses
(`i2c`/`spi`/`uart`), devices, services, `wifi`, custom Wokwi chips,
`runtime_items`/`RUNTIME_EXPORTS` and the per-element contract test, the
diagnostics corpus and the no-panic audit, `check`/`build`/`new`/`version` and
their transcripts, `--format json`, `EmitCx::once`/`cargo_feature`/`needs_heap`,
duplicate-id handling (it passes silently in legacy; M9 decides the behaviour
with a test), pin-validity-per-chip, and the six remaining `helloworld`/`blink`
cells (`esp32s3`, `embassy`). Those cells are the widening doc: the emitters
already carry their branches, and mixing them into this slice would turn a
one-element diff into an emitter-drift hunt.

Also untouched: the `prettyplease`/`syn`/`quote`/`toml_edit` versions, and every
byte of helloworld.

---

## 5. Decisions to make before B1

My recommendation is bold. Say which you want differently and I will fold the
answer back into this file before writing code.

**Q1 — How many blink cells does the gate cover?**
(a) **one: `esp32c3`/`blocking`, matching helloworld's slice.** (b) all four
(`esp32c3`/`esp32s3` × `blocking`/`embassy`), as the plan's phase-1 exit wants.
(b) is three extra goldens and, because helloworld's other cells are still
absent, would surface any drift in the `esp32s3`/`embassy` emitters inside this
slice. (a) keeps the diff about components and leaves all six remaining cells to
one widening doc that can bisect them together.

**Q2 — Does the generated `Attrs` land now?**
(a) **yes, in B3, with `gpio` + `led` as its consumers.** (b) hand-write the two
readers now and bring the generator with `button`.
helloworld's Q3 promised the generator here, and (b) means writing the same
coercion logic a third time. (a) costs ~150 lines of `build.rs` and ~250 of
`attrs.rs` (ported), changes no emitted byte, and is proven by the generated
structs' own tests; (b) is a smaller slice but moves the promise again.

**Q3 — Who owns a component's emitted Rust?**
(a) **the element, through `EmitCx`.** (b) keep a `match` over `Component` in
`rust.rs` and defer the framework to phase 2.
(a) is the point of the slice: it makes "adding a component touches its own
file" true, and it is what the buzzer drill measures. The cost is `quote` in
`espforge-elements` and the `registry` argument-name contract noted in B4. (b)
is a smaller diff today and two edits in every emitter for every future
element — the thing the plan is replacing.

**Q4 — Do we make duplicate ids an error now?**
(a) **no: keep legacy's silent acceptance, decide it in M9.** (b) add the
diagnostic here. Blink has one component, so (b) buys the gate nothing and
invents a code and a message without a corpus to hold them; M9 has the case
listed (`ref-duplicate-id-silently-accepted`) with a decision attached.

---

## 6. Risks

| Risk | Signal | Action |
|---|---|---|
| The framework's shape is wrong for the second element | `button` slice | Accepted: one trait method and one `EmitCx` call change in one file each. Do not add methods nothing calls. |
| Element emits tokens that do not print the way the golden expects | B7 diff on `src/generated.rs` | Emit the field/init as single `quote!` blocks; the anchor string must match `comments::apply`'s post-print text. |
| SAFETY comment anchor misses or matches twice | `comments::apply` errors | Fix the anchor; never loosen `apply` to a prefix match. |
| Overlay lookup picks the wrong id spelling | B7 `diagram.json` diff | Match both `raw` and `rust_name`; test `red-led.pin` and `red_led.pin`. |
| The generator writes into `src/` (the legacy habit) | `git status` after `cargo build` | `OUT_DIR` only; assert clean tree in the step's verify. |
| `linkme` slice dead-stripped | registry empty at runtime | `espforge-core` references `espforge_elements`; "registry is non-empty" test. |
| Blink resolves different option defaults than helloworld | B7 `Cargo.toml` diff | Blink has no `<options>`; if it differs, stop and fix the resolver in its own commit. |
| Golden generated instead of moved | A goldens test that always passes | Copy from `legacy/`; the oracle is the old tree. |
| Compile check needs target/network | B7 | Run by hand, report what it needed; do not assume CI has it. |

---

## 7. Commits

```
B0  (branch only; no commit)
B1  feat(elements): pins, claims, ResourceGraph on the board
B2  feat(schema): gpio and led elements
B3  feat(elements): generated Attrs from the schema
B4  feat(elements): Element registry, LowerCx, gpio and led
B5  feat(emit): components emit their own Rust; overlay resolves .pin
B6  feat(runtime): Led
B7  feat(examples): blink, byte-identical on esp32c3/blocking
```

Every commit is `cargo test --workspace` green and leaves helloworld
byte-identical. B7 is the first proof that blink is real; before it the slice is
the seams for it.

---

## 8. If you say "go"

I start at B1 and go in order, and I stop after B7 with the byte-identical blink
tree, the compile result, and the exact toolchain/target/network that run
needed. Questions I need answered first: Q1–Q4.
