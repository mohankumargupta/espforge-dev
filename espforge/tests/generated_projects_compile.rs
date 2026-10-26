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
use espforge_elements::Runtime;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("espforge/ lives inside the workspace")
        .to_path_buf()
}

/// Generate one cell of an example into a scratch dir and `cargo check` it,
/// with the runtime from this checkout and the shared target dir.
fn check_example(case: &Case, runtime: &Path, target_dir: &Path) {
    let Case {
        example,
        chip,
        runtime: async_model,
    } = case;
    let scratch = tempfile::tempdir().expect("create scratch dir");
    let out = scratch.path().join("out");
    let source = resolve_source(None, Some(example)).expect("embedded example");
    let overrides = Overrides {
        chip: Some(chip.to_string()),
        runtime: Some(*async_model),
        ..Default::default()
    };
    generate_with(&source, &out, &overrides, Some(runtime.to_path_buf()))
        .unwrap_or_else(|e| panic!("generate {example} ({chip}): {e:?}"));

    let output = Command::new("cargo")
        .args(["check", "--quiet"])
        .current_dir(&out)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .expect("run cargo check");
    assert!(
        output.status.success(),
        "generated {example} ({chip}) does not compile:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One generated project to compile: an example, a chip and a runtime.
struct Case {
    example: &'static str,
    chip: &'static str,
    runtime: Runtime,
}

/// The cells this test compiles. `esp32s3` is deliberately absent: it needs
/// the Xtensa toolchain, not this slice.
const CASES: &[Case] = &[
    Case {
        example: "01.Basics/helloworld",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
    },
    Case {
        example: "01.Basics/blink",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
    },
    Case {
        example: "02.Digital/button",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
    },
    Case {
        example: "02.Digital/button_async",
        chip: "esp32c3",
        runtime: Runtime::Embassy,
    },
];

#[test]
#[ignore = "spawns cargo; needs the example's target installed"]
fn helloworld_blink_and_button_compile() {
    let root = workspace_root();
    let runtime = root.join("espforge-runtime");
    // One shared target dir, so repeat runs only rebuild what changed.
    let target_dir = root.join("target").join("generated-check");

    for case in CASES {
        check_example(case, &runtime, &target_dir);
    }
}
