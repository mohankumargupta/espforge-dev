//! The element side of the pipeline: the core section reader, the option
//! resolver and the board IR the emitters consume.
//!
//! In this slice there are no real elements yet. `core` reads
//! `project`/`chip`/`runtime`/`console`/`options`, `options` resolves the
//! schema's option table, and [`model::Board`] carries the result with an
//! uninhabited [`model::Component`], so "no components" is a type fact.

pub mod chips;
pub mod core;
pub mod model;
pub mod options;

pub use core::core_from_section;
pub use espforge_schema::{Compat, Kind, OptValue, OptionDef, Requires, Schema};
pub use model::{Board, Component, Console, Ident, Meta, Runtime};
pub use options::{
    Answer, Answers, DerivedInputs, Origin, Resolved, ResolvedOptions, Why, resolve, resolve_with,
};
