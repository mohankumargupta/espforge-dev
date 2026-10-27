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
    Board, Bus, BusId, ClaimedPin, Component, Conflict, ConflictKind, GpioDecl, Ident, Meta, Owner,
    PeripheralClash, Pin, PinUse, ResourceGraph,
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
    /// `<gpio>` ids named by an `<spi_component cs>` reference: a chip select
    /// that names its own declaration is not a second user of that pin.
    cs_gpios: Vec<String>,
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
            cs_gpios: Vec::new(),
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

    /// Every lowered `<gpio>`, in source order.
    pub fn gpios(&self) -> &[GpioDecl] {
        &self.gpios
    }

    /// Append a bus and return its id.
    pub fn add_bus(&mut self, bus: Bus) -> BusId {
        self.resources.add_bus(bus)
    }

    pub fn bus(&self, id: BusId) -> Option<&Bus> {
        self.resources.bus(id)
    }

    pub fn bus_mut(&mut self, id: BusId) -> Option<&mut Bus> {
        self.resources.bus_mut(id)
    }

    pub fn bus_by_name(&self, name: &str) -> Option<BusId> {
        self.resources.bus_by_name(name)
    }

    pub fn buses(&self) -> &[Bus] {
        self.resources.buses()
    }

    /// Claim a physical peripheral name (`UART1`).
    pub fn claim_peripheral(
        &mut self,
        peripheral: impl Into<String>,
        owner: Owner,
        span: Range<usize>,
    ) -> Result<(), PeripheralClash> {
        self.resources.claim_peripheral(peripheral, owner, span)
    }

    /// Remember that an `<spi_component>` chip select names this `<gpio>`.
    pub fn record_cs_gpio(&mut self, id: String) {
        self.cs_gpios.push(id);
    }

    pub fn cs_gpios(&self) -> &[String] {
        &self.cs_gpios
    }

    /// Every lowered component, in source order.
    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// Whether anything has been reported so far.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
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

    /// Report a refused pin claim in the corpus wording.
    pub fn report_conflict(&mut self, conflict: &Conflict) -> Reported {
        let report = conflict_diagnostic(conflict);
        self.report(report.code, report.span, report.message, report.label)
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
    prelude: Vec<TokenStream>,
    once: std::collections::BTreeSet<String>,
    safety: Vec<(String, Vec<String>)>,
}

/// What an [`EmitCx`] collected, in board order.
pub struct EmitParts {
    pub fields: Vec<TokenStream>,
    pub inits: Vec<TokenStream>,
    /// Statements printed before `Self { .. }`, in first-use order.
    pub prelude: Vec<TokenStream>,
    /// `(anchor, lines)` pairs for the emit crate to turn into comment blocks.
    pub safety: Vec<(String, Vec<String>)>,
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

    /// A statement that must appear before `Self { .. }`, emitted at most once
    /// per key, in first-use order. Returns whether this call inserted it, so
    /// the caller can also emit the matching safety note exactly once.
    ///
    /// The SPI prelude is the caller: two devices on one bus must not build
    /// the bus twice, and a bus with no device must not be built at all.
    pub fn once(&mut self, key: &str, statement: TokenStream) -> bool {
        if self.once.insert(key.to_string()) {
            self.prelude.push(statement);
            true
        } else {
            false
        }
    }

    /// A `// SAFETY: ...` line to place before `anchor` in the printed file.
    pub fn safety_comment(&mut self, anchor: impl Into<String>, line: impl Into<String>) {
        self.safety.push((anchor.into(), vec![line.into()]));
    }

    /// Several comment lines, kept in order, before `anchor`.
    pub fn safety_block(&mut self, anchor: impl Into<String>, lines: &[&str]) {
        self.safety.push((
            anchor.into(),
            lines.iter().map(|line| (*line).to_string()).collect(),
        ));
    }

    pub fn into_parts(self) -> EmitParts {
        EmitParts {
            fields: self.fields,
            inits: self.inits,
            prelude: self.prelude,
            safety: self.safety,
        }
    }
}

/// A refused claim in the wording the diagnostics corpus pins: the code, the
/// title, the label and which span is underlined.
pub struct ConflictReport {
    pub code: DiagCode,
    pub message: &'static str,
    pub label: String,
    pub span: Range<usize>,
}

