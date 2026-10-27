//! Lowering: walk `# peripherals` then `# components`, find the registered
//! element for each tag, and build the board.
//!
//! An unknown tag is reported from the schema ∩ registry, so the "known"
//! list in the message cannot drift from what actually exists.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::{Diagnostic, codes};
use espforge_schema::schema;

use crate::element::{Element, LowerCx};
use crate::model::{Board, Bus, GpioDecl, Meta};
use crate::registry;

/// Lower both hardware sections onto `meta`.
///
/// Diagnostics are collected across both sections and returned together; a
/// `Board` exists only when there are none.
pub fn lower(
    meta: Meta,
    peripherals: &[ParsedNode<'_>],
    components: &[ParsedNode<'_>],
) -> Result<Board, Vec<Diagnostic>> {
    let mut cx = LowerCx::new();
    lower_section("peripherals", peripherals, &mut cx);
    check_bus_gpio_declarations(&mut cx);
    // Legacy stopped at the first bus/gpio conflict before a component could
    // claim the same pin, and the corpus pins that single error. Keep that
    // order: a component is not lowered onto a pin the board already called
    // out. Peripherals are all lowered first, so a `<gpio>` written after the
    // bus that uses its pin still reaches this pass.
    if cx.has_errors() {
        return cx.into_board(meta);
    }
    lower_section("components", components, &mut cx);
    cx.into_board(meta)
}

/// A bus pin that a `<gpio>` declared is a wiring error: the gpio is a second
/// user of the pin, and the diagnostic points at the gpio (as the legacy
/// `check_bus_gpio_declarations` did).
fn check_bus_gpio_declarations(cx: &mut LowerCx) {
    let gpios: Vec<GpioDecl> = cx.gpios().to_vec();
    let mut found: Option<(std::ops::Range<usize>, String)> = None;
    for bus in cx.buses() {
        let (tag, id, pins) = match bus {
            Bus::I2c { id, sda, scl, .. } => ("i2c", id, vec![sda.pin().0, scl.pin().0]),
            Bus::Spi {
                id,
                sclk,
                mosi,
                miso,
                ..
            } => (
                "spi",
                id,
                vec![sclk.pin().0, mosi.pin().0]
                    .into_iter()
                    .chain(miso.iter().map(|pin| pin.pin().0))
                    .collect(),
            ),
            Bus::Uart { id, tx, rx, .. } => ("uart", id, vec![tx.pin().0, rx.pin().0]),
        };
        for gpio in &gpios {
            if pins.contains(&gpio.pin) {
                found = Some((
                    gpio.span.clone(),
                    format!(
                        "<gpio id=\"{}\"> and <{tag} id=\"{}\"> both use GPIO{}",
                        gpio.id.raw, id.raw, gpio.pin
                    ),
                ));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    if let Some((span, label)) = found {
        cx.report(codes::CONFLICT_PIN, span, "pin conflict", label);
    }
}

fn lower_section(section: &str, nodes: &[ParsedNode<'_>], cx: &mut LowerCx) {
    for node in nodes {
        match element_for(section, node.tag_name) {
            Some(element) => {
                let _ = element.lower(node, cx);
            }
            None => {
                let known = known_tags(section);
                let known_list = if known.is_empty() {
                    "(none)".to_string()
                } else {
                    known
                        .iter()
                        .map(|tag| format!("`<{tag}>`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let code = match section {
                    "peripherals" => codes::PERIPHERAL_UNSUPPORTED,
                    _ => codes::COMPONENT_UNSUPPORTED,
                };
                cx.report(
                    code,
                    node_span(node),
                    format!(
                        "`<{}>` is not a known element in `# {section}`; known: {known_list}",
                        node.tag_name
                    ),
                    "remove the element, or fix its tag",
                );
            }
        }
    }
}

/// The registered element whose tag matches **and** whose schema section is
/// the one being walked.
fn element_for(section: &str, tag: &str) -> Option<&'static dyn Element> {
    let schema = schema();
    let definition = schema.element(tag)?;
    if definition.section != section {
        return None;
    }
    registry::element(tag)
}

/// The tags that can appear in `section`: schema elements with an
/// implementation, in schema order.
fn known_tags(section: &str) -> Vec<String> {
    let schema = schema();
    schema
        .elements_in(section)
        .filter(|element| registry::element(&element.tag).is_some())
        .map(|element| element.tag.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Lowered;
    use crate::model::{Component, Console, Direction, OwnerKind, Pin, PinUse, Runtime};
    use espf::parser::parse_espf;

    const BLINK: &str = "# espforge\n\
        \x20 <project name=\"blink\" />\n\
        \x20 <chip type=\"esp32c3\" />\n\
        \x20 <runtime type=\"blocking\" />\n\
        # peripherals\n\
        \x20 <gpio id=\"gpio18\" pin=\"18\" direction=\"output\" />\n\
        # components\n\
        \x20 <led id=\"red_led\" gpio=\"$gpio18\" />\n\
        # devices\n\
        # services\n";

    const BUTTON: &str = "# espforge\n\
        \x20 <project name=\"button\" />\n\
        \x20 <chip type=\"esp32c3\" />\n\
        \x20 <runtime type=\"blocking\" />\n\
        # peripherals\n\
        \x20 <gpio id=\"gpio7\" pin=\"7\" direction=\"output\" />\n\
        \x20 <gpio id=\"gpio9\" pin=\"9\" direction=\"input\" />\n\
        # components\n\
        \x20 <led id=\"red_led\" gpio=\"$gpio7\" active_low=\"false\" />\n\
        \x20 <button id=\"button\" gpio=\"$gpio9\" pull_up=\"true\" />\n\
        # devices\n\
        # services\n";

    fn meta() -> Meta {
        Meta {
            name: "blink".into(),
            description: None,
            chip: "esp32c3".into(),
            runtime: Runtime::Blocking,
            console: Console::Uart,
        }
    }

    fn lower_src(src: &str) -> Result<Board, Vec<Diagnostic>> {
        let parsed = parse_espf(src);
        lower(
            meta(),
            &parsed.sections["peripherals"],
            &parsed.sections["components"],
        )
    }

    #[test]
    fn blink_lowers_to_one_led_on_pin_18() {
        let board = lower_src(BLINK).expect("blink lowers");
        assert_eq!(board.components.len(), 1);
        let Component::Recipe(led) = &board.components[0] else {
            panic!("blink lowers to a led");
        };
        assert_eq!(led.tag, "led");
        assert_eq!(led.id.raw, "red_led");
        assert_eq!(led.id.rust_name(), "red_led");
        assert_eq!(led.pin.pin(), Pin(18));
        assert_eq!(led.flags, [false]);
        assert_eq!(led.overlay_attr("pin").as_deref(), Some("18"));
        assert_eq!(led.overlay_attr("color"), None);
        assert_eq!(
            board.resources.claim_of(Pin(18)).unwrap().owner.kind,
            OwnerKind::Recipe("led")
        );
    }

    #[test]
    fn a_gpio_without_a_direction_can_still_drive_an_led() {
        let src = BLINK.replace(" direction=\"output\"", "");
        let board = lower_src(&src).expect("no direction is allowed");
        assert_eq!(board.components.len(), 1);
    }

    #[test]
    fn an_input_gpio_cannot_drive_an_led() {
        let src = BLINK.replace("direction=\"output\"", "direction=\"input\"");
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, codes::COMPONENT_WRONG_DIRECTION);
        assert_eq!(errors[0].message, "wrong gpio direction");
        assert!(errors[0].labels[0].text.contains("needs an output pin"));
    }

    #[test]
    fn a_led_pointing_at_a_missing_gpio_is_reported() {
        let src = BLINK.replace("$gpio18", "$gpio99");
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_UNKNOWN_GPIO);
        assert_eq!(errors[0].message, "unknown gpio");
        assert!(errors[0].labels[0].text.contains("`gpio99`"));
    }

    #[test]
    fn a_bad_gpio_pin_is_e0502_and_a_bad_direction_is_e0503() {
        let errors = lower_src(&BLINK.replace("pin=\"18\"", "pin=\"eighteen\"")).unwrap_err();
        assert_eq!(errors[0].code, codes::GPIO_INVALID_PIN);
        assert_eq!(errors[0].message, "invalid gpio pin");

        let errors = lower_src(&BLINK.replace("direction=\"output\"", "direction=\"sideways\""))
            .unwrap_err();
        assert_eq!(errors[0].code, codes::GPIO_INVALID_DIRECTION);
        assert_eq!(errors[0].message, "invalid gpio direction");
    }

    #[test]
    fn a_led_id_must_become_a_rust_identifier() {
        let errors = lower_src(&BLINK.replace("id=\"red_led\"", "id=\"9red\"")).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_INVALID_LED);
        assert_eq!(errors[0].message, "invalid led");
        assert!(errors[0].labels[0].text.contains("Rust identifier"));
    }

    #[test]
    fn two_leds_on_one_pin_are_a_conflict() {
        let src = BLINK.replace(
            "  <led id=\"red_led\" gpio=\"$gpio18\" />\n",
            "  <led id=\"red_led\" gpio=\"$gpio18\" />\n  <led id=\"other\" gpio=\"$gpio18\" />\n",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, codes::CONFLICT_PIN_USED);
        assert_eq!(errors[0].message, "pin already used");
        assert_eq!(
            errors[0].labels[0].text,
            "<led id=\"red_led\"> and <led id=\"other\"> both use GPIO18"
        );
    }

    #[test]
    fn an_unknown_element_names_what_is_known() {
        let src = BLINK.replace("# peripherals\n", "# peripherals\n  <pwm id=\"pwm0\" />\n");
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::PERIPHERAL_UNSUPPORTED);
        assert_eq!(
            errors[0].message,
            "`<pwm>` is not a known element in `# peripherals`; known: `<gpio>`, `<i2c>`"
        );
    }

    #[test]
    fn an_element_in_the_wrong_section_is_not_known_there() {
        let src = BLINK.replace(
            "# components\n",
            "# components\n  <gpio id=\"extra\" pin=\"7\" />\n",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_UNSUPPORTED);
        assert!(
            errors[0]
                .message
                .contains("`<gpio>` is not a known element")
        );
    }

    /// The unknown-tag list for `# components` walks `registry::element`, so
    /// the recipe door must show up in it: a component the schema does not know
    /// is told which components exist.
    #[test]
    fn an_unknown_component_names_the_recipe_elements() {
        let src = BLINK.replace("# components\n", "# components\n  <pwm id=\"pwm0\" />\n");
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_UNSUPPORTED);
        assert_eq!(
            errors[0].message,
            "`<pwm>` is not a known element in `# components`; known: `<led>`, `<button>`, `<i2c_component>`"
        );
    }

    #[test]
    fn button_lowers_beside_an_led_and_claims_its_input_pin() {
        let board = lower_src(BUTTON).expect("button lowers");
        assert_eq!(board.components.len(), 2);

        let Component::Recipe(led) = &board.components[0] else {
            panic!("the first component is the led");
        };
        assert_eq!(led.tag, "led");
        assert_eq!(led.id.raw, "red_led");
        assert_eq!(led.pin.pin(), Pin(7));

        let Component::Recipe(button) = &board.components[1] else {
            panic!("the second component is the button");
        };
        assert_eq!(button.tag, "button");
        assert_eq!(button.id.raw, "button");
        assert_eq!(button.pin.pin(), Pin(9));
        assert_eq!(button.flags, [true]);
        assert_eq!(button.overlay_attr("pin").as_deref(), Some("9"));
        assert_eq!(button.overlay_attr("color"), None);

        let led_claim = board.resources.claim_of(Pin(7)).unwrap();
        assert_eq!(led_claim.owner.kind, OwnerKind::Recipe("led"));
        assert_eq!(led_claim.need, PinUse::Output);
        let button_claim = board.resources.claim_of(Pin(9)).unwrap();
        assert_eq!(button_claim.owner.kind, OwnerKind::Recipe("button"));
        assert_eq!(button_claim.need, PinUse::Input);
    }

    #[test]
    fn a_button_pin_may_leave_its_direction_undeclared() {
        let src = BUTTON.replace(
            "<gpio id=\"gpio9\" pin=\"9\" direction=\"input\" />",
            "<gpio id=\"gpio9\" pin=\"9\" />",
        );
        let board = lower_src(&src).expect("no direction is allowed");
        assert_eq!(board.components.len(), 2);
    }

    #[test]
    fn an_output_gpio_cannot_carry_a_button() {
        let src = BUTTON.replace(
            "<gpio id=\"gpio9\" pin=\"9\" direction=\"input\" />",
            "<gpio id=\"gpio9\" pin=\"9\" direction=\"output\" />",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, codes::COMPONENT_WRONG_DIRECTION);
        assert_eq!(errors[0].message, "wrong gpio direction");
        assert_eq!(
            errors[0].labels[0].text,
            "<button id=\"button\"> needs an input pin, but `gpio9` has direction=\"output\""
        );
    }

    #[test]
    fn pull_up_defaults_to_false_and_must_be_a_flag() {
        let src = BUTTON.replace(" pull_up=\"true\"", "");
        let board = lower_src(&src).expect("pull_up defaults");
        let Component::Recipe(button) = &board.components[1] else {
            panic!("the second component is the button");
        };
        assert_eq!(button.tag, "button");
        assert_eq!(button.flags, [false]);

        let src = BUTTON.replace("pull_up=\"true\"", "pull_up=\"sideways\"");
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_INVALID_BUTTON);
        assert_eq!(errors[0].message, "invalid button");
        assert_eq!(
            errors[0].labels[0].text,
            "<button id=\"button\"> pull_up=\"sideways\" must be \"true\" or \"false\""
        );
    }

    #[test]
    fn a_button_needs_a_gpio_that_exists_and_an_id_that_becomes_an_identifier() {
        let errors = lower_src(&BUTTON.replace("$gpio9", "$gpio99")).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_UNKNOWN_GPIO);
        assert_eq!(errors[0].message, "unknown gpio");
        assert_eq!(
            errors[0].labels[0].text,
            "<button id=\"button\"> refers to `gpio99`, which is not a <gpio> in `# peripherals`"
        );

        let errors = lower_src(&BUTTON.replace("id=\"button\"", "id=\"9btn\"")).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_INVALID_BUTTON);
        assert_eq!(errors[0].message, "invalid button");
        assert!(errors[0].labels[0].text.contains("Rust identifier"));
    }

    #[test]
    fn two_buttons_on_one_pin_are_e0702() {
        let src = BUTTON.replace(
            "  <button id=\"button\" gpio=\"$gpio9\" pull_up=\"true\" />\n",
            "  <button id=\"button\" gpio=\"$gpio9\" pull_up=\"true\" />\n  <button id=\"other\" gpio=\"$gpio9\" />\n",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, codes::CONFLICT_PIN_USED);
        assert_eq!(errors[0].message, "pin already used");
        assert_eq!(
            errors[0].labels[0].text,
            "<button id=\"button\"> and <button id=\"other\"> both use GPIO9"
        );
    }

    /// The legacy `conflict-led-vs-button` snapshot: a led (output) and a
    /// button (input) on one pin are a direction clash, not a plain conflict.
    #[test]
    fn a_led_and_a_button_on_one_pin_are_a_direction_clash() {
        let src = BUTTON
            .replace(
                "  <led id=\"red_led\" gpio=\"$gpio7\" active_low=\"false\" />\n  <button id=\"button\" gpio=\"$gpio9\" pull_up=\"true\" />\n",
                "  <led id=\"red_led\" gpio=\"$gpio9\" active_low=\"false\" />\n  <button id=\"btn\" gpio=\"$gpio9\" pull_up=\"true\" />\n",
            )
            // The legacy snapshot leaves the direction undeclared, so the
            // clash is the pin claim's, not a wrong-direction error.
            .replace(
                "<gpio id=\"gpio9\" pin=\"9\" direction=\"input\" />",
                "<gpio id=\"gpio9\" pin=\"9\" />",
            );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, codes::CONFLICT_DIRECTION);
        assert_eq!(errors[0].message, "pin direction clash");
        assert_eq!(
            errors[0].labels[0].text,
            "<led id=\"red_led\"> and <button id=\"btn\">: one pin cannot be both input and output"
        );
    }

    #[test]
    fn button_is_not_a_peripheral() {
        let src = BUTTON.replace(
            "# peripherals\n",
            "# peripherals\n  <button id=\"extra\" gpio=\"$gpio7\" />\n",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::PERIPHERAL_UNSUPPORTED);
        assert_eq!(
            errors[0].message,
            "`<button>` is not a known element in `# peripherals`; known: `<gpio>`, `<i2c>`"
        );
    }

    const I2C: &str = "# espforge\n\
        \x20 <project name=\"i2c\" />\n\
        \x20 <chip type=\"esp32c3\" />\n\
        \x20 <runtime type=\"blocking\" />\n\
        # peripherals\n\
        \x20 <i2c id=\"i2c0\" sda=\"6\" scl=\"7\" frequency=\"100kHz\" />\n\
        # components\n\
        \x20 <i2c_component id=\"i2c_tester\" i2c=\"$i2c0\" />\n\
        # devices\n\
        # services\n";

    #[test]
    fn i2c_lowers_to_a_bus_and_one_component() {
        let board = lower_src(I2C).expect("i2c lowers");
        assert_eq!(board.components.len(), 1);
        let Component::I2c(component) = &board.components[0] else {
            panic!("the component is an i2c component");
        };
        assert_eq!(component.id.raw, "i2c_tester");
        let Bus::I2c {
            id,
            peripheral,
            sda,
            scl,
            khz,
            used_by,
        } = &board.resources.buses()[0]
        else {
            panic!("the bus is an i2c bus");
        };
        assert_eq!(id.raw, "i2c0");
        assert_eq!(peripheral, "I2C0");
        assert_eq!((sda.pin().0, scl.pin().0, *khz), (6, 7, 100));
        assert_eq!(
            used_by.as_ref().map(|id| id.raw.as_str()),
            Some("i2c_tester")
        );
        assert_eq!(
            board.resources.claim_of(Pin(6)).unwrap().need,
            PinUse::Bidirectional
        );
        assert_eq!(
            board.resources.claim_of(Pin(7)).unwrap().need,
            PinUse::Bidirectional
        );
    }

    #[test]
    fn i2c_ids_are_the_three_esp_hal_peripherals() {
        let errors = lower_src(&I2C.replace("id=\"i2c0\"", "id=\"bus\"")).unwrap_err();
        assert_eq!(errors[0].code, codes::I2C_INVALID_ID);
        assert_eq!(errors[0].message, "invalid i2c id");
        assert_eq!(
            errors[0].labels[0].text,
            "<i2c id=\"bus\">: the id must be i2c0, i2c1 or i2c2"
        );
    }

    #[test]
    fn an_i2c_frequency_must_be_khz() {
        let errors =
            lower_src(&I2C.replace("frequency=\"100kHz\"", "frequency=\"fast\"")).unwrap_err();
        assert_eq!(errors[0].code, codes::I2C_INVALID_FREQUENCY);
        assert_eq!(errors[0].message, "invalid i2c frequency");
        assert_eq!(
            errors[0].labels[0].text,
            "<i2c id=\"i2c0\"> frequency=\"fast\" is not a frequency: use e.g. `100kHz`"
        );
    }

    #[test]
    fn i2c_sda_and_scl_on_one_pin_are_i2c_wiring() {
        let errors = lower_src(&I2C.replace("scl=\"7\"", "scl=\"6\"")).unwrap_err();
        assert_eq!(errors[0].code, codes::CONFLICT_I2C_WIRING);
        assert_eq!(errors[0].message, "i2c wiring");
        assert_eq!(
            errors[0].labels[0].text,
            "<i2c id=\"i2c0\"> sda and scl cannot both be GPIO6"
        );
    }

    #[test]
    fn a_bus_pin_on_a_declared_gpio_points_at_the_gpio() {
        let src = I2C.replace(
            "  <i2c id=\"i2c0\"",
            "  <gpio id=\"g\" pin=\"6\" />\n  <i2c id=\"i2c0\"",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::CONFLICT_PIN);
        assert_eq!(errors[0].message, "pin conflict");
        assert_eq!(
            errors[0].labels[0].text,
            "<gpio id=\"g\"> and <i2c id=\"i2c0\"> both use GPIO6"
        );
    }

    #[test]
    fn one_i2c_bus_takes_exactly_one_component() {
        let src = I2C.replace(
            "  <i2c_component id=\"i2c_tester\" i2c=\"$i2c0\" />\n",
            "  <i2c_component id=\"i2c_tester\" i2c=\"$i2c0\" />\n  <i2c_component id=\"other\" i2c=\"$i2c0\" />\n",
        );
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_I2C_BUS_USED);
        assert_eq!(errors[0].message, "i2c bus already used");
        assert_eq!(
            errors[0].labels[0].text,
            "<i2c_component id=\"i2c_tester\"> and <i2c_component id=\"other\"> both use <i2c id=\"i2c0\">"
        );
    }

    #[test]
    fn an_unknown_i2c_reference_is_reported() {
        let errors = lower_src(&I2C.replace("$i2c0", "$i2c9")).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_UNKNOWN_BUS);
        assert_eq!(errors[0].message, "unknown i2c bus");
        assert_eq!(
            errors[0].labels[0].text,
            "<i2c_component id=\"i2c_tester\"> refers to `i2c9`, which is not an <i2c> in `# peripherals`"
        );
    }

    #[test]
    fn an_i2c_address_must_parse() {
        let src = I2C.replace("i2c=\"$i2c0\" />", "i2c=\"$i2c0\" address=\"0xZZ\" />");
        let errors = lower_src(&src).unwrap_err();
        assert_eq!(errors[0].code, codes::COMPONENT_INVALID_I2C);
        assert_eq!(errors[0].message, "invalid i2c_component");
        assert_eq!(
            errors[0].labels[0].text,
            "❌ [i2c_component] 'i2c_tester' invalid hex address '0xZZ'"
        );
    }

    #[test]
    fn the_gpio_direction_is_carried_as_a_domain_value() {
        let src = BLINK.replace("direction=\"output\"", "direction=\"sideways\"");
        let parsed = parse_espf(&src);
        let mut cx = LowerCx::new();
        lower_section("peripherals", &parsed.sections["peripherals"], &mut cx);
        assert!(cx.gpio("gpio18").is_none(), "nothing is recorded on error");

        let parsed = parse_espf(BLINK);
        let mut cx = LowerCx::new();
        lower_section("peripherals", &parsed.sections["peripherals"], &mut cx);
        assert_eq!(
            cx.gpio("gpio18").unwrap().direction,
            Some(Direction::Output)
        );
    }
}
