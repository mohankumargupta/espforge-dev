# Review: simplifying `espforge new` / `examples`

Read against the current tree (`espforge/src/main.rs`, `espforge/src/commands/*`,
`espforge-core/src/{source,project,pipeline}.rs`, `espforge-examples/src/lib.rs`).
No code changed.

## Verdict

The *shape* is right and I would ship it. Five verbs — `examples` → `new` →
`check` → `gen` → `build` — is the standard discover/scaffold/validate/generate/
compile ladder, every verb has one job, and `cli-help.md` reads like a tool
someone can use without reading source. The plan is also close to implementable
as written: the signatures, imports and error conversions in
`cli-simplified.md` line up with what exists.

What I would fix before writing it is mostly *where* the logic lands, not
*what* it does. Four things: exit-code policy is being inherited from the `gen`
module, `new` throws away the user-vs-internal error distinction the repo is
explicitly built around, source discovery becomes a hidden dependency on the
process CWD, and `check` promises more than it verifies. Details below.

## What's good

- **`build` as its own verb.** Today users have to know that `gen` stops before
  cargo. `gen` + `build` makes the boundary explicit and lets `build` own the
  "is cargo even installed" error.
- **`examples` instead of docs.** Listing from `espforge_examples::categories()`
  means the list cannot drift from the embedded tree. `ExampleInfo` flattening
  category/name/title is the right rendering shape.
- **`resolve_example_name` with short-name support.** `espforge new blink`,
  `gen --example blink` and `check --example blink` all work without typing
  `01.Basics/`, and the ambiguous case is *reported* instead of guessed. That's
  the right call, and it's cheap to test (see "Missing").
- **Current-directory discovery.** `espforge check` / `gen` with no arguments
  is what makes the quick-start flow in `cli-help.md` coherent.
- **Fixing `build: wrote ...` in `gen`.** Real bug, good catch; the message is
  produced by the `gen` command today, which is why `build` could not have been
  added cleanly without fixing it.
- **The cheat sheet itself.** Its examples table (21 entries) matches the 21
  embedded example directories, so it was written from the real catalog.

## Changes I'd make to the plan

### 1. Don't hang process policy off `generate.rs` (`check.rs`, `build.rs`)

`cli-simplified.md` makes `user`, `failed` and `exit_code` `pub(super)` in
`generate.rs`, then imports them from `check.rs`. That makes `check` depend on
the `gen` command for the definition of "user error", which is exactly the
thing `main.rs` and `project.rs` say is the binary's job, not one command's.
Put them in `commands/error.rs` (or `commands/mod.rs`) and have `generate`,
`check` and `build` all consume them. Otherwise the third command that needs
them has to import the first, and the `generate` vs `gen` naming makes that
read worse than it is.

Related: `build::run` re-implements the mapping (`Err(Failure::User) => 1`,
`Err(Failure::Internal(..)) => 2`) instead of using `exit_code`, because
`build::execute` returns `Result<ExitCode, Failure>`. Make it `Result<(), Failure>`
and let `exit_code` decide — the two commands should not be able to disagree
about what exit 2 means.

### 2. `new` must not flatten errors to exit 1

`main.rs` documents: *0 success, 1 the user's input is at fault, 2 internal or
IO*. `project.rs` preserves that deliberately: *"deciding that here, from typed
errors, keeps the binary from matching on error text."*

`scaffold_example` returns `Result<Scaffolded, String>` and `new::run` maps
everything to `ExitCode::from(1)`. So "directory already exists" (user) and
"cannot create directory: permission denied" (OS) become the same exit code,
and the second one is now indistinguishable from a typo in a script. Return a
typed error — reuse `ProjectError`, or add `ScaffoldError { User(String), Io(...) }` —
and route it through `exit_code`/`failed`. This is a small change now and
annoying to walk back once scripts depend on it.

### 3. Keep CWD out of `espforge-core`

`resolve_source(None, None) => discover_in_dir(Path::new("."))` means a library
function consults the process working directory. That's fine for the binary and
awkward for core: tests share one CWD and run in parallel, so any test of
discovery would need `set_current_dir`, which is process-global and racy. Pass
the directory in (e.g. `resolve_source(file, example, cwd)`), or keep discovery
in the binary and hand core a resolved `Source`. Core's own docs promise "no
opinion about how a terminal should look"; a promise not to sniff `$PWD` is the
same kind of promise.

Also note `discover_in_dir` deliberately only looks at the top level — good,
otherwise it would find `build/.espforge/source.espf.txt` — but that deserves a
comment, because someone will later "fix" it to recurse.

### 4. `check` currently under-checks relative to what it claims

`project::check` runs `validate` + `app_logic` + `overlay_files`. It never calls
`espforge_emit::generate_from_board`, so it can print `check: 'x' is valid` and
then have `build` fail with an `OverlayError` (unsupported placeholder) or with
manifest drift on write. Two options:

- run emit into memory (refactor `write_checked` so the emit step is reusable)
  and call it a real dry run; or
- keep the cheap check and soften the success line to what it actually
  validated, e.g. `check: 'x' validated` / `check: 'x' is valid; build to
  verify the generated project`.

I'd take the second for now and file the first — a "check says ok, build says
no" pair is the worst outcome for trust, but emitting into memory is a bigger
change than this PR should carry.

