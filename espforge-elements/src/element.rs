//! The element framework: what an element implements, and the contexts it gets.
//!
//! An [`Element`] owns one schema tag. It reads that tag's generated
//! [`Attrs`](crate::generated), reports problems through [`LowerCx`], and
//! pushes a [`Component`] whose [`Lowered`] implementation emits its own Rust
//! through [`EmitCx`]. Nothing here knows the name of a single element.
//!
//! Diagnostics are collected, not returned at the first one, like the core
//! reader: [`LowerCx::report`] pushes and returns the [`Reported`] marker an
//! element uses to stop its own lowering.

use std::ops::Range;

use espf::context::ParsedNode;
use espforge_diagnostics::{DiagCode, Diagnostic};
use proc_macro2::TokenStream;

use crate::model::{
    Board, ClaimedPin, Component, Conflict, ConflictKind, GpioDecl, Ident, Meta, Owner, Pin,
    PinUse, ResourceGraph,
};

/// One element tag: reads its node and lowers it onto the board.
pub trait Element: Sync {
    /// The schema tag this element implements (`gpio`). It must name an
    /// element in `espf.schema.kdl`.
    fn tag(&self) -> &'static str;

    /// Read `node` and record what it lowers to in `cx`. A failure has
    /// already been pushed into `cx` and is reported through [`Reported`].
    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported>;
}

/// A lowered component: it can name itself, emit its Rust, and answer the
/// overlay's `{{ <id>.<attr> }}` tokens.
pub trait Lowered {
    /// The component's id.
    fn id(&self) -> &Ident;

    /// Add this component's `Components` field, initialiser and `SAFETY`
    /// note to `cx`.
    fn emit(&self, board: &Board, cx: &mut EmitCx);

    /// The value of `{{ <id>.<attr> }}` in an example overlay, if this
    /// component can answer `attr`.
    fn overlay_attr(&self, _attr: &str) -> Option<String> {
        None
    }
}

/// Proof that an element pushed a diagnostic into its [`LowerCx`].
///
/// Zero-sized on purpose: there is no way to fail lowering without saying
/// why, so a caller never sees a silent `Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reported;

/// The lowering context: everything an element reads and everything it records.
pub struct LowerCx {
    errors: Vec<Diagnostic>,
    gpios: Vec<GpioDecl>,
    resources: ResourceGraph,
    components: Vec<Component>,
}

impl Default for LowerCx {
    fn default() -> Self {
        Self::new()
    }
}

impl LowerCx {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            gpios: Vec::new(),
            resources: ResourceGraph::default(),
            components: Vec::new(),
        }
    }

    /// Record a located problem and return the marker an element uses to stop.
    pub fn report(
        &mut self,
        code: DiagCode,
        span: Range<usize>,
        message: impl Into<String>,
        label: impl Into<String>,
    ) -> Reported {
        self.errors
            .push(Diagnostic::error(code, span, message, label));
        Reported
    }

    /// The `<gpio>` a component refers to by id.
    pub fn gpio(&self, name: &str) -> Option<&GpioDecl> {
        self.gpios.iter().find(|gpio| gpio.id.raw == name)
    }

    /// Record a lowered `<gpio>`.
    pub fn add_gpio(&mut self, decl: GpioDecl) {
        self.gpios.push(decl);
    }

    /// Claim `pin` for `owner`. A failure carries both sides of the conflict
    /// so the element can keep its own wording.
    pub fn claim_pin(
        &mut self,
        pin: Pin,
        owner: Owner,
        span: Range<usize>,
        need: PinUse,
    ) -> Result<ClaimedPin, Conflict> {
        self.resources.claim(pin, owner, span, need)
    }

    /// Record a lowered component.
    pub fn push(&mut self, component: Component) {
        self.components.push(component);
    }

    /// The node's id, with the span of the id attribute's value.
    pub fn ident(&self, node: &ParsedNode<'_>) -> Ident {
        let span = node
            .get_attr_spanned("id")
            .map(|attr| attr.val_offset..attr.val_offset + attr.val.len())
            .unwrap_or(node.offset..node.offset);
        Ident::new(node.id, span)
    }

    /// Finish lowering. Every collected problem is returned together; a board
    /// is only built when there are none.
    pub fn into_board(self, meta: Meta) -> Result<Board, Vec<Diagnostic>> {
        if self.errors.is_empty() {
            Ok(Board {
                meta,
                resources: self.resources,
                components: self.components,
            })
        } else {
            Err(self.errors)
        }
    }
}

/// The emission context: a plain collector with no knowledge of any element.
#[derive(Debug, Default)]
pub struct EmitCx {
    fields: Vec<TokenStream>,
    inits: Vec<TokenStream>,
    safety: Vec<(String, String)>,
}

/// What an [`EmitCx`] collected, in board order.
pub struct EmitParts {
    pub fields: Vec<TokenStream>,
    pub inits: Vec<TokenStream>,
    /// `(anchor, line)` pairs for the emit crate to turn into comment blocks.
    pub safety: Vec<(String, String)>,
}

impl EmitCx {
    pub fn new() -> Self {
        Self::default()
    }

    /// One field of the generated `Components` struct.
    pub fn field(&mut self, field: TokenStream) {
        self.fields.push(field);
    }

    /// One initialiser inside `Components::new`.
    pub fn init(&mut self, init: TokenStream) {
        self.inits.push(init);
    }

    /// A `// SAFETY: ...` line to place before `anchor` in the printed file.
    pub fn safety_comment(&mut self, anchor: impl Into<String>, line: impl Into<String>) {
        self.safety.push((anchor.into(), line.into()));
    }

    pub fn into_parts(self) -> EmitParts {
        EmitParts {
            fields: self.fields,
            inits: self.inits,
            safety: self.safety,
        }
    }
}

/// The legacy wording for a refused claim, kept with the graph so every
/// element reports conflicts the same way.
pub fn conflict_diagnostic(conflict: &Conflict) -> (&'static str, String) {
    let pin = conflict.pin.0;
    let existing = display(&conflict.existing.owner, true);
    let incoming = display(&conflict.incoming.owner, false);
    match conflict.kind {
        ConflictKind::Direction => (
            "pin already used",
            format!("{existing} and {incoming}: one pin cannot be both input and output"),
        ),
        ConflictKind::Pin | ConflictKind::SelfClash => (
            "pin already used",
            format!("{existing} and {incoming} both use GPIO{pin}"),
        ),
    }
}

fn display(owner: &Owner, slug: bool) -> String {
    let id = if slug {
        owner.id.rust_name()
    } else {
        owner.id.raw.clone()
    };
    format!("<{} id=\"{id}\">", owner.kind.tag())
}

/// The Rust-identifier rule every component id must satisfy.
pub fn is_rust_ident(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
