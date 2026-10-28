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

use espforge_diagnostics::Diagnostic;
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
    I2c,
    Spi,
    Uart,
    I2cComponent,
    SpiComponent,
    UartComponent,
    /// The on-chip Wi-Fi radio (`<wifi>`), which claims the `WIFI` block.
    Wifi,
}

impl OwnerKind {
    /// The schema tag spelling.
    pub fn tag(self) -> &'static str {
        match self {
            OwnerKind::Gpio => "gpio",
            OwnerKind::Recipe(tag) => tag,
            OwnerKind::I2c => "i2c",
            OwnerKind::Spi => "spi",
            OwnerKind::Uart => "uart",
            OwnerKind::I2cComponent => "i2c_component",
            OwnerKind::SpiComponent => "spi_component",
            OwnerKind::UartComponent => "uart_component",
            OwnerKind::Wifi => "wifi",
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

/// Where a bus lives in [`ResourceGraph::buses`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BusId(pub usize);

/// A bus and the pins it owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bus {
    I2c {
        id: Ident,
        /// Upper-cased esp-hal peripheral name (`I2C0`, `I2C1`).
        peripheral: String,
        sda: ClaimedPin,
        scl: ClaimedPin,
        /// Bus frequency in kHz.
        khz: u32,
        /// The component riding on this bus, once one is attached.
        used_by: Option<Ident>,
    },
    Spi {
        id: Ident,
        /// Upper-cased esp-hal peripheral name (`SPI2`).
        peripheral: String,
        sclk: ClaimedPin,
        mosi: ClaimedPin,
        /// `None` is a write-only bus (no MISO pin).
        miso: Option<ClaimedPin>,
        /// Bus frequency in kHz.
        khz: u32,
        mode: u8,
    },
    Uart {
        id: Ident,
        /// Upper-cased esp-hal peripheral name (`UART0`, `UART1`), which may
        /// differ from the id when `uart="UART1"` is written explicitly.
        peripheral: String,
        tx: ClaimedPin,
        rx: ClaimedPin,
        used_by: Option<Ident>,
    },
}

impl Bus {
    /// The bus id as the user wrote it.
    pub fn id(&self) -> &Ident {
        match self {
            Bus::I2c { id, .. } | Bus::Spi { id, .. } | Bus::Uart { id, .. } => id,
        }
    }
}

/// One claim on a named hardware peripheral (`UART1`, `I2C0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralClaim {
    pub owner: Owner,
    /// The claiming element's node span.
    pub span: Range<usize>,
}

/// Two elements claiming the same hardware peripheral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralClash {
    pub peripheral: String,
    pub existing: PeripheralClaim,
    pub incoming: PeripheralClaim,
}

/// Pins, peripherals and buses: the resources a board owns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceGraph {
    pins: BTreeMap<Pin, Claim>,
    peripherals: BTreeMap<String, PeripheralClaim>,
    buses: Vec<Bus>,
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

    /// Claim a named hardware peripheral (`UART1`, `I2C0`) for `owner`.
    ///
    /// A second claim is the "peripheral clash" case: the board drives at
    /// most one component per physical peripheral.
    pub fn claim_peripheral(
        &mut self,
        peripheral: impl Into<String>,
        owner: Owner,
        span: Range<usize>,
    ) -> Result<(), PeripheralClash> {
        let peripheral = peripheral.into();
        let incoming = PeripheralClaim { owner, span };
        if let Some(existing) = self.peripherals.get(&peripheral) {
            return Err(PeripheralClash {
                peripheral,
                existing: existing.clone(),
                incoming,
            });
        }
        self.peripherals.insert(peripheral, incoming);
        Ok(())
    }

    /// Append a bus and return its id. Bus ids are positional and assigned in
    /// construction order (source order).
    pub fn add_bus(&mut self, bus: Bus) -> BusId {
        let id = BusId(self.buses.len());
        self.buses.push(bus);
        id
    }

    pub fn bus(&self, id: BusId) -> Option<&Bus> {
        self.buses.get(id.0)
    }

    pub fn bus_mut(&mut self, id: BusId) -> Option<&mut Bus> {
        self.buses.get_mut(id.0)
    }

    pub fn buses(&self) -> &[Bus] {
        &self.buses
    }

    /// The bus whose id is `name`, in declaration order.
    pub fn bus_by_name(&self, name: &str) -> Option<BusId> {
        self.buses
            .iter()
            .position(|bus| bus.id().as_str() == name)
            .map(BusId)
    }

    /// The claim on `pin`, if any.
    pub fn claim_of(&self, pin: Pin) -> Option<&Claim> {
        self.pins.get(&pin)
    }

    /// Every pin claim, in pin order (the map is a `BTreeMap`).
    pub fn pin_claims(&self) -> impl Iterator<Item = (Pin, &Claim)> {
        self.pins.iter().map(|(&pin, claim)| (pin, claim))
    }

    /// Every peripheral claim, in name order.
    pub fn peripheral_claims(&self) -> impl Iterator<Item = (&str, &PeripheralClaim)> {
        self.peripherals
            .iter()
            .map(|(name, claim)| (name.as_str(), claim))
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
    /// The `<gpio ...>` node's span, for pin-conflict diagnostics.
    pub span: Range<usize>,
}

