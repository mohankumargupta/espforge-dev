//! `<button>`: a push button on a GPIO input.
//!
//! It refers to a `<gpio>` by id, refuses an output pin, claims the pin as an
//! input, and emits its own field, initialiser and `SAFETY` note. All of the
//! wording here is the legacy wording.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::element::{
    Element, EmitCx, LowerCx, Lowered, Reported, conflict_diagnostic, is_rust_ident,
};
use crate::generated::button::Attrs;
use crate::model::{
    Board, ClaimedPin, Component, Direction, Ident, Owner, OwnerKind, Pin, PinUse,
};
use crate::registry::ELEMENTS;

/// The `<button>` element.
pub struct ButtonElement;

#[distributed_slice(ELEMENTS)]
static BUTTON: &dyn Element = &ButtonElement;

impl Element for ButtonElement {
    fn tag(&self) -> &'static str {
        "button"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| invalid(cx, node, issue))?;
        let id = cx.ident(node);

        let Some(gpio) = cx.gpio(attrs.gpio.name).cloned() else {
            return Err(cx.report(
                codes::COMPONENT_UNKNOWN_GPIO,
                node_span(node),
                "unknown gpio",
                format!(
                    "<button id=\"{}\"> refers to `{}`, which is not a <gpio> in `# peripherals`",
                    id.raw, attrs.gpio.name
                ),
            ));
        };

        if let Some(direction) = gpio.direction {
            if direction != Direction::Input {
                return Err(cx.report(
                    codes::COMPONENT_WRONG_DIRECTION,
                    node_span(node),
                    "wrong gpio direction",
                    format!(
                        "<button id=\"{}\"> needs an input pin, but `{}` has direction=\"{}\"",
                        id.raw,
                        gpio.id.raw,
                        match direction {
                            Direction::Output => "output",
                            Direction::Input => "input",
                        }
                    ),
                ));
            }
        }

        if !is_rust_ident(&id.rust_name()) {
            return Err(cx.report(
                codes::COMPONENT_INVALID_BUTTON,
                node_span(node),
                "invalid button",
                format!(
                    "<button id=\"{}\">: the id must become a Rust identifier \
                     (letters, digits, `_`; not starting with a digit)",
                    id.raw
                ),
            ));
        }

        let pin = cx
            .claim_pin(
                Pin(gpio.pin),
                Owner::new(OwnerKind::Button, id.clone()),
                node_span(node),
                PinUse::Input,
            )
            .map_err(|conflict| {
                let (code, message, label) = conflict_diagnostic(&conflict);
                cx.report(code, conflict.incoming.span.clone(), message, label)
            })?;

        cx.push(Component::Button(Button {
            id,
            pin,
            pull_up: attrs.pull_up.value,
        }));
        Ok(())
    }
}

/// One lowered `<button>` over a claimed pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub id: Ident,
    pub pin: ClaimedPin,
    pub pull_up: bool,
}

impl Lowered for Button {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let gpio = format_ident!("GPIO{}", self.pin.pin().0);
        let pull_up = self.pull_up;
        cx.field(quote! { pub #name: espforge_runtime::Button, });
        cx.init(quote! {
            #name: espforge_runtime::Button::new(
                unsafe { registry.peripherals.#gpio.clone_unchecked() },
                #pull_up,
            ),
        });
        cx.safety_comment(
            format!("{name}: espforge_runtime::Button::new("),
            format!(
                "SAFETY: espforge checked that no other component uses GPIO{}.",
                self.pin.pin().0
            ),
        );
    }

    fn overlay_attr(&self, attr: &str) -> Option<String> {
        match attr {
            "pin" => Some(self.pin.pin().0.to_string()),
            _ => None,
        }
    }
}

/// This element's wording for a schema-level problem (`E0602`).
fn invalid(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: crate::attrs::Issue<'_>) -> Reported {
    let id = node.id;
    let label = match issue {
        crate::attrs::Issue::Missing { attr: "gpio" } => {
            format!("<button id=\"{id}\"> needs gpio=\"$<gpio id>\"")
        }
        crate::attrs::Issue::Missing { attr } => {
            format!("Missing required attribute '{attr}'")
        }
        crate::attrs::Issue::Invalid { attr, value } => {
            format!("<button id=\"{id}\"> {attr}=\"{value}\" must be \"true\" or \"false\"")
        }
    };
    cx.report(
        codes::COMPONENT_INVALID_BUTTON,
        node_span(node),
        "invalid button",
        label,
    )
}
