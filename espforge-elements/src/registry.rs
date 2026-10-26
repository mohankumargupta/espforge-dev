//! The element registry: one distributed slice, no hand-kept list.
//!
//! Each element module declares its own `#[distributed_slice(ELEMENTS)]`, so
//! registering an element never edits this file. `espforge-core` references
//! this crate explicitly, and the "registry is non-empty" test guards the
//! other failure mode: a slice the linker drops.

use linkme::distributed_slice;

use crate::element::Element;

/// Every registered element.
///
/// The order is the linker's, not the schema's: callers that need schema
/// order walk the schema and look the element up by tag.
#[distributed_slice]
pub static ELEMENTS: [&'static dyn Element];

/// Every registered element, as a slice.
pub fn elements() -> &'static [&'static dyn Element] {
    &ELEMENTS
}

/// The registered element for `tag`, if any.
pub fn element(tag: &str) -> Option<&'static dyn Element> {
    ELEMENTS
        .iter()
        .copied()
        .find(|element| element.tag() == tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use espforge_schema::schema;

    #[test]
    fn the_registry_is_not_empty() {
        // A `linkme` slice can be dead-stripped; this is the canary.
        assert!(
            !elements().is_empty(),
            "no elements are registered: the distributed slice was dropped"
        );
    }

    #[test]
    fn every_schema_element_has_exactly_one_implementation() {
        let schema = schema();
        for schema_element in &schema.elements {
            // Only sections that lower have elements; `# espforge` is read by
            // the core reader, not by the registry.
            if !matches!(
                schema_element.section.as_str(),
                "peripherals" | "components"
            ) {
                continue;
            }
            let registered: Vec<&str> = elements()
                .iter()
                .filter(|element| element.tag() == schema_element.tag)
                .map(|element| element.tag())
                .collect();
            assert_eq!(
                registered.len(),
                1,
                "`<{}>` is a `{}` element: add exactly one `Element` for it in \
                 espforge-elements/src/{}/{}.rs",
                schema_element.tag,
                schema_element.section,
                schema_element.section,
                schema_element.tag.replace('-', "_"),
            );
        }
    }

    #[test]
    fn every_registered_element_is_a_schema_element() {
        let schema = schema();
        for element in elements() {
            let tag = element.tag();
            let schema_element = schema.element(tag).unwrap_or_else(|| {
                panic!(
                    "`<{tag}>` is registered but not in espf.schema.kdl: add it to the schema \
                     or delete the element from espforge-elements/src"
                )
            });
            assert!(
                matches!(
                    schema_element.section.as_str(),
                    "peripherals" | "components"
                ),
                "`<{tag}>` is registered from a `<{}>` schema element, but only \
                 peripherals and components lower",
                schema_element.section
            );
        }
    }
}
