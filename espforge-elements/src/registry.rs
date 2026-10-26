//! The element registry: one distributed slice, no hand-kept list.
//!
//! Each element module declares its own `#[distributed_slice(ELEMENTS)]`, so
//! registering an element never edits this file. `espforge-core` references
//! this crate explicitly, and the "registry is non-empty" test guards the
//! other failure mode: a slice the linker drops.
//!
//! There is a second door for the recipe tier: a schema element with a
//! `recipe` needs no Rust file, so [`element`] falls back to
//! [`recipes`], one interpreter per recipe element in schema order. The
//! coherence tests keep the two doors from disagreeing.

use std::sync::OnceLock;

use espforge_schema::schema;
use linkme::distributed_slice;

use crate::components::recipe::RecipeElement;
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

/// One [`RecipeElement`] per schema element that declares a `recipe`, in
/// schema order.
///
/// Built once from the schema: a recipe is data, and the element that reads it
/// is one struct that knows no tag.
pub fn recipes() -> &'static [RecipeElement] {
    static RECIPES: OnceLock<Vec<RecipeElement>> = OnceLock::new();
    RECIPES.get_or_init(|| {
        schema()
            .elements
            .iter()
            .filter(|element| element.recipe.is_some())
            .filter(|element| matches!(element.section.as_str(), "peripherals" | "components"))
            .map(RecipeElement::new)
            .collect()
    })
}

/// The registered element for `tag`, if any: a hand-written element first,
/// then a recipe from the schema.
pub fn element(tag: &str) -> Option<&'static dyn Element> {
    ELEMENTS
        .iter()
        .copied()
        .find(|element| element.tag() == tag)
        .or_else(|| {
            recipes()
                .iter()
                .find(|element| element.tag() == tag)
                .map(|element| element as &'static dyn Element)
        })
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
    fn the_recipe_door_reaches_led() {
        let led = element("led").expect("a recipe element is registered");
        assert_eq!(led.tag(), "led");
        assert!(
            recipes().iter().any(|element| element.tag() == "led"),
            "`<led>` is a recipe, not a hand-written element"
        );
    }

    /// An element must have exactly one implementation: a hand-written
    /// `Element` in the slice, or a `recipe` in the schema — never both, never
    /// neither.
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
            let hand_written = elements()
                .iter()
                .filter(|element| element.tag() == schema_element.tag)
                .count();
            let recipe = usize::from(schema_element.recipe.is_some());
            assert_eq!(
                hand_written + recipe,
                1,
                "`<{}>` has {hand_written} hand-written implementation(s) and {recipe} recipe(s): \
                 give it exactly one, so there is nothing to prefer",
                schema_element.tag,
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
