//! `<led>`: an LED on a GPIO output.
//!
//! It refers to a `<gpio>` by id, refuses an input pin, claims the pin as an
//! output, and emits its own field, initialiser and `SAFETY` note. All of the
//! wording here is the legacy wording.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::element::{
    Element, EmitCx, LowerCx, Lowered, Reported, conflict_diagnostic, is_rust_ident,
};
use crate::generated::led::Attrs;
use crate::model::{Board, ClaimedPin, Component, Direction, Ident, Owner, OwnerKind, Pin, PinUse};
use crate::registry::ELEMENTS;

/// The `<led>` element.
pub struct LedElement;

#[distributed_slice(ELEMENTS)]
static LED: &dyn Element = &LedElement;

impl Element for LedElement {
    fn tag(&self) -> &'static str {
        "led"
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
                    "<led id=\"{}\"> refers to `{}`, which is not a <gpio> in `# peripherals`",
                    id.raw, attrs.gpio.name
                ),
            ));
        };

        if let Some(direction) = gpio.direction {
            if direction != Direction::Output {
                return Err(cx.report(
                    codes::COMPONENT_WRONG_DIRECTION,
                    node_span(node),
                    "wrong gpio direction",
                    format!(
                        "<led id=\"{}\"> needs an output pin, but `{}` has direction=\"{}\"",
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
                codes::COMPONENT_INVALID_LED,
                node_span(node),
                "invalid led",
                format!(
                    "<led id=\"{}\">: the id must become a Rust identifier \
                     (letters, digits, `_`; not starting with a digit)",
                    id.raw
                ),
            ));
        }

        let pin = cx
            .claim_pin(
                Pin(gpio.pin),
                Owner::new(OwnerKind::Led, id.clone()),
                node_span(node),
                PinUse::Output,
            )
            .map_err(|conflict| {
                let (message, label) = conflict_diagnostic(&conflict);
                cx.report(
                    codes::CONFLICT_PIN_USED,
                    conflict.incoming.span.clone(),
                    message,
                    label,
                )
            })?;

        cx.push(Component::Led(Led {
            id,
            pin,
            active_low: attrs.active_low.value,
        }));
        Ok(())
    }
}

/// One lowered `<led>` over a claimed pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Led {
    pub id: Ident,
    pub pin: ClaimedPin,
    pub active_low: bool,
}

impl Lowered for Led {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let gpio = format_ident!("GPIO{}", self.pin.pin().0);
        let active_low = self.active_low;
        cx.field(quote! { pub #name: espforge_runtime::Led, });
        cx.init(quote! {
            #name: espforge_runtime::Led::new(
                unsafe { registry.peripherals.#gpio.clone_unchecked() },
                #active_low,
            ),
        });
        cx.safety_comment(
            format!("{name}: espforge_runtime::Led::new("),
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

/// This element's wording for a schema-level problem (`E0601`).
fn invalid(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: crate::attrs::Issue<'_>) -> Reported {
    let id = node.id;
    let label = match issue {
        crate::attrs::Issue::Missing { attr: "gpio" } => {
            format!("<led id=\"{id}\"> needs gpio=\"$<gpio id>\"")
        }
        crate::attrs::Issue::Missing { attr } => {
            format!("Missing required attribute '{attr}'")
        }
        crate::attrs::Issue::Invalid { attr, value } => {
            format!("<led id=\"{id}\"> {attr}=\"{value}\" must be \"true\" or \"false\"")
        }
    };
    cx.report(
        codes::COMPONENT_INVALID_LED,
        node_span(node),
        "invalid led",
        label,
    )
}
