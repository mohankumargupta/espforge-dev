//! `<ili9341>`: an SPI color TFT on a `<spi_component>`.
//!
//! The panel binds to the SPI device component (SCLK, MOSI and CS) and claims
//! its own auxiliary control lines (`dc`, `reset`, and optionally `backlight`)
//! as `PinUse::Output`; a conflict with any other user of those pins is
//! reported through the ordinary pin-conflict diagnostic. One SPI component
//! takes one display: the device takes the component's `SpiDevice` at
//! construction, so a second display on the same chip select is refused.
//!
//! The display owns its SPI device: the `<spi_component>` it names emits its
//! `Components` field as an `Option`, which `Devices::new` takes.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::attrs::{Issue, Ref};
use crate::devices::{
    Rotation, invalid_element, invalid_id, requires_blocking, tft_orientation, unknown_component,
    unknown_gpio, wrong_direction,
};
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::ili9341::Attrs;
use crate::model::{
    Board, ClaimedPin, Component, Device, Direction, Ident, Owner, OwnerKind, Pin, PinUse, Runtime,
};
use crate::registry::ELEMENTS;

/// The `<ili9341>` element.
pub struct Ili9341Element;

#[distributed_slice(ELEMENTS)]
static ILI9341: &dyn Element = &Ili9341Element;

impl Element for Ili9341Element {
    fn tag(&self) -> &'static str {
        "ili9341"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "ili9341", &id));
        }
        if cx.runtime() != Runtime::Blocking {
            return Err(requires_blocking(cx, node, "ili9341", &id));
        }

        let component_name = attrs.spi_component.name;
        let Some(component) = cx
            .components()
            .iter()
            .find(|component| component.id().raw == component_name)
        else {
            return Err(unknown_component(
                cx,
                node,
                "ili9341",
                &id,
                "spi_component",
                "spi_component",
            ));
        };
        let Component::Spi(_) = component else {
            return Err(unknown_component(
                cx,
                node,
                "ili9341",
                &id,
                "spi_component",
                "spi_component",
            ));
        };
        let spi_component = Ident::new(
            component_name,
            crate::devices::span_of(node, "spi_component"),
        );

        if let Some(other) = cx.devices().iter().find_map(|device| match device {
            Device::Ili9341(tft) if tft.spi_component.raw == component_name => {
                Some(tft.id.raw.clone())
            }
            _ => None,
        }) {
            return Err(cx.report(
                codes::DEVICE_INVALID,
                node_span(node),
                "spi component already used",
                format!(
                    "<ili9341 id=\"{other}\"> and <ili9341 id=\"{}\"> both use \
                     <spi_component id=\"{component_name}\">",
                    id.raw
                ),
            ));
        }

        let dc = claim_output(cx, node, &id, "dc", attrs.dc)?;
        let reset = claim_output(cx, node, &id, "reset", attrs.reset)?;
        let backlight = attrs
            .backlight
            .map(|backlight| claim_output(cx, node, &id, "backlight", backlight))
            .transpose()?;

        cx.push_device(Device::Ili9341(Ili9341Device {
            id,
            spi_component,
            dc,
            reset,
            backlight,
            rotation: rotation(attrs.rotation.value),
            rgb: attrs.rgb.value,
        }));
        Ok(())
    }
}

fn rotation(value: crate::generated::ili9341::Rotation) -> Rotation {
    match value {
        crate::generated::ili9341::Rotation::N0 => Rotation::R0,
        crate::generated::ili9341::Rotation::N90 => Rotation::R90,
        crate::generated::ili9341::Rotation::N180 => Rotation::R180,
        crate::generated::ili9341::Rotation::N270 => Rotation::R270,
    }
}

/// Resolve one auxiliary pin reference and claim it as an output.
fn claim_output(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    id: &Ident,
    attr: &str,
    reference: Ref<'_>,
) -> Result<ClaimedPin, Reported> {
    let name = reference.name;
    let Some(gpio) = cx.gpio(name) else {
        return Err(unknown_gpio(cx, node, "ili9341", id, attr, name));
    };
    if gpio.direction == Some(Direction::Input) {
        return Err(wrong_direction(cx, node, "ili9341", id, attr, name));
    }
    let pin = Pin(gpio.pin);
    cx.claim_pin(
        pin,
        Owner::new(OwnerKind::Device("ili9341"), id.clone()),
        node_span(node),
        PinUse::Output,
    )
    .map_err(|conflict| cx.report_conflict(&conflict))
}

/// One lowered `<ili9341>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ili9341Device {
    pub id: Ident,
    /// The `<spi_component>` whose `SpiDevice` handle the panel drives.
    pub spi_component: Ident,
    pub dc: ClaimedPin,
    pub reset: ClaimedPin,
    pub backlight: Option<ClaimedPin>,
    pub rotation: Rotation,
    /// `true` selects RGB color order, `false` the driver's default BGR.
    pub rgb: bool,
}

impl Lowered for Ili9341Device {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let component = format_ident!("{}", self.spi_component.rust_name());
        let dc = crate::components::gpio(self.dc.pin().0);
        let reset = crate::components::gpio(self.reset.pin().0);
        let reset_output = quote! {
            esp_hal::gpio::Output::new(
                #reset,
                esp_hal::gpio::Level::High,
                esp_hal::gpio::OutputConfig::default(),
            )
        };
        let backlight = self.backlight.as_ref().map(|pin| pin.pin().0);
        let backlight = output(backlight);
        let orientation = tft_orientation(self.rotation);
        let color_order = if self.rgb {
            quote! { espforge_drivers::ColorOrder::Rgb }
        } else {
            quote! { espforge_drivers::ColorOrder::Bgr }
        };

        let mut pins = format!("GPIO{}", self.dc.pin().0);
        pins.push_str(&format!("/GPIO{}", self.reset.pin().0));
        if let Some(pin) = self.backlight.as_ref() {
            pins.push_str(&format!("/GPIO{}", pin.pin().0));
        }

        cx.device_field(quote! {
            pub #name: espforge_drivers::Ili9341Display,
        });
        cx.device_init(quote! {
            #name: espforge_drivers::Ili9341Display::new(
                components.#component.take().expect("the display's SPI bus was already taken"),
                esp_hal::gpio::Output::new(
                    #dc,
                    esp_hal::gpio::Level::Low,
                    esp_hal::gpio::OutputConfig::default(),
                ),
                #reset_output,
                #backlight,
                #orientation,
                #color_order,
            ),
        });
        cx.safety_comment(
            format!("{name}: espforge_drivers::Ili9341Display::new("),
            format!("SAFETY: espforge checked that no other component uses {pins}."),
        );
    }
}

/// `Some(Output::new(GPIO<n>, HIGH, ..))`, or `None` when the line is absent.
fn output(pin: Option<u8>) -> proc_macro2::TokenStream {
    match pin {
        Some(pin) => {
            let gpio = crate::components::gpio(pin);
            quote! {
                Some(esp_hal::gpio::Output::new(
                    #gpio,
                    esp_hal::gpio::Level::High,
                    esp_hal::gpio::OutputConfig::default(),
                ))
            }
        }
        None => quote! { None },
    }
}

/// This element's wording for a schema-level problem.
fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: Issue<'_>) -> Reported {
    invalid_element(cx, node, "ili9341", issue)
}
