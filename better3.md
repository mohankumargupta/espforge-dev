# better3: components and devices as data, not code

> Builds on `better.md` (the architectural audit) and `better2.md` (the contributor UX plan).
> This doc does not replace them — it replaces their *end state* with a smaller one.
>
> better2's finish line was: "1 schema block + 1 file + 1 registry line + runtime file + example."
> better3's finish line: **most elements are a schema block and an example. Nothing else exists.**
> And for everything that still needs code, the scaffolder writes the code, the test names the
> remaining gap, and no registry, no dispatch macro line, and no runtime boilerplate are
> ever hand-edited.

## 1. What better2 got to, and why to push past it

After better2's plan ships, adding `<buzzer>` still means:

| # | Where | Why it's still friction |
|---|---|---|
| 1 | `espf.schema.kdl` | fine — this one should stay |
| 2 | `espforge-elements/src/components/buzzer.rs` | ~40 lines of hand-written `lower`/`json`/`emit`/`overlay_attr` that are *near-identical to `led.rs`* |
| 3 | `espforge-elements/src/lib.rs` | a registry line — pure ceremony; lib.rs already knows the file exists |
| 4 | `espforge-runtime/src/components/buzzer.rs` + `pub use` | another near-copy of `led.rs` with names swapped |
| 5 | example dir | fine — acceptance testing lives here |

Three of those five artifacts are the same shape with rename-applied lookups. `led`, `button`,
`buzzer`, `relay`, `fan` differ only in: tag, which pin attr they claim, which runtime struct they
construct, and what JSON fields they carry. Writing that shape by hand — once per component —
is better2's remaining boilerplate. better2 kept it because "no proc-macro DSL" sounded ruthless.
But the inputs (`Attrs` structs) are already generated from KDL, so the deliverable shape
(`json()`, `emit()`, `overlay_attr()`) is already *determined by the schema*. Determined-by-data
means data can deliver it.

The insight better3 is built on:

> **If the schema already says what a component claims, constructs, serializes, and exposes,
> the Rust for it is a rendering of the schema — not a design decision.**

## 2. The three tiers (this replaces the "one trait for everything" idea)

better2 gave every element the same interface. That forces a 40-line file on elements that need
10 lines of meaning. Instead, classify by *how much is actually custom*:

### Tier 0 — declarative (zero Rust): most components, most devices

A GPIO-backed component whose entire behavior is "claim a pin, construct a runtime type,
carry config into the emitted struct" is a **recipe**, declared in the only place it belongs:

```kdl
element "buzzer" section="components" {
  attr "gpio" kind="reference" ref to="gpio" required
  attr "active_low" kind="bool" default="false"
  example "<buzzer gpio=\"$gpio0\" />"

  recipe {
    pin from="gpio"                 // which attr is the claimed pin
    field "active_low" runtime="active_low"   // pass-through config field
    runtime "espforge_runtime::Buzzer"
  }
}
```

That block *is* the element:

- `lower` is generated: resolve the `gpio` ref, `claim_pin(..., Output)`, build `BuzzerLowered`
  from the recipe's field list.
- `json()` is generated: `kind`, `pin`, `active_low` — exactly the recipe's fields.
- `emit()` is generated: the field declaration, the `Buzzer::new(#pin, #active_low)` init, and the
  SAFETY comment derived from the claimed pin.
- `overlay_attr` is generated: `pin` responds, extra `pin_details`/`peripheral` keys as elsewhere.
- The runtime side is generated too (§4), so there is no hand-written `buzzer.rs` anywhere.

The registry, the coherence, the goldens, the contract test — all the machinery better2 built —
run unchanged, but with **no element file to write and no registry line to add**. The schema
loader synthesizes a `DynElement` from the recipe. Tier 0 elements live only in `espf.schema.kdl`.

### Tier 1 — one macro impl (~15 lines): components with one custom quirk

