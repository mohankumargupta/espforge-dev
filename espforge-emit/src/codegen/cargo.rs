//! `Cargo.toml`, built with `toml_edit`.
//!
//! The conditional dependency blocks are ordinary Rust `if`s over
//! [`ResolvedOptions`] instead of template guards, and the comments that carry
//! information (why `profile.dev` is optimized, what `codegen-units = 1`
//! buys) are attached as `toml_edit` decor so they survive serialization.
//!
//! Feature arrays keep the template's order. Cargo does not care, but a golden
//! diff does, so reproducing the order is what keeps byte identity meaningful.

use anyhow::Result;
use espforge_elements::{Console, Meta, ResolvedOptions, Runtime};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value, value};

use super::chip::Chip;
use crate::runtime_source::{RUNTIME_VERSION, RuntimeSource};

/// The emitted `Cargo.toml`.
///
/// `clients` is what the board's `# services` declared: the HTTP, HTTPS,
/// WebSocket, secure-WebSocket and NTP clients are runtime features, so a
/// project only names them when it has them. The dependency crates themselves
/// (`edge-http`, `edge-ws`, `edge-nal-embassy`, `embedded-tls`, `sntpc`) stay
/// in the runtime's own manifest, as the radio stack already does.
pub fn manifest(
    meta: &Meta,
    opts: &ResolvedOptions,
    runtime: &RuntimeSource,
    chip: &Chip,
    clients: Clients,
    displays: Displays,
) -> Result<String> {
    let name = meta.crate_name();
    let mut doc = DocumentMut::new();
    doc["package"] = package(meta, &name);
    doc["bin"] = Item::ArrayOfTables(bin(&name));
    doc["dependencies"] = dependencies(meta, opts, runtime, chip, clients, displays);
    doc["profile"] = profiles();
    Ok(doc.to_string())
}

/// The client services the board declares.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clients {
    pub http: bool,
    pub https: bool,
    pub ws: bool,
    pub wss: bool,
    pub ntp: bool,
}

/// The display devices the board declares. The drivers themselves live in the
/// runtime behind a feature; `embedded-graphics` is what app code draws with,
/// so the project names it directly.
#[derive(Debug, Clone, Copy, Default)]
pub struct Displays {
    pub ssd1306: bool,
    pub ili9341: bool,
}

fn package(meta: &Meta, name: &str) -> Item {
    let mut package = Table::new();
    package["name"] = value(name);
    package["version"] = value("0.1.0");
    package["edition"] = value("2024");
    package["rust-version"] = value("1.95");
    if let Some(description) = &meta.description {
        package["description"] = value(description.as_str());
    }
    Item::Table(package)
}

fn bin(name: &str) -> ArrayOfTables {
    let mut bin = Table::new();
    bin["name"] = value(name);
    bin["path"] = value("./src/bin/main.rs");
    let mut bins = ArrayOfTables::new();
    bins.push(bin);
    bins
}

