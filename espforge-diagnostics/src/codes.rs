//! The stable diagnostic codes this slice can reach, one per message.
//!
//! The ranges are reserved by stage (see the crate docs). A new message gets a
//! new entry here; the numbers are the legacy ones, never renumbered.
//!
//! Codes are data: the human renderer does not print them.

use crate::DiagCode;

macro_rules! codes {
    ($($(#[$doc:meta])* $name:ident = $code:literal;)*) => {
        $(
            $(#[$doc])*
            pub const $name: DiagCode = DiagCode::new($code);
        )*

        /// Every code defined in this module, in declaration order.
        pub const ALL: &[DiagCode] = &[$($name),*];
    };
}

codes! {
    /// A parser-junk line that is not a node.
    SYNTAX_NODE_LINE = "E0101";
    /// A line that should have been a `# section` header.
    SYNTAX_SECTION_HEADER = "E0102";
    /// Two `# section` blocks with the same name.
    STRUCTURE_DUPLICATE_SECTION = "E0103";
    /// A required `# section` block is absent.
    STRUCTURE_MISSING_SECTION = "E0104";

    /// An element the `# espforge` schema does not know.
    CORE_UNKNOWN_ELEMENT = "E0201";
    /// `# espforge` lacks `<project>`, `<chip>` or `<runtime>`.
    CORE_INCOMPLETE = "E0202";
    /// The same element is written twice in `# espforge`.
    CORE_DUPLICATE_ELEMENT = "E0203";
    /// `<chip>` without a `type` attribute.
    CORE_CHIP_WITHOUT_TYPE = "E0204";
    /// A chip name the catalog does not know.
    CORE_UNKNOWN_CHIP = "E0205";
    /// `<runtime>` without a `type` attribute.
    CORE_RUNTIME_WITHOUT_TYPE = "E0206";
    /// A runtime name the catalog does not know.
    CORE_UNKNOWN_RUNTIME = "E0207";
    /// `<console>` without a `type` attribute.
    CORE_CONSOLE_WITHOUT_TYPE = "E0208";
    /// A console name the catalog does not know.
    CORE_UNKNOWN_CONSOLE = "E0209";

    /// An option name that is not in the schema.
    OPTION_UNKNOWN = "E0301";
    /// An option value outside its declared choices.
    OPTION_INVALID_VALUE = "E0302";
    /// An option value that cannot be set in `<options>`.
    OPTION_DERIVED = "E0306";
    /// An option that needs another option that is off.
    OPTION_REQUIRES = "E0307";
    /// An option that the selected chip cannot provide.
    OPTION_UNAVAILABLE = "E0308";
    /// Two options in one selection group are on together.
    OPTION_MUTUALLY_EXCLUSIVE = "E0309";
    /// The selected console does not exist on the chip.
    OPTION_NO_CONSOLE = "E0310";
    /// An attribute value with the wrong type.
    VALUE_WRONG_TYPE = "E0311";
    /// A value that should be a boolean.
    VALUE_NOT_BOOLEAN = "E0312";
    /// A value that should be an unsigned number.
    VALUE_NOT_NUMBER = "E0313";
    /// A key used both as a plain value and as an object.
    VALUE_KEY_KIND = "E0314";
    /// A required attribute is absent.
    VALUE_MISSING_ATTRIBUTE = "E0315";

    /// A peripheral tag no element implements.
    PERIPHERAL_UNSUPPORTED = "E0501";
    /// `<gpio pin>` that is not a number.
    GPIO_INVALID_PIN = "E0502";
    /// `<gpio direction>` outside `input`/`output`.
    GPIO_INVALID_DIRECTION = "E0503";

    /// An `<led>` that cannot be built.
    COMPONENT_INVALID_LED = "E0601";
    /// A `<button>` that cannot be built.
    COMPONENT_INVALID_BUTTON = "E0602";
    /// A component tag no element implements.
    COMPONENT_UNSUPPORTED = "E0606";
    /// An `<led>` referencing a gpio id that does not exist.
    COMPONENT_UNKNOWN_GPIO = "E0608";
    /// A led on an input pin.
    COMPONENT_WRONG_DIRECTION = "E0609";

    /// A pin claimed by two elements of the same kind.
    CONFLICT_PIN_USED = "E0702";
    /// One element needs an input and the other an output.
    CONFLICT_DIRECTION = "E0703";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well_formed(code: &str) -> bool {
        let bytes = code.as_bytes();
        bytes.len() == 5
            && matches!(bytes[0], b'E' | b'W')
            && bytes[1..].iter().all(u8::is_ascii_digit)
    }

    #[test]
    fn every_code_is_well_formed() {
        for code in ALL {
            assert!(
                well_formed(code.as_str()),
                "{} is not `E`/`W` + four digits",
                code.as_str()
            );
        }
    }

    #[test]
    fn no_two_codes_share_a_number() {
        let mut seen = std::collections::BTreeSet::new();
        for code in ALL {
            assert!(
                seen.insert(code.as_str()),
                "{} is declared twice",
                code.as_str()
            );
        }
    }

    #[test]
    fn the_slice_only_defines_the_codes_it_can_reach() {
        for code in ALL {
            let number = &code.as_str()[1..];
            let n: u32 = number.parse().unwrap();
            assert!(
                matches!(
                    n,
                    101..=104 | 201..=209 | 301 | 302 | 306..=315 | 501..=503 | 601 | 602 | 606 | 608 | 609 | 702 | 703
                ),
                "{} is outside the codes this slice defines",
                code.as_str()
            );
        }
    }
}
