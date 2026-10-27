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
        inline,
    } = case;
    let scratch = tempfile::tempdir().expect("create scratch dir");
    let out = scratch.path().join("out");
    let mut source = resolve_source(None, Some(example)).expect("embedded example");
    if let Some(text) = inline {
        source.name = format!("inline:{example}:{chip}");
        source.text = (*text).to_string();
    }
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
    /// A spec to compile instead of the example's own text. The example still
    /// supplies the app logic, so only the hardware changes.
    inline: Option<&'static str>,
}

/// The i2c example with its bus moved to the second instance: esp-hal has
/// `I2C1` on `esp32s3` (not on `esp32c3`), so this is the proof that the
/// runtime bus is not hardcoded to `I2C0`.
const I2C1_ON_S3: &str = "# espforge\n\
    \x20 <project name=\"i2c_example\" />\n\
    \x20 <chip type=\"esp32s3\" />\n\
    \x20 <runtime type=\"blocking\" />\n\
    # peripherals\n\
    \x20 <i2c id=\"i2c1\" sda=\"6\" scl=\"7\" frequency=\"100kHz\" />\n\
    # components\n\
    \x20 <i2c_component id=\"i2c_tester\" i2c=\"$i2c1\" />\n\
    # devices\n\
    # services\n";

/// The cells this test compiles. `esp32c3` is the runnable target here; the
/// one `esp32s3` case exists because the C3 has no `I2C1` and is skipped by
/// the toolchain, not by the emit.
const CASES: &[Case] = &[
    Case {
        example: "01.Basics/helloworld",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
        inline: None,
    },
    Case {
        example: "01.Basics/blink",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
        inline: None,
    },
    Case {
        example: "02.Digital/button",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
        inline: None,
    },
    Case {
        example: "02.Digital/button_async",
        chip: "esp32c3",
        runtime: Runtime::Embassy,
        inline: None,
    },
    Case {
        example: "04.Communication/i2c_example",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
        inline: None,
    },
    Case {
        example: "04.Communication/i2c_example_async",
        chip: "esp32c3",
        runtime: Runtime::Embassy,
        inline: None,
    },
    Case {
        example: "04.Communication/spi_example",
        chip: "esp32c3",
        runtime: Runtime::Blocking,
        inline: None,
    },
    Case {
        example: "04.Communication/spi_example_async",
        chip: "esp32c3",
        runtime: Runtime::Embassy,
        inline: None,
    },
    Case {
        example: "04.Communication/i2c_example",
        chip: "esp32s3",
        runtime: Runtime::Blocking,
        inline: Some(I2C1_ON_S3),
    },
];

#[test]
#[ignore = "spawns cargo; needs the example's target installed"]
fn examples_with_buses_compile() {
    let root = workspace_root();
    let runtime = root.join("espforge-runtime");
    // One shared target dir, so repeat runs only rebuild what changed.
    let target_dir = root.join("target").join("generated-check");

    for case in CASES {
        check_example(case, &runtime, &target_dir);
    }
}
