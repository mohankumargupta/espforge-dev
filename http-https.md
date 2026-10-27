# http-https.md: HTTP and HTTPS Client Elements in espforge

> **Design and Implementation Specification**  
> **Target:** Adding outgoing HTTP and HTTPS Client service elements to `espforge` using [`sysgrok/edge-net`](https://github.com/sysgrok/edge-net) (`edge-http`, `edge-nal`, `edge-nal-embassy`, and `embedded-tls`).  
> **Status:** Final Draft / Ready for Implementation  
> **Scope:** Outgoing client capabilities (`<http_client>`, `<https_client>`). Runtime code lives in `espforge-runtime/src/services/`. Servers are out of scope.

---

## 1. Executive Summary & Goals

`espforge` generates bare-metal `no_std` Rust firmware for ESP32 microcontrollers. With Wi-Fi station support established in Phase 4 (`<wifi>` in `# peripherals` and `<wifi_station>` in `# components`), applications need an ergonomic, idiomatic way to consume REST APIs, query cloud services, and stream telemetry.

This document specifies the end-to-end design for adding **HTTP and HTTPS client** services to `espforge` via the [`sysgrok/edge-net`](https://github.com/sysgrok/edge-net) suite:
- **`edge-http`**: Async, `no_std`, zero-alloc/low-alloc HTTP client based on `httparse`.
- **`edge-nal-embassy`**: Bridge connecting `embassy-net`'s network `Stack` to `edge-nal`'s network transport traits (`TcpConnect`, `Dns`).
- **`embedded-tls`**: Lightweight async TLS 1.3 implementation for `no_std` embedded targets, enabling HTTPS without the standard library.

### Core Tenets
1. **Services Layer (Rank 4):** HTTP and HTTPS client elements live in `# services`. They do not claim hardware pins; they bind to network interface components (such as `<wifi_station>`) via `network="$interface"`.
2. **Dedicated Runtime Module (`src/services/`):** Hardware peripherals and pin drivers belong in `espforge-runtime/src/hw/`. Pure software services (HTTP, NTP, MQTT) live in `espforge-runtime/src/services/`.
3. **Dedicated Client Elements:**
   - `<http_client>`: Fast, lightweight unencrypted HTTP/1.1 client (port 80 default).
   - `<https_client>`: TLS 1.3 encrypted HTTPS client over `embedded-tls` (port 443 default).
4. **No Stringly-Typed Diagnostics:** Every validation check uses typed diagnostic codes in the `E08xx` range reserved for services.
5. **Compile-Time Safety & Selective Dep Trees:** Firmware only pulls in TLS dependencies (`embedded-tls`) and crypto buffers when `<https_client>` is explicitly declared in the `.espf` spec.

---

## 2. Architecture & Layer Ranking

`espforge` enforces strict vertical layering across its five sections:

```
Rank 0: # espforge       (Project settings, chip, runtime)
Rank 1: # peripherals    (<wifi>: on-chip radio)
Rank 2: # components     (<wifi_station>: Wi-Fi credentials, association, DHCP)
Rank 3: # devices        (External hardware modules, e.g., W5500 SPI Ethernet)
Rank 4: # services       (<http_client>, <https_client>)
```

### Layer Rules
1. **Downward Reference Constraint:** Elements in `# services` can reference `# components` (e.g. `<wifi_station>`) or `# devices` (e.g. future Ethernet devices), but never `# peripherals`.
2. **Network Interface Binding:** Every HTTP/HTTPS client service must declare a `network` attribute pointing to an element providing an `embassy-net` `Stack<'static>`.
3. **Runtime Prerequisite:** All HTTP/HTTPS client services require `runtime="embassy"`. Attempting to use them under `blocking` produces a compile-time diagnostic (`E0802`).

---

## 3. Element Specifications (`espf.schema.kdl`)

Two elements are added to the `# services` section of `espf.schema.kdl`:

### 3.1 `<http_client>`
```kdl
element "http_client" section="services" {
    summary "Async HTTP client over an established network interface"
    description "Provides outgoing HTTP/1.1 requests (GET, POST, PUT, DELETE) using edge-http."
    attr "id" kind="ident" required=#true
    attr "network" kind="ref" to="wifi_station" required=#true {
        help "The network interface component providing TCP/IP connectivity."
    }
    attr "rx_buffer_size" kind="u32" default="2048" {
        help "Internal receive buffer size in bytes for headers and response body."
    }
    attr "tx_buffer_size" kind="u32" default="1024" {
        help "Internal transmit buffer size in bytes for headers and request body."
    }
    attr "timeout_ms" kind="u32" default="10000" {
        help "Socket connection and read/write timeout in milliseconds."
    }
    example "minimal" #"""
        # services
          <http_client id="client" network="$home" />
        """#
    example "custom buffers" #"""
        # services
          <http_client id="client" network="$home" rx_buffer_size="4096" timeout_ms="5000" />
        """#
    see_also "components/wifi_station"
    see_also "services/https_client"
}
```

### 3.2 `<https_client>`
```kdl
element "https_client" section="services" {
    summary "Async TLS 1.3 encrypted HTTPS client over an established network interface"
    description "Provides outgoing HTTPS requests using edge-http layered over embedded-tls."
    attr "id" kind="ident" required=#true
    attr "network" kind="ref" to="wifi_station" required=#true {
        help "The network interface component providing TCP/IP connectivity."
    }
    attr "rx_buffer_size" kind="u32" default="4096" {
        help "Receive buffer size in bytes (TLS records require larger buffers; min 4096)."
    }
    attr "tx_buffer_size" kind="u32" default="2048" {
        help "Transmit buffer size in bytes."
    }
    attr "timeout_ms" kind="u32" default="15000" {
        help "Connection, TLS handshake, and read/write timeout in milliseconds."
    }
    attr "insecure" kind="bool" default="false" {
        help "When true, skips server X.509 certificate verification. Emits a build warning."
    }
    example "secure client" #"""
        # services
          <https_client id="secure_client" network="$home" />
        """#
    example "insecure for development" #"""
        # services
          <https_client id="dev_client" network="$home" insecure="true" />
        """#
    see_also "components/wifi_station"
    see_also "services/http_client"
}
```

---

## 4. Diagnostic Codes & Validation Rules

Diagnostics use the dedicated `E08xx` range in `espforge-diagnostics::codes`:

| Code | Constant Name | Condition | Severity | Message / Remedy |
|---|---|---|---|---|
| `E0801` | `SERVICE_UNKNOWN_NETWORK` | `network="$name"` does not match any declared network component | Error | `unknown network interface: refers to 'name' which is not in # components` |
| `E0802` | `SERVICE_REQUIRES_EMBASSY` | Client declared with `runtime="blocking"` | Error | `HTTP/HTTPS client requires embassy runtime; change runtime to embassy` |
| `E0803` | `SERVICE_BUFFER_TOO_SMALL` | Client buffer is below minimum required (`rx < 512` for HTTP, `rx < 4096` for HTTPS) | Error | `buffer too small: TLS requires rx_buffer_size >= 4096 bytes` |
| `E0804` | `SERVICE_INSECURE_WARNING` | `<https_client>` has `insecure="true"` | Warning | `insecure TLS verification enabled: server certificates will not be validated` |

### Validation Checks in `LowerCx`
1. **Network Binding Resolution:**
   ```rust
   let net_name = attrs.network.name;
   if !cx.has_network_provider(net_name) {
       return Err(cx.report(
           codes::SERVICE_UNKNOWN_NETWORK,
           node_span(node),
           "unknown network interface",
           format!("<{} id=\"{}\"> refers to `{net_name}`, which is not a network component in # components", node.tag_name, id.raw)
       ));
   }
   ```
2. **Runtime Verification:**
   If `cx.meta().runtime != Runtime::Embassy`, report `codes::SERVICE_REQUIRES_EMBASSY` anchored at the service tag span.
3. **TLS Buffer Constraints:**
   For `https_client`, if `rx_buffer_size < 4096`, report `codes::SERVICE_BUFFER_TOO_SMALL`. TLS 1.3 records and cipher suites fail on undersized record envelopes.

---

## 5. Board IR & Lowering (`espforge-elements`)

### 5.1 Extending the Board Model (`espforge-elements/src/model.rs`)
Add `Service` representations to the board IR:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Service {
    HttpClient(HttpClientService),
    HttpsClient(HttpsClientService),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpClientService {
    pub id: Ident,
    pub network_ref: Ident,
    pub rx_buffer_size: u32,
    pub tx_buffer_size: u32,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpsClientService {
    pub id: Ident,
    pub network_ref: Ident,
    pub rx_buffer_size: u32,
    pub tx_buffer_size: u32,
    pub timeout_ms: u32,
    pub insecure: bool,
}
```

Update `Board`:
```rust
#[derive(Debug, Clone)]
pub struct Board {
    pub meta: Meta,
    pub resources: ResourceGraph,
    pub components: Vec<Component>,
    pub services: Vec<Service>, // Populated during lowering
}
```

### 5.2 Element Implementations
Implement `Element` for:
- `espforge-elements/src/services/http_client.rs`
- `espforge-elements/src/services/https_client.rs`

Both register via `#[distributed_slice(ELEMENTS)]`.

---

## 6. Code Generation (`espforge-emit`)

### 6.1 `Cargo.toml` Dependencies (`codegen/cargo.rs`)
When `board.services` contains `HttpClientService` or `HttpsClientService`:

```toml
# HTTP client stack
edge-net = { version = "0.8", default-features = false, features = ["edge-http"] }
edge-http = { version = "0.8", default-features = false }
edge-nal = { version = "0.5" }
edge-nal-embassy = { version = "0.5" }
httparse = { version = "1.9", default-features = false }
```

When `HttpsClientService` is present:
```toml
# TLS 1.3 encryption
embedded-tls = { version = "0.17", default-features = false, features = ["async"] }
```

### 6.2 `src/generated.rs` (Services Struct Emission)
In `espforge-emit/src/codegen/rust.rs`:

```rust
#[doc = " Constructed services."]
pub struct Services {
    pub client: espforge_runtime::HttpClient,
}

impl Services {
    pub fn new(components: &Components) -> Self {
        let stack = components.home.stack();
        let client = espforge_runtime::HttpClient::new(stack);
        Self { client }
    }
}
```

For `<https_client id="secure">`:
```rust
#[doc = " Constructed services."]
pub struct Services {
    pub secure: espforge_runtime::HttpsClient,
}

impl Services {
    pub fn new(components: &Components) -> Self {
        let stack = components.home.stack();
        let secure = espforge_runtime::HttpsClient::new(stack, false); // insecure: false
        Self { secure }
    }
}
```

### 6.3 `src/bin/main.rs` Startup Sequencing
Because HTTP and HTTPS clients do not run background worker loops, `main.rs` requires **no spawned background tasks**. The lifecycle is clean and synchronous during initialization:

```rust
// 1. Peripherals & heap allocation (72 KiB for HTTP, 96 KiB for HTTPS)
let peripherals = esp_hal::init(esp_hal::Config::default());
esp_alloc::heap_allocator!(size: 96 * 1024);

// 2. Hardware components
let mut registry = PeripheralRegistry::new(peripherals);
let components = Components::new(&mut registry);

// 3. Start RTOS timer scheduler
let timg0 = TimerGroup::new(registry.peripherals.TIMG0);
esp_rtos::start(timg0.timer0, registry.peripherals.FROM_CPU_INTR0);

// 4. Start Wi-Fi background association & DHCP
components.home.start(registry.peripherals.WIFI, &spawner).expect("wifi start failed");

// 5. Build services with stack reference
let services = Services::new(&components);

// 6. Seal Context into StaticCell
let ctx = Context { logger, delay, components, devices, services };
let ctx = CTX.init(ctx);

// 7. App entry
app::setup(ctx, spawner).await;
loop {
    app::forever(ctx).await;
}
```

---

## 7. Runtime Abstraction (`espforge-runtime`)

### 7.1 Crate Layout & Features (`espforge-runtime`)

Software services are distinctly separated from hardware drivers:
- Hardware peripherals, buses, and pin-level drivers reside under `src/hw/`.
- Network software protocols and application services reside under `src/services/`.

In `espforge-runtime/Cargo.toml`:
```toml
[dependencies]
edge-net = { version = "0.8", default-features = false, features = ["edge-http"], optional = true }
edge-http = { version = "0.8", default-features = false, optional = true }
edge-nal = { version = "0.5", optional = true }
edge-nal-embassy = { version = "0.5", optional = true }
embedded-tls = { version = "0.17", default-features = false, features = ["async"], optional = true }

[features]
http-client = ["wifi", "dep:edge-net", "dep:edge-http", "dep:edge-nal", "dep:edge-nal-embassy"]
https-client = ["http-client", "dep:embedded-tls"]
```

In `espforge-runtime/src/lib.rs`:
```rust
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
#[cfg(feature = "http-client")]
pub mod services;

#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
#[cfg(feature = "http-client")]
pub use services::*;
```

In `espforge-runtime/src/services/mod.rs`:
```rust
mod http_client;
pub use http_client::*;

#[cfg(feature = "https-client")]
mod https_client;
#[cfg(feature = "https-client")]
pub use https_client::*;
```

### 7.2 `HttpClient` (`espforge-runtime/src/services/http_client.rs`)
```rust
use embassy_net::Stack;
use edge_nal_embassy::Embassy;

#[derive(Clone, Copy)]
pub struct HttpClient {
    stack: Stack<'static>,
}

impl HttpClient {
    pub fn new(stack: Stack<'static>) -> Self {
        Self { stack }
    }

    /// Performs an async HTTP GET request, returning a slice containing the response body.
    pub async fn get<'a>(
        &self,
        url: &str,
        rx_buf: &'a mut [u8],
    ) -> Result<&'a [u8], HttpError> {
        let (host, port, path) = parse_http_url(url, 80)?;
        let nal = Embassy::new(self.stack);

        let mut client = edge_http::io::client::Connection::new(
            rx_buf,
            &nal,
            (host, port),
        ).await.map_err(HttpError::Connect)?;

        client.initiate_request(true, edge_http::Method::Get, path, &[
            ("Host", host),
            ("Connection", "close"),
        ]).await.map_err(HttpError::Request)?;

        let mut response = client.initiate_response().await.map_err(HttpError::Response)?;
        let len = response.read_to_end().await.map_err(HttpError::Read)?;

        Ok(&rx_buf[..len])
    }

    /// Performs an async HTTP POST request with payload and content-type.
    pub async fn post<'a>(
        &self,
        url: &str,
        content_type: &str,
        payload: &[u8],
        rx_buf: &'a mut [u8],
    ) -> Result<&'a [u8], HttpError> {
        let (host, port, path) = parse_http_url(url, 80)?;
        let nal = Embassy::new(self.stack);

        let mut client = edge_http::io::client::Connection::new(
            rx_buf,
            &nal,
            (host, port),
        ).await.map_err(HttpError::Connect)?;

        let payload_len_str = format_u32(payload.len() as u32);
        client.initiate_request(false, edge_http::Method::Post, path, &[
            ("Host", host),
            ("Content-Type", content_type),
            ("Content-Length", payload_len_str.as_str()),
            ("Connection", "close"),
        ]).await.map_err(HttpError::Request)?;

        client.write_all(payload).await.map_err(HttpError::Write)?;

        let mut response = client.initiate_response().await.map_err(HttpError::Response)?;
        let len = response.read_to_end().await.map_err(HttpError::Read)?;

        Ok(&rx_buf[..len])
    }
}
```

### 7.3 `HttpsClient` (`espforge-runtime/src/services/https_client.rs`)
`HttpsClient` handles the TLS 1.3 handshake on top of TCP before invoking `edge-http`:

```rust
use embassy_net::Stack;
use edge_nal_embassy::Embassy;
use embedded_tls::{TlsConfig, TlsConnection, TlsContext, UnsecureProvider};

#[derive(Clone, Copy)]
pub struct HttpsClient {
    stack: Stack<'static>,
    insecure: bool,
}

impl HttpsClient {
    pub fn new(stack: Stack<'static>, insecure: bool) -> Self {
        Self { stack, insecure }
    }

    pub async fn get<'a>(
        &self,
        url: &str,
        rx_buf: &'a mut [u8],
    ) -> Result<&'a [u8], HttpError> {
        let (host, port, path) = parse_http_url(url, 443)?;
        let nal = Embassy::new(self.stack);

        // 1. Open raw TCP connection
        let mut tcp_socket = nal.connect((host, port)).await.map_err(HttpError::Connect)?;

        // 2. Perform TLS 1.3 Handshake using embedded-tls
        let mut record_read_buf = [0u8; 4096];
        let mut record_write_buf = [0u8; 2048];

        let config = if self.insecure {
            TlsConfig::new().with_verify_mode(embedded_tls::Verify::None)
        } else {
            TlsConfig::new()
        };

        let mut tls = TlsConnection::new(
            &mut tcp_socket,
            &mut record_read_buf,
            &mut record_write_buf,
        );

        tls.open(TlsContext::new(&config, UnsecureProvider::new()))
            .await
            .map_err(HttpError::Tls)?;

        // 3. Issue HTTP request over TLS stream
        // ... (wrap `tls` in edge_http connection)
        Ok(&rx_buf[..0])
    }
}
```

---

## 8. Application Code Integration (`app.rs`)

### 8.1 Example 1: Plain HTTP Telemetry GET/POST
```rust
use crate::{Context, service};

pub async fn setup(ctx: &'static Context) {
    ctx.logger.info("Waiting for Wi-Fi connection...");
    ctx.components.home.wait_connected().await;
    ctx.logger.info("Connected to Wi-Fi. Ready to make requests.");
}

pub async fn forever(ctx: &'static Context) {
    let client = service!(ctx, client);
    let mut rx_buf = [0u8; 1024];

    match client.get("http://api.local/v1/status", &mut rx_buf).await {
        Ok(body) => {
            if let Ok(text) = core::str::from_utf8(body) {
                ctx.logger.info(format_args!("Status received: {text}"));
            }
        }
        Err(e) => {
            ctx.logger.warn(format_args!("Request failed: {:?}", e));
        }
    }

    ctx.delay.delay_ms(10000).await;
}
```

### 8.2 Example 2: Secure HTTPS Cloud Sync
```rust
use crate::{Context, service, component};

pub async fn setup(ctx: &'static Context) {
    ctx.components.home.wait_connected().await;
    ctx.logger.info("Syncing state with cloud over HTTPS...");
}

pub async fn forever(ctx: &'static Context) {
    let https = service!(ctx, secure_client);
    let mut rx_buf = [0u8; 2048];

    let payload = b"{\"device\":\"esp32c3\",\"status\":\"online\"}";

    match https.post("https://telemetry.example.com/api", "application/json", payload, &mut rx_buf).await {
        Ok(resp) => {
            ctx.logger.info(format_args!("Cloud ack: {} bytes", resp.len()));
            component!(ctx, status_led).toggle();
        }
        Err(err) => {
            ctx.logger.error(format_args!("TLS/HTTPS post failed: {:?}", err));
        }
    }

    ctx.delay.delay_ms(30000).await;
}
```

---

## 9. Implementation Roadmap & Milestones

```
T0: Branch & baseline test verification
T1: Schema additions (<http_client>, <https_client>) & Diagnostics (E0801..E0804)
T2: Runtime types under espforge-runtime/src/services/
T3: Element lowering & Board IR in espforge-elements
T4: Codegen in espforge-emit (Cargo.toml dependencies, Services struct in rust.rs)
T5: Examples & Golden Tests (05.Networking/http_client, 05.Networking/https_client)
T6: Hardware-free CI compilation check via generated_projects_compile
```

### Milestone Breakdown

#### T1 — Schema & Diagnostics
- Add `E0801`–`E0804` constants to `espforge-diagnostics/src/codes.rs`.
- Add `http_client` and `https_client` blocks to `espforge-schema/src/espf.schema.kdl`.
- Verify: `cargo test -p espforge-schema` and `cargo test -p espforge-diagnostics`.

#### T2 — Runtime Abstraction in `espforge-runtime/src/services/`
- Add `edge-net`, `edge-http`, `edge-nal-embassy`, and `embedded-tls` dependencies behind `http-client` and `https-client` features in `espforge-runtime/Cargo.toml`.
- Create directory `espforge-runtime/src/services/`.
- Implement `espforge-runtime/src/services/mod.rs`, `http_client.rs`, and `https_client.rs`.
- Wire into `espforge-runtime/src/lib.rs`.
- Verify: Compile for RISC-V target:
  ```bash
  cargo check -p espforge-runtime --features https-client,esp-hal/esp32c3,esp-radio/esp32c3,esp-radio/unstable --target riscv32imc-unknown-none-elf
  ```

#### T3 — Lowering in `espforge-elements`
- Update `model.rs` with `Service::HttpClient` and `Service::HttpsClient`.
- Add `services/http_client.rs` and `services/https_client.rs`.
- Implement `LowerCx` checks: unknown network provider (`E0801`), blocking runtime error (`E0802`), buffer sizing bounds (`E0803`), insecure warning (`E0804`).
- Verify: Unit tests verifying all diagnostic paths.

#### T4 — Codegen in `espforge-emit`
- In `codegen/cargo.rs`: Add conditional inclusion of `edge-net` and `embedded-tls`.
- In `codegen/rust.rs`: Populate `pub struct Services { ... }` and `Services::new` method.
- In `codegen/rust.rs`: If `https_client` is used, bump heap allocator size from `72 * 1024` to `96 * 1024`.
- Verify: Test comparing generated `Cargo.toml` and `generated.rs` against expected token output.

#### T5 — Embedded Examples & Goldens
- Create `espforge-examples/src/examples/05.Networking/http_client/` and `https_client/`.
- Add declared cells in `example.toml` (`esp32c3`, `embassy`).
- Run golden suite to baseline the new example trees.
- Verify: `cargo test -p espforge --test golden_projects` is green for all cells.

#### T6 — Compile Gate & Closeout
- Add `http_client` and `https_client` cases to `espforge/tests/generated_projects_compile.rs`.
- Verify: `cargo test -p espforge --test generated_projects_compile -- --ignored` compiles cleanly without warnings.
- Run `cargo fmt --all --check` and `cargo clippy --workspace`.

---

## 10. Memory Budget & Technical Constraints

1. **Stack Memory Headroom:**
   `edge-http` client connection parsing requires roughly 1.5 KB to 2.5 KB of stack space. Because HTTP calls are invoked inside the user's `app::forever` or spawned tasks, user tasks must have standard Embassy task stack frames (default is adequate, no special overrides required).
2. **Heap Memory Allocation:**
   - Unencrypted HTTP client: Existing `72 * 1024` (72 KiB) heap is completely sufficient.
   - Encrypted HTTPS client: Bump allocator in `main.rs` to `96 * 1024` (96 KiB) to support `embedded-tls` 1.3 crypto context scratch buffers and record ring buffers.
3. **Socket Resources:**
   `embassy-net` allocates sockets via `StackResources<N>`. The `StackResources<4>` currently allocated in `espforge-runtime/src/hw/wifi.rs` provides enough slots for DHCP, DNS, and 2 active concurrent client TCP/TLS connections.

---

## 11. Acceptance Criteria / Definition of Done

- [ ] Schema declares `<http_client>` and `<https_client>` with full validation and self-contained examples.
- [ ] Runtime code lives cleanly under `espforge-runtime/src/services/` and is re-exported on target platforms under feature flags.
- [ ] Lowering produces located diagnostic codes (`E0801`–`E0804`) on configuration errors.
- [ ] Generated `Cargo.toml` includes `edge-net`, `edge-http`, and `embedded-tls` only when HTTP/HTTPS elements exist.
- [ ] `Context` provides `service!(ctx, client)` and `service!(ctx, secure_client)` for executing async requests.
- [ ] Golden tests pass for HTTP examples on `esp32c3`/`embassy`.
- [ ] `generated_projects_compile` passes `cargo check` on the generated HTTP project for `riscv32imc-unknown-none-elf`.
- [ ] Zero regression on existing goldens (HelloWorld, Blink, Button, I2C, SPI, UART).