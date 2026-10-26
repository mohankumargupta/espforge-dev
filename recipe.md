# recipe.md — the recipe tier, extracted from `led` and `button`

The rest of phase 2, minus `check --format json` (that is `check.md`). `button.md`
measured what a GPIO component costs today: **seven sites, three of them
bookkeeping** (`codes.rs`, `components/mod.rs`, the `Component`/`OwnerKind`
pair in `model.rs`), plus the element file itself. This doc removes all four.

The question it answers is the plan's own phase-2 gate: **does adding a GPIO
component become a schema block plus an example, with no Rust file, no registry
edit and no `match` anywhere?** The answer is measured by the buzzer drill in
T5, in the same form `button.md` used.

The hard constraint is the same as every slice: **the 14 golden cells stay
byte-identical**. Migrating `led` and `button` to recipes is only finished when
their generated trees are unchanged to the byte — a recipe that prints the same
meaning with different tokens has failed.

---

## 1. The gate

Four checks. 1 and 2 are "it still works"; 3 and 4 are "the tier is real".

1. **Byte-identical.** `cargo test -p espforge --test golden_projects` is
   14/14 green, with `led` and `button` no longer having a Rust element.
   `blink`'s four cells prove a recipe reproduces an output-polarity component;
   `button`'s four and `button_async`'s two prove the input side, including the
   async project; `helloworld`'s four (no components at all) prove the machinery reached no
   project it should not have. No cell may be regenerated.
2. **It still compiles.** `cargo check` on the generated
   `02.Digital/button` (`esp32c3`, `blocking`) and `02.Digital/button_async`
   (`esp32c3`, `embassy`). The emitted `Components` field, initialiser and
   `SAFETY` line now come from the recipe interpreter, so this is the check that
   the interpreter emits what a hand-written element did. Needs rustc 1.99.0,
   the `riscv32imc-unknown-none-elf` target and crates.io, exactly as in
   `button.md`; Xtensa is still out of scope.
3. **The two-site drill.** Add a throwaway `<buzzer>` (GPIO output, one bool) to
   the schema plus one example, generate it, and count the edit sites. Target:
   **two** — schema block, example. Zero Rust files, zero registry edits, zero
   `codes.rs` edits, zero `model.rs` edits. Measured in a scratch tree and
   reverted; nothing about the drill is committed.
