//! Elements that live in `# devices`: drivers for a specific part.
//!
//! A device refers *down* to the component it is attached to
//! (`i2c_component=`, `spi_component=`) and, for its auxiliary control lines,
//! to `<gpio>` peripherals. It never names a bus pin directly. The shared
//! wording of its diagnostics lives here, so the element files differ only in
//! what makes them different.
//!
//! The drivers are blocking-only: [`Rotation`] is rendered into the driver's
//! own orientation type, and a display declared under the embassy runtime is
//! refused with [`requires_blocking`] rather than producing code that cannot
//! compile.

pub mod ili9341;
pub mod ssd1306;

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use proc_macro2::TokenStream;
use quote::quote;

use crate::attrs::Issue;
use crate::element::{LowerCx, Reported};
use crate::model::Ident;

/// A display rotation in degrees clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

/// The SSD1306 rotation for this value.
pub(crate) fn oled_rotation(rotation: Rotation) -> TokenStream {
    match rotation {
        Rotation::R0 => quote! { espforge_runtime::DisplayRotation::Rotate0 },
        Rotation::R90 => quote! { espforge_runtime::DisplayRotation::Rotate90 },
        Rotation::R180 => quote! { espforge_runtime::DisplayRotation::Rotate180 },
        Rotation::R270 => quote! { espforge_runtime::DisplayRotation::Rotate270 },
    }
}

/// The `sky-ili9341` orientation for this rotation.
pub(crate) fn tft_orientation(rotation: Rotation) -> TokenStream {
    match rotation {
        Rotation::R0 => quote! { espforge_runtime::Orientation::Portrait },
        Rotation::R90 => quote! { espforge_runtime::Orientation::Landscape },
        Rotation::R180 => quote! { espforge_runtime::Orientation::PortraitFlipped },
        Rotation::R270 => quote! { espforge_runtime::Orientation::LandscapeFlipped },
    }
}

/// The value span of one attribute on `node`, or the node itself.
pub(crate) fn span_of(node: &ParsedNode<'_>, attr: &str) -> std::ops::Range<usize> {
    node.get_attr_spanned(attr)
        .map(|a| a.val_offset..a.val_offset + a.val.len())
        .unwrap_or_else(|| node_span(node))
}

/// A device id that cannot become a Rust identifier.
pub(crate) fn invalid_id(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    tag: &str,
    id: &Ident,
) -> Reported {
    cx.report(
        codes::DEVICE_INVALID_ID,
        node_span(node),
        "invalid device id",
        format!(
            "<{tag} id=\"{}\">: the id must be a valid Rust identifier \
             (letters, digits, `_`; not starting with a digit)",
            id.raw
        ),
    )
}

/// A schema-level problem (missing or invalid attribute).
pub(crate) fn invalid_element(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    tag: &str,
    issue: Issue<'_>,
) -> Reported {
    let id = node.id;
    let message = match issue {
        Issue::Missing { attr } => {
            format!("<{tag} id=\"{id}\"> is missing `{attr}`")
        }
        Issue::Invalid { attr, value } => {
            let expected = match attr {
                "rotation" => "one of 0, 90, 180, 270",
                "size" => "128x64 or 128x32",
                "rgb" => "true or false",
                _ => "a valid value",
            };
            format!("<{tag} id=\"{id}\"> {attr}=\"{value}\" must be {expected}")
        }
    };
    cx.report(
        codes::DEVICE_INVALID,
        node_span(node),
        format!("invalid {tag}"),
        message,
    )
}

/// A `i2c_component=` / `spi_component=` reference to a component that is not
/// on the board.
pub(crate) fn unknown_component(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    tag: &str,
    id: &Ident,
    component: &str,
    kind: &str,
) -> Reported {
    cx.report(
        codes::DEVICE_UNKNOWN_COMPONENT,
        span_of(node, component),
        "unknown bus component",
        format!(
            "<{tag} id=\"{}\"> refers to `{name}`, which is not an <{kind}> in `# components`",
            id.raw,
            name = node
                .get_attr_spanned(component)
                .map(|a| a.val.trim_start_matches('$'))
                .unwrap_or_default(),
        ),
    )
}

/// An auxiliary pin reference to a `<gpio>` that does not exist.
pub(crate) fn unknown_gpio(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    tag: &str,
    id: &Ident,
    attr: &str,
    name: &str,
) -> Reported {
    cx.report(
        codes::DEVICE_UNKNOWN_GPIO,
        span_of(node, attr),
        "unknown gpio",
        format!(
            "<{tag} id=\"{}\"> refers to `{name}`, which is not a <gpio> in `# peripherals`",
            id.raw
        ),
    )
}

/// A device auxiliary pin on a GPIO declared as an input.
pub(crate) fn wrong_direction(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    tag: &str,
    id: &Ident,
    attr: &str,
    name: &str,
) -> Reported {
    cx.report(
        codes::DEVICE_WRONG_DIRECTION,
        span_of(node, attr),
        "wrong gpio direction",
        format!(
            "<{tag} id=\"{}\"> needs an output pin, but `{name}` has direction=\"input\"",
            id.raw
        ),
    )
}

/// A blocking-only display declared under the embassy runtime.
pub(crate) fn requires_blocking(
    cx: &mut LowerCx,
    node: &ParsedNode<'_>,
    tag: &str,
    id: &Ident,
) -> Reported {
    cx.report(
        codes::DEVICE_REQUIRES_BLOCKING,
        node_span(node),
        "display requires blocking",
        format!(
            "<{tag} id=\"{}\"> needs the blocking runtime; change \
             <runtime type=\"..\" /> to blocking",
            id.raw
        ),
    )
}