fn dependencies(
    meta: &Meta,
    opts: &ResolvedOptions,
    runtime: &RuntimeSource,
    chip: &Chip,
    clients: Clients,
    displays: Displays,
) -> Item {
    let embassy = meta.runtime == Runtime::Embassy;
    let alloc = opts.is_on("alloc");
    // `<wifi>` in `# peripherals` turns the derived `wifi` option on; the radio
    // stack is what that option means. BLE arrives with its own element.
    let wifi = opts.is_on("wifi");
    let radio = wifi;
    let chip_name = chip.name.as_str();

    let mut deps = Table::new();
    deps["espforge-runtime"] = Item::Value(runtime_dependency(
        runtime, embassy, wifi, clients, displays,
    ));
    deps["static_cell"] = value("2.1.1");
    deps["critical-section"] = value("1.2.0");
    deps["log"] = value("0.4.27");

    let mut hal = vec![chip_name];
    if opts.is_on("unstable-hal") {
        hal.push("unstable");
    }
    hal.push("log-04");
    deps["esp-hal"] = Item::Value(dep("~1.2.2", &hal));

    if embassy || radio {
        let mut features = Vec::new();
        if radio {
            features.push("esp-radio");
        }
        if alloc {
            features.push("esp-alloc");
        }
        if embassy {
            features.push("embassy");
        }
        features.push(chip_name);
        features.push("log-04");
        deps["esp-rtos"] = Item::Value(dep("0.4.0", &features));
    }

    deps["esp-bootloader-esp-idf"] = Item::Value(dep("0.6.0", &["log-04", chip_name]));
    deps["esp-println"] = Item::Value(dep_without_defaults(
        "0.18.0",
        &[chip_name, console(meta), "log-04"],
    ));
    deps["esp-backtrace"] = Item::Value(dep("0.20.0", &[chip_name, "panic-handler", "println"]));
    if alloc {
        deps["esp-alloc"] = Item::Value(dep("0.11.0", &[chip_name]));
    }

    if radio {
        deps["embedded-io"] = value("0.7.1");
        if embassy {
            deps["embedded-io-async"] = value("0.7.0");
            if wifi {
                deps["embassy-net"] = Item::Value(dep(
                    "0.9.1",
                    &["tcp", "udp", "dhcpv4", "medium-ethernet", "log"],
                ));
                if let Some(mut key) = deps.key_mut("embassy-net") {
                    key.leaf_decor_mut().set_prefix(
                        "# for more networking protocol support see https://crates.io/crates/edge-net\n",
                    );
                }
            }
        }
        if wifi {
            deps["smoltcp"] = Item::Value(dep_without_defaults(
                "0.14.0",
                &[
                    "medium-ethernet",
                    "multicast",
                    "proto-dhcpv4",
                    "proto-dns",
                    "proto-ipv4",
                    "socket-dns",
                    "socket-raw",
                    "socket-tcp",
                    "socket-udp",
                    "socket-icmp",
                    "log",
                ],
            ));
        }
        let mut features = vec![chip_name];
        if wifi {
            features.push("wifi");
        }
        if alloc {
            features.push("esp-alloc");
        }
        features.push("unstable");
        features.push("log-04");
        deps["esp-radio"] = Item::Value(dep("1.0.0-beta.1", &features));
    }

    if embassy {
        deps["embassy-executor"] = Item::Value(dep("0.10.0", &["log"]));
        deps["embassy-time"] = Item::Value(dep("0.5.0", &["log"]));
        deps["embassy-sync"] = value("0.8.0");
    }

    // A display is drawn on with `embedded-graphics` from `app.rs`; the driver
    // crate itself is a runtime feature, so it is not named here.
    if displays.ssd1306 || displays.ili9341 {
        deps["embedded-graphics"] = value("0.8.1");
    }
    Item::Table(deps)
}

/// `[profile.dev]` / `[profile.release]`, with the template's comments.
fn profiles() -> Item {
    let mut dev = Table::new();
    dev.decor_mut().set_prefix(
        "# For fine tuning these settings, please refer to https://doc.rust-lang.org/cargo/reference/profiles.html\n",
    );
    dev["opt-level"] = value("s");
    if let Some(mut key) = dev.key_mut("opt-level") {
        key.leaf_decor_mut().set_prefix(
            "# The default debug profile is too slow and too big for resource-constrained devices.\n# Always build with some optimizations enabled.\n",
        );
    }

    let mut release = Table::new();
    let mut codegen_units = Value::from(1);
    codegen_units
        .decor_mut()
        .set_suffix("       # LLVM can perform better optimizations using a single thread");
    release["codegen-units"] = Item::Value(codegen_units);
    let mut debug = Value::from(2);
    debug
        .decor_mut()
        .set_suffix("               # prefer slower builds but better debugging experience");
    release["debug"] = Item::Value(debug);
    release["lto"] = value("fat");
    release["opt-level"] = value("s");

    let mut profile = Table::new();
    // `[profile.dev]` and `[profile.release]` are the headers; the parent is
    // implicit, as it was in the template.
    profile.set_implicit(true);
    profile["dev"] = Item::Table(dev);
    profile["release"] = Item::Table(release);
    Item::Table(profile)
}

