# helloworld.md — helloworld, end to end, in the new architecture

The smallest slice of `finalplan.md` that produces a working `01.Basics/helloworld`
(`esp32c3`, `blocking`), and then stops. It exists to put the load-bearing part of
the foundation in place — one parse → lower → emit → write path, proven on one
example against the legacy golden — so everything after it is widening work
rather than rework.

Everything else in phase 1 (`blink`, `led`, `gpio`, the element framework, the
rest of the CLI, CI, diagnostics corpus) gets its own doc after this one. If a
step here does not change a byte of the generated project or prove that it is
real, it is not in this slice.

---

## 1. The gate

Three checks. "Running end to end" means 1 and 2; 3 is what a user does.

1. **Byte-identical.** A test generates the example into a temp dir **in-process**
   and compares the whole tree — 13 files — against
   `tests/golden/01.Basics.helloworld.esp32c3.blocking/`, moved over from the
   legacy tree. Byte for byte, including `.espforge/manifest.json` (no trailing
   newline), `README.txt` (trailing newline), and the FNV-1a hashes in the
   manifest.
2. **It compiles.** `cargo check` inside the generated project, with
   `espforge-runtime = { path = ... }` pointing at the new runtime crate,
   succeeds. This is a proxy for "works": we cannot flash here, and the legacy
   repo's own CI uses the same proxy.
3. **The command a user types.**
   `espforge gen --example 01.Basics/helloworld --out /tmp/hw` writes the same
   tree.

The oracle is the **legacy tree**, not the packed snapshot the plan warns about.
Everything ported here is moved with its tests, not retyped.

---

## 2. What helloworld's bytes actually depend on

This table is the scope argument. A crate is in this slice only if the 13 files
change without it, or if check 2 cannot run.

| Crate | Why it is here now | What it contains at the end of the slice |
|---|---|---|
| `espforge-diagnostics` | The parser and the `# espforge` reader produce typed failures; codes are the contract | `Diagnostic`, renderer, codes for syntax/structure/core/options only |
| `espf` | Reads the example | parser, `ParsedNode`, values, spans; no writer, no `Refs` |
| `espforge-schema` | Option defaults + `derived` + the element vocabulary in one place | `espf.schema.kdl` (core section only), one KDL walker, `LazyLock` |
| `espforge-elements` | `Meta`, option resolution, the empty `Board` seam | core section reader, `Answers`/`resolve`, `Board`, `Component` (no variants) |
| `espforge-emit` | The bytes | ported chip/cargo/config/wokwi/diagram/rust/plan/comments/overlay/equiv, manifest, README, write |
| `espforge-core` | parse → options → emit → write, with no printing | `source`, `pipeline`, `project::generate` |
| `espforge-runtime` | Only for check 2 | `Logger`, `Delay`, `embedded_hal` re-export |
| `espforge-examples` | The example's `.espf` and `app.rs` | read half only: `categories`, `find`, `Example`, `ExampleFile` |
| `espforge` | Only for check 3 | one `gen` subcommand |

Notably absent even though phase 1 needs them eventually: `linkme`, the `Element`
trait, `LowerCx`, `ResourceGraph`, pin claims, `Lowered`/`EmitCx`, the generated
`Attrs` build script, and the runtime's `Led`. None of them can change a byte of a
project with no peripherals and no components, so building them now would be
unverified scaffolding.

---

## 3. Steps

Each step ends green and is one commit. "Verify" is the command that has to pass
before the next step starts.

### H0 — bootstrap, `legacy/`, the one golden

**Do**

- `git switch -c rewrite`.
- `mkdir legacy`, `git mv` into it: `espf/`, the eleven `espforge*` crate dirs,
  `examples/`, `testing/`, `Cargo.lock`, `dist-workspace.toml`, `README.md`.
  `.github/` stays at the root until H8. Plan docs stay at the root.
- New root `Cargo.toml`: `resolver = "3"`, the nine members,
  `exclude = ["legacy"]`, `[workspace.package]` version `0.6.0` / edition `2024`
  / `rust-version = "1.95"`, and `[workspace.dependencies]` with only what the
  slice uses: the `espforge-*` paths, `espf`, `winnow 0.7`,
  `annotate-snippets 0.12`, `serde_json 1`, `serde 1`, `anyhow 1`, `kdl 6`,
  `toml_edit 0.22`, `syn 3`, `quote 1`, `proc-macro2 1`, `prettyplease 0.3`,
  `toml 1`, `clap 4`, `include_dir 0.7.4` (`glob`),
  `esp-metadata-generated 0.5.3` (`build-script`), `tempfile 3`.
  No `linkme` yet.
