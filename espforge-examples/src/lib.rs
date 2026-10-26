//! Embedded ESPF examples for the `espforge` binary: the read half.
//!
//! The example tree is embedded at compile time and addressed as
//! `<category>/<name>` through [`find`]. Writing an example to disk
//! (`materialize`) and listing (`list`) arrive with the commands that need
//! them (phase 6); until then nothing here can leak a test-only `example.toml`
//! into a user's project.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::LazyLock;

use include_dir::{Dir, include_dir};

/// The example tree, embedded at compile time from `src/examples`.
pub static EXAMPLES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/examples");

/// Name of the ESPF source file inside each example directory.
const EXAMPLE_FILE: &str = "example.espf.txt";

/// A file inside an example, with its path relative to the example directory.
pub struct ExampleFile {
    /// Relative path, e.g. `app/rust/app.rs` or `example.espf.txt`.
    pub path: String,
    /// Verbatim embedded contents (examples may contain binaries, e.g. wasm).
    pub contents: &'static [u8],
}

/// A top-level example category, e.g. `01.Basics`.
pub struct Category {
    /// Category directory name, e.g. `01.Basics`.
    pub name: String,
    /// Examples directly under this category, sorted by name.
    pub examples: Vec<Example>,
}

/// A single example (a directory containing `example.espf.txt`).
pub struct Example {
    /// Example directory name, e.g. `helloworld`.
    pub name: String,
    /// Human-facing title taken from the banner at the top of the example
    /// file, when one is present (e.g. `HELLO WORLD EXAMPLE`).
    pub title: Option<String>,
    /// Full contents of the example's `example.espf.txt`.
    pub content: String,
    /// Every embedded file of the example, including `example.espf.txt` itself.
    pub files: Vec<ExampleFile>,
}

static CATALOG: LazyLock<Vec<Category>> = LazyLock::new(|| {
    let mut categories: Vec<Category> = EXAMPLES_DIR.dirs().filter_map(category_from_dir).collect();
    categories.sort_by(|a, b| a.name.cmp(&b.name));
    for category in &mut categories {
        category.examples.sort_by(|a, b| a.name.cmp(&b.name));
    }
    categories
});

/// All categories that contain at least one example, sorted by name.
pub fn categories() -> &'static [Category] {
    &CATALOG
}

/// Find an example by `category/name`, e.g. `01.Basics/helloworld`.
pub fn find(example_path: &str) -> Option<&'static Example> {
    let (category_name, example_name) = example_path.split_once('/')?;
    let category = categories().iter().find(|c| c.name == category_name)?;
    category.examples.iter().find(|e| e.name == example_name)
}

fn category_from_dir(cat_dir: &'static Dir<'static>) -> Option<Category> {
    let examples: Vec<Example> = cat_dir.dirs().filter_map(example_from_dir).collect();
    (!examples.is_empty()).then_some(Category {
        name: path_segment(cat_dir.path()),
        examples,
    })
}

fn example_from_dir(ex_dir: &'static Dir<'static>) -> Option<Example> {
    let espf = ex_dir
        .files()
        .find(|f| f.path().file_name() == Some(OsStr::new(EXAMPLE_FILE)))?;
    let content = espf.contents_utf8()?.to_string();
    let title = extract_title(&content);
    let mut files = Vec::new();
    collect_files(ex_dir, String::new(), &mut files);
    Some(Example {
        name: path_segment(ex_dir.path()),
        title,
        content,
        files,
    })
}

/// Recursively collect an example's files with paths relative to the example
/// directory, composing names level by level (independent of `include_dir`'s
/// path-prefix choices).
fn collect_files(dir: &Dir<'static>, prefix: String, out: &mut Vec<ExampleFile>) {
    for file in dir.files() {
        let name = path_segment(file.path());
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        out.push(ExampleFile {
            path: rel,
            contents: file.contents(),
        });
    }
    for sub in dir.dirs() {
        let name = path_segment(sub.path());
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        collect_files(sub, rel, out);
    }
}

/// Last segment of an embedded path as a String ("" when it has none).
fn path_segment(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_string()
}

/// Pull the human title out of the example ADF: the text between the opening
/// `====` banner and the closing one, trimmed. Returns nothing when the
/// top-of-file layout is different.
fn extract_title(content: &str) -> Option<String> {
    let mut lines = content.lines().skip_while(|l| l.trim().is_empty());
    match lines.next()? {
        // File opens with a `====` banner: the next non-banner line is the title.
        first if is_banner(first) => lines
            .find(|l| !is_banner(l) && !l.trim().is_empty())
            .map(|l| l.trim().to_string()),
        first => Some(first.trim().to_string()),
    }
}

fn is_banner(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| c == '=')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_embed_and_parse() {
        let categories = categories();
        assert!(!categories.is_empty());
        let basics = &categories[0];
        assert_eq!(basics.name, "01.Basics");
        let helloworld = basics
            .examples
            .iter()
            .find(|e| e.name == "helloworld")
            .expect("01.Basics/helloworld must be embedded");
        assert_eq!(helloworld.title.as_deref(), Some("HELLO WORLD EXAMPLE"));
        assert!(helloworld.content.contains("# espforge"));
    }

    #[test]
    fn find_addresses_examples() {
        let hello = find("01.Basics/helloworld").expect("01.Basics/helloworld must exist");
        assert_eq!(hello.title.as_deref(), Some("HELLO WORLD EXAMPLE"));
        assert!(find("01.Basics/no-such-example").is_none());
        assert!(find("no-such-category/helloworld").is_none());
        assert!(find("no-slash").is_none());
    }

    #[test]
    fn example_records_every_file() {
        let hello = find("01.Basics/helloworld").expect("helloworld must exist");
        let paths: Vec<&str> = hello.files.iter().map(|f| f.path.as_str()).collect();
        for expected in ["example.espf.txt", "app/rust/app.rs", "example.toml"] {
            assert!(paths.contains(&expected), "missing {expected} in {paths:?}");
        }
        let espf = hello
            .files
            .iter()
            .find(|f| f.path == "example.espf.txt")
            .expect("espf must be among the files");
        let text = std::str::from_utf8(espf.contents).expect("espf must be utf8");
        assert_eq!(text, hello.content);
    }
}
