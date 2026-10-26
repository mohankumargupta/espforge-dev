//! The board IR: what the `# espforge` section says, and what lowering built.
//!
//! In this slice a project has no peripherals and no components, so [`Board`]
//! carries only its [`Meta`] and an empty component list. `Component` has zero
//! variants on purpose: "this project has no components" is proved by the type
//! rather than assumed, and the first variant arrives with `led`.

use std::ops::Range;

/// Which async model the generated project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Blocking,
    Embassy,
}

/// Where `print!`/log output goes (esp-println's backend).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Console {
    /// Plain UART. Works on every chip.
    #[default]
    Uart,
    /// Built-in USB-Serial-JTAG peripheral (not on the original ESP32).
    JtagSerial,
}

/// Contents of the `# espforge` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub name: String,
    pub description: Option<String>,
    pub chip: String,
    pub runtime: Runtime,
    pub console: Console,
}

impl Meta {
    /// The project name as a valid crate/package identifier
    /// (`"example project"` -> `example_project`).
    pub fn crate_name(&self) -> String {
        let slug: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let slug = slug.trim_matches('_').to_string();
        if slug.is_empty() {
            "espforge_project".to_string()
        } else if slug.starts_with(|c: char| c.is_ascii_digit()) {
            format!("p_{slug}")
        } else {
            slug
        }
    }
}

/// An element id: the text as written, plus where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// The id exactly as written (`red-led`).
    pub raw: String,
    /// Span of the id attribute's value.
    pub span: Range<usize>,
}

impl Ident {
    pub fn new(raw: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            raw: raw.into(),
            span,
        }
    }

    /// The id as written.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The id as a Rust identifier (`red-led` -> `red_led`).
    pub fn rust_name(&self) -> String {
        self.raw.replace('-', "_")
    }
}

/// One lowerable component.
///
/// Zero variants in this slice: `match`ing on it proves no component can reach
/// an emitter yet. `Led` is the first variant, and it carries its pin claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Component {}

/// Everything the emitters need: the project settings and its components.
#[derive(Debug, Clone)]
pub struct Board {
    pub meta: Meta,
    pub components: Vec<Component>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str) -> Meta {
        Meta {
            name: name.into(),
            description: None,
            chip: "esp32c3".into(),
            runtime: Runtime::Blocking,
            console: Console::default(),
        }
    }

    #[test]
    fn crate_name_is_slugged() {
        assert_eq!(meta("example project").crate_name(), "example_project");
        assert_eq!(meta("Hello-World").crate_name(), "hello_world");
        assert_eq!(meta("3d").crate_name(), "p_3d");
        assert_eq!(meta("!!!").crate_name(), "espforge_project");
    }

    #[test]
    fn console_defaults_to_uart() {
        assert_eq!(Console::default(), Console::Uart);
    }

    #[test]
    fn ident_rust_name_replaces_dashes() {
        assert_eq!(Ident::new("red-led", 0..0).rust_name(), "red_led");
        assert_eq!(Ident::new("status", 0..0).as_str(), "status");
    }
}
