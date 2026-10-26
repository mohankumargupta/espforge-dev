//! Check 2 of the slice gate: the project we generate actually compiles.
//!
//! Byte-identical goldens prove the emitter reproduces the legacy tree; this
//! proves the tree is a real crate. It is `#[ignore]`d because it spawns cargo
//! and needs the generated project's target installed:
//!
//! ```text
//! rustup target add riscv32imc-unknown-none-elf
//! cargo test -p espforge --test generated_projects_compile -- --ignored
//! ```
//!
//! The generated `Cargo.toml` points at this checkout's `espforge-runtime`
//! (via `--runtime-path`, here passed straight to `generate_with`), so the
//! runtime is compiled by this test too.

use std::path::{Path, PathBuf};
use std::process::Command;

use espforge_core::answers::Overrides;
use espforge_core::project::generate_with;
use espforge_core::source::resolve_source;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("espforge/ lives inside the workspace")
        .to_path_buf()
}

#[test]
#[ignore = "spawns cargo; needs the example's target installed"]
fn helloworld_compiles() {
    let root = workspace_root();
    let runtime = root.join("espforge-runtime");
    // One shared target dir, so repeat runs only rebuild what changed.
    let target_dir = root.join("target").join("generated-check");

    let scratch = tempfile::tempdir().expect("create scratch dir");
    let out = scratch.path().join("out");
    let source = resolve_source(None, Some("01.Basics/helloworld")).expect("embedded example");
    generate_with(&source, &out, &Overrides::default(), Some(runtime))
        .expect("generate 01.Basics/helloworld");

    let output = Command::new("cargo")
        .args(["check", "--quiet"])
        .current_dir(&out)
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run cargo check");
    assert!(
        output.status.success(),
        "generated helloworld does not compile:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