- Nine crates as `src/lib.rs` stubs (the CLI as `main.rs`), with the plan's
  dependency edges and nothing else. `espforge_examples` keeps its underscore
  package name so imports do not churn.
- `espforge-examples`: port only the read half of the legacy crate, with
  **only `01.Basics/helloworld`** under `src/examples/`. `materialize`/`list`
  wait for the command that needs them (phase 6); until then nothing can leak
  `example.toml` into a user's project.
- Copy `legacy/espforge/tests/golden/01.Basics.helloworld.esp32c3.blocking` to
  `espforge/tests/golden/` under the same slug. The other 29 goldens stay in
  `legacy/` and arrive with their examples.

**Verify:** `cargo build --workspace`, `cargo test --workspace` (the ported
`find("01.Basics/helloworld")` test passes), and `git status` is clean.

### H1 — `espforge-diagnostics`

**Do** — port from `legacy/espforge-diagnostics` with its tests:

- `Diagnostic`, `Severity`, `Label`, `Edit`, `Fix`, `Diagnostics`, `DiagCode`,
  `codes.rs`, the annotate-snippets renderer.
- Only codes this slice can reach, with the legacy numbers: `E0101–E0104`,
  `E0201–E0209`, `E0301`, `E0302`, `E0306–E0315`. Never renumber.
- **Delete** `for_message`, `dynamic_code`, the title→code table and `E0999`.
  Constructors take a `DiagCode` first; there is no way to build one without it.
- Add the two structural tests: every constant matches `^[EW]\d{4}$` and is
  unique.

**Verify:** `cargo test -p espforge-diagnostics`; `grep -rn "for_message\|E0999"`
returns nothing.

### H2 — `espf`

**Do** — port `parser.rs`, `value.rs`, `lib.rs` and the span helpers, with their
tests:

- `Parsed` (sections, junk, structure errors, `header_offsets`), `ParsedNode`,
  `TagAttribute`, dotted keys, `+=`, comment blocks, trailing `// ` comments,
  `Value`/`Object` assembly, byte spans.
- Errors are `Diagnostic`s built at the point of failure with a code; the
  `EspfError` intermediate disappears and tests assert codes.
- Drop `writer.rs` and `context::Refs`.

**Verify:** `cargo test -p espf`; `cargo tree -p espf` shows only
`espforge-diagnostics` from the workspace.

### H3 — `espforge-schema`

**Do**

- `src/espf.schema.kdl`, the phase-1 file cut to what is implemented: five
  sections (four of them empty of elements), elements `project`, `chip`,
  `runtime`, `console`, `options`, and options `unstable-hal` (default true),
  `alloc` (default true), `embassy` (derived), `probe-rs`,
  `stack-smashing-protection`, `wokwi` (default true). No `wifi`,
  `ble-trouble`, `keep-zig-source`, `gpio`, `led`, devices, `status="planned"`.
  Every element carries a `summary` and a one-line `example`.
- `load(&str) -> Result<Schema, Vec<SchemaError>>` — the **only** KDL walker in
  the workspace. Spans everywhere; checks reserved names (`help`, `examples`,
  `overview`), unique tags, `ref to=`/`see_also` targets, summary + example
  present.
- `pub fn schema() -> &'static Schema` via `LazyLock` over `include_str!`.

**Verify:** `cargo test -p espforge-schema` (loads; each example fragment parses
with `espf`); `git status` clean after a build.

### H4 — `espforge-elements`: core section and options

**Do**

- `model`: `Meta`, `Console`, `Runtime`, `Ident` (with `rust_name()`),
  `Board { meta, components: Vec<Component> }`, and `pub enum Component {}` —
  zero variants, so "this project has no components" is proved by the type
  rather than assumed. `ClaimedPin`, `ResourceGraph` and the pin graph arrive
  with `led`.
- `core`: port `legacy/espforge-options/src/section.rs` almost verbatim —
  `core_from_section(section_offset, nodes)` → `(Meta, Option<&ParsedNode>)`,
  same messages, same spans, same codes. It reads attributes by hand because
  that is what the oracle does (see Q3).
- `options`: port `Answers`, `Origin`, `resolve` (explicit → defaults → derived
  → `requires` → chip `compatible` → `selection_group` → console/jtag) and
  `chips.rs`, with the resolver's tests. Derived option names come from
  `derived=#true`; the `runtime` element supplies `embassy`. No `DERIVED`
  constant. Wi-Fi/BLE options are not in this schema, so those tests stay in
  `legacy/`.

**Verify:** `cargo test -p espforge-elements`; a test resolves the real
helloworld header to `Meta { chip: esp32c3, runtime: Blocking, console: Uart }`
and `unstable-hal=on, alloc=on, wokwi=on`, everything else off.

