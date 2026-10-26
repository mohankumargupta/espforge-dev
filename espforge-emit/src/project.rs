//! The project as a whole: layering an example's files over the generated set
//! ([`layer_overlay`], [`OverlayFile`], [`Rendered`]) and the `README.txt`
//! ([`readme`]).
//!
//! The generated files themselves come from [`crate::codegen`]; this module is
//! what happens after them and before the ownership manifest.

use anyhow::{Context as _, Result};
use espforge_elements::{Board, ResolvedOptions};

use crate::artifact::{Artifact, Asset, Ownership};
use crate::codegen;

/// One file of an example, with its path relative to the example directory.
#[derive(Debug, Clone, Copy)]
pub struct OverlayFile<'a> {
    pub path: &'a str,
    pub contents: &'a [u8],
}

/// Everything a render produces: text files and binary assets.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub artifacts: Vec<Artifact>,
    pub assets: Vec<Asset>,
}

/// Layer an example's files over an already-generated project:
///
/// * `diagram.json` / `wokwi.toml` replace the generated ones, with their
///   `{{ board }}`, `{{ serialMonitor }}` and `{{ <component>.pin }}`
///   placeholders resolved by [`codegen::overlay`]. They only apply when the
///   `wokwi` option emitted the generated versions.
/// * Everything else (`example.espf.txt`, `app/`, a custom Wokwi chip) is the
///   CLI's business and ignored here. Custom Wokwi chips return in phase 3.
pub fn layer_overlay(
    board: &Board,
    opts: &ResolvedOptions,
    rendered: &mut Rendered,
    overlay: &[OverlayFile],
) -> Result<()> {
    if !opts.is_on("wokwi") {
        return Ok(());
    }
    for f in overlay {
        if !matches!(f.path, "diagram.json" | "wokwi.toml") {
            continue;
        }
        if let Some(a) = rendered.artifacts.iter_mut().find(|a| a.path == f.path) {
            let text = std::str::from_utf8(f.contents)
                .with_context(|| format!("example `{}` is not valid UTF-8", f.path))?;
            a.content = codegen::overlay::render(text, f.path, board)?;
        }
    }
    Ok(())
}

