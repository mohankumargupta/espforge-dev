//! Fixtures shared by the codegen backends' unit tests.
//!
//! `wifi` and `embassy` are derived flags: they follow `# peripherals` and the
//! runtime, and setting them by hand is a resolver error. So the fixture takes
//! the runtime and the peripheral list, not an option flag for them.

use espforge_elements::{Answers, Console, DerivedInputs, Meta, ResolvedOptions, Runtime, resolve};
use espforge_schema::schema;

use super::chip::Chip;

pub struct Case {
    pub meta: Meta,
    pub opts: ResolvedOptions,
}

/// A resolved project: `hello world` on `chip`, with `flags` applied.
///
/// `defaults` is the `<options defaults=...>` switch; `peripherals` are the
/// tags in `# peripherals` (a `<wifi>` there is the only way to turn `wifi`
/// on).
pub fn case(
    chip: &str,
    runtime: Runtime,
    defaults: bool,
    flags: &[(&str, &str)],
    peripherals: &[&str],
) -> Case {
    let meta = Meta {
        runtime,
        ..meta(chip)
    };
    let mut answers = Answers::empty(0..0);
    answers.defaults = defaults;
    for (name, value) in flags {
        answers
            .apply_flag(schema(), name, value)
            .unwrap_or_else(|e| panic!("option `{name}={value}`: {e:?}"));
    }
    let derived = DerivedInputs {
        peripherals: peripherals
            .iter()
            .map(|name| (name.to_string(), 0..1))
            .collect(),
        console_span: None,
    };
    let opts = resolve(&meta, &answers, &derived)
        .unwrap_or_else(|e| panic!("resolving the fixture: {e:?}"));
    Case { meta, opts }
}

pub fn chip(name: &str) -> Chip {
    Chip::resolve(name).unwrap()
}

/// The project settings every fixture starts from.
pub fn meta(chip: &str) -> Meta {
    Meta {
        name: "hello world".into(),
        description: Some("demo".into()),
        chip: chip.into(),
        runtime: Runtime::Blocking,
        console: Console::Uart,
    }
}
