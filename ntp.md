# ntp.md: NTP Client Element in espforge

> **Design and Implementation Specification**  
> **Target:** Adding an outgoing Network Time Protocol (SNTP) client service element (`<ntp>`) to `espforge` using [`vpetrigo/sntpc`](https://github.com/vpetrigo/sntpc) and `embassy-net` UDP sockets.  
> **Status:** Final Draft / Ready for Implementation  
> **Scope:** Outgoing unicast SNTP client capabilities (`<ntp>`). Runtime code lives in `espforge-runtime/src/services/`. Server/listening daemons are out of scope.

---

## 1. Executive Summary & Goals

`espforge` generates bare-metal `no_std` Rust firmware for ESP32 microcontrollers. Following Wi-Fi station support (`<wifi>` in `# peripherals` and `<wifi_station>` in `# components`), along with HTTP/HTTPS and WebSocket clients in `# services`, embedded systems frequently need accurate real-world wall-clock time for timestamping sensor readings, TLS certificate validity checking, data logging, and scheduled operations.

This document specifies the complete, end-to-end design for adding an **NTP client service** (`<ntp>`) to `espforge` using the [`vpetrigo/sntpc`](https://github.com/vpetrigo/sntpc) SNTPv4 (RFC 5905 / RFC 4330) crate:
- **`sntpc`**: Async-first, `no_std`, zero-alloc SNTP client implementing unicast client mode (mode 3 $\rightarrow$ mode 4) with rollover-safe timestamp extraction.
- **`embassy-net` UDP Integration**: Uses `embassy_net::udp::UdpSocket` and `embassy-net` DNS to resolve NTP pools (`pool.ntp.org`, `time.google.com`, `time.cloudflare.com`) and exchange 48-byte SNTP datagrams.
- **Monotonic Timestamp Generator**: Employs `embassy_time::Instant` to provide round-trip delay and clock offset calculations without requiring an RTC.

### Core Tenets
1. **Services Layer (Rank 4):** The `<ntp>` element lives in `# services`. It claims no physical pins; it references a network interface component (such as `<wifi_station>`) via `network="$interface"`.
2. **Dedicated Runtime Module (`espforge-runtime/src/services/`):** Implementation code lives in `espforge-runtime/src/services/ntp.rs`. Hardware peripherals stay isolated in `src/hw/`.
3. **Ergonomic, Non-Blocking Time API:** The runtime wraps `sntpc::get_time` into a high-level `NtpClient` exposing `get_time()`, `get_time_from(server, port)`, and `sync_clock()`, returning seconds and fractions since the Unix epoch (1970-01-01).
4. **Compile-Time Safety & Selective Dep Trees:** `sntpc` dependencies are strictly feature-gated behind the `ntp` feature in `espforge-runtime`. Projects without `<ntp>` in their `.espf` spec incur zero dependency or code overhead.
5. **No Stringly-Typed Diagnostics:** All spec validation checks emit typed diagnostic codes (`E0801`, `E0802`, `E0805`) matching the repository's diagnostics contract.

---

## 2. Architecture & Layer Ranking

`espforge` enforces strict vertical layering across its five sections:

```
Rank 0: # espforge       (Project settings, chip type, embassy runtime)
Rank 1: # peripherals    (<wifi>: on-chip radio peripheral)
Rank 2: # components     (<wifi_station>: SSID, authentication, DHCP stack)
Rank 3: # devices        (External hardware modules, e.g. W5500 SPI Ethernet)
Rank 4: # services       (<http_client>, <https_client>, <ws_client>, <ntp>)
```

### Layer Rules
1. **Downward Reference Constraint:** Elements in `# services` can reference `# components` (e.g. `<wifi_station>`) or `# devices` (e.g. future Ethernet controllers), but never `# peripherals`.
2. **Network Interface Binding:** Every `<ntp>` element must declare a `network` attribute pointing to an element providing an `embassy-net` `Stack<'static>` via `.stack()`.
3. **Runtime Prerequisite:** NTP client synchronization is async and requires `runtime="embassy"`. Declaring `<ntp>` under `blocking` produces compile-time diagnostic `E0802`.

---

## 3. Element Specification (`espforge-schema/src/espf.schema.kdl`)

Add the `ntp` element block to the `# services` section in `espforge-schema/src/espf.schema.kdl`:

```kdl
element "ntp" section="services" {
    summary "Async SNTP client for wall-clock time synchronization"
    description "Queries NTP/SNTPv4 servers (RFC 5905) over an established network interface using sntpc."
    attr "id" kind="ident" required=#true
    attr "network" kind="ref" to="wifi_station" required=#true {
        help "The network interface component providing UDP/IP connectivity."
    }
    attr "server" kind="str" default="pool.ntp.org" {
        help "Host name or IP address of the default NTP server pool."
    }
    attr "port" kind="u32" default="123" {
        help "UDP port number for NTP queries (standard port is 123)."
    }
    attr "timeout_ms" kind="u32" default="5000" {
        help "Timeout in milliseconds for DNS resolution and UDP packet exchange."
    }
    example "minimal" #"""
        # services
          <ntp id="clock" network="$home" />
        """#
    example "custom server and timeout" #"""
        # services
          <ntp id="clock" network="$home" server="time.google.com" timeout_ms="3000" />
        """#
    see_also "components/wifi_station"
    see_also "services/http_client"
}
```

---

## 4. Diagnostics & Validation (`espforge-diagnostics`)

Validation rules follow the `E08xx` range for services in `espforge-diagnostics/src/codes.rs`:

| Code | Constant Name | Condition | Severity | Message / Label |
|---|---|---|---|---|
| `E0801` | `SERVICE_UNKNOWN_NETWORK` | `network="$name"` does not match any declared network component in `# components` | Error | `unknown network interface: <ntp id=".."> refers to '..', which is not a network component in # components` |
| `E0802` | `SERVICE_REQUIRES_EMBASSY` | `<ntp>` declared under `runtime="blocking"` | Error | `NTP client requires embassy: <ntp id=".."> needs the embassy runtime; change <runtime type=".." /> to embassy` |
| `E0805` | `SERVICE_INVALID` | Invalid Rust identifier in `id="..."`, empty `server`, or `port == 0 \|\| port > 65535` | Error | `invalid ntp: <ntp id="..">: the id must become a Rust identifier` |
| `E0806` | `SERVICE_UNSUPPORTED` | Tag placed in an invalid section | Error | `<ntp> is not a known element in # ...` |

### Validation Checks in `LowerCx`
1. **Identifier Check:** Verify that `id` satisfies `is_rust_ident`.
2. **Runtime Verification:** Ensure `cx.runtime() == Runtime::Embassy`.
3. **Network Provider Check:** Call `cx.has_network_provider(network.as_str())`.
4. **Port Validation:** Ensure `port > 0 && port <= 65535`.
5. **Server Validation:** Ensure `server` is non-empty.

---

## 5. Board Model & Lowering (`espforge-elements`)

### 5.1 Extending the Board IR (`espforge-elements/src/model.rs`)
Update `service_enum!` in `espforge-elements/src/model.rs`:

```rust
service_enum! {
    HttpClient(crate::services::http_client::HttpClientService),
    HttpsClient(crate::services::https_client::HttpsClientService),
    WsClient(crate::services::ws_client::WsClientService),
    WssClient(crate::services::wss_client::WssClientService),
    Ntp(crate::services::ntp::NtpService),
}
```

Add the `NtpService` struct:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtpService {
    pub id: Ident,
    pub network: Ident,
    pub server: String,
    pub port: u16,
    pub timeout_ms: u32,
}
```

Add a helper method to `Board`:
```rust
impl Board {
    // ...
    /// Whether any NTP client service is declared.
    pub fn uses_ntp(&self) -> bool {
        self.services.iter().any(|service| matches!(service, Service::Ntp(_)))
    }
}
```

### 5.2 Element Implementation (`espforge-elements/src/services/ntp.rs`)
Create `espforge-elements/src/services/ntp.rs`:

```rust
//! `<ntp>`: an SNTP client service over a network component.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::attrs::Issue;
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::ntp::Attrs;
use crate::model::{Board, Ident, Runtime, Service};
use crate::registry::ELEMENTS;
use crate::services::{invalid_element, invalid_id, network_stack, span_of, unknown_network};

pub struct NtpElement;

#[distributed_slice(ELEMENTS)]
static NTP: &dyn Element = &NtpElement;

impl Element for NtpElement {
    fn tag(&self) -> &'static str {
        "ntp"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);
        let network = Ident::new(attrs.network.name, span_of(node, "network"));

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "ntp", &id));
        }
        if cx.runtime() != Runtime::Embassy {
            return Err(cx.report(
                codes::SERVICE_REQUIRES_EMBASSY,
                node_span(node),
                "NTP client requires embassy",
                format!(
                    "<ntp id=\"{}\"> needs the embassy runtime; change \
                     <runtime type=\"..\" /> to embassy",
                    id.raw
                ),
            ));
        }
        if !cx.has_network_provider(network.as_str()) {
            return Err(unknown_network(cx, node, "ntp", &id, &network));
        }

        let port = attrs.port.value;
        if port == 0 || port > 65535 {
            return Err(cx.report(
                codes::SERVICE_INVALID,
                span_of(node, "port"),
                "invalid ntp port",
                format!("<ntp id=\"{}\"> port=\"{port}\" must be in 1..=65535", id.raw),
            ));
        }

        cx.push_service(Service::Ntp(NtpService {
            id,
            network,
            server: attrs.server.value.to_string(),
            port: port as u16,
            timeout_ms: attrs.timeout_ms.value,
        }));
        Ok(())
    }
}

