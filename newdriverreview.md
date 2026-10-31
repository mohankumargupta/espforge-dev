# `newdriverreview.md`: Review of `newdriverdesign.md`

> **Verdict:** the diagnosis is right, the goal is right, the mechanism is not.
> Adopt the direction (generic lowering + dynamic features), **do not implement
> §4 as written**. The `device_recipe` block as specified cannot reproduce two of
> the four display goldens, silently drops four existing validations, and moves
> the schema out of the only crate that can ship it.

---

## 1. What the document gets right

These parts are worth keeping verbatim:

1. **The coupling complaint is accurate.** `struct Displays`
   (`espforge-emit/src/codegen/cargo.rs:56`), `device_enum!` plus
   `uses_ssd1306`/`uses_ili9341`/`takes_spi_component`
   (`espforge-elements/src/model.rs:561-670`), and the two hand-written elements
   are exactly the four-crate tax the doc describes.
2. **Dynamic feature emission is correct and safe.** `board.driver_features()`
   replacing the manual `Displays { ssd1306, ili9341 }` flag plumbing is a
   strict improvement with no behavioural risk. This piece should land on its
   own, regardless of what happens to the rest.
3. **"One crate to add a driver" is the correct north star**, and it matches the
   repo's own precedent: the `# components` recipe tier already proves the idea
   (`espforge-elements/src/components/recipe.rs`), and
   `espforge-elements/src/registry.rs` already enforces "exactly one
   implementation: a Rust file **or** a schema recipe, never both."
4. **The walkthrough in §5 is the right shape for a contributor doc.** A driver
   should be: feature, module, schema block. Keep that prose.

---

## 2. Blocking issues

### 2.1 The constructor template cannot express the drivers we already have

This is the fatal one. `device_recipe.constructor` is a string whose `{name}`
placeholders are replaced by the **attribute's spelling**. Every display
attribute we have is not a Rust literal:

| Attribute | ESPF spelling | What emission must produce today | Source |
|---|---|---|---|
| `size="128x64"` | `128x64` | `espforge_drivers::Oled128x64`, **in two positions** (type parameter *and* value) | `devices/ssd1306.rs:125-145` |
| `rotation="90"` | `90` | `espforge_drivers::DisplayRotation::Rotate90` | `devices/mod.rs:37-45` |
| `rotation="90"` (TFT) | `90` | `espforge_drivers::Orientation::Landscape` — a *different* mapping for the same attribute | `devices/mod.rs:47-55` |
| `rgb="false"` | `false` | `espforge_drivers::ColorOrder::Bgr` | `devices/ili9341.rs:189-194` |
| `dc="$gpio4"` | pin number | `esp_hal::gpio::Output::new(GPIO4, Level::Low, OutputConfig::default())` — a nested expression, not a value | `devices/ili9341.rs:205-212` |
| `backlight` absent | — | `None`; when present `Some(Output::new(…))` — two different shapes | `devices/ili9341.rs:225-241` |

So `Ssd1306Display::new(components.{i2c_component}, {size}, {rotation})` expands
to `…new(components.display_bus, 128x64, 90)`, which does not compile. To fix it
you need a per-driver, per-attribute mapping from spelling to arbitrary Rust
expression — i.e. codegen. That is the very thing the design says it removes.
The template only works for drivers whose attributes are all numeric/boolean
literals passed straight through, which is no driver in the repo today.

**Also unspecified:** `render_constructor(&self.constructor_template, self)` is
called but never defined — no signature, no escaping rule, no behaviour for a
missing placeholder, no rule for an absent optional pin, no rule for `{id}`.

### 2.2 It loses four validations and a safety comment

The generic lowerer in §4.3 replicates the *easy* half of each element and drops
the rest:

| Lost invariant | Where it lives today | Consequence |
|---|---|---|
| Bus **kind** must be the declared kind (`i2c_component` must not be an `SpiComponent`) | `devices/ssd1306.rs:57-66`, `devices/ili9341.rs:68-78` | `rail="…"` typo'd onto an SPI component passes lowering and emits uncompilable code |
| One device per SPI component | `devices/ili9341.rs:86-101` | Two TFTs on one chip select both `take()` the bus; the second panics at runtime |
| `cx.safety_comment(…)` naming the claimed pins | `devices/ili9341.rs:218-224` | Emission uses `unsafe { … clone_unchecked() }` with no `SAFETY:` note; goldens change |
| Attribute-level spans in diagnostics | `devices/mod.rs:span_of`, used by `unknown_gpio`/`wrong_direction` | The design reports `node_span(node)` everywhere, so the caret lands on the tag, not the bad value |

`device_recipe { bus "i2c_component" }` names the *attribute*, never the
*component kind*, so the generic lowerer cannot know what to type-check against.
The schema already has the vocabulary for this: `attr … kind="ref" to="…"` — the
recipe should carry `expect = "i2c_component"` and the lowerer should match the
`Component` variant.