/// `espforge-runtime = { path?, version, features? }`.
///
/// `wifi` implies `embassy` (the schema's `requires`), so the Wi-Fi feature
/// list carries both, in that order. `https-client` implies `http-client` and
/// `wss-client` implies `ws-client` (the runtime's own features), so listing
/// just the stronger of each pair is enough.
fn runtime_dependency(
    source: &RuntimeSource,
    embassy: bool,
    wifi: bool,
    clients: Clients,
    displays: Displays,
) -> Value {
    let mut table = InlineTable::new();
    if let RuntimeSource::Path(path) = source {
        table.insert("path", Value::from(path.replace('\\', "/")));
    }
    table.insert("version", Value::from(RUNTIME_VERSION));
    let mut features: Vec<&str> = Vec::new();
    if embassy || wifi || clients.http || clients.https || clients.ws || clients.wss || clients.ntp
    {
        features.push("embassy");
    }
    if wifi {
        features.push("wifi");
    }
    if displays.ssd1306 {
        features.push("ssd1306");
    }
    if displays.ili9341 {
        features.push("ili9341");
    }
    if clients.ntp {
        features.push("ntp");
    }
    if clients.wss {
        features.push("wss-client");
    } else if clients.ws {
        features.push("ws-client");
    }
    if clients.https {
        features.push("https-client");
    } else if clients.http {
        features.push("http-client");
    }
    if !features.is_empty() {
        table.insert("features", Value::Array(array(&features)));
    }
    Value::InlineTable(table)
}

/// `{ version = "...", features = [...] }`.
fn dep(version: &str, features: &[&str]) -> Value {
    let mut table = InlineTable::new();
    table.insert("version", Value::from(version));
    table.insert("features", Value::Array(array(features)));
    Value::InlineTable(table)
}

/// The same, with `default-features = false`.
fn dep_without_defaults(version: &str, features: &[&str]) -> Value {
    let mut table = InlineTable::new();
    table.insert("version", Value::from(version));
    table.insert("default-features", Value::from(false));
    table.insert("features", Value::Array(array(features)));
    Value::InlineTable(table)
}

fn array(items: &[&str]) -> Array {
    let mut array = Array::new();
    for item in items {
        array.push(*item);
    }
    array
}