Two smaller parity gaps: `CheckArgs` flattens `AnswerArgs` but not
`--runtime-path`, so `espforge check --runtime-path ./rt` is an unknown flag
while `gen`/`build` accept it. And `ESPFORGE_RUNTIME_PATH` is only validated by
`gen`/`build`. Either add the flag to `check` for symmetry or say in the help
that `check` does not resolve the runtime.

### 5. `new` should scaffold the whole example, not "app + overlays + spec"

`scaffold_example` hand-assembles: spec text, `src/app.rs` from
`app_logic(source)`, then `overlay_files(source, true)`. Three problems:

- **`true` is hardcoded.** `keep-zig-source` is a resolved *option*; `new`
  doesn't resolve options, so it guesses. Today no example ships a `chip/`
  tree, so it's a no-op — but it will silently copy Zig sources that `gen` then
  refuses to carry, or vice versa. A scaffold has no business making that call.
- **`overlay_files` is a filter, not a materializer.** It only knows the four
  overlay names plus `chip/`; it drops everything else. Anything added to an
  example later (`.cargo/config.toml`, assets) is invisible to `new` until
  someone edits `OVERLAY_FILES`.
- **It contradicts `espforge-examples`' own roadmap.** That crate's module doc
  says writing an example to disk (`materialize`) and listing (`list`) "arrive
  with the commands that need them (phase 6)". This is phase 6. Adding `list()`
  and `materialize()` there — returning `(rel_path, bytes)` for *every* embedded
  file except `example.toml`, with `example.espf.txt` renamed to
  `<short>.espf` — makes `new` about five lines and removes the option guess.

That also answers the `ExampleInfo` question: if the catalog owns `list()`,
`espforge-core` doesn't need a rendering-shaped struct (`category`, `title` are
display data), and core keeps its "nothing here prints" contract. If you do
keep `ExampleInfo` in core, update the `espforge-examples` doc comment so the
two crates don't document different plans.

### 6. Scaffolded spec content is a product decision, not an accident

The `.espf` written by `new` is the embedded example verbatim, which means the
user's `blink.espf` starts with:

```text
====================
HELLO WORLD EXAMPLE
====================
```

and ends with ~40 lines of the "ESPF FILE FORMAT" tutorial. It parses fine
(comment blocks are handled in `espf/src/parser.rs`), and keeping the banner is
arguably nice — `examples` titles come from it, and the file stays
self-describing. But the trailing tutorial in someone's project file is noise.
Decide it explicitly: I'd keep the banner and strip the trailing format block,
or keep everything and say so in `cli-help.md`. Right now the plan does neither
on purpose.

### 7. One noun for the thing

`examples` footer says `espforge new <name> <dir>`, `cli-help.md` §3 says
`<template>`, `NewArgs` calls it `example`, and `GenArgs` documents `--example`
as `CATEGORY/NAME` while accepting bare names. Pick one word ("example" matches
the crate, the command and the flag) and use it in clap help, the footer, the
cheat sheet and the docs. While you're there, update `GenArgs`' help for
`--example` and the `after_help` in `main.rs`, which still advertises
`--example 01.Basics/helloworld` as if short names didn't exist.

Also worth considering: `espforge new` with no arguments gives a clap error.
Since `new` is the one command with a mandatory registry lookup, having bare
`espforge new` list the available examples (or accepting `--list`) is cheaper
than making the user run `examples` first and copy a name.

### 8. `gen`'s trailing hint is now wrong

`execute` ends with `next: cd build && cargo build`. With `build` existing, that
hint should be `next: espforge build`. Note that `build` reuses
`generate::execute`, so it will print both the `gen:` summary and the
`next: ...` line before cargo runs. Either let `execute` return `Written` and
print in the command layer, or give `build` a quiet path. Printing is the one
thing currently blocking reuse, and this is the moment it bites.

## Tests that will need to change

- `espforge/src/main.rs::the_only_subcommand_is_gen` — asserts exactly `["gen"]`.
- `espforge/src/commands/generate.rs::source_must_be_exactly_one_of_file_or_example` —
  asserts `(None, None)` is an error; discovery makes it a success when a `.espf`
  is present in CWD.
- `espforge-core/src/source.rs::file_and_example_together_or_neither_are_rejected` —
  same `(None, None)` assumption.

New tests worth the lines, none expensive:

1. every short example name resolves to exactly one key (guards `new blink`
   against a future `02.Digital/blink`);
2. `resolve_example_name` ambiguity and "did you mean" paths;
3. `new` into a non-empty dir is a user error, `new` into a fresh dir produces
   the `.espf`, `src/app.rs`, and the diagram, and `gen` on that directory then
   succeeds — that last assertion is the one that proves the whole flow works,
   and it fails today if the scaffold and the discovery disagree about where
   the app logic lives;
4. `check` on the scaffolded project exits 0 (and on a broken spec, 1).

## Suggested order

1. Move `user`/`failed`/`exit_code` into a shared command module; add typed
   scaffold errors.
2. `espforge-examples`: `list()` + `materialize()`; `new` and `examples` become
   thin.
3. Discovery with an explicit base directory; update the three affected tests.
4. `check` (with the softened success line), then `build` on `Result<(), Failure>`.
5. Fix `gen`'s `next:` hint and the vocabulary pass over all help text.

Nothing here doubts the shape — it's the right five verbs in the right order.
The work is keeping the new commands from inheriting `gen`'s internals and
keeping "user error" vs "our error" honest as the surface grows.
