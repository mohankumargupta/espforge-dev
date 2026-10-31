# cli-simplified.md

A minimal, logic-only implementation guide tailored to the exact structure of `espforge`, `espforge-core`, and `espforge-examples`.

---

## 1. `espforge/src/commands/generate.rs`

### 1.1 Fix CLI output text
In `execute`:
```rust
// Change:
println!("build: wrote '{}' to {}", written.crate_name, args.out.display());

// To:
println!("gen: wrote '{}' to {}", written.crate_name, args.out.display());
```

### 1.2 Export error helpers and add `exit_code`
Change visibility of `user` and `failed` from private to `pub(super)`:
```rust
pub(super) fn user(msg: impl std::fmt::Display) -> Failure {
    eprintln!("❌ {msg}");
    Failure::User
}

pub(super) fn failed(error: ProjectError, source: &Source) -> Failure {
    match error {
        ProjectError::Spec(diagnostics) => {
            report::diagnostics(&source.name, &source.text, &diagnostics);
            Failure::User
        }
        ProjectError::User(message) => user(message),
        ProjectError::Internal(e) => Failure::Internal(format!("{e:#}")),
    }
}
```

Add `exit_code` and use it in `run`:
```rust
pub(super) fn exit_code(result: Result<(), Failure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::User) => ExitCode::from(1),
        Err(Failure::Internal(message)) => {
            eprintln!("❌ {message}");
            ExitCode::from(2)
        }
    }
}

pub fn run(args: GenArgs) -> ExitCode {
    exit_code(execute(&args))
}
```

---

## 2. `espforge-core/src/source.rs`

### 2.1 Add example listing and resolution
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleInfo {
    pub key: String,
    pub category: String,
    pub name: String,
    pub title: Option<String>,
}

pub fn example_list() -> Vec<ExampleInfo> {
    espforge_examples::categories()
        .iter()
        .flat_map(|category| {
            category.examples.iter().map(move |example| ExampleInfo {
                key: format!("{}/{}", category.name, example.name),
                category: category.name.clone(),
                name: example.name.clone(),
                title: example.title.clone(),
            })
        })
        .collect()
}

pub fn example_keys() -> Vec<String> {
    example_list().into_iter().map(|e| e.key).collect()
}

pub fn resolve_example_name(input: &str) -> Result<String, String> {
    let keys = example_keys();

    if keys.iter().any(|k| k == input) {
        return Ok(input.to_string());
    }

    let short = |k: &str| k.rsplit('/').next().unwrap_or(k).to_string();
    let matches: Vec<&String> = keys.iter().filter(|k| short(k) == input).collect();

    match matches.len() {
        1 => Ok(matches[0].clone()),
        0 => {
            let names: Vec<String> = keys.iter().map(|k| short(k)).collect();
            let hint = closest(input, &names)
                .map(|n| format!(" Did you mean '{n}'?"))
                .unwrap_or_default();
            Err(format!(
                "no example named '{input}'.{hint}\n   Run `espforge examples` to see all examples."
            ))
        }
        _ => {
            let list = matches
                .iter()
                .map(|k| format!("  - {k}"))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "'{input}' matches more than one example:\n{list}\n   Use the full name, e.g. `--example {}`.",
                matches[0]
            ))
        }
    }
}