An element that is 90% recipe but has say one extra claim (e.g., `<button>`'s debounce note, or a
component that emits an extra `let` still means opening one file — but the file is a
declaration of the *delta*, not a re-implementation:

```rust
// espforge-elements/src/components/buzzer.rs — exists ONLY because buzzer has a quirk
element_recipe! {
    tag: "buzzer",
    lower: |node, cx| {
        let pin = cx.gpio_ref(node)?.claim(cx, PinUse::Output)?;
        cx.push(BuzzerLowered::from_recipe(node, cx, pin))
    },
    // everything the recipe can express (json, emit, overlay) stays generated
}
```

The macro checks the declared `recipe` block in the schema at test time and fails compilation of
the *test* if the schema and the code disagree (which fields are generated, which overridden).
A Tier 1 element is: schema block + one small file + example. Still no registry line, because
registration is distributed (§3).

### Tier 2 — full `Element` impl: the genuinely weird

`wifi` (heap, cargo features, net stack), `ili9341`/`tmp102` (bus binding + multiple extra pins),
`spi`/`i2c` (shared `once()` prelude). These are the elements better.md's trait was designed for,
and they keep the full `Element`/`Lowered` interface from better2 §7. Expected population: the
5-ish buses/peripherals/devices plus any new chip with a real protocol — a small minority.

### The rule that keeps honesty

The contract test asserts, per element: **declared tier vs. measured requirement**. If a Tier 0
recipe element's `lower`/`emit` prove expressible, it must not have grown a Rust file; if an
element file's impl contains nothing beyond recipe expression, the test fails with
"`buzzer.rs is a Tier 0 candidate — delete it and move the delta into the recipe`".
Tiers are enforced, not aspirational, so the codebase cannot rot back into every-element-is-Rust.

## 3. Delete the registry entirely: distributed registration

better2's `pub const ELEMENTS: &[...] = &[&Gpio, &Led, ...]` is the last hand-maintained list
(the "remember to add one line" failure mode). Kill it with `linkme`'s distributed slices:

```rust
// in the element file / macro expansion — no central list anywhere
#[distributed_slice(ELEMENTS)]
static BUZZER: DynElement = DynElement::of(Buzzer);
```

- An element file registers itself by existing.
- Tier 0 elements register themselves when the schema loader encounters a `recipe` block —
  zero Rust, zero registration.
- `lib.rs` in `espforge-elements` becomes four `pub mod` + nothing else.
- Duplicate tags, tags colliding with a schema element, and schema elements with neither a
  recipe nor Rust are all caught by the coherence test with the exact file path to touch.

This deletes the last "you must touch N places" site. The contributor's mental model collapses to
one sentence: **"to add a component or device, you either write data or write one file — and the
repo tells you which when you get it wrong."**

## 4. Generated runtime types for Tier 0 and devices

The runtime type for a `pin + config-fields` component is also a rendering of the recipe — so
generate it instead of hand-copying `led.rs`:

- `espforge-runtime` grows `src/components/from_recipe.rs` defining a generic
  `RecipeComponent<P, F…>` (or a `derive(RecipeStruct)` macro driven by a small generated table in
  `OUT_DIR`).
- For `buzzer`, the generated item is literally `espforge_runtime::Buzzer`, a struct with
  `pin: u8, active_low: bool` and `pub fn new(pin: u8, active_low: bool) -> Self`, plus the usual
  `pin()` accessor. Emission paths and the cheap `runtime_items() ⊆ RUNTIME_EXPORTS` test work
  against it exactly as against hand-written structs.
- If a Tier 0 recipe produces a runtime type that a human then wants to *extend* (say, a
  `beep(freq, duration_ms)` method on `Buzzer`), the file rule is:
  open `espforge-runtime/src/components/buzzer_impl.rs`, which the generated struct automatically
  `impl`-merges (`mod buzzer_impl { impl Buzzer { ... } }`). Generated + hand-extended, one file
  for the *extension* only, schema stays the owner of the shape.
- Devices (Tier 2, `binds`-style per better2 §4 Job 3) keep `runtime_item` / `cargo_dep` in KDL
  collected by `EmitCx` — that part of better2 is already right and unchanged.

The point is not macro cleverness; it is that **`Led::new(pin, active_low)` and
`Buzzer::new(pin, active_low)` were never actually different programs.** Renaming based codegen is
what a generator is for, and the schema at that point already knows everything.

## 5. The scaffolder turns "read the docs" into "run one command and fix the red test"

better2's contributor doc was still a checklist to *read*. better3 makes the checklist a program:

```
$ espforge dev new element buzzer --section components --kind pin
✓ wrote recipe block into  espf.schema.kdl            (edit the two TODOs: which pin attr, which runtime type)
✓ wrote example             examples/02.Components/buzzer/buzzer.espf
! contract test is red:     `cargo test contract.buzzer` → "espforge_runtime::Buzzer does not exist"
  → run: espforge dev new runtime-component buzzer   (or edit the recipe `runtime` type to reuse one)
```

And:

```
$ espforge dev new runtime-component buzzer
✓ generated runtime struct  espforge-runtime/src/components/buzzer.rs from the recipe
✓ contract test green:      `cargo test contract.buzzer`
✓ goldens updated:          `UPDATE_GOLDENS=1 cargo test golden.buzzer`
```

Design point: the scaffolder emits **a failing contract test first**, not just files — the red
test is the "what to do next" teacher better2 §3.4 demanded, delivered as a tool instead of a
docs page. Every generation step above is idempotent and re-runnable, so a half-finished element
(fork interrupted, etc.) is repaired by running the same command again.

`espforge dev` is host-CLI-only (behind the same target-arch cfg gate as the rest of espf) and
is three small functions over the existing schema loader + contract harness — no new framework.

## 6. Diagnostics get tier-awareness

One addition to better2 §8: the "no registry entry for `<x>`" and tier-enforcement errors must
name the *remedy* as a tool invocation, not a description:

- schema element without recipe or Rust →
  "`<buzzer>` has no recipe or impl. Run: `espforge dev new element buzzer --section components`."
- Tier 0 candidate file detected → "`buzzer.rs adds nothing the recipe doesn't cover; delete it
  and move the exception into the recipe block, or declare `tier = 1` if it genuinely needs code`."
- schema `recipe` referencing a nonexistent `runtime` path → names the kind of item and the
  `dev new runtime-component` command.

Same rules as before: typed codes, never `E0999`, never prose-derived — just keyed off the new
tier dimension so error text routes to the *smallest* correct next action.

## 7. What this changes in better.md / better2.md's plan

Everything from better2's cut list and migration order (P0–P5) stands. better3 modifies the
*mid-game* and *end state*:

| better2's end state | better3's end state | Change |
|---|---|---|
| trait + registry line per element, hand-written per-element Rust | tiers, recipe in schema, distributed registration | `Element` trait stays as the Tier 2 substrate; most elements never implement it |
| hand-written runtime types per element, one file + `pub use` each | generated from recipe, hand-extension only for methods | `espforge-runtime/src/components/{led,button,buzzer}?` stop being near-copies |
| registry as `pub const ELEMENTS` (one line per element) | `linkme` distributed slices; Tier 0 self-registers from schema | the "add one line to lib.rs" step stops existing |
| contributor doc = 5-line checklist to read | `espforge dev new element` + red-test loop | docs shrink to a page describing tiers and commands; the checklist is executable |
| migration P4 introduces trait+registry first | P4 introduces trait + recipe machinery together (recipe machinery is just "schema → synthesized `DynElement`") | still `led` first as the pathfinder, goldens byte-identical |

Nothing new in the "will NOT do" column: still no dynamic plugins, no external element registry,
no `Box<dyn Lowered>` until third-party elements are a stated goal — `linkme` is compile-time
lingering-of-statics inside this repo, not extensibility. `Proc-macro` expansion is confined to
the existing generated-attrs machinery + the recipe struct generation, both of which better2
already accepted; better3 does not add a new DSL surface (the recipe is schema data, not syntax).

## 8. Definition of done

- [ ] `<buzzer>` as Tier 0: **one KDL block + one example. Zero Rust files. ZERO registry edits.**
- [ ] `<led>` and `<button>` also migrate to Tier 0 recipes (they are the same shape) — proving the
      recipe covers the common case, not just the toy one.
- [ ] Tier 1/2 elements use distributed registration; `pub const ELEMENTS` and any central list are gone.
- [ ] Tier enforcement test: a Tier 0 candidate Rust file is a test failure, not a style nudge.
- [ ] Runtime `Buzzer`-style types generated from recipe with generatable `new()`; hand-extension
      file exists for the cases where a method is needed.
- [ ] `espforge dev new element` + `dev new runtime-component` both exist, are idempotent, and land
      the contributor in a red→green contract-test loop with no reading required beyond tool output.
- [ ] better2's §12 checklist still holds (≤3 crates touched, no `E0999`, no `MATRIX`, etc.) —
      better3 is strictly a *reduction* of it, not an extension.

## 9. Cost estimate if this lands

| Task | better2 | better3 |
|---|---|---|
| GPIO component | 1 KDL block + 1 ~40-line Rust file + 1 registry line + 1 runtime file + 1 example | 1 KDL block (recipe) + 1 example |
| runtime type for it | hand-written file + `pub use` | generated (0 edits) |
| device (`tmp102`-class) | KDL `binds` block + 1 device file | KDL `binds` + `cargo_dep` + 1 file only if it has a protocol quirk (`ili9341`); `tmp102`-class = recipe + binds, no file |
| new bus/peripheral (Tier 2) | 1 file + registry | unchanged (this is the real work and deserves the hand-written code) |

Where better2 said "if it takes longer than 15 minutes, we failed," better3's bar is: **a GPIO
component or simple device is a *data entry* task, doable by someone who has never read a Rust
file in this repo — and the scaffold command walks them through the rare cases where Rust is
actually needed.**
