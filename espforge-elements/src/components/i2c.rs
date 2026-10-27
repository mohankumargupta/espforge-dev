//! `<i2c_component>`: one component on an `<i2c>` bus.
//!
//! The schema allows exactly one component per bus, because the generated
//! `Components` field owns the bus itself. A scanner is the bus-level shape:
//! an id and a `$i2c` reference, no address.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::attrs::Issue;
use crate::components::{gpio, peripheral_access, rate, runtime_type};
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::i2c_component::Attrs;
use crate::model::{Board, Bus, BusId, Component, Ident};
use crate::registry::ELEMENTS;

/// The `<i2c_component>` element.
pub struct I2cComponentElement;

#[distributed_slice(ELEMENTS)]
static I2C_COMPONENT: &dyn Element = &I2cComponentElement;

impl Element for I2cComponentElement {
    fn tag(&self) -> &'static str {
        "i2c_component"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);
        let address = attrs.address.map(|address| address.value);
        let bus_name = attrs.i2c.name;

        let Some(bus) = cx.bus_by_name(bus_name) else {
            return Err(cx.report(
                codes::COMPONENT_UNKNOWN_BUS,
                node_span(node),
                "unknown i2c bus",
                format!(
                    "<i2c_component id=\"{}\"> refers to `{bus_name}`, which is not an <i2c> in `# peripherals`",
                    id.raw
                ),
            ));
        };

        if let Some(other) = used_by(cx, bus) {
            let bus_id = cx
                .bus(bus)
                .map_or("", |bus| bus.id().raw.as_str())
                .to_string();
            return Err(cx.report(
                codes::COMPONENT_I2C_BUS_USED,
                node_span(node),
                "i2c bus already used",
                format!(
                    "<i2c_component id=\"{}\"> and <i2c_component id=\"{}\"> both use <i2c id=\"{bus_id}\">",
                    other.rust_name(),
                    id.raw
                ),
            ));
        }

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "i2c_component", &id));
        }

        set_used_by(cx, bus, id.clone());
        cx.push(Component::I2c(I2cComponent { id, bus, address }));
        Ok(())
    }
}

/// One lowered `<i2c_component>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I2cComponent {
    pub id: Ident,
    pub bus: BusId,
    /// The address the component declares, when it has one. A bus-level
    /// component such as an I2C scanner deliberately has none.
    pub address: Option<u8>,
}

impl Lowered for I2cComponent {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, board: &Board, cx: &mut EmitCx) {
        let Some(Bus::I2c {
            peripheral,
            sda,
            scl,
            khz,
            ..
        }) = board.resources.bus(self.bus)
        else {
            return;
        };
        let name = format_ident!("{}", self.id.rust_name());
        let runtime = runtime_type(board.meta.runtime);
        let peripheral = peripheral_access(peripheral);
        let (sda_pin, scl_pin) = (sda.pin().0, scl.pin().0);
        let sda = gpio(sda_pin);
        let scl = gpio(scl_pin);
        let rate = rate(*khz);

        cx.field(quote! {
            pub #name: espforge_runtime::I2cBus<#runtime>,
        });
        cx.init(quote! {
            #name: espforge_runtime::I2cBus::build(
                #peripheral,
                #sda,
                #scl,
                espforge_runtime::I2cConfig { frequency: #rate },
            )
            .expect("I2C bus configuration failed"),
        });
        cx.safety_block(
            format!("{name}: espforge_runtime::I2cBus::build("),
            &[
                "SAFETY: espforge checked that no other component uses this",
                &format!("peripheral or GPIO{sda_pin}/GPIO{scl_pin}."),
            ],
        );
    }
}

/// The component riding on a bus, if any.
fn used_by(cx: &LowerCx, bus: BusId) -> Option<Ident> {
    match cx.bus(bus) {
        Some(Bus::I2c { used_by, .. }) => used_by.clone(),
        _ => None,
    }
}

/// Record the component that rides on a bus.
fn set_used_by(cx: &mut LowerCx, bus: BusId, id: Ident) {
    if let Some(Bus::I2c { used_by, .. }) = cx.bus_mut(bus) {
        *used_by = Some(id);
    }
}

fn invalid_id(cx: &mut LowerCx, node: &ParsedNode<'_>, tag: &str, id: &Ident) -> Reported {
    cx.report(
        codes::COMPONENT_INVALID_ID,
        node_span(node),
        format!("invalid {tag}"),
        format!(
            "<{tag} id=\"{}\">: the id must become a Rust identifier (letters, digits, `_`; not starting with a digit)",
            id.raw
        ),
    )
}

/// This element's wording for a schema-level problem.
fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: Issue<'_>) -> Reported {
    let id = node.id;
    match issue {
        Issue::Missing { attr } => cx.report(
            codes::COMPONENT_INVALID_I2C,
            node_span(node),
            "invalid i2c_component",
            format!("<i2c_component id=\"{id}\"> missing required attribute '{attr}'"),
        ),
        Issue::Invalid { value, .. } if value.starts_with("0x") || value.starts_with("0X") => cx
            .report(
                codes::COMPONENT_INVALID_I2C,
                node_span(node),
                "invalid i2c_component",
                format!("❌ [i2c_component] '{id}' invalid hex address '{value}'"),
            ),
        Issue::Invalid { value, .. } => cx.report(
            codes::COMPONENT_INVALID_I2C,
            node_span(node),
            "invalid i2c_component",
            format!("❌ [i2c_component] '{id}' invalid decimal address '{value}'"),
        ),
    }
}
