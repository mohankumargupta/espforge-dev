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
    use espforge_schema::{AttrDef, AttrKind, schema};

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
    /// neither. Pure, so the "two doors" failure is exercised without adding a
    /// real element.
    fn implementation_problem(
        element: &espforge_schema::Element,
        hand_written: &[&str],
    ) -> Option<String> {
        let registered = hand_written
            .iter()
            .filter(|tag| **tag == element.tag.as_str())
            .count();
        match (registered, element.recipe.is_some()) {
            (1, false) | (0, true) => None,
            (0, false) => Some(format!(
                "`<{}>` has no implementation: add a `recipe` to its schema block, or \
                 `espforge-elements/src/{}/{}.rs`",
                element.tag,
                element.section,
                element.tag.replace('-', "_"),
            )),
            _ => Some(format!(
                "`<{}>` has {registered} hand-written implementation(s) and a `recipe`: delete \
                 `espforge-elements/src/{}/{}.rs`, its schema block already has a `recipe`",
                element.tag,
                element.section,
                element.tag.replace('-', "_"),
            )),
        }
    }

    #[test]
    fn every_schema_element_has_exactly_one_implementation() {
        let schema = schema();
        let hand_written: Vec<&str> = elements().iter().map(|element| element.tag()).collect();
        for element in &schema.elements {
            // Only sections that lower have elements; `# espforge` is read by
            // the core reader, not by the registry.
            if !matches!(element.section.as_str(), "peripherals" | "components") {
                continue;
            }
            if let Some(problem) = implementation_problem(element, &hand_written) {
                panic!("{problem}");
            }
        }
    }

    /// A component with a `recipe` *and* a registered implementation has two
    /// doors and must be caught, naming the file to delete.
    #[test]
    fn a_recipe_beside_a_rust_file_is_refused() {
        let schema = schema();
        let led = schema.element("led").expect("`<led>` is a schema element");
        let problem = implementation_problem(led, &["led"]).expect("two doors is a problem");
        assert!(
            problem.contains("delete `espforge-elements/src/components/led.rs`"),
            "{problem}"
        );
        assert!(problem.contains("already has a `recipe`"), "{problem}");
        // The recipe door alone is the whole implementation.
        assert!(implementation_problem(led, &[]).is_none());
    }

    /// An `AttrDef` of `kind`, for the shape predicate's own unit test.
    fn attr(name: &str, kind: AttrKind) -> AttrDef {
        AttrDef {
            name: name.into(),
            kind,
            required: false,
            default: None,
            range: None,
            help: None,
            span: 0..0,
        }
    }

    /// The shape a recipe describes: an `id`, exactly one `ref` and booleans.
    fn is_recipe_shaped(attrs: &[AttrDef]) -> bool {
        let mut idents = 0;
        let mut refs = 0;
        for attr in attrs {
            match attr.kind {
                AttrKind::Ident => idents += 1,
                AttrKind::Bool => {}
                AttrKind::Ref { .. } => refs += 1,
                _ => return false,
            }
        }
        idents == 1 && refs == 1
    }

    /// Why a `# components` element that is recipe-shaped but not a recipe is a
    /// problem, if it is. The predicate has its own unit test below, so the
    /// failure path is exercised without adding a real element.
    fn tier_problem(tag: &str, attrs: &[AttrDef], has_recipe: bool) -> Option<String> {
        (is_recipe_shaped(attrs) && !has_recipe).then(|| {
            format!(
                "`<{tag}>` is a recipe component: add `recipe …` to its schema block and delete \
                 `components/{tag}.rs`"
            )
        })
    }

    #[test]
    fn a_recipe_shaped_component_without_a_recipe_fails() {
        let recipe_shaped = [
            attr("id", AttrKind::Ident),
            attr(
                "gpio",
                AttrKind::Ref {
                    to: vec!["gpio".into()],
                },
            ),
            attr("active_low", AttrKind::Bool),
        ];
        assert!(is_recipe_shaped(&recipe_shaped));
        assert_eq!(
            tier_problem("buzzer", &recipe_shaped, false).unwrap(),
            "`<buzzer>` is a recipe component: add `recipe …` to its schema block and delete \
             `components/buzzer.rs`"
        );
        // A real recipe is not a problem.
        assert!(tier_problem("buzzer", &recipe_shaped, true).is_none());

        // A bus component with an integer address is out of shape.
        let bus = [
            attr("id", AttrKind::Ident),
            attr(
                "i2c",
                AttrKind::Ref {
                    to: vec!["i2c".into()],
                },
            ),
            attr("address", AttrKind::U32),
        ];
        assert!(!is_recipe_shaped(&bus));
        assert!(tier_problem("screen", &bus, false).is_none());

        // Two refs is not one pin.
        let two_refs = [
            attr("id", AttrKind::Ident),
            attr(
                "a",
                AttrKind::Ref {
                    to: vec!["gpio".into()],
                },
            ),
            attr(
                "b",
                AttrKind::Ref {
                    to: vec!["gpio".into()],
                },
            ),
        ];
        assert!(!is_recipe_shaped(&two_refs));
    }

    /// The real schema has to obey the tier: every recipe-shaped component is a
    /// recipe.
    #[test]
    fn every_recipe_shaped_component_declares_a_recipe() {
        let schema = schema();
        for element in schema.elements_in("components") {
            if let Some(problem) =
                tier_problem(&element.tag, &element.attrs, element.recipe.is_some())
            {
                panic!("{problem}");
            }
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