impl Lowered for NtpService {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let stack = network_stack(&self.network);
        let server = &self.server;
        let port = self.port;
        let timeout_ms = self.timeout_ms as u64;

        cx.service_field(quote! {
            pub #name: espforge_runtime::NtpClient,
        });
        cx.service_init(quote! {
            #name: espforge_runtime::NtpClient::new(
                #stack,
                espforge_runtime::NtpConfig {
                    server: #server,
                    port: #port,
                    timeout_ms: #timeout_ms,
                },
            ),
        });
    }
}

fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: Issue<'_>) -> Reported {
    invalid_element(cx, node, "ntp", issue)
}
```

Register the module in `espforge-elements/src/services/mod.rs`:
```rust
pub mod ntp;
```

---

## 6. Code Generation (`espforge-emit`)

### 6.1 `Cargo.toml` Generation (`codegen/cargo.rs`)
Update `Clients` struct in `espforge-emit/src/codegen/cargo.rs`:

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct Clients {
    pub http: bool,
    pub https: bool,
    pub ws: bool,
    pub wss: bool,
    pub ntp: bool,
}
```

Update `runtime_dependency`:
```rust
fn runtime_dependency(
    source: &RuntimeSource,
    embassy: bool,
    wifi: bool,
    clients: Clients,
) -> Value {
    let mut table = InlineTable::new();
    if let RuntimeSource::Path(path) = source {
        table.insert("path", Value::from(path.replace('\\', "/")));
    }
    table.insert("version", Value::from(RUNTIME_VERSION));
    let mut features: Vec<&str> = Vec::new();
    if embassy || wifi || clients.http || clients.https || clients.ws || clients.wss || clients.ntp {
        features.push("embassy");
    }
    if wifi {
        features.push("wifi");
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
```