4. **Nothing hand-kept.** After migration,
   `grep -rn '"led"\|"button"' espforge-elements/src` returns only test code
   (`lower.rs`, `model.rs`, `lib.rs`'s `Attrs` test);
   `espforge-elements/src/components/` holds `recipe.rs` and `mod.rs` only. If
   either fails, a per-element `match` survived and the tier has not landed.

Standing conditions: `cargo fmt --check`, `cargo clippy --workspace`,
`cargo test --workspace` green, and `git status` clean except the plan docs.

---

## 2. What the two elements say the recipe must express

From `button.md` §4, with the destination of each fact added. This table is the
whole grammar: if a row has nowhere to go, the recipe is wrong.

| Fact | `led` | `button` | Where it lives after |
|---|---|---|---|
| tag | `led` | `button` | the `element` node (already a fact) |
| the pin reference | `gpio="$id"` | `gpio="$id"` | `recipe pin="gpio"` |
| the gpio's required direction | `output` | `input` | `recipe direction="output"` / `"input"` |
| the claim's `PinUse` | `Output` | `Input` | derived from `direction` |
| bool config + default | `active_low`, false | `pull_up`, false | already an `attr` block — **no new syntax** |
| runtime type | `espforge_runtime::Led` | `espforge_runtime::Button` | `recipe runtime="…"` |
| constructor arguments | `(pin, active_low)` | `(pin, pull_up)` | `(pin, <bools in schema order>)` |
| "invalid component" code | `E0601` | `E0602` | `recipe invalid="E0601"`, default `E0615` |
| overlay `{{ id.pin }}` | `pin` | `pin` | fixed rule, no schema surface |
| claim owner kind | `OwnerKind::Led` | `OwnerKind::Button` | `OwnerKind::Recipe("led")` |
| unknown gpio, wrong direction, conflicts | `E0608`, `E0609`, `E0702/3` | same | the framework, shared by every recipe |

Everything is either data the schema already holds or one property of a new
`recipe` child node. The recipe adds **four facts**: the pin attribute, the
direction, the runtime path, and optionally the code. It adds no expression
language, no field list and no method list.

Synthesized lowering, in order (this is the body `led.rs` and `button.rs` have
today, with the tag interpolated):

1. `id` from the node, span included;
2. the pin attribute read as a `$` reference (`attrs::reference`), a missing one
   being `<tag id="…"> needs gpio="$<gpio id>"`;
3. the referenced `<gpio>` looked up — unknown is `E0608` "unknown gpio";
4. a declared direction that is not the recipe's is `E0609` "wrong gpio
   direction", worded `needs an output pin` / `needs an input pin`;
5. the id must become a Rust identifier — the recipe's code, message
   `invalid <tag>`;
6. the pin is claimed as `OwnerKind::Recipe(tag)` with the recipe's `PinUse`,
   conflicts going through `conflict_diagnostic`;
7. any remaining `bool` attribute is read with its schema default, in schema
   order, and becomes a constructor argument;
8. the component is pushed.

The emitted Rust is the same shape for every recipe element:

```rust
pub #name: #runtime_type,
#name: #runtime_type::new(
    unsafe { registry.peripherals.#gpio.clone_unchecked() },
    #(#flags),*
),
// SAFETY: espforge checked that no other component uses GPIO#pin.
```

The `runtime` path is spliced as a token stream, so `espforge_runtime::Led`
prints as `espforge_runtime::Led` after `prettyplease`, and the `SAFETY` anchor
(`red_led: espforge_runtime::Led::new(`) is built from the same string.

### What is deliberately *not* recipe-shaped

`button_async` adds no element behaviour at all — `wait_for_pressed` comes from
the runtime type and the `embassy` feature. **Runtime types are a separate
mechanism from the recipe**, and this doc does not generate them.

The reason is in the two APIs: `Led` has `on`/`off`/`toggle`, `Button` has
`is_pressed`/`wait_for_pressed`, both with materially different bodies. A
generated `RecipeComponent<P, F…>` able to express both would need a declared
method list — a second DSL over the one we just removed. Instead the recipe
binds an existing type by path, and a component that needs new behaviour adds
one `pub struct` to `espforge-runtime`, which the ledger records as a runtime
site, not an element site. The drill's buzzer binds `espforge_runtime::Led`
(a digital output with polarity — `on`/`off`/`toggle` is the buzzer's API), so
the drill really is two sites.

---

## 3. Scope

| Crate | Why it is here | What changes |
|---|---|---|
| `espforge-diagnostics` | one generic component code | `COMPONENT_INVALID` (`E0615`, free: legacy stops at `E0614`) |
| `espf` | nothing | unchanged |
| `espforge-schema` | the `recipe` node is schema data | `Recipe`/`Direction` types, parsing in the single walker, validation; depends on `espforge-diagnostics` for the typed code |
| `espforge-elements` | the interpreter, and the bookkeeping it deletes | `components/recipe.rs`; `Component::Recipe` + `RecipeComponent`; `OwnerKind::Recipe(&str)`; registry fallback; `led.rs`/`button.rs` deleted; `mod.rs` loses two lines |
| `espforge-emit` | two test fixtures build a `Led` by hand | **test code only** (`rust.rs`, `overlay.rs`); no emitter change |
| `espforge-core`, `espforge` | nothing | unchanged |
| `espforge-runtime` | nothing | unchanged; no generated types |
| `espforge-examples` | nothing | unchanged; the drill's example is reverted |

No new external dependencies. `espforge-schema` gains one workspace-internal
dependency (`espforge-diagnostics`), which is legal: diagnostics is the leftmost
crate and depends on nothing in the workspace, so no cycle is possible.

Explicitly out of scope, each with its own doc or phase: `Lowered::json`,
`check --format json`, the no-panic audit and `board_json` (`check.md`);
`RUNTIME_EXPORTS`, `runtime_items`, the per-element contract test
(`contracts.md`); generated runtime types (above); `tier=1` escape hatches (see
Q5); `espforge dev new element` (phase 6); recipes for peripherals, buses,
devices, services and Wi-Fi (phases 3–5); the diagnostics corpus; the Xtensa
compile check.

---

## 4. The recipe block

```kdl
element "led" section="components" {
    recipe pin="gpio" direction="output" runtime="espforge_runtime::Led" invalid="E0601"
    summary "An LED on a GPIO output"
    attr "id" kind="ident" required=#true
    attr "gpio" kind="ref" to="gpio" required=#true
    attr "active_low" kind="bool" default="false" { help "true when the LED lights on a LOW pin." }
    …
}
```

| Property | Required | Meaning |
|---|---|---|
| `pin` | yes | the element's `attr` of kind `ref` that names the `<gpio>` |
| `direction` | yes | `input` or `output`: the direction the referenced gpio must declare, and the `PinUse` claimed |
| `runtime` | yes | the Rust type constructed as `new(pin, <bools…>)` |
| `invalid` | no | the `DiagCode` for "cannot be built"; default `E0615` |

Validation, all in the schema walker so a bad recipe is a load error with the
node's span, never a runtime surprise:

| Rule | Failure says |
|---|---|
| only in `# components` | "`recipe` is for components; a peripheral with no pin reference needs an `Element`" |
| `pin` names an `attr` with `kind="ref"` on the same element | "`<tag>` has no ref attribute `x`; recipe pin must name one of: `attr` names" |
| `direction` is `input`/`output` | "`direction` must be `input` or `output`" |
| every other attribute is `ident` or `bool` | "`<tag> address` is `u32`: a recipe component is an id, one pin reference and booleans; write an `Element`" |
| `runtime` is `ident(::ident)*` | "`runtime` must be a Rust path such as `espforge_runtime::Led`" |
| `invalid` is in `codes::ALL` | "`E0601` is not a diagnostic code this workspace defines" |
| no `recipe` node is given twice | "`recipe` given twice" |

The `invalid` rule is why `espforge-schema` takes the diagnostics dependency:
`invalid="E0601"` becomes a `DiagCode` at load time, so nothing downstream
turns a string into a code.

`E0615` is the tier's default, for an element whose "invalid" message is the
generic `<buzzer id="b"> needs gpio="$<gpio id>"`. The *message* stays
tag-derived (`invalid <tag>`), so a new element still reads specifically; only
the number is shared, and `led`/`button` keep the legacy `E0601`/`E0602` for
their own codes.

---

## 5. Why an interpreter, not generated impls

Two ways to turn a recipe block into a `&'static dyn Element`:

| | (a) interpreter in `espforge-elements` | (b) `build.rs` writes `impl Recipe` + `linkme` statics |
|---|---|---|
| schema crate | same: `Recipe` data | same |
| elements crate | one `RecipeElement` over `Recipe` data | a generated trait impl per element |
| build.rs | unchanged | grows a second generated file, included with `#[distributed_slice]` |
| new dependency | none | none |
| failure mode | a lookup path with two doors | codegen that must stay in step with `Attrs` |

**Recommendation: (a).** The recipe is data; the element that reads it is one
struct that does not know any tag. (b) is not smaller (the per-element trait
impls are generated, but the generator is new machinery in a build script that
currently only writes `Attrs`), and it puts registration in `OUT_DIR`, where the
registry's canary test can no longer see it as a source fact.

Cost of (a): `registry::element(tag)` grows a fallback. The hand-written slice
is consulted first, then a lazily built, leaked list of `RecipeElement`s from
`schema()`. Both doors are covered by the coherence tests in T4, so "an element
exists twice or not at all" is a test failure either way.

`Attrs` stays generated for every element — the hand-written tier keeps using
it, and recipe elements simply do not. That is also a check: a recipe whose
attributes do not exist in the schema cannot get this far, because the recipe
is validated against the element's own `attr` blocks.

---

## 6. Steps

Each step ends green and is one commit. "Verify" is the command that has to pass
before the next step starts, and every step keeps all 14 cells byte-identical.

### T0 — branch

`git switch -c recipe` from `button`. Nothing else.

### T1 — the `recipe` node (no element uses it yet)

- `espforge-schema`: `espforge-diagnostics` dependency; `Element.recipe:
  Option<Recipe>`; `Recipe { pin, direction, runtime, invalid, span }` and
  `Direction { Input, Output }`.
- Parse `recipe` in `parse_element`'s child match and add it to the "expected
  summary, description, attr, example or see_also" label.
- Validate the seven rules in §4, each as a `SchemaError` at the node's span.
- Tests in the schema crate: a full recipe parses; a recipe outside components
  is refused; `pin` naming a non-ref attr lists the refs it could name; a
  `u32` attribute is refused with the "write an `Element`" wording; a bad
  `runtime` path; an unknown code; a duplicated `recipe` node.

**Verify:** `cargo test -p espforge-schema`, `cargo test --workspace` (goldens
included), `cargo clippy --workspace`. Nothing generated changes, because no
schema element has a recipe yet — a behaviour diff here is a bug in T1.

### T2 — the tier, and `led` as its first user

- `espforge-elements/src/components/recipe.rs`:
  `RecipeElement { element: &'static espforge_schema::Element }` implementing
  `Element` with the eight-step lowering in §2, and
  `Component::Recipe(RecipeComponent)` carrying `{ id, tag, pin, runtime, flags }`.
  `RecipeComponent::new(tag, id, pin, flags)` resolves `tag`'s recipe from the
  schema — used by the lowering and by the two emitter test fixtures, so there
  is one constructor and it cannot disagree with the schema.
- `model.rs`: add `Component::Recipe(RecipeComponent)` and
  `OwnerKind::Recipe(&'static str)`; delete `OwnerKind::Led`; `component_enum!`
  keeps one variant line (phase 3 adds bus components to it).
- `registry.rs`: `recipes()` (leaked, schema order) and the two-door `element()`.
- Delete `espforge-elements/src/components/led.rs` and its `pub mod` line;
  `model.rs`'s own tests use `OwnerKind::Recipe("led")` where they used
  `OwnerKind::Led`.
- Schema: `led` gains its `recipe` line, exactly as in §4.
- `lower.rs`: the led tests match `Component::Recipe` and additionally assert
  `tag == "led"`; every message assertion stays byte-for-byte as it is.
- `espforge-emit`: the two fixtures build `RecipeComponent::new("led", …)`
  instead of `Led { … }`; assertions unchanged.

**Verify:** 14/14 cells identical — `blink`'s four are the ones that matter,
since they are the only led output. `cargo test --workspace` green.

### T3 — `button` migrates, and the bookkeeping disappears

- Schema: `button` gains
  `recipe pin="gpio" direction="input" runtime="espforge_runtime::Button" invalid="E0602"`.
- Delete `components/button.rs` and its module line; delete `Component::Button`
  and `OwnerKind::Button`.
- `lower.rs`: button tests match `Component::Recipe`, asserting
  `tag == "button"` and `flags == [true]` where `pull_up="true"` was written;
  the direction-conflict and pull-up-default assertions stay unchanged.

**Verify:** 14/14 cells identical, with `button` ×4 and `button_async` ×2 now
produced by the recipe. `components/mod.rs` is `pub mod recipe;` and a doc
comment — the two-line diff that the drill no longer pays.

### T4 — the tests that stop the tier rotting

- **Coherence:** a component with a `recipe` *and* a registered implementation
  fails, naming the file to delete and the recipe block that already covers it.
  (Without this, the fallback in `element()` silently prefers one of two.)
- **Tier enforcement:** a `# components` element with no `recipe`, whose
  attributes are an `ident` + exactly one `ref` + booleans, fails with
  "`<tag>` is a recipe component: add `recipe …` to its schema block and delete
  `components/<tag>.rs`". The predicate is a function with its own unit test, so
  the failure path is exercised without adding a real element.
- **Known tags:** the unknown-tag message for `# components` lists recipe tags
  too (it already walks `registry::element`, so this is a regression test for
  the fallback).
- **No dead names:** after T3 this is a `grep` in the T5 checklist, not a test.

**Verify:** `cargo test --workspace`; a deliberate local experiment (add a
recipe line without deleting `led.rs`) fails with the coherence message, then
revert — recorded in the commit message, not committed.

### T5 — measure, and record

- `cargo fmt --all --check`, `cargo clippy --workspace`, `cargo test --workspace`
  (14/14), `git status` clean.
- `cargo check` on the two generated projects per §1.2.
- **The drill.** In a scratch state: add
  `element "buzzer" section="components" { recipe pin="gpio" direction="output" runtime="espforge_runtime::Led" … }`
  plus one example directory (`02.Digital/buzzer`: `example.espf.txt`,
  `example.toml`, `app/rust/app.rs`), run `cargo run -p espforge -- gen
  --example 02.Digital/buzzer --out /tmp/buzzer`, and diff the tree against
  `02.Digital/button` to see only name-derived files differ. Note that
  `cargo test --test golden_projects` is *expected* to fail during the drill
  (a declared cell with no golden); that is why the drill is reverted before the
  suite runs. Then `git checkout -- .` and `rm -rf` the example directory, and
  confirm the suite is green again.
- `grep -rn '"led"\|"button"' espforge-elements/src` → tests only.
- Write §11 (measured) with the file list, the drill count and the compile
  results, in the same shape as `button.md` §10.

---

## 7. Decisions to make before T1

My recommendation is bold. Say which you want differently and I will fold the
answer back into this file before writing code.

**Q1 — Interpreter or generated impls?**
(a) **interpreter in `espforge-elements` (§5a).** (b) `build.rs` writes a
`Recipe` impl and a `linkme` static per recipe element (§5b).
(a) is one struct over schema data and leaves the build script alone; (b) keeps
static dispatch but puts registration in `OUT_DIR`. Both need the same `Recipe`
data in the schema crate.

**Q2 — Is there a default code for recipe elements, or must every recipe name
one?**
(a) **default `E0615` (`COMPONENT_INVALID`), `led`/`button` keep `E0601`/
`E0602`.** (b) every recipe must write `invalid=`, and a new component adds a
`codes.rs` line.
(b) keeps "one code per message" strictly, at the cost of the `codes.rs` edit
the drill is trying to eliminate — three sites instead of two. (a) shares one
number for a message whose text is already tag-derived, and `codes::ALL`'s
range test grows by one entry either way. `E0615` is free; legacy component
codes stop at `E0614`.

**Q3 — Do `led` and `button` migrate in this slice?**
(a) **yes: `led` in T2, `button` in T3.** (b) leave them hand-written and use
only a new element to prove the tier.
(b) is a smaller diff and keeps `led.rs` as a reference implementation,
but it leaves two mechanisms that produce identical bytes, and the recipe is
then only proven on an element with no golden — the plan calls `led` and
`button` "the same shape" as the proof, and §2 is that claim tested. Cost of
(a) is honest: two emitter test fixtures and two public types deleted.

**Q4 — `Component::Recipe` one variant, or keep `Led`/`Button` variants and add
`Recipe` for new elements?**
(a) **one `Recipe` variant; delete the other two (T2, T3).** (b) keep all three.
(b) keeps `led`/`button` on the hand-written path beside the recipe, so the
tier-enforcement test in T4 must either fire on both or carry a hand-kept
exception list — the one thing the plan bans. It also leaves the two
bookkeeping rows (`Component` variant, `OwnerKind` variant) that `button.md`
measured in place, for two elements that no longer need them. (a) is the
entire point of the slice, and the enum stays because phase 3's bus components
are not recipes.

**Q5 — Tier enforcement by schema shape, or a `tier=1` opt-out?**
(a) **shape predicate only: `id` + one ref + booleans must be a recipe.** (b)
add `tier=1` on the `element` node as an escape hatch.
(a) needs no new schema surface and already passes over genuinely un-recipe-like
components (`i2c_component` has an integer `address`, so it is out of shape).
(b) is speculative until an element with a real quirk needs it, and the failure
message tells that contributor which two things to change. When a real Tier 1
appears, add the property then — with the quirk it needs.

**Q6 — Constructor rule: implicit booleans, or a declared field list?**
(a) **implicit: `new(pin, <bool attrs in schema order>)`.** (b) the recipe lists
`field "active_low"` lines.
(a) adds no fact that is not already in the `attr` block, and an element that
wants a non-bool argument is not recipe-shaped (Q5's predicate says so). (b)
would let the recipe reorder or skip booleans, which nothing needs today.

---

## 8. Risks

| Risk | Signal | Action |
|---|---|---|
| The interpreter emits different tokens | any of the 14 cells differs | Compare the `generated.rs` diff first; the recipe must reproduce `Led`'s bytes exactly, never re-baseline |
| `runtime` path splicing breaks the `SAFETY` anchor | missing `// SAFETY` comment in a golden | Anchor text must be built from the same string; fix the interpreter, not the golden |
| Two doors in `registry::element` disagree | coherence test added in T4 fails, or `known_tags` misses `<led>` | The fallback must be the only second door; do not special-case lowercase tags |
| `OwnerKind::Recipe(&str)` weakens the closed set | a hand-written element wanting its own kind | The set stays closed for hand-written kinds; recipes are one variant by design and `tag()` returns the schema tag |
| Migration changes a diagnostic's code or wording | `lower.rs` assertions | The generic wording is `led`'s wording with the tag interpolated; if a message needs to change, it is not this slice |
| The drill needs a third site (a runtime type) | T5 counts three | Record it and stop: the gate is not met, and the doc says so rather than claiming two |
| Tier enforcement blocks a legitimate busy element | T4's predicate fires on a real element | Write the reason in this doc, extend the recipe instead of adding `tier=1` (Q5) |

---

## 9. Commits

```
T0  (branch only; no commit)
T1  feat(schema): recipe node for pin components
T2  feat(elements): recipe tier; led is a recipe
T3  feat(elements): button is a recipe
T4  test(elements): recipe coherence and tier enforcement
T5  docs(recipe): gate measured
```

Every commit is `cargo test --workspace` green with 14/14 cells identical. T1
changes no generated byte (nothing declares a recipe yet); T2 and T3 change no
generated byte (the recipe reproduces the hand-written element); T4 adds tests
only.

---

## 10. If you say "go"

I start at T1 and go in order, stopping after T3 if any cell differs, so the
regression is one commit wide rather than buried under `button`'s migration.
Questions I need answered first: Q1–Q6. On completion I record the measured
drill — file count per site, whether `codes.rs` was touched, and the compile
results — in §11 of this file, in `button.md`'s format, and re-measure the
buzzer drill for the plan's phase-2 table.

---

## 11. Measured (T5)

The slice shipped as five commits on `recipe`, all green. The gate:

- **14/14 golden cells byte-identical.** `cargo test -p espforge --test
  golden_projects` passes with `led` and `button` no longer having a Rust
  element (helloworld 4, blink 4, button 4, button_async 2). No cell was
  regenerated: the recipe reproduces the hand-written elements' tokens to the
  byte, the `SAFETY` anchor built from the same runtime string included.
- **It compiles.** `cargo test -p espforge --test generated_projects_compile
  -- --ignored` passed for all four cases, including the two this slice is
  about: `02.Digital/button` (`esp32c3`, `blocking`) and
  `02.Digital/button_async` (`esp32c3`, `embassy`). rustc 1.99.0 with
  `riscv32imc-unknown-none-elf` installed; the ESP-HAL, esp-rtos and embassy
  crates were already in the shared `target/generated-check` dir, so the run
  finished in ~16s with no fetch.
- `cargo fmt --all --check`, `cargo clippy --workspace` and
  `cargo test --workspace` are green, and `git status` is clean except the
  untracked plan docs.

**The drill, measured.** A throwaway `<buzzer>` (GPIO output, one bool) added to
the schema plus one example, generated with
`cargo run -p espforge -- gen --example 02.Digital/buzzer --out /tmp/buzzer`:

| Site | File |
|---|---|
| schema block | `espforge-schema/src/espf.schema.kdl` |
| example | `espforge-examples/src/examples/02.Digital/buzzer/…` |

**Two sites.** Zero Rust files, zero registry edits, zero `codes.rs` edits, zero
`model.rs` edits. The command exited 0 and wrote the same 13-file tree a
hand-written element writes: `src/generated.rs` carried
`pub buzzer: espforge_runtime::Led`, the
`unsafe { registry.peripherals.GPIO18.clone_unchecked() }` initialiser and the
`// SAFETY` line, and no `match` anywhere was touched. The buzzer took the
recipe's default `E0615`, so it needed no code of its own.

`cargo test --test golden_projects` failed during the drill, exactly as
predicted (a declared cell with no golden), so the drill was reverted
(`git checkout -- .`, `rm -rf` the example directory) before the suite ran, and
the suite is green again.

The phase-1 baseline was seven files, three of them bookkeeping (`button.md`
§10: `codes.rs`, the module line, the variant + owner kind). The recipe removes
all three. The tier's gate is met at two sites, and `finalplan.md` §7's
"End of phase 2 | 2" is the measured number rather than the plan's.

**Nothing hand-kept.** `grep -rn '"led"\|"button"' espforge-elements/src`
returns test code only (`lower.rs`, `model.rs`, `registry.rs`), and
`espforge-elements/src/components/` holds `recipe.rs` and `mod.rs` only.
`led.rs` and `button.rs` are deleted, and with them `Component::Button`,
`OwnerKind::Led` and `OwnerKind::Button`; `component_enum!` is one variant line
and `OwnerKind` is `Gpio` plus `Recipe(&'static str)`.

Five further files carry the tier itself, which a new element does not touch —
the drill above edited none of them:

- `espforge-schema/src/lib.rs` — the `Recipe`/`Direction` types, the seven load
  rules and their tests.
- `espforge-diagnostics/src/codes.rs` — one shared `E0615`, added once for the
  tier rather than once per element.
- `espforge-elements/src/{components/recipe.rs,registry.rs,model.rs,lower.rs}` —
  the interpreter, the second door in `registry::element`, and the T4
  coherence and tier-enforcement tests.
- `espforge-emit/src/codegen/{rust.rs,overlay.rs}` — two test fixtures build a
  `RecipeComponent::new("led", …)` instead of a `Led { … }`; no emitter change.

The coherence path was exercised for real, not only in a pure unit test: with a
stub `led` element temporarily registered beside the recipe, the suite failed
with ``<led> has 1 hand-written implementation(s) and a `recipe`: delete
`espforge-elements/src/components/led.rs`, its schema block already has a
`recipe` ``. The stub was reverted; nothing about it is committed.