### 2.3 It re-implements the typed `Attrs` layer by hand

`espforge-elements/build.rs` generates one `Attrs` struct per schema element into
`OUT_DIR`, and `espforge-elements/src/attrs.rs` is documented as "the only place
a spelling becomes a typed value", with presence and defaults read from the
schema. The design bypasses all of it with:

```rust
if let Some(val) = node.get_attr(&attr.name).or(attr.default.as_deref()) {
    properties.insert(attr.name.clone(), val.to_string());   // <-- no coercion, no validation
}
```

Three concrete defects in five lines:

- `kind="choice"` values are never checked against `choices`, so
  `rotation="45"` becomes a property instead of a diagnostic.
- `id` is an attribute and is neither the bus nor an aux pin, so **`properties`
  contains `"id"`**.
- `BTreeMap` re-sorts alphabetically, so any positional emission loses schema
  order.

Any real fix keeps `Attrs::from_node` (it is schema-driven and already generic
across elements) and stores typed values, not strings.

### 2.4 `takes_spi_component` becomes a substring sniff

```rust
r.constructor_template.contains(".take()")
```

Today this is a typed fact: `Board::takes_spi_component` matches
`Device::Ili9341` (`model.rs:665-675`), and `components/spi.rs` emits its field as
`Option` because of it. Under the design, editing a constructor string silently
changes the *component's* emitted field type. Declare it: `takes_bus = #true`.

### 2.5 The schema merge is not publishable

```rust
pub const DRIVERS_SCHEMA_KDL: &str = include_str!("../../espforge-drivers/drivers.schema.kdl");
```

`include_str!` reaching outside the crate root means `espforge-schema`'s package
does not contain the file: `cargo package`/`publish` and any vendored consumer
break, and the build depends on a sibling checkout layout. §4.2 also leaves
`espforge-elements/build.rs:24` (which loads `SCHEMA_KDL` only) generating
`Attrs` for the *unmerged* schema — so new device elements would get no `Attrs`
at all, failing the very "single source of truth" property the design claims.

Note the tension the document never resolves: **the drivers crate cannot own a
file that the schema crate must embed.** Pick one:

- **(a)** fragments live in `espforge-schema/schema/devices.kdl`, composed by
  `load`/`schema()`; adding a driver touches 2 crates; or
- **(b)** `espforge-schema` gains `fn load_all(&[&str])`, each consumer composes
  `SCHEMA_KDL + espforge_drivers::DRIVERS_SCHEMA_KDL`; adding a driver touches 1
  crate, but every consumer must remember to compose (a coherence test can
  enforce it).

`(b)` preserves the stated goal. It needs the drivers crate depended on by
`espforge-elements` — fine, but say so, because it inverts today's dependency
graph and needs the `no_std` drivers crate to stay host-compilable (it already
is, per the crate docs).

### 2.6 The sketch does not compile

`LowerCx::components(&self) -> &[Component]`
(`espforge-elements/src/element.rs:241`). §4.3 keeps `component` alive across
`cx.claim_pin(…)` (which is `&mut self`, line 252) and uses it again in
`cx.push_device(… component.id().clone() …)`. Borrowck rejects that. Clone
`component.id()` before the aux-pin loop. Minor by itself — but it is evidence
that the specification's code was never type-checked, which is exactly the risk
in §2.1.

### 2.7 The registry's coherence test will fail, and §6 does not say so

`every_schema_element_has_exactly_one_implementation`
(`registry.rs:122-145`) iterates `peripherals | components | devices | services` and
requires each element to be **either** in the `linkme` slice **or** carry a
`recipe`. A `device_recipe` element is neither, so the test panics with "add a
`recipe` to its schema block, or `espforge-elements/src/devices/<tag>.rs`".
`element()`'s fallback is likewise filtered to `peripherals | components`
(`registry.rs:53-66`). Both need a third door — and the door should extend the
*existing* predicate rather than fork a second mechanism.

---

## 3. Smaller issues

- **`embedded-graphics` for a CO2 sensor.** §4.4 adds it whenever
  `!board.devices.is_empty()`. A `<scd4x>` would pull a graphics crate. It is a
  display concern today (`cargo.rs:52-58`). Make it part of the recipe
  (`deps "embedded-graphics"`), or derive it from the feature list.
- **A single-variant enum is ceremony.** `enum Device { Recipe(RecipeDevice) }`
  buys nothing over `struct Device` unless a second variant exists. Either make
  the escape hatch real (see §4) or drop the enum.
- **The "zero edits" claim contradicts §4.2/§4.4.** §2 and §6 claim 0 edits to
  `espforge-schema`, `espforge-emit` and `espforge-elements`; the migration
  itself edits all three, and the `Displays`-using tests in
  `espforge-emit/src/codegen/cargo.rs` (seven call sites) plus
  `espforge-elements/src/lower.rs:1362,1442` (asserting `uses_ssd1306`) must be
  rewritten. The claim is true *after* migration; say that.
