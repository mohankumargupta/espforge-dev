//! The board IR: what the `# espforge` section says, and what lowering built.
//!
//! A [`Board`] carries its [`Meta`], the [`ResourceGraph`] of pins it owns and
//! the components on them. A [`ClaimedPin`] can only be made by
//! [`ResourceGraph::claim`], so a board that holds one cannot have a pin
//! conflict: validation and construction are the same act.
//!
//! The graph is cut to what the elements need. Buses (`add_bus`, `bus`,
//! `used_by`) return in phase 3, shared peripherals (`claim_peripheral`) with
//! the elements that share them, and the bus kinds join [`OwnerKind`] with
//! their buses. Recipe components share the one [`OwnerKind::Recipe`] variant.

use std::collections::BTreeMap;
use std::ops::Range;

/// Which async model the generated project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Blocking,
    Embassy,
}

/// Where `print!`/log output goes (esp-println's backend).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Console {
    /// Plain UART. Works on every chip.
    #[default]
    Uart,
    /// Built-in USB-Serial-JTAG peripheral (not on the original ESP32).
    JtagSerial,
}

/// Contents of the `# espforge` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub name: String,
    pub description: Option<String>,
    pub chip: String,
    pub runtime: Runtime,
    pub console: Console,
}

impl Meta {
    /// The project name as a valid crate/package identifier
    /// (`"example project"` -> `example_project`).
    pub fn crate_name(&self) -> String {
        let slug: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let slug = slug.trim_matches('_').to_string();
        if slug.is_empty() {
            "espforge_project".to_string()
        } else if slug.starts_with(|c: char| c.is_ascii_digit()) {
            format!("p_{slug}")
        } else {
            slug
        }
    }
}

/// An element id: the text as written, plus where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// The id exactly as written (`red-led`).
    pub raw: String,
    /// Span of the id attribute's value.
    pub span: Range<usize>,
}

impl Ident {
    pub fn new(raw: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            raw: raw.into(),
            span,
        }
    }

    /// The id as written.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The id as a Rust identifier (`red-led` -> `red_led`).
    pub fn rust_name(&self) -> String {
        self.raw.replace('-', "_")
    }
}

/// A GPIO number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pin(pub u8);

/// What a pin is asked to do. Two claims that demand opposite directions are a
/// wiring error; anything else on an occupied pin is a plain conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinUse {
    Input,
    Output,
    /// A bus pin that both sides drive (I2C SDA/SCL) or that the peripheral
    /// samples at its own rate (SPI MISO).
    Bidirectional,
    /// No declared direction (`<gpio>` without `direction`).
    Unspecified,
}

impl PinUse {
    /// Whether a claim for `self` and one for `other` cannot share a pin
    /// because one needs an input and the other an output.
    pub fn opposes(self, other: Self) -> bool {
        matches!(
            (self, other),
            (PinUse::Input, PinUse::Output) | (PinUse::Output, PinUse::Input)
        )
    }
}

/// The element kinds that can own a resource. The set is closed on purpose:
/// adding `<pwm>` adds one variant, not a string tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OwnerKind {
    Gpio,
    /// A component built from a recipe; carries its schema tag.
    Recipe(&'static str),
    // The bus kinds arrive with their buses.
}

impl OwnerKind {
    /// The schema tag spelling.
    pub fn tag(self) -> &'static str {
        match self {
            OwnerKind::Gpio => "gpio",
            OwnerKind::Recipe(tag) => tag,
        }
    }
}

/// Who claims a resource: its kind and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub kind: OwnerKind,
    pub id: Ident,
}

impl Owner {
    pub fn new(kind: OwnerKind, id: Ident) -> Self {
        Self { kind, id }
    }
}

/// One recorded claim: the owner, the node span a diagnostic points at, and
/// what the pin is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub owner: Owner,
    /// The owning element's node span (`<led ...>`), not the id value.
    pub span: Range<usize>,
    pub need: PinUse,
}

/// A pin that is owned by exactly one claim.
///
/// Fields are private and there is no public constructor: the only way to get
/// one is [`ResourceGraph::claim`], which is what makes an invalid `Board`
/// unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedPin {
    pin: Pin,
    claim: Claim,
}

impl ClaimedPin {
    pub fn pin(&self) -> Pin {
        self.pin
    }

    pub fn owner(&self) -> &Owner {
        &self.claim.owner
    }

    pub fn claim(&self) -> &Claim {
        &self.claim
    }
}

/// Why a second claim on one pin was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// Two elements want the same pin.
    Pin,
    /// One needs an input and the other an output.
    Direction,
    /// One element claims the same pin twice (a UART whose tx and rx match).
    SelfClash,
}