/// UART parity, in the IR's own vocabulary. The lowering maps the generated
/// schema choice onto this; the emitter maps it onto `esp_hal`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Parity {
    #[default]
    None,
    Even,
    Odd,
}

/// UART stop bits, in the IR's own vocabulary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StopBits {
    #[default]
    One,
    Two,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Framing {
    pub parity: Parity,
    pub stop_bits: StopBits,
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
    I2c(crate::components::i2c::I2cComponent),
    Spi(crate::components::spi::SpiComponent),
    Uart(crate::components::uart::UartComponent),
    WifiStation(crate::components::wifi_station::WifiStationComponent),
}

/// One line per service variant generates [`Service::id`] and the
/// [`Lowered`](crate::Lowered) forwarding, so a new service is its own file
/// plus its own line here — the same shape as `component_enum!`.
macro_rules! service_enum {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        /// A service in the board: software that rides on an interface, not a
        /// pin. Lowered after the components, so it can refer to one.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Service {
            $($variant($ty)),*
        }

        impl Service {
            /// The service's id.
            pub fn id(&self) -> &Ident {
                match self {
                    $(Service::$variant(s) => crate::element::Lowered::id(s),)*
                }
            }
        }

        impl crate::element::Lowered for Service {
            fn id(&self) -> &Ident {
                self.id()
            }

            fn emit(&self, board: &Board, cx: &mut crate::element::EmitCx) {
                match self {
                    $(Service::$variant(s) => s.emit(board, cx),)*
                }
            }
        }
    };
}

service_enum! {
    HttpClient(crate::services::http_client::HttpClientService),
    HttpsClient(crate::services::https_client::HttpsClientService),
    WsClient(crate::services::ws_client::WsClientService),
    WssClient(crate::services::wss_client::WssClientService),
    Ntp(crate::services::ntp::NtpService),
}

/// Everything the emitters need: the project settings, the resources it owns,
/// the components on them and the services over them.
#[derive(Debug, Clone)]
pub struct Board {
    pub meta: Meta,
    pub resources: ResourceGraph,
    pub components: Vec<Component>,
    pub services: Vec<Service>,
    /// Warnings collected while lowering. Warnings never stop the build, so
    /// they ride on the board rather than in the error list.
    pub warnings: Vec<Diagnostic>,
}

impl Board {
    /// Whether any plain-HTTP client service is declared.
    pub fn uses_http(&self) -> bool {
        self.services
            .iter()
            .any(|service| matches!(service, Service::HttpClient(_)))
    }

    /// Whether any TLS (HTTPS) client service is declared.
    pub fn uses_https(&self) -> bool {
        self.services
            .iter()
            .any(|service| matches!(service, Service::HttpsClient(_)))
    }

    /// Whether any plain WebSocket client service is declared.
    pub fn uses_ws(&self) -> bool {
        self.services
            .iter()
            .any(|service| matches!(service, Service::WsClient(_)))
    }

    /// Whether any TLS (secure WebSocket) client service is declared.
    pub fn uses_wss(&self) -> bool {
        self.services
            .iter()
            .any(|service| matches!(service, Service::WssClient(_)))
    }

    /// Whether any NTP client service is declared.
    pub fn uses_ntp(&self) -> bool {
        self.services
            .iter()
            .any(|service| matches!(service, Service::Ntp(_)))
    }
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
            services: Vec::new(),
            warnings: Vec::new(),
        };

        assert!(board.components.is_empty());
        assert_eq!(board.resources.claim_of(Pin(18)).unwrap().span, 10..21);
    }
}