fn closest(input: &str, candidates: &[String]) -> Option<String> {
    let limit = 2.max(input.len() / 3);
    candidates
        .iter()
        .map(|c| (edit_distance(input, c), c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c.clone())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}
```

### 2.2 Add directory discovery
```rust
pub fn discover_in_dir(dir: &Path) -> Result<Source, String> {
    if !dir.is_dir() {
        return Err(format!("'{}' is not a directory", dir.display()));
    }

    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read '{}': {e}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".espf") || n.ends_with(".espf.txt"))
        })
        .collect();
    found.sort();

    match found.len() {
        1 => from_file(&found[0]),
        0 => Err(format!(
            "no .espf or .espf.txt file found in '{}'\n   Give a FILE or --example NAME, or run `espforge new <example> <dir>` to start a project.",
            dir.display()
        )),
        _ => {
            let list = found
                .iter()
                .map(|p| format!("  - {}", p.file_name().unwrap().to_string_lossy()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "multiple ESPF files found in '{}':\n{list}\n   Name the one you want, e.g. `espforge gen {}`.",
                dir.display(),
                found[0].display()
            ))
        }
    }
}
```

### 2.3 Update `resolve_source`
Replace `resolve_source` with:
```rust
pub fn resolve_source(file: Option<&Path>, example: Option<&str>) -> Result<Source, String> {
    match (file, example) {
        (Some(_), Some(_)) => Err("give either FILE or --example, not both".to_string()),
        (Some(p), None) if p.is_dir() => discover_in_dir(p),
        (Some(p), None) => from_file(p),
        (None, None) => discover_in_dir(Path::new(".")),
        (None, Some(name)) => {
            let key = resolve_example_name(name)?;
            let ex = espforge_examples::find(&key)
                .ok_or_else(|| format!("no example '{key}'"))?;
            Ok(Source {
                name: format!("embedded:{key}"),
                text: ex.content.clone(),
                dir: None,
                example: Some(ex),
            })
        }
    }
}
```

---

## 3. `espforge-core/src/scaffold.rs` (New File)

Create `espforge-core/src/scaffold.rs`:
```rust
use std::path::Path;
use crate::source::{app_logic, overlay_files, resolve_example_name, resolve_source};

#[derive(Debug)]
pub struct Scaffolded {
    pub espf_name: String,
    pub files: Vec<String>,
}

pub fn scaffold_example(name: &str, dir: &Path) -> Result<Scaffolded, String> {
    let key = resolve_example_name(name)?;
    let source = resolve_source(None, Some(&key))?;

    if std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!(
            "'{}' already exists and is not empty; pick a new directory name",
            dir.display()
        ));
    }

    let short = key.rsplit('/').next().unwrap_or(&key);
    let espf_name = format!("{short}.espf");
    let mut files: Vec<(String, Vec<u8>)> = vec![
        (espf_name.clone(), source.text.clone().into_bytes()),
        ("src/app.rs".to_string(), app_logic(&source)?),
    ];
    files.extend(overlay_files(&source, true)?);

    for (rel, bytes) in &files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create '{}': {e}", parent.display()))?;
        }
        std::fs::write(&path, bytes)
            .map_err(|e| format!("cannot write '{}': {e}", path.display()))?;
    }

    Ok(Scaffolded {
        espf_name,
        files: files.into_iter().map(|(rel, _)| rel).collect(),
    })
}
```

In `espforge-core/src/lib.rs`, declare the module:
```rust
pub mod scaffold;
```

---

## 4. `espforge-core/src/project.rs`

Add `check` to `espforge-core/src/project.rs`:
```rust
pub fn check(source: &Source, overrides: &Overrides) -> Result<Vec<String>, ProjectError> {
    let checked = pipeline::validate(&source.text, overrides)?;
    crate::source::app_logic(source).map_err(ProjectError::User)?;
    crate::source::overlay_files(source, checked.resolved.is_on("keep-zig-source"))
        .map_err(ProjectError::User)?;
    Ok(checked.notes)
}
```

---

## 5. `espforge/src/commands/examples.rs` (New File)

Create `espforge/src/commands/examples.rs`:
```rust
use std::process::ExitCode;
use espforge_core::source::{ExampleInfo, example_list};

pub fn run() -> ExitCode {
    print!("{}", render(&example_list()));
    ExitCode::SUCCESS
}

pub fn render(list: &[ExampleInfo]) -> String {
    let width = list.iter().map(|e| e.name.len()).max().unwrap_or(0);
    let mut out = String::new();
    let mut current = "";
    for example in list {
        if example.category != current {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&example.category);
            out.push('\n');
            current = &example.category;
        }
        let line = format!(
            "  {:<width$}  {}",
            example.name,
            example.title.as_deref().unwrap_or("")
        );
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.push_str("\nCreate a project:  espforge new <name> <dir>\n");
    out
}
```

---

## 6. `espforge/src/commands/new.rs` (New File)

Create `espforge/src/commands/new.rs`:
```rust
use std::path::PathBuf;
use std::process::ExitCode;
use clap::Args;
use espforge_core::scaffold::scaffold_example;

#[derive(Args, Debug, Clone)]
pub struct NewArgs {
    pub example: String,
    pub dir: PathBuf,
}

