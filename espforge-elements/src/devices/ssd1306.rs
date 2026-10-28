//! `<ssd1306>`: an I2C OLED panel on an `<i2c_component>`.
//!
//! The panel lives in `# devices`: it binds to the already-validated I2C bus
//! component (`i2c_component="$display_bus"`) and claims no pin of its own.
//! Its handle lands in the generated `Devices` struct, so `app.rs` draws on it
//! through `device!(ctx, oled)`.

use espf::context::ParsedNode;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::devices::{
    Rotation, invalid_element, invalid_id, oled_rotation, requires_blocking, unknown_component,
};
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::ssd1306::Attrs;
use crate::model::{Board, Component, Device, Ident, Runtime};
use crate::registry::ELEMENTS;

/// The `<ssd1306>` element.
pub struct Ssd1306Element;

#[distributed_slice(ELEMENTS)]
static SSD1306: &dyn Element = &Ssd1306Element;

impl Element for Ssd1306Element {
    fn tag(&self) -> &'static str {
        "ssd1306"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "ssd1306", &id));
        }
        if cx.runtime() != Runtime::Blocking {
            return Err(requires_blocking(cx, node, "ssd1306", &id));
        }

        let component_name = attrs.i2c_component.name;
        let Some(component) = cx
            .components()
            .iter()
            .find(|component| component.id().raw == component_name)
        else {
            return Err(unknown_component(
                cx,
                node,
                "ssd1306",
                &id,
                "i2c_component",
                "i2c_component",
            ));
        };
        let Component::I2c(_) = component else {
            return Err(unknown_component(
                cx,
                node,
                "ssd1306",
                &id,
                "i2c_component",
                "i2c_component",
            ));
        };
        let i2c_component = Ident::new(
            component_name,
            crate::devices::span_of(node, "i2c_component"),
        );

        cx.push_device(Device::Ssd1306(Ssd1306Device {
            id,
            i2c_component,
            size: OledSize::from(attrs.size.value),
            rotation: rotation(attrs.rotation.value),
        }));
        Ok(())
    }
}

/// The panel dimensions the driver was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OledSize {
    S128x64,
    S128x32,
}

impl From<crate::generated::ssd1306::Size> for OledSize {
    fn from(size: crate::generated::ssd1306::Size) -> Self {
        match size {
            crate::generated::ssd1306::Size::N128x64 => OledSize::S128x64,
            crate::generated::ssd1306::Size::N128x32 => OledSize::S128x32,
        }
    }
}

fn rotation(value: crate::generated::ssd1306::Rotation) -> Rotation {
    match value {
        crate::generated::ssd1306::Rotation::N0 => Rotation::R0,
        crate::generated::ssd1306::Rotation::N90 => Rotation::R90,
        crate::generated::ssd1306::Rotation::N180 => Rotation::R180,
        crate::generated::ssd1306::Rotation::N270 => Rotation::R270,
    }
}

/// One lowered `<ssd1306>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ssd1306Device {
    pub id: Ident,
    /// The `<i2c_component>` whose `I2cBus` handle the panel drives.
    pub i2c_component: Ident,
    pub size: OledSize,
    pub rotation: Rotation,
}

impl Lowered for Ssd1306Device {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let component = format_ident!("{}", self.i2c_component.rust_name());
        let (size_type, size_value) = match self.size {
            OledSize::S128x64 => (
                quote! { espforge_runtime::Oled128x64 },
                quote! { espforge_runtime::Oled128x64 },
            ),
            OledSize::S128x32 => (
                quote! { espforge_runtime::Oled128x32 },
                quote! { espforge_runtime::Oled128x32 },
            ),
        };
        let rotation = oled_rotation(self.rotation);

        cx.device_field(quote! {
            pub #name: espforge_runtime::Ssd1306Display<#size_type>,
        });
        cx.device_init(quote! {
            #name: espforge_runtime::Ssd1306Display::new(
                components.#component,
                #size_value,
                #rotation,
            ),
        });
    }
}

/// This element's wording for a schema-level problem.
fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: crate::attrs::Issue<'_>) -> Reported {
    invalid_element(cx, node, "ssd1306", issue)
}