fn console(meta: &Meta) -> &'static str {
    match meta.console {
        Console::Uart => "uart",
        Console::JtagSerial => "jtag-serial",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::test_support::{case, chip};

    fn cargo(defaults: bool, flags: &[(&str, &str)], runtime: Runtime) -> String {
        cargo_with(defaults, flags, runtime, Clients::default())
    }

    fn cargo_with(
        defaults: bool,
        flags: &[(&str, &str)],
        runtime: Runtime,
        clients: Clients,
    ) -> String {
        let case = case("esp32c3", runtime, defaults, flags, &[]);
        manifest(
            &case.meta,
            &case.opts,
            &RuntimeSource::Published,
            &chip("esp32c3"),
            clients,
            Displays::default(),
        )
        .unwrap()
    }

    fn parse(text: &str) -> toml_edit::DocumentMut {
        text.parse().unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    /// A dependency's rendered value, without the key's leading decor.
    fn dependency(doc: &toml_edit::DocumentMut, name: &str) -> String {
        doc["dependencies"][name]
            .as_value()
            .unwrap_or_else(|| panic!("{name} is not a value"))
            .to_string()
            .trim()
            .to_string()
    }

    #[test]
    fn the_blocking_project_has_no_async_dependencies() {
        let doc = parse(&cargo(true, &[], Runtime::Blocking));
        let deps = doc["dependencies"].as_table().unwrap();
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\" }"
        );
        assert!(deps.get("embassy-executor").is_none());
        assert!(deps.get("esp-rtos").is_none());
        assert!(deps.get("esp-radio").is_none());
        assert_eq!(
            features(&doc, "esp-hal"),
            vec!["esp32c3", "unstable", "log-04"]
        );
        assert_eq!(
            features(&doc, "esp-println"),
            vec!["esp32c3", "uart", "log-04"]
        );
    }

    #[test]
    fn the_embassy_project_adds_the_executor_rtos_and_sync_crates() {
        let doc = parse(&cargo(true, &[], Runtime::Embassy));
        assert_eq!(
            features(&doc, "esp-rtos"),
            vec!["esp-alloc", "embassy", "esp32c3", "log-04"]
        );
        assert!(doc["dependencies"].get("embassy-sync").is_some());
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\", features = [\"embassy\"] }"
        );
        // Embassy alone does not drag in the radio stack; `<wifi>` does.
        for name in ["esp-radio", "embassy-net", "smoltcp", "embedded-io"] {
            assert!(doc["dependencies"].get(name).is_none(), "{name}");
        }
    }

    /// The legacy `wifi_station` golden: the only description we have of what
    /// a working Wi-Fi project's manifest contains. `legacy/` is the oracle
    /// until phase 7 retires it, and this test retires with it.
    fn legacy_wifi_station() -> toml_edit::DocumentMut {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../legacy/espforge/tests/golden/",
            "05.Networking.wifi_station.esp32c3.embassy/Cargo.toml"
        );
        parse(
            &std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("reading the oracle {path}: {e}")),
        )
    }

    /// A `<wifi>` radio must produce the golden's dependency table verbatim:
    /// every line of it is load-bearing (the `esp-radio` feature on
    /// `esp-rtos`, the `edge-net` comment, the long `smoltcp` feature list),
    /// and `toml_edit` renders it without a diff if we get it right.
    #[test]
    fn wifi_brings_the_radio_stack_verbatim() {
        let radio = case("esp32c3", Runtime::Embassy, true, &[], &["wifi"]);
        let doc = parse(
            &manifest(
                &radio.meta,
                &radio.opts,
                &RuntimeSource::Published,
                &chip("esp32c3"),
                Clients::default(),
                Displays::default(),
            )
            .unwrap(),
        );
        assert_eq!(
            doc["dependencies"].to_string(),
            legacy_wifi_station()["dependencies"].to_string()
        );
    }

    #[test]
    fn the_description_is_written_as_a_string_and_defaults_can_be_off() {
        let case = case("esp32s3", Runtime::Blocking, false, &[], &[]);
        let doc = parse(
            &manifest(
                &case.meta,
                &case.opts,
                &RuntimeSource::Path("..\\rt".into()),
                &chip("esp32s3"),
                Clients::default(),
                Displays::default(),
            )
            .unwrap(),
        );
        assert_eq!(doc["package"]["description"].as_str(), Some("demo"));
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ path = \"../rt\", version = \"0.6.0\" }"
        );
        // with defaults off, neither alloc nor unstable-hal is implied
        assert!(doc["dependencies"].get("esp-alloc").is_none());
        assert_eq!(features(&doc, "esp-hal"), vec!["esp32s3", "log-04"]);
    }

    #[test]
    fn the_profile_comments_carry_through() {
        let text = cargo(true, &[], Runtime::Blocking);
        assert!(
            text.contains("# The default debug profile is too slow"),
            "{text}"
        );
        assert!(
            text.contains("# LLVM can perform better optimizations"),
            "{text}"
        );
        assert!(text.contains("opt-level = \"s\""), "{text}");
    }

    /// A client service is a runtime *feature*: the project names it, and the
    /// runtime's own manifest owns the `edge-*`/`embedded-tls` crates (the same
    /// split the Wi-Fi radio stack uses).
    #[test]
    fn an_http_client_asks_the_runtime_for_the_feature() {
        let doc = parse(&cargo_with(
            true,
            &[],
            Runtime::Embassy,
            Clients {
                http: true,
                ..Default::default()
            },
        ));
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\", features = [\"embassy\", \"http-client\"] }"
        );
        // Nothing extra lands in the project's own manifest.
        for name in ["edge-http", "edge-net", "embedded-tls"] {
            assert!(doc["dependencies"].get(name).is_none(), "{name}");
        }
    }

    #[test]
    fn an_https_client_implies_the_http_feature_in_the_runtime() {
        let doc = parse(&cargo_with(
            true,
            &[],
            Runtime::Embassy,
            Clients {
                https: true,
                ..Default::default()
            },
        ));
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\", features = [\"embassy\", \"https-client\"] }"
        );
    }

    /// A WebSocket client is a runtime feature too, and `wss` implies the
    /// plain `ws` one (the runtime's own feature graph).
    #[test]
    fn a_ws_client_asks_the_runtime_for_the_feature() {
        let doc = parse(&cargo_with(
            true,
            &[],
            Runtime::Embassy,
            Clients {
                ws: true,
                ..Default::default()
            },
        ));
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\", features = [\"embassy\", \"ws-client\"] }"
        );
        for name in ["edge-ws", "edge-http", "embedded-tls"] {
            assert!(doc["dependencies"].get(name).is_none(), "{name}");
        }
    }

    #[test]
    fn a_wss_client_implies_the_ws_feature_in_the_runtime() {
        let doc = parse(&cargo_with(
            true,
            &[],
            Runtime::Embassy,
            Clients {
                wss: true,
                ..Default::default()
            },
        ));
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\", features = [\"embassy\", \"wss-client\"] }"
        );
    }

    /// The SNTP client is a runtime feature as well, and it does not bring
    /// the HTTP or TLS crates with it.
    #[test]
    fn an_ntp_client_asks_the_runtime_for_the_feature() {
        let doc = parse(&cargo_with(
            true,
            &[],
            Runtime::Embassy,
            Clients {
                ntp: true,
                ..Default::default()
            },
        ));
        assert_eq!(
            dependency(&doc, "espforge-runtime"),
            "{ version = \"0.6.0\", features = [\"embassy\", \"ntp\"] }"
        );
        for name in ["sntpc", "edge-http", "edge-ws", "embedded-tls"] {
            assert!(doc["dependencies"].get(name).is_none(), "{name}");
        }
    }

    /// A display is a runtime feature plus `embedded-graphics` in the project;
    /// neither driver crate is named by the project itself.
    #[test]
    fn a_display_asks_the_runtime_for_its_feature() {
        let case = case("esp32c3", Runtime::Blocking, true, &[], &[]);
        for (displays, expected) in [
            (
                Displays {
                    ssd1306: true,
                    ili9341: false,
                },
                "{ version = \"0.6.0\", features = [\"ssd1306\"] }",
            ),
            (
                Displays {
                    ssd1306: false,
                    ili9341: true,
                },
                "{ version = \"0.6.0\", features = [\"ili9341\"] }",
            ),
        ] {
            let doc = parse(
                &manifest(
                    &case.meta,
                    &case.opts,
                    &RuntimeSource::Published,
                    &chip("esp32c3"),
                    Clients::default(),
                    displays,
                )
                .unwrap(),
            );
            assert_eq!(dependency(&doc, "espforge-runtime"), expected);
            assert_eq!(dependency(&doc, "embedded-graphics"), "\"0.8.1\"");
            for name in ["ssd1306", "display-interface", "sky-ili9341"] {
                assert!(doc["dependencies"].get(name).is_none(), "{name}");
            }
        }

        // A project with no display names neither.
        let doc = parse(&cargo(true, &[], Runtime::Blocking));
        assert!(doc["dependencies"].get("embedded-graphics").is_none());
    }

    fn features(doc: &toml_edit::DocumentMut, dep: &str) -> Vec<String> {
        doc["dependencies"][dep]["features"]
            .as_array()
            .unwrap_or_else(|| panic!("{dep} has no features"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }
}