### H5 — `espforge-emit`

**Do** — port verbatim first, then cut, one `// returns in phase N` comment per
cut:

| File | Cut |
|---|---|
| `chip.rs`, `config.rs`, `diagram.rs`, `comments.rs`, `plan.rs`, `manifest.rs`, `runtime_source.rs`, `equiv.rs` | nothing |
| `cargo.rs` | wifi/ble/radio/smoltcp/embassy-net/trouble/bt-hci/embedded-io (phase 4) |
| `wokwi.rs` | `chip_section` (phase 3) |
| `overlay.rs` | everything except `{{ board }}` and `{{ serialMonitor }}`; the `{{ id.attr }}` lookup stays but no component can match yet |
| `rust.rs` | SPI/I2C/UART arms, `wifi()`, `Services::new`, bus preludes (phases 3–4); `generated()`/`safety_comments()` keep their loops over `board.components` |
| `project.rs` | chip/zig seeding (phase 3) |
| `lib.rs` | nothing: `generate_from_board`, `write`, `write_assets`, `write_all` |

- With zero components the loops must emit exactly the empty `Components`,
  `Devices {}` and `Services {}` of the golden.
- `RUNTIME_VERSION` and the manifest's `template_version` both become
  `env!("CARGO_PKG_VERSION")` (0.6.0).

**Verify:** `cargo test -p espforge-emit`; a test that an empty board for
`esp32c3`/blocking yields the 12 planned artifacts, each of which parses
(`syn`/`toml_edit`/`serde_json`).

### H6 — `espforge-core` and the golden test

**Do**

- `source.rs`: the source is a FILE or `--example`; app logic comes from the
  embedded `app/rust/app.rs`; overlay files come from the example. No cwd
  discovery, no chip/zig collection yet.
- `pipeline.rs`: parse → structure → core → options (no flags yet).
- `project.rs`: `generate(source, out)` — emit, layer the overlay, copy
  `src/app.rs`, write with ownership, classify failures. Nothing prints.