/// The legacy wording for a refused claim, kept with the graph so every
/// element reports conflicts the same way.
///
/// The blame order and primary span follow the legacy `conflicts.rs` loops for
/// the cases the corpus pins. The corpus fixes each word: never rewrite one of
/// these labels without re-running it.
pub fn conflict_diagnostic(conflict: &Conflict) -> ConflictReport {
    let pin = conflict.pin.0;
    let existing = &conflict.existing;
    let incoming = &conflict.incoming;

    match conflict.kind {
        ConflictKind::Direction => ConflictReport {
            code: espforge_diagnostics::codes::CONFLICT_DIRECTION,
            message: "pin direction clash",
            label: format!(
                "{} and {}: one pin cannot be both input and output",
                display(&existing.owner, true),
                display(&incoming.owner, false)
            ),
            span: incoming.span.clone(),
        },
        ConflictKind::SelfClash => match existing.owner.kind {
            crate::model::OwnerKind::I2c => ConflictReport {
                code: espforge_diagnostics::codes::CONFLICT_I2C_WIRING,
                message: "i2c wiring",
                label: format!(
                    "{} sda and scl cannot both be GPIO{pin}",
                    display(&existing.owner, false)
                ),
                span: incoming.span.clone(),
            },
            crate::model::OwnerKind::Spi => ConflictReport {
                code: espforge_diagnostics::codes::CONFLICT_SPI_WIRING,
                message: "spi wiring",
                label: format!(
                    "{} sclk and mosi cannot both be GPIO{pin}",
                    display(&existing.owner, false)
                ),
                span: incoming.span.clone(),
            },
            crate::model::OwnerKind::Uart => ConflictReport {
                code: espforge_diagnostics::codes::CONFLICT_UART_WIRING,
                message: "uart wiring",
                label: format!(
                    "{} tx and rx cannot both be GPIO{pin}",
                    display(&existing.owner, false)
                ),
                span: incoming.span.clone(),
            },
            _ => ConflictReport {
                code: espforge_diagnostics::codes::CONFLICT_PIN_USED,
                message: "pin already used",
                label: format!("{} claims GPIO{pin} twice", display(&existing.owner, false)),
                span: incoming.span.clone(),
            },
        },
        ConflictKind::Pin => pin_conflict(conflict),
    }
}

fn pin_conflict(conflict: &Conflict) -> ConflictReport {
    use crate::model::OwnerKind::*;

    let pin = conflict.pin.0;
    let existing = &conflict.existing;
    let incoming = &conflict.incoming;
    let (code, message, label, span) = match (&existing.owner.kind, &incoming.owner.kind) {
        (Recipe(a), Recipe(b)) if a == b => (
            espforge_diagnostics::codes::CONFLICT_PIN_USED,
            "pin already used",
            format!(
                "{} and {} both use GPIO{pin}",
                display(&existing.owner, true),
                display(&incoming.owner, false)
            ),
            incoming.span.clone(),
        ),
        (SpiComponent, SpiComponent) => (
            espforge_diagnostics::codes::CONFLICT_PIN_USED,
            "cs pin already used",
            format!(
                "{} and {} both use GPIO{pin}",
                display(&existing.owner, true),
                display(&incoming.owner, true)
            ),
            incoming.span.clone(),
        ),
        (Spi, SpiComponent) => (
            espforge_diagnostics::codes::CONFLICT_PIN,
            "pin conflict",
            format!(
                "{}'s cs and {} both use GPIO{pin}",
                display(&incoming.owner, false),
                display(&existing.owner, false)
            ),
            incoming.span.clone(),
        ),
        (I2c, Uart) | (Uart, I2c) => {
            let (i2c, uart) = if existing.owner.kind == I2c {
                (existing, incoming)
            } else {
                (incoming, existing)
            };
            (
                espforge_diagnostics::codes::CONFLICT_PIN,
                "pin conflict",
                format!(
                    "{} and {} both use GPIO{pin}",
                    display(&i2c.owner, false),
                    display(&uart.owner, false)
                ),
                i2c.span.clone(),
            )
        }
        (_, Recipe(_)) => (
            espforge_diagnostics::codes::CONFLICT_PIN,
            "pin conflict",
            format!(
                "{} and {} both use GPIO{pin}",
                display(&incoming.owner, true),
                display(&existing.owner, false)
            ),
            incoming.span.clone(),
        ),
        (Recipe(_), _) => (
            espforge_diagnostics::codes::CONFLICT_PIN,
            "pin conflict",
            format!(
                "{} and {} both use GPIO{pin}",
                display(&existing.owner, true),
                display(&incoming.owner, false)
            ),
            existing.span.clone(),
        ),
        _ => (
            espforge_diagnostics::codes::CONFLICT_PIN,
            "pin conflict",
            format!(
                "{} and {} both use GPIO{pin}",
                display(&existing.owner, false),
                display(&incoming.owner, false)
            ),
            existing.span.clone(),
        ),
    };
    ConflictReport {
        code,
        message,
        label,
        span,
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn once_emits_one_statement_per_key_in_first_use_order() {
        let mut cx = EmitCx::new();
        assert!(cx.once("spi2_bus", quote! { let a = 1; }));
        assert!(!cx.once("spi2_bus", quote! { let a = 1; }));
        assert!(cx.once("spi3_bus", quote! { let b = 2; }));

        let parts = cx.into_parts();
        let statements: Vec<String> = parts.prelude.iter().map(ToString::to_string).collect();
        assert_eq!(statements, ["let a = 1 ;", "let b = 2 ;"]);
    }
}