pub fn run(args: NewArgs) -> ExitCode {
    match scaffold_example(&args.example, &args.dir) {
        Ok(done) => {
            println!("new: created '{}' in {}", done.espf_name, args.dir.display());
            println!("\nNext steps:\n  cd {}\n  espforge build", args.dir.display());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("❌ {message}");
            ExitCode::from(1)
        }
    }
}
```

---

## 7. `espforge/src/commands/check.rs` (New File)

Create `espforge/src/commands/check.rs`:
```rust
use std::path::PathBuf;
use std::process::ExitCode;
use clap::Args;
use espforge_core::project;
use espforge_core::source::resolve_source;

use super::answers::AnswerArgs;
use super::generate::{Failure, exit_code, failed, user};

#[derive(Args, Debug, Clone, Default)]
pub struct CheckArgs {
    pub file: Option<PathBuf>,
    #[arg(long, value_name = "NAME", conflicts_with = "file")]
    pub example: Option<String>,
    #[command(flatten)]
    pub answers: AnswerArgs,
}

pub fn run(args: CheckArgs) -> ExitCode {
    exit_code(execute(&args))
}

pub fn execute(args: &CheckArgs) -> Result<(), Failure> {
    let source = resolve_source(args.file.as_deref(), args.example.as_deref()).map_err(user)?;
    let notes = project::check(&source, &args.answers.overrides())
        .map_err(|e| failed(e, &source))?;
    for note in &notes {
        eprintln!("{note}");
    }
    println!("check: '{}' is valid", source.name);
    Ok(())
}
```

---

## 8. `espforge/src/commands/build.rs` (New File)

Create `espforge/src/commands/build.rs`:
```rust
use std::process::{Command, ExitCode};
use clap::Args;
use super::generate::{self, Failure, GenArgs};

#[derive(Args, Debug, Clone)]
pub struct BuildArgs {
    #[command(flatten)]
    pub gen: GenArgs,
    #[arg(long)]
    pub release: bool,
}

pub fn run(args: BuildArgs) -> ExitCode {
    match execute(&args) {
        Ok(code) => code,
        Err(Failure::User) => ExitCode::from(1),
        Err(Failure::Internal(message)) => {
            eprintln!("❌ {message}");
            ExitCode::from(2)
        }
    }
}

fn execute(args: &BuildArgs) -> Result<ExitCode, Failure> {
    generate::execute(&args.gen)?;

    let mut cmd = Command::new("cargo");
    cmd.arg("build").current_dir(&args.gen.out);
    if args.release {
        cmd.arg("--release");
    }

    let status = cmd.status().map_err(|e| {
        Failure::Internal(format!(
            "could not run `cargo` ({e}). Is Rust installed and on your PATH?"
        ))
    })?;

    if status.success() {
        println!("build: finished");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("❌ cargo build failed (see above)");
        Ok(ExitCode::from(1))
    }
}
```

---

## 9. Register Commands

### 9.1 `espforge/src/commands/mod.rs`
Update to export the new modules:
```rust
pub mod answers;
pub mod build;
pub mod check;
pub mod examples;
pub mod generate;
pub mod new;
pub mod report;
```

### 9.2 `espforge/src/main.rs`
In `Commands` enum:
```rust
#[derive(Subcommand)]
enum Commands {
    #[command(
        name = "gen",
        after_help = "Examples:\n  espforge gen\n  espforge gen hello.espf --chip esp32s3 --option alloc=false\n  espforge gen --example helloworld --out /tmp/hw"
    )]
    Gen(commands::generate::GenArgs),

    #[command(name = "examples")]
    Examples,

    #[command(name = "new")]
    New(commands::new::NewArgs),

    #[command(name = "check")]
    Check(commands::check::CheckArgs),

    #[command(name = "build")]
    Build(commands::build::BuildArgs),
}
```

In `main()` dispatch:
```rust
fn main() -> ExitCode {
    match Cli::parse().command {
        Some(Commands::Gen(args)) => commands::generate::run(args),
        Some(Commands::Examples) => commands::examples::run(),
        Some(Commands::New(args)) => commands::new::run(args),
        Some(Commands::Check(args)) => commands::check::run(args),
        Some(Commands::Build(args)) => commands::build::run(args),
        None => {
            Cli::command()
                .print_help()
                .expect("rendering help should not fail");
            ExitCode::SUCCESS
        }
    }
}
```