/// A refused pin claim, carrying both sides so the lowering code can keep the
/// exact wording users see today (which side is underlined, which is named
/// first) without re-checking ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub pin: Pin,
    /// The claim that already owns the pin.
    pub existing: Claim,
    /// The claim that was refused.
    pub incoming: Claim,
}

/// Pins: the resources a board owns.
///
/// Buses and shared peripherals return in phase 3, with the elements that
/// need them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceGraph {
    pins: BTreeMap<Pin, Claim>,
}

impl ResourceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim `pin` for `owner`.
    ///
    /// A free pin is recorded and returned. An occupied one is refused, and
    /// the graph is unchanged: a failed claim never steals the pin from the
    /// first owner.
    pub fn claim(
        &mut self,
        pin: Pin,
        owner: Owner,
        span: Range<usize>,
        need: PinUse,
    ) -> Result<ClaimedPin, Conflict> {
        let incoming = Claim { owner, span, need };

        if let Some(existing) = self.pins.get(&pin) {
            let kind = if existing.owner.kind == incoming.owner.kind
                && existing.owner.id == incoming.owner.id
            {
                ConflictKind::SelfClash
            } else if existing.need.opposes(incoming.need) {
                ConflictKind::Direction
            } else {
                ConflictKind::Pin
            };
            return Err(Conflict {
                kind,
                pin,
                existing: existing.clone(),
                incoming,
            });
        }

        self.pins.insert(pin, incoming.clone());
        Ok(ClaimedPin {
            pin,
            claim: incoming,
        })
    }

    /// The claim on `pin`, if any.
    pub fn claim_of(&self, pin: Pin) -> Option<&Claim> {
        self.pins.get(&pin)
    }

    /// Every pin claim, in pin order (the map is a `BTreeMap`).
    pub fn pin_claims(&self) -> impl Iterator<Item = (Pin, &Claim)> {
        self.pins.iter().map(|(&pin, claim)| (pin, claim))
    }
}

/// The `direction` a `<gpio>` declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Output,
    Input,
}

/// A lowered `<gpio>`: an id, a pin, and the declared direction, if any.
///
/// It claims nothing; the first component that uses the pin does, as in the
/// legacy lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpioDecl {
    pub id: Ident,
    pub pin: u8,
    pub direction: Option<Direction>,
}

/// One line per component variant generates [`Component::id`] and the
/// [`Lowered`](crate::Lowered) forwarding, so adding a component is its own
/// file plus its own line here.
macro_rules! component_enum {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        /// A component in the board.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Component {
            $($variant($ty)),*
        }

        impl Component {
            /// The component's id.
            pub fn id(&self) -> &Ident {
                match self {
                    $(Component::$variant(c) => crate::element::Lowered::id(c),)*
                }
            }
        }

        impl crate::element::Lowered for Component {
            fn id(&self) -> &Ident {
                self.id()
            }

            fn emit(&self, board: &Board, cx: &mut crate::element::EmitCx) {
                match self {
                    $(Component::$variant(c) => c.emit(board, cx),)*
                }
            }

            fn overlay_attr(&self, attr: &str) -> Option<String> {
                match self {
                    $(Component::$variant(c) => c.overlay_attr(attr),)*
                }
            }
        }
    };
}

component_enum! {
    Recipe(crate::components::recipe::RecipeComponent),
}

/// Everything the emitters need: the project settings, the resources it owns
/// and the components on them.
#[derive(Debug, Clone)]
pub struct Board {
    pub meta: Meta,
    pub resources: ResourceGraph,
    pub components: Vec<Component>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(raw: &str, at: usize) -> Ident {
        Ident::new(raw, at..at + raw.len())
    }

    fn owner(kind: OwnerKind, raw: &str, at: usize) -> Owner {
        Owner::new(kind, id(raw, at))
    }

    fn meta(name: &str) -> Meta {
        Meta {
            name: name.into(),
            description: None,
            chip: "esp32c3".into(),
            runtime: Runtime::Blocking,
            console: Console::default(),
        }
    }

    /// `<led ...>` at `at` as the diagnostics corpus would see it.
    fn led_claim(
        g: &mut ResourceGraph,
        pin: u8,
        name: &str,
        at: usize,
    ) -> Result<ClaimedPin, Conflict> {
        g.claim(
            Pin(pin),
            owner(OwnerKind::Recipe("led"), name, at + 1),
            at..at + 4 + name.len(),
            PinUse::Output,
        )
    }

    #[test]
    fn crate_name_is_slugged() {
        assert_eq!(meta("example project").crate_name(), "example_project");
        assert_eq!(meta("Hello-World").crate_name(), "hello_world");
        assert_eq!(meta("3d").crate_name(), "p_3d");
        assert_eq!(meta("!!!").crate_name(), "espforge_project");
    }

    #[test]
    fn console_defaults_to_uart() {
        assert_eq!(Console::default(), Console::Uart);
    }