- **Diagnostic wording is load-bearing.** `invalid_element` carries per-attribute
  expected-value text (`"rotation" => "one of 0, 90, 180, 270"`,
  `devices/mod.rs`, `invalid_element`). A generic lowerer cannot reproduce it from
  stringly properties, so those tests and goldens will churn. The doc's checklist
  should include "message wording stays byte-identical" — the repo clearly cares
  (it is why `attrs.rs` returns `Issue` instead of formatting).
- **`in # components`-style errors are good; keep them.** The generic path's
  messages are close to the handwritten ones — good instinct, worth stating as a
  requirement rather than leaving to chance.

---

## 4. Recommended revision

Split the goal in two, because they have very different risk:

**Phase 1 — dynamic features (do now, independent of everything else).**
`Board::driver_features()` → `drivers_dependency`, delete `struct Displays`,
delete `cargo::Displays` threading, gate `embedded-graphics` on the feature set.
Goldens must not move. ~1 hour, no schema change.

**Phase 2 — generic devices, but with typed slots, not a constructor template.**
Keep the recipe idea and fix §2.1 by replacing the one-string constructor with
positional slots whose conversions are data, not a parser:

```kdl
device_recipe {
    bus "i2c_component" expect="i2c_component"
    feature "ssd1306"
    runtime_type "espforge_drivers::Ssd1306Display<{size}>"
    arg "size"     map { "128x64" "espforge_drivers::Oled128x64"  "128x32" "espforge_drivers::Oled128x32" }
    arg "rotation" map { "0" "espforge_drivers::DisplayRotation::Rotate0"  "90" "espforge_drivers::DisplayRotation::Rotate90" } # etc
    arg "dc"       pin map { n "esp_hal::gpio::Output::new(GPIO{n}, esp_hal::gpio::Level::Low, esp_hal::gpio::OutputConfig::default())" }
}
```

Rules that make it implementable and checkable:

1. Slot order is schema `arg` order; `pin` slots expand `{n}` and are wrapped in
   `Some(…)/None` by presence — no expression grammar, no parser.
2. Unknown/unlisted value → a diagnostic that quotes the `choices` (keeps §3's
   wording requirement).
3. `expect` forces the component-kind match (fixes §2.2).
4. `takes_bus = #true` replaces `.contains(".take()")` (§2.4).
5. The lowerer emits `cx.safety_comment` from the claimed pins, as the
   hand-written elements do.
6. Validation order and messages are copied from the current elements, and the
   existing `registry.rs` coherence test is extended by one word
   (`|| element.device_recipe.is_some()`), not by a parallel door.

**Phase 3 — escape hatch, deliberately.** Keep `#[distributed_slice(ELEMENTS)]`
as the path for a driver the recipe cannot express (async sensors needing a
`.await` init, DMA, multi-bus parts). `<ili9341>` is arguably already that case.
The honest end state is "simple drivers are schema-only; complex ones are one
Rust file plus a schema block" — which is still 1 crate, and does not pretend
that a template language can absorb arbitrary driver wiring.

---

## 5. Acceptance criteria to add to §6

The current checklist verifies the happy path. It needs the guards that would
have caught the above:

- [ ] `cargo test --workspace` green, and the four `06.Displays.*` goldens
  **byte-identical** (including `SAFETY:` comments and Cargo feature order).
- [ ] `<ssd1306 rail="$spi_bus">` (wrong component kind) is refused with the
  existing diagnostic.
- [ ] A second `<ili9341>` on the same `<spi_component>` is refused.
- [ ] `rotation="45"` is refused, quoting the allowed values.
- [ ] A `<scd4x>` project's `Cargo.toml` contains no `embedded-graphics`.
- [ ] `cargo package -p espforge-schema` succeeds (the schema is self-contained).
- [ ] `espforge-elements/build.rs` generates `Attrs` for device elements too.
- [ ] The registry coherence test covers the device-recipe door (adding a
  `device_recipe` element with a hand-written `.rs` is refused).

---

## 6. Scorecard

| Aspect | Grade | Note |
|---|---|---|
| Problem identification | A | Accurate and evidence-based |
| Architectural direction | B+ | Right idea; own precedent (component recipes) under-used |
| Mechanism (`constructor` template, stringly properties) | D | Cannot express today's drivers; would not compile |
| Preserved invariants | D | 4 validations + safety comment lost, unmentioned |
| Schema ownership / packaging | C- | `include_str!` outside the crate root is not shippable |
| Migration plan & checklist | C | Correct files named, but tests and coherence gates omitted |
| Risk framing | C | No statement of what changes in goldens or diagnostics |

**Recommendation:** land Phase 1 now. Rewrite §4 around typed slots (§4 above)
before anyone implements Phase 2, and add the §5 criteria to the document's own
checklist.
