//! The front half shared by every command: parse -> structure -> `# espforge`
//! core -> options -> flag overrides -> resolve.
//!
//! One function, so the commands cannot disagree. Nothing here prints: the
//! binary decides how a failure looks.

use espf::diag::junk_diagnostics;
use espf::parser::{Parsed, parse_espf};
use espforge_diagnostics::{Diagnostic, Diagnostics};
use espforge_elements::{
    Answers, DerivedInputs, Meta, ResolvedOptions, core_from_section, resolve,
};
use espforge_schema::schema;

use crate::answers::Overrides;

// ---------------------------------------------------------------- validation

/// Why a pipeline stage stopped.
#[derive(Debug)]
pub enum Invalid {
    /// Located diagnostics (rendered as snippets).
    Diagnostics(Diagnostics),
    /// A command-line answer that has no source location.
    Message(String),
}

impl From<Vec<Diagnostic>> for Invalid {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        Invalid::Diagnostics(diagnostics)
    }
}

impl From<String> for Invalid {
    fn from(m: String) -> Self {
        Invalid::Message(m)
    }
}

impl Invalid {
    /// The diagnostics this failure carries.
    pub fn diagnostics(&self) -> Diagnostics {
        match self {
            Invalid::Diagnostics(diagnostics) => diagnostics.clone(),
            Invalid::Message(_) => Vec::new(),
        }
    }

    /// The one-line `❌` text for a failure that has no snippet.
    pub fn message(&self) -> Option<&str> {
        match self {
            Invalid::Message(m) => Some(m),
            Invalid::Diagnostics(_) => None,
        }
    }
}

/// A fully validated spec: final settings (file + flag overrides), resolved
/// options and the parse result.
pub struct Checked<'a> {
    pub parsed: Parsed<'a>,
    /// Core settings after flag overrides.
    pub meta: Meta,
    /// Option answers after flag overrides.
    pub answers: Answers,
    pub resolved: ResolvedOptions,
    /// `note: ...` lines for flags that changed something.
    pub notes: Vec<String>,
}

/// Parse, check syntax/structure, read `# espforge`, apply overrides and
/// resolve the options.
pub fn validate<'a>(src: &'a str, overrides: &Overrides) -> Result<Checked<'a>, Invalid> {
    let parsed = parse_espf(src);

    // 1. Syntax (junk lines) and structure (duplicate / missing sections).
    let mut errors = junk_diagnostics(&parsed.junk);
    errors.extend(parsed.structure_errors.iter().cloned());
    if !errors.is_empty() {
        errors.sort_by_key(|e| e.primary.start);
        return Err(Invalid::Diagnostics(errors));
    }

    // 2. `# espforge` core and `<options>` (all five sections exist now).
    let header = parsed.header_offsets["espforge"];
    let espforge_nodes = &parsed.sections["espforge"];
    let (mut meta, opt_node) = core_from_section(header, espforge_nodes)?;
    let mut answers = Answers::from_node(schema(), opt_node, header..header)?;
    let derived = DerivedInputs::from_nodes(espforge_nodes, &parsed.sections["services"]);

    // 3. Overrides win over the file; collect a note when they change
    // something.
    let mut notes = Vec::new();
    overrides.apply_core(&mut meta, &mut notes)?;
    overrides.apply_options(schema(), &mut answers, &mut notes)?;

    // 4. Resolve against the (possibly overridden) answers.
    let resolved = resolve(&meta, &answers, &derived)?;

    Ok(Checked {
        parsed,
        meta,
        answers,
        resolved,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> &'static str {
        &espforge_examples::find("01.Basics/helloworld")
            .unwrap()
            .content
    }

    #[test]
    fn the_helloworld_header_validates() {
        let c = validate(hello(), &Overrides::default())
            .ok()
            .expect("hello validates");
        assert!(c.notes.is_empty());
        assert_eq!(c.meta.chip, "esp32c3");
        assert!(c.resolved.is_on("unstable-hal"));
        assert!(c.resolved.is_on("alloc"));
        assert!(c.resolved.is_on("wokwi"));
        assert!(!c.resolved.is_on("embassy"));
    }

    #[test]
    fn flags_override_and_leave_a_note() {
        let overrides = Overrides {
            chip: Some("esp32s3".into()),
            ..Default::default()
        };
        let c = validate(hello(), &overrides).ok().expect("validates");
        assert_eq!(c.meta.chip, "esp32s3");
        assert_eq!(c.notes.len(), 1);
    }

    #[test]
    fn bad_flags_are_messages_not_spans() {
        let overrides = Overrides {
            chip: Some("esp99".into()),
            ..Default::default()
        };
        assert!(matches!(
            validate(hello(), &overrides).err(),
            Some(Invalid::Message(_))
        ));
    }

    #[test]
    fn missing_sections_are_reported_before_anything_else() {
        let source = "# espforge\n  <project name=\"h\" />\n";
        let err = validate(source, &Overrides::default()).err().unwrap();
        let Invalid::Diagnostics(diagnostics) = err else {
            panic!("expected located diagnostics")
        };
        assert!(
            diagnostics
                .iter()
                .any(|e| e.message.contains("missing section `# services`")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn core_and_option_errors_are_located() {
        let source = hello().replace("esp32c3", "esp99");
        let err = validate(&source, &Overrides::default()).err().unwrap();
        let Invalid::Diagnostics(diagnostics) = err else {
            panic!("expected located diagnostics")
        };
        assert_eq!(diagnostics[0].message, "unknown chip");
    }

    #[test]
    fn a_junk_line_is_located() {
        let source = hello().replace(
            "<runtime type=\"blocking\" />",
            "<runtime type=\"blocking\"",
        );
        let err = validate(&source, &Overrides::default()).err().unwrap();
        let Invalid::Diagnostics(diagnostics) = err else {
            panic!("expected located diagnostics")
        };
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message == "malformed node line"),
            "{diagnostics:?}"
        );
    }
}