In `espforge-emit/src/codegen/mod.rs`:
```rust
Source::Cargo => cargo::manifest(
    meta,
    opts,
    runtime,
    &chip,
    cargo::Clients {
        http: board.uses_http(),
        https: board.uses_https(),
        ws: board.uses_ws(),
        wss: board.uses_wss(),
        ntp: board.uses_ntp(),
    },
)?,
```

### 6.2 Heap Allocation in `main.rs`
NTP packets are 48-byte UDP datagrams without TLS crypto scratch buffers. The standard Wi-Fi station heap allocation (`72 * 1024` bytes) is completely sufficient. No additional heap bump is required for NTP.

---

## 7. Runtime Abstraction (`espforge-runtime`)

### 7.1 Crate Dependencies & Features (`espforge-runtime/Cargo.toml`)
Add `sntpc` dependency and feature to `espforge-runtime/Cargo.toml`:

```toml
[dependencies]
# SNTP client library (no_std, async)
sntpc = { version = "0.11", default-features = false, features = ["async"], optional = true }

[features]
# NTP client service using sntpc over embassy-net UDP
ntp = ["wifi", "dep:sntpc"]
```

### 7.2 Service Module Layout
In `espforge-runtime/src/services/mod.rs`:
```rust
#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "ntp"
))]
mod ntp;
#[cfg(all(
    any(target_arch = "riscv32", target_arch = "xtensa"),
    feature = "ntp"
))]
pub use ntp::*;
```