/// `README.txt`: says what is machine-owned and what is yours.
pub fn readme(project_name: &str, artifacts: &[Artifact], assets: &[Asset]) -> Artifact {
    let mut owned: Vec<&str> = artifacts
        .iter()
        .filter(|a| a.ownership == Ownership::Owned)
        .map(|a| a.path.as_str())
        .collect();
    let mut seeded: Vec<&str> = artifacts
        .iter()
        .filter(|a| a.ownership == Ownership::SeedOnce)
        .map(|a| a.path.as_str())
        .chain(
            assets
                .iter()
                .filter(|a| a.ownership == Ownership::SeedOnce)
                .map(|a| a.path.as_str()),
        )
        .collect();
    owned.push("README.txt");
    owned.sort_unstable();
    seeded.sort_unstable();
    let list = |v: &[&str]| {
        if v.is_empty() {
            "  (none)".to_string()
        } else {
            v.iter()
                .map(|p| format!("  - {p}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    Artifact::owned(
        "README.txt",
        format!(
            "espforge generated project: {project_name}\n\
==========================================\n\
\n\
SOURCE OF TRUTH\n\
  This directory is GENERATED from an .espf file. Edit the .espf (and your\n\
  src/app.rs next to it), then re-run `espforge build`.\n\
\n\
MACHINE-OWNED (do not edit; regenerated every build):\n\
{}\n\
\n\
SEEDED ONCE (written the first time, then yours; never overwritten):\n\
{}\n\
\n\
REGENERATE:\n\
  espforge build <your.espf> --out <this dir>\n",
            list(&owned),
            list(&seeded)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::test_support::case;
    use crate::runtime_source::RuntimeSource;
    use espforge_elements::Runtime;

    fn components() -> (Board, ResolvedOptions) {
        let case = case("esp32c3", Runtime::Blocking, true, &[], &[]);
        let board = Board {
            meta: case.meta.clone(),
            resources: espforge_elements::ResourceGraph::default(),
            components: Vec::new(),
        };
        (board, case.opts)
    }

    fn generated(board: &Board, opts: &ResolvedOptions, overlay: &[OverlayFile]) -> Rendered {
        let artifacts = codegen::generate(board, &board.meta, opts, &RuntimeSource::Published)
            .unwrap_or_else(|e| panic!("{e:#}"));
        let mut rendered = Rendered {
            artifacts,
            assets: Vec::new(),
        };
        layer_overlay(board, opts, &mut rendered, overlay).unwrap();
        rendered
    }

    fn file<'a>(r: &'a Rendered, path: &str) -> &'a Artifact {
        r.artifacts
            .iter()
            .find(|a| a.path == path)
            .unwrap_or_else(|| panic!("no {path}"))
    }

    fn overlay_files(files: &[(&'static str, &'static [u8])]) -> Vec<OverlayFile<'static>> {
        files
            .iter()
            .map(|(path, contents)| OverlayFile { path, contents })
            .collect()
    }

    const EXAMPLE_DIAGRAM: &str = "{\n  \"version\": 1,\n  \"parts\": [\n    { \"type\": \"{{ board }}\", \"id\": \"esp\", \"attrs\": { \"flashSize\": \"16\" } }\n  ],\n  \"connections\": [ [ \"esp:TX\", \"{{ serialMonitor }}:RX\", \"\", [] ] ]\n}\n";

    #[test]
    fn an_example_diagram_replaces_the_generated_one_with_tokens_resolved() {
        let (board, opts) = components();
        let overlay = overlay_files(&[("diagram.json", EXAMPLE_DIAGRAM.as_bytes())]);
        let rendered = generated(&board, &opts, &overlay);
        let diagram = &file(&rendered, "diagram.json").content;
        let json: serde_json::Value = serde_json::from_str(diagram).unwrap();
        assert_eq!(json["parts"][0]["type"], "board-esp32-c3-devkitm-1");
        assert_eq!(json["connections"][0][1], "$serialMonitor:RX");
        assert!(!diagram.contains("{{"), "{diagram}");
    }

    #[test]
    fn an_unknown_token_fails_the_layer_with_a_typed_error() {
        let (board, opts) = components();
        let overlay = overlay_files(&[("diagram.json", b"{{ nope }}")]);
        let artifacts =
            codegen::generate(&board, &board.meta, &opts, &RuntimeSource::Published).unwrap();
        let mut rendered = Rendered {
            artifacts,
            assets: Vec::new(),
        };
        let err = layer_overlay(&board, &opts, &mut rendered, &overlay).unwrap_err();
        assert!(
            err.downcast_ref::<codegen::overlay::OverlayError>()
                .is_some(),
            "{err:#}"
        );
    }

    #[test]
    fn wokwi_off_drops_the_example_wokwi_files_too() {
        let (board, _) = components();
        let off = case(
            "esp32c3",
            Runtime::Blocking,
            true,
            &[("wokwi", "false")],
            &[],
        );
        let overlay = overlay_files(&[("diagram.json", EXAMPLE_DIAGRAM.as_bytes())]);
        let rendered = generated(&board, &off.opts, &overlay);
        assert!(rendered.artifacts.iter().all(|a| a.path != "diagram.json"));
        assert!(rendered.artifacts.iter().all(|a| a.path != "wokwi.toml"));
        assert!(rendered.assets.is_empty());
    }

    #[test]
    fn non_wokwi_example_files_are_ignored() {
        let (board, opts) = components();
        let overlay = overlay_files(&[
            ("example.espf.txt", b"# espforge\n"),
            ("app/rust/app.rs", b"pub fn setup() {}\n"),
        ]);
        let rendered = generated(&board, &opts, &overlay);
        assert!(
            rendered
                .artifacts
                .iter()
                .all(|a| a.path != "app/rust/app.rs")
        );
        assert!(
            rendered
                .artifacts
                .iter()
                .all(|a| a.path != "example.espf.txt")
        );
    }

    #[test]
    fn readme_lists_owned_and_seeded_files() {
        let (board, opts) = components();
        let rendered = generated(&board, &opts, &[]);
        let readme = readme("hello_world", &rendered.artifacts, &rendered.assets);
        assert!(readme.content.contains("README.txt"));
        assert!(readme.content.contains("  - Cargo.toml"));
        assert!(readme.content.contains("  - rust-toolchain.toml"));
        assert!(
            readme.content.contains("MACHINE-OWNED"),
            "the two lists stay distinct: {}",
            readme.content
        );
    }
}
