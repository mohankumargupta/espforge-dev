//! Where an ESPF spec comes from: a file or an embedded example.
//!
//! In this slice there is no current-directory discovery: a caller gives a
//! FILE or `--example`. App logic and overlay files come from the example
//! (embedded) or from next to the file. Custom Wokwi chips (`chip.json` /
//! `chip.wasm`) return in phase 3.

use anyhow::Result;
use espforge_emit::RuntimeSource;
use std::path::{Path, PathBuf};

pub struct Source {
    /// Name used in diagnostics: the path, or `embedded:<category>/<name>`.
    pub name: String,
    pub text: String,
    /// Directory of the `.espf` (None for embedded examples).
    pub dir: Option<PathBuf>,
    pub example: Option<&'static espforge_examples::Example>,
}

pub fn from_file(file: &Path) -> Result<Source, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read '{}': {e}", file.display()))?;
    let dir = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(Source {
        name: file.to_string_lossy().into_owned(),
        text,
        dir: Some(dir),
        example: None,
    })
}

/// FILE or `--example`, never both and never neither.
pub fn resolve_source(file: Option<&Path>, example: Option<&str>) -> Result<Source, String> {
    match (file, example) {
        (Some(_), Some(_)) => Err("give either FILE or --example, not both".to_string()),
        (Some(f), None) => from_file(f),
        (None, None) => Err("give a FILE or --example <category/name>".to_string()),
        (None, Some(path)) => {
            let ex = espforge_examples::find(path)
                .ok_or_else(|| format!("no example '{path}'; it must be <category>/<name>"))?;
            Ok(Source {
                name: format!("embedded:{path}"),
                text: ex.content.clone(),
                dir: None,
                example: Some(ex),
            })
        }
    }
}

/// Files next to a `.espf` that customise the generated Wokwi setup.
const OVERLAY_FILES: [&str; 2] = ["diagram.json", "wokwi.toml"];

/// The example files the render may layer on top.
pub fn overlay_files(src: &Source) -> Result<Vec<(String, Vec<u8>)>, String> {
    if let Some(ex) = src.example {
        return Ok(ex
            .files
            .iter()
            .filter(|f| OVERLAY_FILES.contains(&f.path.as_str()))
            .map(|f| (f.path.clone(), f.contents.to_vec()))
            .collect());
    }
    let dir = src
        .dir
        .as_ref()
        .ok_or_else(|| "this source has no directory".to_string())?;
    let mut out = Vec::new();
    for name in OVERLAY_FILES {
        let p = dir.join(name);
        if p.is_file() {
            out.push((
                name.to_string(),
                std::fs::read(&p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
            ));
        }
    }
    Ok(out)
}

/// App logic lives in `src/app.rs` first, `app/rust/app.rs` second.
pub fn find_app(dir: &Path) -> Option<PathBuf> {
    let preferred = dir.join("src").join("app.rs");
    if preferred.is_file() {
        return Some(preferred);
    }
    let example_layout = dir.join("app").join("rust").join("app.rs");
    example_layout.is_file().then_some(example_layout)
}

/// The user's app-logic bytes, copied verbatim into the generated project.
pub fn app_logic(src: &Source) -> Result<Vec<u8>, String> {
    if let Some(ex) = src.example {
        return ex
            .files
            .iter()
            .find(|f| f.path == "app/rust/app.rs")
            .map(|f| f.contents.to_vec())
            .ok_or_else(|| "this example has no app/rust/app.rs".to_string());
    }
    let dir = src
        .dir
        .as_ref()
        .ok_or_else(|| "this source has no directory to read app logic from".to_string())?;
    let found = find_app(dir).ok_or_else(|| {
        format!(
            "missing {} (or {}): your app logic (setup/forever)",
            dir.join("src").join("app.rs").display(),
            dir.join("app").join("rust").join("app.rs").display()
        )
    })?;
    std::fs::read(&found).map_err(|e| e.to_string())
}

/// Which `espforge-runtime` the generated `Cargo.toml` references: the
/// published crate, or `--runtime-path` / `ESPFORGE_RUNTIME_PATH`.
pub fn runtime_source(runtime_path: Option<PathBuf>) -> Result<RuntimeSource, String> {
    let path =
        runtime_path.or_else(|| std::env::var_os("ESPFORGE_RUNTIME_PATH").map(PathBuf::from));
    match path {
        Some(p) => {
            let abs = std::path::absolute(&p).map_err(|e| e.to_string())?;
            if !abs.is_dir() {
                return Err(format!("runtime path '{}' is not a directory", p.display()));
            }
            Ok(RuntimeSource::Path(abs.to_string_lossy().into_owned()))
        }
        None => Ok(RuntimeSource::Published),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_and_example_together_or_neither_are_rejected() {
        assert!(resolve_source(Some(Path::new("x.espf")), Some("01.Basics/helloworld")).is_err());
        let err = resolve_source(None, None).err().unwrap();
        assert!(err.contains("FILE or --example"), "{err}");
        assert!(resolve_source(None, Some("no/such")).is_err());
    }

    #[test]
    fn an_example_source_carries_its_text_and_files() {
        let src = resolve_source(None, Some("01.Basics/helloworld")).unwrap();
        assert_eq!(src.name, "embedded:01.Basics/helloworld");
        assert!(src.text.contains("# espforge"));
        assert!(src.dir.is_none());
        assert!(src.example.is_some());
        let overlays = overlay_files(&src).unwrap();
        assert!(overlays.is_empty(), "helloworld has no Wokwi overlay file");
        assert!(
            String::from_utf8(app_logic(&src).unwrap())
                .unwrap()
                .contains("Hello World!")
        );
    }

    #[test]
    fn find_app_prefers_src_app_rs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_app(dir.path()).is_none());
        std::fs::create_dir_all(dir.path().join("app/rust")).unwrap();
        std::fs::write(dir.path().join("app/rust/app.rs"), "").unwrap();
        assert!(find_app(dir.path()).unwrap().ends_with("app/rust/app.rs"));
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/app.rs"), "").unwrap();
        assert!(find_app(dir.path()).unwrap().ends_with("src/app.rs"));
    }

    #[test]
    fn a_runtime_path_must_be_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let ok = runtime_source(Some(dir.path().to_path_buf())).unwrap();
        assert!(matches!(ok, RuntimeSource::Path(_)));
        assert!(runtime_source(Some(dir.path().join("nope"))).is_err());
    }
}