### 7.3 NTP Runtime Implementation (`espforge-runtime/src/services/ntp.rs`)
Implement the `NtpUdpSocket` and `NtpTimestampGenerator` traits from `sntpc` directly over `embassy_net::udp::UdpSocket` and `embassy_time::Instant`:

```rust
//! `NtpClient`: an outgoing SNTP client using `sntpc` over an `embassy-net` stack.

use core::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Instant, with_timeout};
use sntpc::{NtpContext, NtpResult, NtpTimestampGenerator, NtpUdpSocket, get_time};

/// Configuration options for the NTP client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtpConfig {
    /// Host name or IP address of the NTP server (e.g. "pool.ntp.org").
    pub server: &'static str,
    /// UDP port (default is 123).
    pub port: u16,
    /// Query and network timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for NtpConfig {
    fn default() -> Self {
        Self {
            server: "pool.ntp.org",
            port: 123,
            timeout_ms: 5000,
        }
    }
}

/// Errors returned by the NTP client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtpError {
    /// DNS lookup failed to resolve the NTP server host.
    Dns,
    /// The UDP socket failed to bind, send, or receive.
    Socket,
    /// The query timed out before a valid response was received.
    Timeout,
    /// The NTP server returned an invalid or unparseable response.
    Protocol,
}

/// Timestamp representation returned by successful synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtpTimestamp {
    /// Seconds since the Unix epoch (1970-01-01 00:00:00 UTC).
    pub unix_seconds: u64,
    /// Subsecond fraction in microseconds (0..999_999).
    pub microseconds: u32,
    /// Estimated round-trip network delay in microseconds.
    pub roundtrip_delay_us: u32,
    /// Estimated server clock offset in microseconds.
    pub offset_us: i64,
}

/// An outgoing SNTP client. `Copy`, so it can sit on the generated `Services` struct.
#[derive(Clone, Copy)]
pub struct NtpClient {
    stack: Stack<'static>,
    config: NtpConfig,
}

impl NtpClient {
    pub fn new(stack: Stack<'static>, config: NtpConfig) -> Self {
        Self { stack, config }
    }

    /// Query the configured NTP server for the current network time.
    pub async fn get_time(&self) -> Result<NtpTimestamp, NtpError> {
        self.get_time_from(self.config.server, self.config.port).await
    }

    /// Query a specific NTP server address and port.
    pub async fn get_time_from(&self, server: &str, port: u16) -> Result<NtpTimestamp, NtpError> {
        let addr = self.resolve(server, port).await?;
        let timeout = Duration::from_millis(self.config.timeout_ms);

        match with_timeout(timeout, self.query(addr)).await {
            Ok(res) => res,
            Err(_) => Err(NtpError::Timeout),
        }
    }

    async fn query(&self, server_addr: SocketAddr) -> Result<NtpTimestamp, NtpError> {
        let mut rx_meta = [PacketMetadata::EMPTY; 2];
        let mut rx_buf = [0u8; 256];
        let mut tx_meta = [PacketMetadata::EMPTY; 2];
        let mut tx_buf = [0u8; 256];

        let mut socket = UdpSocket::new(
            self.stack,
            &mut rx_meta,
            &mut rx_buf,
            &mut tx_meta,
            &mut tx_buf,
        );

        socket.bind(0).map_err(|_| NtpError::Socket)?;

        let ntp_socket = EmbassyNtpSocket { socket };
        let timestamp_gen = EmbassyTimestampGen::new();
        let context = NtpContext::new(timestamp_gen);

        let result: NtpResult = get_time(server_addr, ntp_socket, context)
            .await
            .map_err(|_| NtpError::Protocol)?;

        // NTP epoch begins on Jan 1, 1900. Unix epoch begins on Jan 1, 1970.
        // Difference is 70 years = 2,208,988,800 seconds.
        const NTP_TO_UNIX_OFFSET_SECS: u64 = 2_208_988_800;

        let ntp_secs = result.sec();
        let unix_seconds = ntp_secs.saturating_sub(NTP_TO_UNIX_OFFSET_SECS);
        let microseconds = sntpc::fraction_to_microseconds(result.sec_fraction());

        Ok(NtpTimestamp {
            unix_seconds,
            microseconds,
            roundtrip_delay_us: result.roundtrip() as u32,
            offset_us: result.offset(),
        })
    }

    async fn resolve(&self, host: &str, port: u16) -> Result<SocketAddr, NtpError> {
        if let Ok(ip) = host.parse::<Ipv4Addr>() {
            return Ok(SocketAddr::V4(SocketAddrV4::new(ip, port)));
        }

        match self.stack.dns_query(host, DnsQueryType::A).await {
            Ok(addrs) => match addrs.first() {
                Some(embassy_net::IpAddress::Ipv4(v4)) => {
                    Ok(SocketAddr::V4(SocketAddrV4::new(*v4, port)))
                }
                _ => Err(NtpError::Dns),
            },
            Err(_) => Err(NtpError::Dns),
        }
    }
}

// ---------------------------------------------------------------------------
// Adapter: NtpUdpSocket for embassy-net UdpSocket
// ---------------------------------------------------------------------------

struct EmbassyNtpSocket<'a> {
    socket: UdpSocket<'a>,
}

impl<'a> NtpUdpSocket for EmbassyNtpSocket<'a> {
    async fn send_to<A: core::net::ToSocketAddrs>(&self, buf: &[u8], addr: A) -> sntpc::Result<usize> {
        // Resolve target socket address
        let target = addr
            .to_socket_addrs()
            .map_err(|_| sntpc::Error::Network)?
            .next()
            .ok_or(sntpc::Error::Network)?;

        let endpoint = match target {
            SocketAddr::V4(v4) => {
                embassy_net::IpEndpoint::new(embassy_net::IpAddress::Ipv4(*v4.ip()), v4.port())
            }
            SocketAddr::V6(v6) => {
                embassy_net::IpEndpoint::new(embassy_net::IpAddress::Ipv6(*v6.ip()), v6.port())
            }
        };

        // Note: socket.send_to requires &mut self; in embassy-net, interior mutability or &mut ref is used.
        // We implement send_to/recv_from with embassy-net primitives.
        unsafe {
            let socket_mut = &mut *(&self.socket as *const _ as *mut UdpSocket<'a>);
            socket_mut
                .send_to(buf, endpoint)
                .await
                .map(|_| buf.len())
                .map_err(|_| sntpc::Error::Network)
        }
    }

    async fn recv_from(&self, buf: &mut [u8]) -> sntpc::Result<(usize, SocketAddr)> {
        unsafe {
            let socket_mut = &mut *(&self.socket as *const _ as *mut UdpSocket<'a>);
            let (n, from) = socket_mut.recv_from(buf).await.map_err(|_| sntpc::Error::Network)?;

            let addr = match from.endpoint.addr {
                embassy_net::IpAddress::Ipv4(v4) => {
                    SocketAddr::V4(SocketAddrV4::new(v4, from.endpoint.port))
                }
                _ => return Err(sntpc::Error::Network),
            };

            Ok((n, addr))
        }
    }
}

// ---------------------------------------------------------------------------
// Adapter: NtpTimestampGenerator for embassy-time Instant
// ---------------------------------------------------------------------------

struct EmbassyTimestampGen {
    start: Instant,
}

impl EmbassyTimestampGen {
    fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl NtpTimestampGenerator for EmbassyTimestampGen {
    fn init(&mut self) {
        self.start = Instant::now();
    }

    fn timestamp_sec(&self) -> u64 {
        // Monotonic elapsed time in seconds for round-trip calculations
        Instant::now().duration_since(self.start).as_secs()
    }

    fn timestamp_subsec_micros(&self) -> u32 {
        let micros = Instant::now().duration_since(self.start).as_micros();
        (micros % 1_000_000) as u32
    }
}
```

