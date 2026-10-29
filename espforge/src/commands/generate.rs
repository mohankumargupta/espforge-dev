//! `espforge gen` (`gen` is a keyword, so the module is `generate`): ESPF source -> firmware project, and stop before cargo.
//!
//! Source (FILE, `--example`, or the single `.espf` in the current directory)
//! -> validate -> generate -> write. Everything is [`espforge_core`]'s; this
//! command turns its typed failures into exit codes.
//!
//! Exit codes: 0 success, 1 user errors (diagnostics already printed),
//! 2 internal / IO failure.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use espforge_core::project::{self, ProjectError};
use espforge_core::source::{Source, resolve_source};

use super::answers::AnswerArgs;
use super::report;

#[derive(Args, Debug, Clone, Default)]
pub struct GenArgs {
    /// Path to the ESPF project file (default: the single .espf in the current directory).
    /// App logic: src/app.rs or app/rust/app.rs next to it
    pub file: Option<PathBuf>,
    /// Use an embedded example instead of a file, e.g. 01.Basics/helloworld
    #[arg(long, value_name = "CATEGORY/NAME", conflicts_with = "file")]
    pub example: Option<String>,
    /// Output directory for the generated project
    #[arg(short, long, default_value = "build")]
    pub out: PathBuf,
    /// Use a local espforge-runtime crate instead of the published one
    /// (or set ESPFORGE_RUNTIME_PATH)
    #[arg(long)]
    pub runtime_path: Option<PathBuf>,
    #[command(flatten)]
    pub answers: AnswerArgs,
}

/// Why a generation stopped.
#[derive(Debug)]
pub enum Failure {
    /// The user's input is at fault; the report is already printed.
    User,
    /// Something broke inside espforge or the OS, with its full chain.
    Internal(String),
}

/// Print `❌ msg` and return the user-error marker.
fn user(msg: impl std::fmt::Display) -> Failure {
    eprintln!("❌ {msg}");
    Failure::User
}

/// A core failure with an exit code: located spec problems render as
/// snippets, user problems as the one-line `❌`, ours as exit 2.
fn failed(error: ProjectError, source: &Source) -> Failure {
    match error {
        ProjectError::Spec(diagnostics) => {
            report::diagnostics(&source.name, &source.text, &diagnostics);
            Failure::User
        }
        ProjectError::User(message) => user(message),
        ProjectError::Internal(e) => Failure::Internal(format!("{e:#}")),
    }
}

pub fn run(args: GenArgs) -> ExitCode {
    match execute(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::User) => ExitCode::from(1),
        Err(Failure::Internal(message)) => {
            eprintln!("❌ {message}");
            ExitCode::from(2)
        }
    }
}

pub fn execute(args: &GenArgs) -> Result<(), Failure> {
    let source = resolve_source(args.file.as_deref(), args.example.as_deref()).map_err(user)?;
    let written = project::generate_with(
        &source,
        &args.out,
        &args.answers.overrides(),
        args.runtime_path.clone(),
    )
    .map_err(|e| failed(e, &source))?;

    for note in &written.notes {
        eprintln!("{note}");
    }
    println!(
        "build: wrote '{}' to {}",
        written.crate_name,
        args.out.display()
    );
    println!("  next: cd {} && cargo build", args.out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn example_args(example: &str, out: &Path) -> GenArgs {
        GenArgs {
            example: Some(example.into()),
            out: out.to_path_buf(),
            ..Default::default()
        }
    }

    fn is_user<T: std::fmt::Debug>(r: Result<T, Failure>) -> bool {
        matches!(r, Err(Failure::User))
    }

    #[test]
    fn generates_the_helloworld_example() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        execute(&example_args("01.Basics/helloworld", &out)).unwrap();
        for f in [
            "Cargo.toml",
            "build.rs",
            "src/lib.rs",
            "src/bin/main.rs",
            "src/app.rs",
            "README.txt",
            ".espforge/manifest.json",
            ".espforge/source.espf.txt",
        ] {
            assert!(out.join(f).is_file(), "missing {f}");
        }
        assert!(
            std::fs::read_to_string(out.join("src/app.rs"))
                .unwrap()
                .contains("Hello World!")
        );
    }

    #[test]
    fn rebuilding_is_fine_but_edited_owned_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let args = example_args("01.Basics/helloworld", &out);
        execute(&args).unwrap();
        execute(&args).unwrap();
        std::fs::write(out.join("src/lib.rs"), "// mine").unwrap();
        assert!(is_user(execute(&args)));
    }

    #[test]
    fn flags_override_the_file_and_leave_notes() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let mut args = example_args("01.Basics/helloworld", &out);
        args.answers.chip = Some("esp32s3".into());
        execute(&args).unwrap();
        let cargo = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("\"esp32s3\""), "{cargo}");
    }

    #[test]
    fn bad_flags_are_user_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = example_args("01.Basics/helloworld", &dir.path().join("out"));
        args.answers.options = vec!["nope=1".into()];
        assert!(is_user(execute(&args)));
    }

    #[test]
    fn spec_errors_are_user_errors_with_snippets() {
        let dir = tempfile::tempdir().unwrap();
        let hello = espforge_examples::find("01.Basics/helloworld").unwrap();
        let f = dir.path().join("hello.espf");
        std::fs::write(&f, hello.content.replace("esp32c3", "esp99")).unwrap();
        let args = GenArgs {
            file: Some(f),
            out: dir.path().join("out"),
            ..Default::default()
        };
        assert!(is_user(execute(&args)));
    }

    #[test]
    fn source_must_be_exactly_one_of_file_or_example() {
        let none = GenArgs {
            out: std::env::temp_dir().join("espforge-gen-none"),
            ..Default::default()
        };
        assert!(is_user(execute(&none)));
        let both = GenArgs {
            file: Some("x.espf".into()),
            example: Some("01.Basics/helloworld".into()),
            ..Default::default()
        };
        assert!(is_user(execute(&both)));
        let missing = GenArgs {
            example: Some("no/such".into()),
            ..Default::default()
        };
        assert!(is_user(execute(&missing)));
    }

    #[test]
    fn runtime_path_is_validated_and_used() {
        let dir = tempfile::tempdir().unwrap();
        let rt = dir.path().join("rt");
        std::fs::create_dir_all(&rt).unwrap();
        let out = dir.path().join("out");
        let mut args = example_args("01.Basics/helloworld", &out);
        args.runtime_path = Some(rt.clone());
        execute(&args).unwrap();
        let cargo = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
        assert!(
            cargo.contains(&rt.to_string_lossy().replace('\\', "/")),
            "{cargo}"
        );

        args.runtime_path = Some(dir.path().join("not-a-dir"));
        assert!(is_user(execute(&args)));
    }
}