- `espforge/tests/golden_projects.rs`, using `espforge-core` in-process (no
  subprocess; the `espforge` binary is still H0's stub until H8). `example.toml`
  sits beside the embedded example (`chips = ["esp32c3"]`,
  `runtimes = ["blocking"]`), is discovered through `categories()`, and each
  declared cell exercises the whole tree — the 12 emitted artifacts plus the
  copied `src/app.rs`, 13 files — with an `expected/actual` diff and
  `UPDATE_GOLDENS=1`. A declared cell with no golden dir is an error, and a
  golden dir with no declared cell is an error; that replaces `MATRIX`.

**Verify:** check 1 in §1: `cargo test -p espforge --test golden_projects` green;
deleting one byte turns it red with a readable diff.

### H7 — runtime, and check 2

**Do**

- `espforge-runtime`: port `Logger`, `Delay` (blocking and `embassy` feature) and
  the `embedded_hal` re-export, behind the plan's single
  `#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))] mod hw` +
  `pub use hw::*`. `Led`, `Button`, buses, wifi stay in `legacy/`.
- An `#[ignore]`d test that generates helloworld into a temp dir through
  `project::generate` with `RuntimeSource::Path(...)` pointing at that crate, and
  runs `cargo check` (shared `CARGO_TARGET_DIR`, so repeat runs are cheap). The
  CLI's `--runtime-path` flag only learns to set this in H8.

**Verify:** check 2 in §1, run by hand; record in the commit message what
toolchain/target/network it needed.

### H8 — `espforge gen`

**Do**

- `clap` with one subcommand: `gen [FILE] --example <category/name> --out <dir>
  --chip --runtime --console --name --defaults --option --runtime-path`.
  Exit codes 0/1/2; diagnostics rendered by `espforge-diagnostics`; the legacy
  footer wording. `--help` lists only `gen`.
- Delete the legacy CI workflow and add the minimal one: `cargo fmt --check`,
  `cargo test --workspace`, and the ignored compile test as its own job. No
  `git diff --exit-code`, because nothing generated is committed.

**Verify:** check 3 in §1 (`diff -r` against the golden tree is empty), and
`cargo test --workspace` from a clean `target/`.

---

## 4. Deliberately absent

`blink`, `led`, `gpio`, any second `Component` variant, `Lowered`/`EmitCx`,
`overlay_attr`, `runtime_items`, `ResourceGraph` and pin claims, the `Element`
trait, `LowerCx`, the `linkme` registry, the coherence test, the generated
`Attrs` build script, `materialize`/`list`, `check`/`build`/`version` and their
transcripts, `check --format json`, the diagnostics corpus, the no-panic audit,
the per-element contract test, `esp32s3`, `embassy` cells, wifi/buses/devices,
custom Wokwi chips, recipes, and the `espf` catalog.

Also untouched: `prettyplease`/`syn`/`quote`/`toml_edit` versions. Byte identity
is downstream of them.

---

## 5. Decisions to make before H0

My recommendation is bold. Say which you want differently and I will fold the
answer back into this file before writing code.

**Q1 — Does the gate include compiling the generated project?**
(a) **yes: the gate includes `cargo check` on what we generate.** (b) bytes
only.
(b) leaves `espforge-runtime` empty and drops H7, and would let us ship a golden
we have never compiled — precisely the gap the plan's
`generated_projects_compile` exists to close. (a) costs a ~150-line runtime and
one ignored test.

**Q2 — Does the `gen` command belong in this slice?**
(a) **yes, `gen` only.** (b) no CLI; H6's test is the deliverable, `gen` lands
with `check`.
(a) is ~120 lines of clap plus rendering, and it is the difference between "a
test passes" and "helloworld runs". (b) is smaller but leaves the pipeline with
no user-facing entry point until the next slice.

**Q3 — Generated `Attrs` now, or the hand-written core reader?**
The plan (M2/M3) puts the generator before the core section. But for helloworld
the generated structs would cover five elements and ~eight attributes, and the
hand-written reader already exists in `legacy/espforge-options/src/section.rs`
with eight tests and the exact messages we must reproduce.
(a) **port `section.rs` now; build the generator in the `gpio`/`led` slice,
where it has more than one consumer and a reason to exist.** (b) generator first,
as the plan says.
(b) is the plan's order and avoids rewriting the core reader later; (a) removes
the largest piece of new, un-oracled code from a slice whose whole point is
"small and provable", at the cost of writing the core reader twice (or leaving
it hand-written).

**Q4 — How much of the options resolver?**
(a) **whole port with its tests.** (b) defaults + derived only.
The resolver is proven leaf logic and `helloworld`'s `Cargo.toml` depends on its
defaults; half-porting a decision table is how drift starts. `requires`/
`compatible`/`selection_group` are dead for this schema, so this is a cost paid
now for a path already known to work — (a).

**Q5 — Golden harness shape?**
(a) **`example.toml` + discovery from the first test**, with exactly one declared
cell. (b) one plain `helloworld_is_byte_identical()` test now, discovery with
`blink`.
(a) settles the format while there is nothing to retrofit and costs ~40 lines;
(b) is smaller but guarantees the `MATRIX`-shaped interim the plan bans.

**Q6 — Schema content?**
(a) **only the five core elements.** (b) also declare `gpio` and `led` now,
unimplemented.
(b) would violate the coherence rule this slice cannot yet test — "an element is
in the schema iff it is registered" — and buys nothing for helloworld. They
arrive with their implementations, per (a).

---

## 6. Risks

| Risk | Signal | Action |
|---|---|---|
| Port drift breaks byte identity | H6 golden diff | Port verbatim; no formatting "improvements"; bisect with `equiv` before touching bytes |
| The empty-component path is not what the loop prints | `src/generated.rs` | Compare the zero-component output before adding anything else |
| `prettyplease`/`syn` version bump reformats everything | H6 diff | Pin the versions; never bump in this slice |
| Manifest hash mismatch | H6 | Port `checksum` verbatim (FNV-1a 64, `{hash:016x}`), `template_version` 0.6.0, no trailing newline |
| Compile check needs nightly/target/network | H7 | Run by hand, report what it needed; do not assume CI has it |
| Deferring the generator means core is rewritten | `gpio`/`led` slice | Accepted in Q3; the rewrite is one file and the tests carry over |

---

## 7. Commits

```
H0  chore: new workspace, legacy/ oracle, helloworld embedded + golden
H1  feat(diagnostics): typed codes, no for_message, no E0999
H2  feat(espf): parser to Diagnostic-with-code
H3  feat(schema): phase-1 espf.schema.kdl and the single KDL walker
H4  feat(elements): Meta, core section, options resolver, empty Board seam
H5  feat(emit): ported emitters; zero-component helloworld
H6  feat(core): generate-to-disk, helloworld byte-identical
H7  feat(runtime): Logger, Delay; generated project compiles
H8  feat(cli): gen
```

Every commit is `cargo test --workspace` green. H6 is the first real proof; before
it the slice is plumbing.

---

## 8. If you say "go"

I start at H0 and go in order, stopping after H6 to show you the byte-identical
result before the runtime and the CLI. Questions I need answered first: Q1–Q6.
