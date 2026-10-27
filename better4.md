# better4: the boring version — same goal, techniques other projects already proved

> Builds on `better.md` (the audit), `better2.md` (the cut list and contributor UX),
> `better3.md` (the tiers/recipe end state).
>
> **What this doc changes:** better3's end state is smaller than better2's, but it is *harder to
> build —* it needs a new recipe interpreter in the schema loader, a tier-enforcement test, a
> runtime type generator with `impl`-merge, `linkme` distributed slices, and a new `espforge dev`
> CLI with subcommands. That is five pieces of novel machinery, none of which exists elsewhere.
>
> **better4 keeps the goal and most of the plan, and replaces the novel machinery with patterns
> that other well-known projects already ship.** Everything below names the project it comes from.
> If a technique has no precedent, it does not belong in this migration.

## 0. Lineage: what better4 keeps, drops, and substitutes

| Proposal | From | better4 | Substitute (with precedent) |
|---|---|---|---|
| Audit of ~20 edit sites / 9 crates; root causes A–G | better.md §1–2 | **keep as-is** | — |
| Cut list: kill 2 `build.rs` that write into `src/`, 4 KDL walkers, `for_message`, `route_driver`, `MATRIX`/`CELLS`, duplicated constants | better2 §5 | **keep as-is** | — |
| Typed diagnostic codes passed at construction, never derived from titles | better2 §8 | **keep as-is** | rustc diagnostics |
| Schema-driven refs/strings (`ref to=`, derived `(only …)` list), schema ⇔ registry coherence test | better2 §6 | **keep as-is** | Linux DT bindings |
| `binds` for devices, `cargo_dep`/`runtime_item` collected by `EmitCx` | better2 §4 | **keep as-is** | DT `compatible` + driver match table |
| Auto-discovered golden matrix, per-element contract test | better2 §9 | **keep, simplified** | `trybuild`/`insta` glob-corpus convention |
| One `Element` trait + registry line per coded element | better.md §3.1 | **keep, thinned** | `enum_dispatch`/`strum`-style derive dispatch |
| KDL `recipe { … }` blocks synthesized into elements; tier 0/1/2 classification; tier-enforcement test | better3 §2 | **drop** | one `pin_element!` `macro_rules!` (~10 lines of macro) |
| `linkme` distributed slices, no registry line | better3 §3 | **drop** | keep one registry line; compile + coherence-checked (see §3.4) |
| Generated runtime types + `buzzer_impl.rs` `impl`-merge | better3 §4 | **drop** | one-line `pin_runtime!` macro in the runtime crate |
| `espforge dev new element` CLI with red-test loop | better3 §5 | **drop** | `templates/` dir + `just new-element` recipe (the repo already uses `just`) |

Why drop, in one line each:

- **Recipe interpreter** — a second parser/semantic layer over the same KDL, invented here.
  A `macro_rules!` template gives the same "data, not code" feel with zero new language surface
  and rust-analyzer can expand it (`rust-analyzer: Expand macro`), so it is inspectable.
- **Tiers** — a classification system enforced by a test that must reason about what code
  *could* have expressed. Tests should assert facts, not counterfactuals. With the macro,
  "simple component" is already cheap, so tiers have nothing to enforce.
- **`linkme`** — distributed slices rely on linker retention of statics; they have recurring
  trouble with LTO, duplicate-symbol detection, some targets, and are invisible to `cargo doc`
  navigation. A plain `pub const ELEMENTS` is greppable, ordered, and the coherence test already
  catches a missing line *by name*. We are trading "no line at all" for "one line, machine-checked"
  to avoid a dependency whose failure mode is a confusing link error.