    #[test]
    fn ident_rust_name_replaces_dashes() {
        assert_eq!(Ident::new("red-led", 0..0).rust_name(), "red_led");
        assert_eq!(Ident::new("status", 0..0).as_str(), "status");
    }

    #[test]
    fn claiming_a_free_pin_records_its_owner_and_span() {
        let mut g = ResourceGraph::new();
        let p = led_claim(&mut g, 18, "red-led", 40).unwrap();

        assert_eq!(p.pin(), Pin(18));
        assert_eq!(p.owner().kind, OwnerKind::Recipe("led"));
        assert_eq!(p.owner().id.raw, "red-led");
        assert_eq!(p.owner().id.span, 41..48);
        assert_eq!(p.claim().span, 40..51);
        assert_eq!(p.claim().need, PinUse::Output);
        assert_eq!(g.claim_of(Pin(18)).unwrap().owner.id.raw, "red-led");
    }

    #[test]
    fn a_second_claim_names_both_owners_and_keeps_the_first() {
        let mut g = ResourceGraph::new();
        led_claim(&mut g, 18, "red_led", 10).unwrap();
        let conflict = led_claim(&mut g, 18, "other", 50).unwrap_err();

        assert_eq!(conflict.kind, ConflictKind::Pin);
        assert_eq!(conflict.pin, Pin(18));
        assert_eq!(conflict.existing.owner.id.raw, "red_led");
        assert_eq!(conflict.incoming.owner.id.raw, "other");
        assert_eq!(conflict.existing.span, 10..21);
        assert_eq!(conflict.incoming.span, 50..59);
        // The failed claim did not steal the pin.
        assert_eq!(g.claim_of(Pin(18)).unwrap().owner.id.raw, "red_led");
    }

    #[test]
    fn opposite_directions_are_a_direction_clash() {
        let mut g = ResourceGraph::new();
        led_claim(&mut g, 18, "red_led", 10).unwrap();
        let conflict = g
            .claim(
                Pin(18),
                owner(OwnerKind::Gpio, "input_gpio", 40),
                40..47,
                PinUse::Input,
            )
            .unwrap_err();

        assert_eq!(conflict.kind, ConflictKind::Direction);
        assert_eq!(conflict.existing.owner.kind, OwnerKind::Recipe("led"));
        assert_eq!(conflict.incoming.owner.kind, OwnerKind::Gpio);
    }

    #[test]
    fn one_element_claiming_the_same_pin_twice_is_a_self_clash() {
        let mut g = ResourceGraph::new();
        let owner = owner(OwnerKind::Gpio, "gpio5", 10);
        g.claim(Pin(5), owner.clone(), 10..35, PinUse::Unspecified)
            .unwrap();
        let conflict = g
            .claim(Pin(5), owner, 10..35, PinUse::Unspecified)
            .unwrap_err();

        assert_eq!(conflict.kind, ConflictKind::SelfClash);
        assert_eq!(conflict.existing.owner.id.raw, "gpio5");
        assert_eq!(conflict.incoming.owner.id.raw, "gpio5");
    }

    /// The two ownership cases `blink` can reach, expressed as claims. The
    /// graph detects them and keeps both sides; the element's wording stays
    /// with the lowering that knows the element.
    #[test]
    fn led_conflicts_are_expressed_as_claims() {
        let mut g = ResourceGraph::new();
        led_claim(&mut g, 18, "red_led", 10).unwrap();
        let conflict = led_claim(&mut g, 18, "other", 50).unwrap_err();

        assert_eq!(conflict.kind, ConflictKind::Pin);
        assert_eq!(
            (
                conflict.existing.owner.kind,
                conflict.existing.owner.id.raw.as_str()
            ),
            (OwnerKind::Recipe("led"), "red_led")
        );
        assert_eq!(
            (
                conflict.incoming.owner.kind,
                conflict.incoming.owner.id.raw.as_str()
            ),
            (OwnerKind::Recipe("led"), "other")
        );
    }

    #[test]
    fn pin_claims_are_listed_in_pin_order() {
        let mut g = ResourceGraph::new();
        led_claim(&mut g, 18, "red_led", 10).unwrap();
        led_claim(&mut g, 4, "blue_led", 40).unwrap();

        let pins: Vec<u8> = g.pin_claims().map(|(pin, _)| pin.0).collect();
        assert_eq!(pins, [4, 18]);
    }

    #[test]
    fn a_board_holds_its_resources() {
        let mut resources = ResourceGraph::new();
        led_claim(&mut resources, 18, "red-led", 10).unwrap();
        let board = Board {
            meta: meta("blink"),
            resources,
            components: Vec::new(),
        };

        assert!(board.components.is_empty());
        assert_eq!(board.resources.claim_of(Pin(18)).unwrap().span, 10..21);
    }
}