---

## 8. Application Code Integration (`app.rs`)

### 8.1 Example 1: Periodic Time Sync & UTC Logging
```rust
use crate::{component, service, Context};
use embassy_executor::Spawner;

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("Connecting to Wi-Fi...");
    component!(ctx, home).wait_connected().await;
    ctx.logger.info("Wi-Fi connected. Starting NTP synchronization.");
}

pub async fn forever(ctx: &'static Context) {
    let ntp = service!(ctx, clock);

    ctx.logger.info("Querying NTP server pool.ntp.org:123...");
    match ntp.get_time().await {
        Ok(time) => {
            ctx.logger.info(format_args!(
                "NTP sync successful! Unix Epoch: {}.{:06} s (RTT: {} us, Offset: {} us)",
                time.unix_seconds,
                time.microseconds,
                time.roundtrip_delay_us,
                time.offset_us
            ));

            // Format hours, minutes, seconds for display (UTC)
            let seconds_in_day = time.unix_seconds % 86400;
            let hours = seconds_in_day / 3600;
            let minutes = (seconds_in_day % 3600) / 60;
            let seconds = seconds_in_day % 60;

            ctx.logger.info(format_args!(
                "Current UTC Time: {:02}:{:02}:{:02}",
                hours, minutes, seconds
            ));
        }
        Err(err) => {
            ctx.logger.warn(format_args!("NTP query failed: {:?}", err));
        }
    }

    // SNTP specification (RFC 4330) recommends poll interval >= 60 seconds
    ctx.delay.delay_ms(60000).await;
}
```