- **Runtime codegen + `impl`-merge** — generating a struct and then merging hand-written `impl`
  blocks into it is the most fragile thing in better3. `Buzzer::new(pin, active_low)` and
  `Led::new(pin, active_low)` are the same program with different names; `macro_rules!` has been
  the standard answer to exactly that since Rust 1.0 (`bitflags`, `log`, `structopt`'s prelude).
- **New CLI subcommands** — a scaffolder is a program that must be tested, versioned and kept
  in sync with the schema. A template directory plus `sed` is 20 lines of `just` and a
  contributor can read it in full.

## 1. Principles (better2's, tightened)

1. **One fact, one place.** Tag, attrs, ranges, defaults, `ref to=`, examples, device bindings,
   cargo deps live in `espf.schema.kdl`. Everything else derives.
2. **Delete before abstract.** No new crate, trait or dependency until the cut list in better2 §5
   is executed.
3. **Borrow a pattern before inventing one.** Every mechanism in this doc names a project that
   ships it. If you want to add a sixth, bring a precedent too.
4. **The repo remembers.** No `MATRIX`, no `CELLS = 30`, no hand-copied `(only …)` strings, no
   title-matched codes. Globs, loops and tests remember.
5. **Errors teach the next command/file**, rustc-style: name the missing piece and the exact fix,
   never `E0999`, never a bare `Result<(), String>`.
6. **Still no plugin system, no proc-macro DSL, no `Box<dyn Lowered>`.** Same "will NOT do" list
   as better2 §11.

## 2. Techniques borrowed, and where each lands

| # | Project | The technique | Where espforge applies it |
|---|---|---|---|
| 1 | **ESPHome** (YAML device config → firmware) | A component declares its config schema next to its codegen; the framework auto-discovers components by directory and validates every entry against the schema | Element files live in `espforge-elements/src/{peripherals,components,devices}/`; the build script scans that directory; schema validates |
| 2 | **Linux kernel DT bindings + `of_match_table`** | The schema (bindings YAML) is authoritative for what a device looks like; the driver only *registers a match entry* naming the compatible string it handles | `binds "i2c_component"` in KDL + one registry line per coded element; coherence test asserts schema ⇔ driver, exactly like binding ⇔ driver CI checks |
| 3 | **`knuffel`** (typed KDL parsing, matklad) | Parse KDL straight into typed structs with derive-like annotations; one grammar, no hand-walked nodes | Replace the four walkers (`option_literal`, `element_literal`, `attr_literal`, `kind_literal`) with one typed model (`ElementDef`, `AttrDef`) shared by loader and build scripts. *If `knuffel` is too heavy a dep, hand-roll the typed structs — the point is one parser, not the crate.* |
| 4 | **protobuf / `tonic-build` / `sqlx`** | `build.rs` writes generated code to `OUT_DIR`, consumed via `include!`; nothing generated is ever committed | Bindings `build.rs` stops writing into `src/`; `generated_*.rs` leave git; the CI `git diff --exit-code` guard is deleted |
| 5 | **`enum_dispatch` / `strum`** | Derive or macro generates the per-variant forwarding so `enum` + `match` stops being an expression problem | Keep `enum Component` (free `Clone`/`PartialEq`/exhaustiveness) and generate its `Lowered` forwarding from a one-line-per-variant macro |
| 6 | **rustc diagnostics** | Stable code assigned at *construction*; suggestions say the fix; prose never determines the code | `Diag::at_node(codes::COMPONENT_INVALID_BUZZER, node, title, msg)`; `for_message` and the 70-entry title table die; every message ends in a remedy ("add X", "run Y") |
| 7 | **`trybuild` (`tests/ui/`), `insta` (snapshot globs)** | Test corpora are *globbed*, never listed; adding a file adds coverage with no harness edit | Golden cells derived from `examples::categories() × offered_chips()` instead of `MATRIX`; diagnostics corpus and transcript cases discovered by glob |
| 8 | **`cargo generate` / cookiecutter** | Scaffolding = copy a template dir + substitute names; the template is reviewable in one screen | `templates/element/` + `just new-element buzzer` (the repo already uses `just`); no new CLI crate |
| 9 | **ESP-IDF `library.json` / Cargo manifests** | Dependencies are data on the component, aggregated by the build | `cargo_dep "tmp102-driver" "0.1"` in KDL, collected by `EmitCx`, rendered by `cargo.rs` |
| 10 | **`bitflags`, `log`, `anyhow` context** | `macro_rules!` for the 80% shape, trait impl for the 20% that is genuinely custom | `pin_element!` for GPIO-shaped components; full `Element` impl for buses, `wifi`, `ili9341` |

Explicitly *not* borrowed: `linkme`/`inventory` (see §0), Kconfig-style option trees, dynamic
plugin loading, any proc-macro attribute DSL.

## 3. The design

### 3.1 One schema model, one parser

A tiny `espforge-schema` crate holds `ElementDef`, `AttrDef`, `Section`, and **one** KDL reader.
`catalog::load`, `schema::load`, and every build script depend on it. The four bespoke walkers are
deleted. Parsing at first use inside a `LazyLock` is microseconds; `espforge-options/build.rs`
exists only to precompute what the loaders already compute, so it goes away entirely
(better.md §3.6, unchanged).

```rust
// espforge-schema/src/lib.rs — the only code that reads the KDL
pub struct ElementDef {
    pub tag: String, pub section: Section, pub status: Status,
    pub attrs: Vec<AttrDef>, pub binds: Option<String>,
    pub cargo_deps: Vec<(String, String)>, pub runtime_items: Vec<String>,
    pub examples: Vec<String>, pub message_overrides: Vec<(String, String)>,
}
pub fn load() -> &'static LazyLock<Schema>;   // embedded espf.schema.kdl
```

### 3.2 Codegen to `OUT_DIR`, one generator only

Keep exactly one generator: the typed `Attrs` structs (real compile-time value — the schema's
kinds/ranges/defaults become Rust types). It also emits two lists:

```rust
// OUT_DIR/schema_generated.rs, include!'d
pub struct Attrs { /* … */ }             // typed attribute bags
pub const SCHEMA_TAGS: &[&str] = &["gpio", "i2c", "led", "button", …];  // for coherence test
```

Stop writing to `src/`, delete the committed `generated_*.rs`, drop the CI diff guard.
(Precedent: `tonic-build`, `sqlx`, `bindgen` — nothing generated is ever in git.)

### 3.3 The element interface: macro for the common case, trait for the rest

The `Element`/`Lowered` traits from better.md §3.1 stay, but gain **default methods** so a
hand-written impl only overrides what it actually changes (the serde/`Display`-via-`Debug`
convention). The common GPIO shape is one macro invocation:

```rust
// espforge-elements/src/components/buzzer.rs — the whole file
use crate::prelude::*;

pin_element! {
    tag: "buzzer",
    claim: gpio => Output,               // which ref attr is claimed, as what
    runtime: espforge_runtime::Buzzer,   // constructor called by emit
    pass: [active_low],                  // config fields: Attrs → JSON → emitted args
    json_kind: "buzzer",
}
```

The macro expands (inspectable with *rust-analyzer: Expand macro*) to:

- `lower`: resolve the `gpio` ref, `claim_pin(…, Output)`, build the lowered struct from `pass` —
  identical to better3's generated `lower`, just at compile time instead of load time;
- `json()`: `kind` + `pin` + the `pass` fields;
- `emit()`: the struct field, the `Buzzer::new(#pin, #active_low)` init, and the SAFETY comment
  derived from the claimed pin;
- `overlay_attr()`: `pin` (and `peripheral`, matching what `led` emits today).

Anything with a real quirk (buses with `cx.once(…)`, `wifi`'s heap, `ili9341`'s extra pins)
implements the trait by hand in its own file — the macro is sugar over the same trait, never a
second system. `led`, `button`, `buzzer`, `relay` are macro invocations; that is the tier idea
from better3 §2 *without* the tiers, the enforcement test, or the schema interpreter.

### 3.4 Dispatch stays a closed enum; the registry stays one checked line

```rust
// espforge-elements/src/lib.rs
registry! {
    gpio, i2c, spi, uart,                 // peripherals
    led, button, buzzer,                  // components — one word per coded element
    tmp102, ili9341,                      // devices
    wifi,                                 // services
}
```

`registry!` expands to (a) the `enum Component` variant list, (b) the forwarding impl generated
per variant (the `enum_dispatch` technique), (c) `pub const ELEMENTS`. One word per element,
not a `&dyn` reference to keep straight; adding a coded element without the word is caught by:

```rust
#[test]
fn schema_and_registry_agree() {
    // SCHEMA_TAGS (generated, §3.2) minus status="planned"  ==  registry tags
    // failure message: "`<buzzer>` has a schema element but no registry entry —
    //  add `buzzer,` to registry! in espforge-elements/src/lib.rs
    //  (or mark it status=\"planned\" in espf.schema.kdl)"
}
```

This is the DT-bindings ⇔ driver check from §2.2: cheap, exact, and names the file.
Exhaustiveness, `Clone`, `PartialEq` and the existing model tests are all preserved —
better.md §3.3's Option 1, with the dispatch macro generated rather than hand-written.

### 3.5 Schema is the only form to fill in (better2 §6, unchanged)

Derived, never typed by hand: the `(only …)` supported-elements strings, `Refs::resolve`
(iterate `ref to=` + `section=` instead of hard-coding `i2c`/`spi`/`gpio`/`uart` and
`key.contains("component")`), `PeripheralKind`, per-element `*_issue`/`describe` functions
(replaced by one generic `issue_to_diagnostic(&ElementDef, Issue)` with optional `message=`
overrides), `NOT_YET_SUPPORTED`, `RUNTIME_VERSION`, and `status="planned"` ⇔ unregistered.

### 3.6 Devices: `binds` is a compatible-string, nothing more (better2 Job 3, unchanged)

```kdl
element "tmp102" section="devices" {
  binds "i2c_component"
  runtime_item "espforge_drivers::Tmp102"
  cargo_dep "tmp102-driver" "0.1"
  example "<tmp102 i2c_component=\"$temp_bus\" />"
}
```

`route_driver`, `drivers/generated_registry.rs`, the directory-scanning `build.rs` and
`devices::reject_if_present` are deleted; the device file's `lower` resolves its bound component
and claims pins through it. A dangling `binds` target is a schema-coherence failure with the
target named.

### 3.7 Runtime: one macro line, one `cfg` wall, generic instances

```rust
// espforge-runtime/src/components/buzzer.rs — the whole file
pin_runtime!(Buzzer, active_low: bool);   // struct + new(pin, active_low) + pin()
```

plus better.md §3.8 unchanged: `I2cBus::build`/`SpiBus::build` take `impl …Instance + 'static`
(what `UartDevice::build` already does), and all `#[cfg(any(target_arch = "riscv32",
target_arch = "xtensa"))]` collapses into one `mod hw { … }` re-exported with `pub use hw::*;`.
Adding a runtime component = one macro line inside `mod hw`.

### 3.8 Tests glob, they don't enumerate (§2.7)

- **Goldens:** cells = `examples::categories() × offered_chips()` filtered by each example's
  declared chips/runtime; `MATRIX`, `COMPILE_MATRIX`'s hand list and `CELLS = 30` are replaced by
  directory globs. Committed golden trees and `UPDATE_GOLDENS=1` stay exactly as they are.
- **Contract test (the big one, from better2 §9):** for every registry element × every schema
  `example`: wrap in a minimal file → `check` → lower → codegen → parse output with `syn` → assert
  `json()`/`overlay_attr()` don't panic → assert emitted codes are registered. A macro-written
  element gets full coverage from having one `example` line.
- **Runtime surface:** each element declares `runtime_items()`; assert they're all present in a
  `RUNTIME_EXPORTS` list the runtime crate publishes. Catches `Led::new` renames without the
  ignored full-compile test.
- **Diagnostics corpus:** globbed; `(only …)` text now derives from the schema, so it changes only
  when real support changes (one reviewed `UPDATE_DIAGNOSTICS=1` refresh).

### 3.9 Scaffolding: a template you can read, not a CLI you must trust

```
templates/element/
  element.rs.tmpl        // the pin_element! block with {{tag}}/{{runtime}} placeholders
  runtime.rs.tmpl        // the pin_runtime! line
  example.espf.tmpl
```

```just
# justfile (the repo already uses just)
new-element tag section="components" runtime="":
    #!/usr/bin/env bash
    set -euo pipefail
    # copy templates, substitute {{tag}}/{{runtime}}, append `{{tag}},` to registry!,
    # then print: cargo test contract.{{tag}}   ← the red→green loop, per better3 §5
```

Idempotent (re-running repairs a half-finished element), 20 lines, readable in full, and it ends
by printing the contract-test command — the "repo tells you what's next" behavior better2 §3.4
wanted, delivered by `echo` instead of a new binary. If the project later wants a real CLI,
the template is what it calls.

## 4. Adding `<buzzer>` after better4

```bash
$ just new-element buzzer
✓ espforge-elements/src/components/buzzer.rs     (pin_element! block — edit the TODOs)
✓ espforge-runtime/src/components/buzzer.rs     (pin_runtime! line)
✓ examples/02.Components/buzzer/example.espf
✓ registry! gained `buzzer,`
→ next: cargo test contract.buzzer
```

| | better2 | better3 | **better4** |
|---|---|---|---|
| schema block | ✓ | ✓ (recipe) | ✓ |
| element file | ~40 lines hand-written | 0 (generated at load) | **1 macro block, ~6 lines** |
| registry edit | 1 line | 0 | **1 word (tool-inserted, test-checked)** |
| runtime file | ~60-line copy of `led.rs` | generated + optional `impl` merge | **1 macro line** |
| example | ✓ | ✓ | ✓ (template-written) |
| new machinery | trait + `LowerCx`/`EmitCx` | + recipe interpreter + tiers + runtime generator + `linkme` + CLI | **+ one `macro_rules!`, one `registry!`** |

Time budget: 15 minutes for a GPIO component, as better2 promised; zero new parser layers,
zero new dependencies beyond (optionally) `knuffel`.

## 5. Migration order (goldens byte-identical at every step; effort is one-person-days)

Each phase is independently shippable and ends with `cargo test` green. Phases 1–3 are the same
as better2's P0–P3; only 4–6 changed.

| Phase | Work | Est. |
|---|---|---|
| **P0 — devices stop lying** | `binds`, migrate `tmp102` then `ili9341`, delete `reject_if_present` + stub registry; fix/remove `ssd1306_example`, un-nest `05.Networking/wifi/http_example` | 2–3 d |
| **P1 — pure deletions** | options `build.rs` → `LazyLock`; bindings → `OUT_DIR`; one `NOT_YET_SUPPORTED`; version from Cargo; derived `(only …)` strings (one reviewed snapshot refresh) | 1–2 d |
| **P2 — typed codes** | thread `DiagCode` through, delete `for_message` + title table; test becomes "every constant unique, well-formed, referenced" | 1–2 d |
| **P3 — schema single-source** | `espforge-schema` typed model replaces the 4 walkers; `Refs::resolve` over `ref to=`/`section`; drop `PeripheralKind`; coherence test | 2–3 d |
| **P4 — pathfinder** | `Element`/`Lowered` with default methods, `pin_element!`, `registry!` (enum + generated dispatch), `LowerCx`/`EmitCx`. Migrate **`led` only**; `01.Basics.blink.*` goldens must not change; then `button`, then buses (they exercise `once()`), then `wifi`. Delete each old `match` arm as you go | 3–5 d |
| **P5 — runtime + tests + scaffold** | `pin_runtime!`, generic `Instance` bounds, single `mod hw`; globbed goldens; contract test; `templates/` + `just new-element` | 3–4 d |

Total ≈ **12–19 person-days**, same range as better2's plan, with better3's end-state machinery
omitted entirely. Phase 0 from `better.md` still applies: add a throwaway `<buzzer>` on current
code first and count the edit sites as the regression metric.

## 6. Risks and trade-offs

- **`pin_element!` could grow a second dialect.** Guard: the macro accepts exactly the keys in
  §3.3; a feature that doesn't fit *goes into a hand-written `Element` impl*, never into a new
  macro branch. If you catch yourself adding a third key, that's an element that deserves the
  trait instead.
- **One registry word remains.** Accepted cost of keeping the closed enum; the coherence test
  names the file and the exact line to add, so it's a *told*, not a *remembered*, step. Revisit
  only if third-party elements become a stated goal (then reconsider `Box<dyn Lowered>` +
  registration, better.md §3.3 Option 2 — still not `linkme`).
- **Macro-generated code is less greppable.** Mitigated by keeping the generated surface small
  (lower/json/emit/overlay for one shape) and by rust-analyzer's expand hint; the hand-written
  path is always available and always first-class.
- **Globbed tests can silently grow.** Goldens are still committed and reviewed; `CELLS = 30`
  dies but `assert!(!cells.is_empty())` and a printed count keep surprises visible.
- **Message wording** (better2's note stands): generic issue text with `message=` overrides where
  exact wording matters; one reviewed snapshot refresh, then wording changes are real changes.
- **Emit ordering:** `EmitCx::once` appends in first-use order so bus preludes keep golden order.

## 7. Definition of done

- [ ] `<buzzer>` = 1 schema block + 1 `pin_element!` block + 1 registry word + 1 `pin_runtime!` line + 1 example — ≤5 places, ≤3 crates, 15 minutes.
- [ ] `led`, `button`, `buzzer` are macro blocks; buses/`wifi`/`ili9341` are hand `Element` impls — no tiers, no tier test.
- [ ] `route_driver`, `reject_if_present`, `for_message`, committed `generated_*.rs`, `MATRIX`, `CELLS`, both codegen-into-`src/` build scripts are gone.
- [ ] One KDL parser (`espforge-schema`); `SCHEMA_TAGS` generated; coherence failure message names the file and the line.
- [ ] Contract test runs over every registry element × every schema example; `runtime_items ⊆ RUNTIME_EXPORTS` is a normal test.
- [ ] `just new-element` exists, is idempotent, and ends by printing the contract-test command.
- [ ] Goldens + diagnostics corpus green, byte-identical through P1–P4; `ili9341`/`tmp102` pass `check`.
- [ ] No new dependency except optionally `knuffel`; no new binary crate; no proc-macro, no plugin system.