### 8.2 Example 2: Synchronizing Multiple Fallback Servers
```rust
use crate::{component, service, Context};
use embassy_executor::Spawner;

const NTP_SERVERS: &[&str] = &[
    "time.cloudflare.com",
    "time.google.com",
    "pool.ntp.org",
];

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    component!(ctx, home).wait_connected().await;
    ctx.logger.info("Network ready.");
}

pub async fn forever(ctx: &'static Context) {
    let ntp = service!(ctx, clock);
    let mut synced = false;

    for server in NTP_SERVERS {
        ctx.logger.info(format_args!("Trying NTP server: {server}..."));
        match ntp.get_time_from(server, 123).await {
            Ok(ts) => {
                ctx.logger.info(format_args!(
                    "Synced with {server}! Unix Timestamp: {}",
                    ts.unix_seconds
                ));
                synced = true;
                break;
            }
            Err(e) => {
                ctx.logger.warn(format_args!("Failed from {server}: {:?}", e));
            }
        }
    }

    if !synced {
        ctx.logger.error("All NTP servers failed to respond.");
    }

    ctx.delay.delay_ms(120000).await;
}
```

---

## 9. Implementation Roadmap & Milestones

```
T0: Branch & baseline test verification
T1: Schema additions (<ntp>) & Diagnostic codes in espforge-diagnostics
T2: Runtime implementation in espforge-runtime/src/services/ntp.rs
T3: Element lowering & Board IR in espforge-elements/src/services/ntp.rs
T4: Codegen in espforge-emit (cargo.rs dependencies and rust.rs Services emission)
T5: Embedded example & Golden baseline (05.Networking/ntp)
T6: Hardware-free compilation gate via generated_projects_compile
```

### Milestone Breakdown

#### T1 — Schema & Diagnostics
1. In `espforge-diagnostics/src/codes.rs`: Ensure `SERVICE_UNKNOWN_NETWORK` (`E0801`), `SERVICE_REQUIRES_EMBASSY` (`E0802`), `SERVICE_INVALID` (`E0805`), and `SERVICE_UNSUPPORTED` (`E0806`) are defined and included in `ALL`.
2. In `espforge-schema/src/espf.schema.kdl`: Add the `<ntp>` element block to `# services`.
3. Verify: `cargo test -p espforge-schema` and `cargo test -p espforge-diagnostics`.

#### T2 — Runtime Abstraction in `espforge-runtime`
1. Add `sntpc = { version = "0.11", default-features = false, features = ["async"], optional = true }` to `espforge-runtime/Cargo.toml`.
2. Add `ntp = ["wifi", "dep:sntpc"]` to `[features]`.
3. Create `espforge-runtime/src/services/ntp.rs` implementing `NtpClient`, `NtpTimestamp`, `EmbassyNtpSocket`, and `EmbassyTimestampGen`.
4. Re-export in `src/services/mod.rs` and `src/lib.rs`.
5. Verify target build:
   ```bash
   cargo check -p espforge-runtime --features ntp,esp-hal/esp32c3,esp-radio/esp32c3,esp-radio/unstable --target riscv32imc-unknown-none-elf
   ```

#### T3 — Lowering in `espforge-elements`
1. In `espforge-elements/src/model.rs`: Add `Service::Ntp(NtpService)` and update `uses_ntp()`.
2. Create `espforge-elements/src/services/ntp.rs` implementing `Element` and `Lowered`.
3. Verify: Unit tests verifying network binding, blocking runtime error (`E0802`), invalid port (`E0805`), and clean lowering.

#### T4 — Codegen in `espforge-emit`
1. In `espforge-emit/src/codegen/cargo.rs`: Add `pub ntp: bool` to `Clients` and map it to runtime feature `"ntp"`.
2. In `espforge-emit/src/codegen/mod.rs`: Pass `ntp: board.uses_ntp()`.
3. In `espforge-emit/src/codegen/rust.rs`: Ensure `Services` struct emits `pub <id>: espforge_runtime::NtpClient` and initialisation in `Services::new`.
4. Verify: `cargo test -p espforge-emit`.

#### T5 — Embedded Example & Golden Baseline
1. Create `espforge-examples/src/examples/05.Networking/ntp/`:
   - `example.espf.txt`
   - `example.toml` (`chips = ["esp32c3", "esp32s3"]`, `runtimes = ["embassy"]`)
   - `app/rust/app.rs`
2. Baseline golden project outputs in `espforge/tests/golden/05.Networking.ntp.esp32c3.embassy`.
3. Verify: `cargo test -p espforge --test golden_projects`.

#### T6 — Compile Gate & Cleanliness
1. Add `("05.Networking/ntp", "esp32c3", Runtime::Embassy)` to `espforge/tests/generated_projects_compile.rs`.
2. Run target build:
   ```bash
   cargo test -p espforge --test generated_projects_compile -- --ignored
   ```
3. Run workspace sanity checks:
   ```bash
   cargo fmt --check
   cargo clippy --workspace
   cargo test --workspace
   ```

---

## 10. Memory Budget & Technical Constraints

1. **Stack Allocation:**
   An SNTP packet is fixed at 48 bytes. DNS queries and UDP packet reception buffers use 256 bytes on the task stack during `query()`. Default Embassy task stack sizes (2 KiB to 4 KiB) provide ample headroom.
2. **Heap Memory:**
   SNTP requests and UDP socket operations do not allocate from the heap. The default `72 * 1024` (72 KiB) heap configured for Wi-Fi station mode is completely sufficient.
3. **Socket Resources:**
   `embassy-net` allocates UDP sockets from `StackResources<N>`. The `StackResources<4>` allocated in `espforge-runtime/src/hw/wifi.rs` supports concurrent DHCP, DNS, and up to 2 active UDP/TCP client sockets. Because `UdpSocket` is created and dropped inside `query()`, sockets are returned to the pool immediately upon completion.
4. **RFC 4330 Minimum Poll Interval:**
   Per RFC 4330 §5, SNTP clients must not poll NTP servers more frequently than once every 60 seconds. Application examples and documentation must explicitly demonstrate polling intervals $\ge$ 60 seconds (e.g. 60,000 ms to 120,000 ms).

---

## 11. Acceptance Criteria / Definition of Done

- [ ] `espf.schema.kdl` defines `<ntp>` in `# services` with `id`, `network`, `server`, `port`, and `timeout_ms` attributes and self-contained examples.
- [ ] `espforge-runtime/src/services/ntp.rs` implements `NtpClient` using `sntpc` over `embassy_net::udp::UdpSocket` and `embassy_time::Instant`.
- [ ] Specifying `<ntp>` under `runtime="blocking"` produces diagnostic `E0802`.
- [ ] Specifying an unknown network interface produces diagnostic `E0801`.
- [ ] Generated `Cargo.toml` activates feature `"ntp"` on `espforge-runtime` if and only if `<ntp>` is declared.
- [ ] User applications access the client via `service!(ctx, clock)` and receive structured `NtpTimestamp` results with Unix seconds and microsecond precision.
- [ ] Generated NTP example compiles cleanly with `cargo check` for `riscv32imc-unknown-none-elf`.
- [ ] All 28+ existing golden projects (HelloWorld, Blink, Button, I2C, SPI, UART, WiFi, HTTP, HTTPS, WebSockets) remain 100% green and byte-identical.
