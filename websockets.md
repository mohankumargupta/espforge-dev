# websockets.md: WebSocket Client Elements in espforge

> **Design and Implementation Specification**  
> **Target:** Adding outgoing WebSocket client service elements (`<ws_client>`, `<wss_client>`) to `espforge` using [`sysgrok/edge-net`](https://github.com/sysgrok/edge-net) (`edge-ws`, `edge-http`, `edge-nal-embassy`, and `embedded-tls`).  
> **Status:** Final Draft / Ready for Implementation  
> **Scope:** Outgoing client capabilities (`ws://` plain and `wss://` secure TLS 1.3). Runtime code lives in `espforge-runtime/src/services/`. Server/listening endpoints are out of scope.

---

## 1. Executive Summary & Goals

`espforge` generates bare-metal `no_std` Rust firmware for ESP32 microcontrollers. Following the addition of HTTP/HTTPS client services (`http-https.md`), connected IoT devices frequently require persistent, bidirectional, full-duplex communication channels for telemetry streaming, remote control, and real-time events.

This document specifies the complete, end-to-end design for adding **WebSocket client services** (`<ws_client>` for unencrypted `ws://` and `<wss_client>` for TLS 1.3 encrypted `wss://`) to `espforge` using the [`sysgrok/edge-net`](https://github.com/sysgrok/edge-net) ecosystem:
- **`edge-ws`**: Async, `no_std`, zero-alloc/no-alloc implementation of the RFC 6455 WebSocket framing protocol.
- **`edge-http`**: Async HTTP/1.1 client connection engine with native WebSocket upgrade handshake support (`initiate_ws_upgrade_request`, `is_ws_upgrade_accepted`, and `conn.release()`).
- **`edge-nal-embassy`**: Transport bridge adapting `embassy-net`'s async network `Stack` into `edge-nal` traits (`TcpConnect`, `Dns`).
- **`embedded-tls`**: Lightweight async TLS 1.3 engine for `no_std` embedded targets, enabling `wss://` on top of raw TCP sockets without the standard library.

### Core Architectural Decisions
1. **Services Layer (Rank 4):** WebSocket clients belong in `# services`. They do not claim physical GPIOs or buses; they reference a network interface component (such as `<wifi_station>`) via `network="$interface"`.
2. **Distinct Elements for Plain and Secure:**
   - `<ws_client>`: Fast, zero-crypto WebSocket client for local networks and unencrypted telemetry (`ws://`, port 80 default).
   - `<wss_client>`: Secure TLS 1.3 encrypted WebSocket client (`wss://`, port 443 default) with larger buffers and optional certificate bypass (`insecure="true"`).
3. **Dedicated Runtime Module (`espforge-runtime/src/services/`):** Implementation code lives alongside `http_client.rs` and `https_client.rs`. Hardware peripherals stay isolated in `src/hw/`.
4. **Clean Session & Framing Abstraction:** The runtime wraps the two-phase lifecycle (HTTP 101 Switching Protocols upgrade handshake, followed by raw RFC 6455 framed read/write) into an ergonomic `WsSession` type providing `send_text`, `send_binary`, `recv`, and `close`.
5. **Compile-Time Feature Gating:** Projects only compile `edge-ws` when `<ws_client>` is present, and only include `embedded-tls` when `<wss_client>` is present.

---

## 2. Architecture & Layer Ranking

`espforge` enforces strict vertical layering across its five sections:

```
Rank 0: # espforge       (Project settings, chip type, embassy runtime)
Rank 1: # peripherals    (<wifi>: on-chip radio peripheral)
Rank 2: # components     (<wifi_station>: SSID, authentication, DHCP stack)
Rank 3: # devices        (Future external Ethernet controllers, e.g. W5500)
Rank 4: # services       (<http_client>, <https_client>, <ws_client>, <wss_client>)
```

### Layer Rules
1. **Downward Reference Constraint:** Elements in `# services` can reference `# components` (e.g. `<wifi_station>`) or `# devices`, but never `# peripherals`.
2. **Network Interface Binding:** Every `<ws_client>` and `<wss_client>` must declare a `network` attribute pointing to an element that exposes an `embassy-net` `Stack<'static>` via `.stack()`.
3. **Runtime Prerequisite:** All WebSocket client services require `runtime="embassy"`. Declaring them under `blocking` produces a compile-time diagnostic (`E0802`).

---

## 3. Element Specifications (`espforge-schema/src/espf.schema.kdl`)

Add two element blocks to `# services` in `espforge-schema/src/espf.schema.kdl`:

### 3.1 `<ws_client>`
```kdl
element "ws_client" section="services" {
    summary "Async WebSocket client over an established network interface"
    description "Provides outgoing RFC 6455 WebSocket connections (ws://) using edge-ws and edge-http."
    attr "id" kind="ident" required=#true
    attr "network" kind="ref" to="wifi_station" required=#true {
        help "The network interface component providing TCP/IP connectivity."
    }
    attr "rx_buffer_size" kind="u32" default="2048" {
        help "Internal receive buffer size in bytes for handshake headers and incoming frame payloads."
    }
    attr "tx_buffer_size" kind="u32" default="1024" {
        help "Internal transmit buffer size in bytes for outgoing frame payloads."
    }
    attr "timeout_ms" kind="u32" default="10000" {
        help "Handshake connection and message timeout in milliseconds."
    }
    example "minimal" #"""
        # services
          <ws_client id="ws" network="$home" />
        """#
    example "custom buffers" #"""
        # services
          <ws_client id="ws" network="$home" rx_buffer_size="4096" tx_buffer_size="2048" timeout_ms="15000" />
        """#
    see_also "components/wifi_station"
    see_also "services/wss_client"
    see_also "services/http_client"
}
```

### 3.2 `<wss_client>`
```kdl
element "wss_client" section="services" {
    summary "Async TLS 1.3 encrypted WebSocket client over an established network interface"
    description "Provides outgoing secure RFC 6455 WebSocket connections (wss://) using edge-ws over embedded-tls."
    attr "id" kind="ident" required=#true
    attr "network" kind="ref" to="wifi_station" required=#true {
        help "The network interface component providing TCP/IP connectivity."
    }
    attr "rx_buffer_size" kind="u32" default="4096" {
        help "Receive buffer size in bytes (TLS records require larger buffers; minimum 4096)."
    }
    attr "tx_buffer_size" kind="u32" default="2048" {
        help "Transmit buffer size in bytes."
    }
    attr "timeout_ms" kind="u32" default="15000" {
        help "Connection, TLS handshake, and WebSocket upgrade timeout in milliseconds."
    }
    attr "insecure" kind="bool" default="false" {
        help "When true, skips server X.509 certificate verification. Emits a build warning."
    }
    example "secure client" #"""
        # services
          <wss_client id="secure_ws" network="$home" />
        """#
    example "insecure for development" #"""
        # services
          <wss_client id="dev_ws" network="$home" insecure="true" />
        """#
    see_also "components/wifi_station"
    see_also "services/ws_client"
    see_also "services/https_client"
}
```

---

## 4. Diagnostics & Validation (`espforge-diagnostics`)

Validation rules reuse and extend the `E08xx` service diagnostics range:

| Code | Constant Name | Condition | Severity | Message / Label |
|---|---|---|---|---|
| `E0801` | `SERVICE_UNKNOWN_NETWORK` | `network="$name"` does not match any declared network component | Error | `unknown network interface: refers to 'name' which is not in # components` |
| `E0802` | `SERVICE_REQUIRES_EMBASSY` | Declared with `runtime="blocking"` | Error | `WebSocket client requires embassy runtime; change <runtime type=".." /> to embassy` |
| `E0803` | `SERVICE_BUFFER_TOO_SMALL` | `rx_buffer_size < 512` (for `ws_client`) or `< 4096` (for `wss_client`) | Error | `buffer too small: <tag id=".."> rx_buffer_size="..." is below the minimum of N bytes` |
| `W0804` | `SERVICE_INSECURE_WARNING` | `<wss_client>` has `insecure="true"` | Warning | `insecure TLS verification enabled: server certificates will not be validated` |
| `E0805` | `SERVICE_INVALID` | Invalid identifier or missing required attribute | Error | `<ws_client id="..">: the id must become a Rust identifier` |

### Validation in `LowerCx`
- Check identifier syntax via `is_rust_ident`.
- Check runtime is `Runtime::Embassy`.
- Check `cx.has_network_provider(network.as_str())`.
- Verify buffer thresholds:
  - `<ws_client>`: `rx_buffer_size >= 512`
  - `<wss_client>`: `rx_buffer_size >= 4096`
- If `<wss_client>` has `insecure="true"`, record warning `codes::SERVICE_INSECURE_WARNING`.

---

## 5. Board Model & Lowering (`espforge-elements`)

### 5.1 Extending the Board IR (`espforge-elements/src/model.rs`)
Update `Service` enum in `model.rs`:

```rust
service_enum! {
    HttpClient(crate::services::http_client::HttpClientService),
    HttpsClient(crate::services::https_client::HttpsClientService),
    WsClient(crate::services::ws_client::WsClientService),
    WssClient(crate::services::wss_client::WssClientService),
}
```

Add the service structs in `model.rs` (or referenced through the modules):
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsClientService {
    pub id: Ident,
    pub network: Ident,
    pub rx_buffer_size: u32,
    pub tx_buffer_size: u32,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WssClientService {
    pub id: Ident,
    pub network: Ident,
    pub rx_buffer_size: u32,
    pub tx_buffer_size: u32,
    pub timeout_ms: u32,
    pub insecure: bool,
}
```

Update helper methods on `Board`:
```rust
impl Board {
    pub fn uses_ws(&self) -> bool {
        self.services.iter().any(|s| matches!(s, Service::WsClient(_)))
    }

    pub fn uses_wss(&self) -> bool {
        self.services.iter().any(|s| matches!(s, Service::WssClient(_)))
    }
}
```

### 5.2 Element Implementations
Implement `Element` and `Lowered` in:
- `espforge-elements/src/services/ws_client.rs`
- `espforge-elements/src/services/wss_client.rs`

Register them in `ELEMENTS`:
```rust
#[distributed_slice(ELEMENTS)]
static WS_CLIENT: &dyn Element = &WsClientElement;

#[distributed_slice(ELEMENTS)]
static WSS_CLIENT: &dyn Element = &WssClientElement;
```

In `Lowered::emit`:
- For `WsClientService`:
  ```rust
  let name = format_ident!("{}", self.id.rust_name());
  let stack = network_stack(&self.network);
  let rx = self.rx_buffer_size;
  let tx = self.tx_buffer_size;
  let timeout = self.timeout_ms;

  cx.service_field(quote! {
      pub #name: espforge_runtime::WsClient,
  });
  cx.service_init(quote! {
      #name: espforge_runtime::WsClient::new(
          #stack,
          espforge_runtime::WsClientConfig {
              rx_buffer_size: #rx as usize,
              tx_buffer_size: #tx as usize,
              timeout_ms: #timeout as u64,
          },
      ),
  });
  ```
- For `WssClientService`:
  ```rust
  let name = format_ident!("{}", self.id.rust_name());
  let stack = network_stack(&self.network);
  let rx = self.rx_buffer_size;
  let tx = self.tx_buffer_size;
  let timeout = self.timeout_ms;
  let insecure = self.insecure;

  cx.service_field(quote! {
      pub #name: espforge_runtime::WssClient,
  });
  cx.service_init(quote! {
      #name: espforge_runtime::WssClient::new(
          #stack,
          espforge_runtime::WssClientConfig {
              rx_buffer_size: #rx as usize,
              tx_buffer_size: #tx as usize,
              timeout_ms: #timeout as u64,
          },
          #insecure,
      ),
  });
  ```

---

## 6. Code Generation (`espforge-emit`)

### 6.1 `Cargo.toml` Generation (`codegen/cargo.rs`)
Extend `Clients` in `codegen/cargo.rs`:
```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct Clients {
    pub http: bool,
    pub https: bool,
    pub ws: bool,
    pub wss: bool,
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
    if embassy || wifi || clients.http || clients.https || clients.ws || clients.wss {
        features.push("embassy");
    }
    if wifi {
        features.push("wifi");
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

In `codegen/mod.rs`, pass the flags:
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
    },
)?,
```

### 6.2 Heap Sizing in `codegen/rust.rs`
Like HTTPS, `wss_client` uses `embedded-tls`, which requires TLS record scratch buffers:
```rust
fn heap_line(board: &Board, opts: &ResolvedOptions) -> TokenStream {
    if board.uses_https() || board.uses_wss() {
        quote::quote! { esp_alloc::heap_allocator!(size: 96 * 1024); }
    } else if opts.is_on("wifi") {
        quote::quote! { esp_alloc::heap_allocator!(size: 72 * 1024); }
    } else {
        quote::quote! {}
    }
}
```

---

## 7. Runtime Abstraction (`espforge-runtime`)

### 7.1 Crate Dependencies & Features (`espforge-runtime/Cargo.toml`)
Update dependencies:
```toml
[dependencies]
# WebSocket frame handling (sysgrok/edge-net sub-crate)
edge-ws = { version = "0.8", default-features = false, optional = true }
# HTTP connection & upgrade handshake
edge-http = { version = "0.8", default-features = false, features = ["io"], optional = true }
edge-nal = { version = "0.7", default-features = false, optional = true }
edge-nal-embassy = { version = "0.9", default-features = false, features = ["tcp", "dns", "proto-ipv4", "medium-ethernet"], optional = true }
embedded-io-async = { version = "0.7", optional = true }
embedded-tls = { version = "0.19", default-features = false, optional = true }
rand_core = { version = "0.6", default-features = false, optional = true }

[features]
ws-client = [
    "wifi",
    "dep:edge-ws",
    "dep:edge-http",
    "dep:edge-nal",
    "dep:edge-nal-embassy",
    "dep:embedded-io-async",
    "dep:rand_core",
]
wss-client = ["ws-client", "dep:embedded-tls"]
```

### 7.2 Service Module Structure
In `espforge-runtime/src/services/mod.rs`:
```rust
mod http_client;
pub use http_client::*;

#[cfg(feature = "https-client")]
mod https_client;
#[cfg(feature = "https-client")]
pub use https_client::*;

#[cfg(feature = "ws-client")]
mod ws_client;
#[cfg(feature = "ws-client")]
pub use ws_client::*;

#[cfg(feature = "wss-client")]
mod wss_client;
#[cfg(feature = "wss-client")]
pub use wss_client::*;
```

### 7.3 Shared WebSocket Frame Protocol & Types
Both `WsClient` and `WssClient` share core message and error models:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsError {
    BadUrl,
    Dns,
    Connect,
    UpgradeFailed,
    Socket,
    FrameError,
    PayloadTooLarge,
    Timeout,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Text,
    Binary,
    Ping,
    Pong,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WsMessage<'a> {
    pub message_type: MessageType,
    pub payload: &'a [u8],
}
```

### 7.4 Plain WebSocket Client (`ws_client.rs`)

```rust
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use edge_http::io::client::Connection;
use edge_http::ws::{MAX_BASE64_KEY_LEN, MAX_BASE64_KEY_RESPONSE_LEN, NONCE_LEN};
use edge_nal::{AddrType, Dns, TcpConnect};
use edge_nal_embassy::{Dns as EmbassyDns, Tcp, TcpBuffers};
use edge_ws::{FrameHeader, FrameType};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{Read, Write};
use rand_core::RngCore;

use super::http_client::parse_url;
use crate::services::https_client::EspRng; // Reusable EspRng wrapper over esp_hal::rng::Rng

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WsClientConfig {
    pub rx_buffer_size: usize,
    pub tx_buffer_size: usize,
    pub timeout_ms: u64,
}

pub struct WsClient {
    stack: Stack<'static>,
    config: WsClientConfig,
}

impl WsClient {
    pub fn new(stack: Stack<'static>, config: WsClientConfig) -> Self {
        Self { stack, config }
    }

    /// Establish an async WebSocket session. Runs the HTTP upgrade handshake and
    /// returns an active session holding the open TCP socket.
    pub async fn connect<'a>(
        &self,
        url: &str,
        buffer: &'a mut [u8],
    ) -> Result<WsSession<'a>, WsError> {
        let (host, port, path) = parse_ws_url(url, 80, false)?;
        let addr = self.resolve(host, port).await?;

        // 1. Setup connection buffers & open TCP socket
        let mut rng = EspRng::new();
        let timeout = Duration::from_millis(self.config.timeout_ms);

        with_timeout(timeout, async {
            let mut socket = embassy_net::tcp::TcpSocket::new(
                self.stack,
                &mut buffer[..1024],
                &mut buffer[1024..2048],
            );
            socket.connect(addr).await.map_err(|_| WsError::Connect)?;

            // 2. Perform WebSocket handshake (RFC 6455)
            let mut nonce = [0u8; NONCE_LEN];
            rng.fill_bytes(&mut nonce);
            let mut key_buf = [0u8; MAX_BASE64_KEY_LEN];
            let mut resp_buf = [0u8; MAX_BASE64_KEY_RESPONSE_LEN];

            let mut conn: Connection<'_, _> = Connection::new(&mut buffer[2048..], &socket, addr);
            conn.initiate_ws_upgrade_request(Some(host), None, path, None, &nonce, &mut key_buf)
                .await
                .map_err(|_| WsError::UpgradeFailed)?;
            conn.initiate_response().await.map_err(|_| WsError::UpgradeFailed)?;

            if !conn.is_ws_upgrade_accepted(&nonce, &mut resp_buf).map_err(|_| WsError::UpgradeFailed)? {
                return Err(WsError::UpgradeFailed);
            }
            conn.complete().await.map_err(|_| WsError::UpgradeFailed)?;

            let (socket, _) = conn.release();
            Ok(WsSession {
                socket,
                rng,
                tx_buffer_size: self.config.tx_buffer_size,
            })
        })
        .await
        .map_err(|_| WsError::Timeout)?
    }
}
```

### 7.5 WebSocket Active Session (`WsSession`)
`WsSession` wraps the underlying transport (whether plain TCP or TLS) and enforces RFC 6455 client masking:

```rust
pub struct WsSession<'a, S: Read + Write = embassy_net::tcp::TcpSocket<'a>> {
    socket: S,
    rng: EspRng,
    tx_buffer_size: usize,
}

impl<'a, S: Read + Write> WsSession<'a, S> {
    /// Send an unfragmented UTF-8 text frame (masked per RFC 6455).
    pub async fn send_text(&mut self, text: &str) -> Result<(), WsError> {
        self.send_frame(FrameType::Text(false), text.as_bytes()).await
    }

    /// Send an unfragmented binary frame (masked per RFC 6455).
    pub async fn send_binary(&mut self, data: &[u8]) -> Result<(), WsError> {
        self.send_frame(FrameType::Binary(false), data).await
    }

    /// Send a ping frame.
    pub async fn ping(&mut self, payload: &[u8]) -> Result<(), WsError> {
        self.send_frame(FrameType::Ping, payload).await
    }

    /// Close the WebSocket session gracefully.
    pub async fn close(&mut self, status_code: u16) -> Result<(), WsError> {
        let code_bytes = status_code.to_be_bytes();
        self.send_frame(FrameType::Close, &code_bytes).await?;
        self.socket.flush().await.map_err(|_| WsError::Socket)?;
        Ok(())
    }

    /// Receive a single frame, storing its unmasked payload into `rx_buf`.
    pub async fn recv<'b>(&mut self, rx_buf: &'b mut [u8]) -> Result<WsMessage<'b>, WsError> {
        let header = FrameHeader::recv(&mut self.socket)
            .await
            .map_err(|_| WsError::FrameError)?;

        let payload_len = header.payload_len as usize;
        if payload_len > rx_buf.len() {
            return Err(WsError::PayloadTooLarge);
        }

        let slice = &mut rx_buf[..payload_len];
        header.recv_payload(&mut self.socket, slice)
            .await
            .map_err(|_| WsError::FrameError)?;

        let message_type = match header.frame_type {
            FrameType::Text(_) => MessageType::Text,
            FrameType::Binary(_) => MessageType::Binary,
            FrameType::Ping => MessageType::Ping,
            FrameType::Pong => MessageType::Pong,
            FrameType::Close => MessageType::Close,
            _ => return Err(WsError::FrameError),
        };

        Ok(WsMessage {
            message_type,
            payload: slice,
        })
    }

    async fn send_frame(&mut self, frame_type: FrameType, payload: &[u8]) -> Result<(), WsError> {
        if payload.len() > self.tx_buffer_size {
            return Err(WsError::PayloadTooLarge);
        }

        let mask = self.rng.next_u32();
        let header = FrameHeader {
            frame_type,
            payload_len: payload.len() as u64,
            mask_key: Some(mask),
        };

        header.send(&mut self.socket).await.map_err(|_| WsError::Socket)?;
        header.send_payload(&mut self.socket, payload).await.map_err(|_| WsError::Socket)?;
        self.socket.flush().await.map_err(|_| WsError::Socket)?;
        Ok(())
    }
}
```

### 7.6 Secure WebSocket Client (`wss_client.rs`)
`WssClient` layers `embedded-tls` over the TCP socket before issuing the HTTP upgrade handshake:

```rust
use alloc::vec;
use embedded_tls::{Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, UnsecureProvider};

pub struct WssClient {
    stack: Stack<'static>,
    config: WssClientConfig,
    insecure: bool,
}

impl WssClient {
    pub fn new(stack: Stack<'static>, config: WssClientConfig, insecure: bool) -> Self {
        Self { stack, config, insecure }
    }

    pub async fn connect<'a>(
        &self,
        url: &str,
        buffer: &'a mut [u8],
    ) -> Result<WsSession<'a, TlsConnection<'a, embassy_net::tcp::TcpSocket<'a>, Aes128GcmSha256>>, WsError> {
        let (host, port, path) = parse_ws_url(url, 443, true)?;
        let addr = self.resolve(host, port).await?;

        let mut socket = embassy_net::tcp::TcpSocket::new(
            self.stack,
            &mut buffer[..1024],
            &mut buffer[1024..2048],
        );
        socket.connect(addr).await.map_err(|_| WsError::Connect)?;

        let mut record_read = vec![0u8; self.config.rx_buffer_size.max(4096)];
        let mut record_write = vec![0u8; self.config.tx_buffer_size.max(2048)];

        let mut tls = TlsConnection::<_, Aes128GcmSha256>::new(socket, &mut record_read, &mut record_write);
        let config = TlsConfig::new().with_server_name(host);
        let rng = EspRng::new();

        tls.open(TlsContext::new(&config, UnsecureProvider::new::<Aes128GcmSha256>(rng)))
            .await
            .map_err(|_| WsError::Connect)?;

        // Execute WebSocket upgrade over TLS stream
        // ... (Handshake via edge-http connection over tls stream)
    }
}
```

### 7.7 URL Parser for WebSockets
```rust
pub(crate) fn parse_ws_url(url: &str, default_port: u16, secure: bool) -> Result<(&str, u16, &str), WsError> {
    let rest = if secure {
        url.strip_prefix("wss://").ok_or(WsError::BadUrl)?
    } else {
        url.strip_prefix("ws://").ok_or(WsError::BadUrl)?
    };

    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(WsError::BadUrl);
    }

    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            let port = port.parse().map_err(|_| WsError::BadUrl)?;
            Ok((host, port, path))
        }
        _ => Ok((authority, default_port, path)),
    }
}
```

---

## 8. Application Code Integration (`app.rs`)

### 8.1 Example 1: Plain WebSocket Echo & Command Loop
```rust
use crate::{component, service, Context};
use embassy_executor::Spawner;

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("Connecting to Wi-Fi...");
    component!(ctx, home).wait_connected().await;
    ctx.logger.info("Wi-Fi connected. Ready for WebSocket communication.");
}

pub async fn forever(ctx: &'static Context) {
    let ws = service!(ctx, ws);
    let mut conn_buf = [0u8; 4096];
    let mut rx_buf = [0u8; 1024];

    ctx.logger.info("Opening WebSocket connection to ws://echo.local/ws...");
    match ws.connect("ws://echo.local/ws", &mut conn_buf).await {
        Ok(mut session) => {
            ctx.logger.info("Connected! Sending greeting...");
            let _ = session.send_text("Hello from ESP32!").await;

            // Read echo response
            match session.recv(&mut rx_buf).await {
                Ok(msg) => {
                    if let Ok(text) = core::str::from_utf8(msg.payload) {
                        ctx.logger.info(format_args!("Received echo: {text}"));
                    }
                }
                Err(e) => ctx.logger.error(format_args!("Recv error: {e:?}")),
            }

            let _ = session.close(1000).await;
        }
        Err(err) => ctx.logger.error(format_args!("Connection failed: {err:?}")),
    }

    ctx.delay.delay_ms(10000).await;
}
```

### 8.2 Example 2: Secure TLS 1.3 Telemetry Streaming
```rust
use crate::{component, service, Context};
use embassy_executor::Spawner;

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    component!(ctx, home).wait_connected().await;
    ctx.logger.info("Connected to network.");
}

pub async fn forever(ctx: &'static Context) {
    let wss = service!(ctx, secure_ws);
    let mut conn_buf = [0u8; 8192];
    let mut rx_buf = [0u8; 1024];

    match wss.connect("wss://telemetry.example.com/v1/stream", &mut conn_buf).await {
        Ok(mut session) => {
            let payload = b"{\"sensor\":\"temp\",\"value\":24.5}";
            let _ = session.send_text(core::str::from_utf8(payload).unwrap()).await;

            match session.recv(&mut rx_buf).await {
                Ok(msg) => ctx.logger.info(format_args!("Ack: {} bytes", msg.payload.len())),
                Err(e) => ctx.logger.warn(format_args!("Stream error: {e:?}")),
            }
        }
        Err(e) => ctx.logger.error(format_args!("WSS failed: {e:?}")),
    }

    ctx.delay.delay_ms(5000).await;
}
```

---

## 9. Implementation Roadmap & Milestones

```
T0: Branch & baseline test verification
T1: Schema elements (<ws_client>, <wss_client>) & Diagnostics codes in espforge-diagnostics
T2: Runtime implementation in espforge-runtime/src/services/ (ws_client.rs, wss_client.rs)
T3: Element lowering & Board IR in espforge-elements (services/ws_client.rs, services/wss_client.rs)
T4: Codegen in espforge-emit (cargo.rs dependencies and rust.rs Services emission)
T5: Embedded examples & golden baseline (05.Networking/ws_client, 05.Networking/wss_client)
T6: Hardware-free compilation gate via generated_projects_compile
```

### Step-by-Step Execution Plan

#### T1 — Schema & Diagnostics
1. Add `SERVICE_UNKNOWN_NETWORK` (`E0801`), `SERVICE_REQUIRES_EMBASSY` (`E0802`), `SERVICE_BUFFER_TOO_SMALL` (`E0803`), `SERVICE_INSECURE_WARNING` (`W0804`), `SERVICE_INVALID` (`E0805`) to `espforge-diagnostics/src/codes.rs`.
2. Add `<ws_client>` and `<wss_client>` blocks to `espforge-schema/src/espf.schema.kdl`.
3. Verify: `cargo test -p espforge-schema` and `cargo test -p espforge-diagnostics`.

#### T2 — Runtime Types in `espforge-runtime/src/services/`
1. Add `edge-ws = { version = "0.8", default-features = false, optional = true }` and `rand_core` to `espforge-runtime/Cargo.toml`.
2. Configure `ws-client` and `wss-client` features.
3. Create `espforge-runtime/src/services/ws_client.rs` and `wss_client.rs`.
4. Export types through `src/services/mod.rs` and `src/lib.rs`.
5. Verify host and target compilations:
   ```bash
   cargo test -p espforge-runtime
   cargo check -p espforge-runtime --features wss-client,esp-hal/esp32c3,esp-radio/esp32c3,esp-radio/unstable --target riscv32imc-unknown-none-elf
   ```

#### T3 — Lowering in `espforge-elements`
1. Update `model.rs` with `Service::WsClient` and `Service::WssClient` variants in `service_enum!`.
2. Implement `Element` and `Lowered` in `services/ws_client.rs` and `services/wss_client.rs`.
3. Implement `LowerCx` validation checks (network provider, embassy runtime, buffer floors).
4. Verify: Unit tests checking validation success and diagnostic failures.

#### T4 — Codegen in `espforge-emit`
1. Extend `Clients` in `codegen/cargo.rs` to track `ws` and `wss`.
2. In `codegen/rust.rs`, ensure `heap_line` selects `96 * 1024` if `board.uses_wss()`.
3. Update `rust.rs` to emit `Services` fields and initialization for `WsClient` and `WssClient`.
4. Verify: `cargo test -p espforge-emit`.

#### T5 — Embedded Examples & Goldens
1. Add `espforge-examples/src/examples/05.Networking/ws_client/` and `wss_client/`.
2. Create `example.espf.txt`, `example.toml`, and `app/rust/app.rs` for each.
3. Baseline golden project outputs in `tests/golden/`.
4. Verify: `cargo test -p espforge --test golden_projects`.

#### T6 — Compile Check Gate & Final Cleanliness
1. Add cases to `espforge/tests/generated_projects_compile.rs`.
2. Run target build:
   ```bash
   cargo test -p espforge --test generated_projects_compile -- --ignored
   ```
3. Run workspace-wide checks:
   ```bash
   cargo fmt --check
   cargo clippy --workspace
   cargo test --workspace
   ```

---

## 10. Memory Budget & Technical Constraints

1. **Stack Memory Headroom:**
   RFC 6455 frame header parsing is minimal (2 to 14 bytes per frame header). Masking is applied in-place. Standard Embassy task stack allocations (2 KB to 4 KB) are sufficient for WebSocket processing.
2. **Heap Memory Allocation:**
   - Plain `<ws_client>`: The default `72 * 1024` (72 KiB) heap used for Wi-Fi station mode is sufficient.
   - Secure `<wss_client>`: Requires `96 * 1024` (96 KiB) heap to hold `embedded-tls` 1.3 crypto scratch buffers and TLS record framing rings.
3. **Socket Resources:**
   Active WebSocket sessions hold an open TCP socket. The `StackResources<4>` pool allocated in `espforge-runtime/src/hw/wifi.rs` supports concurrent DHCP, DNS, and up to 2 active TCP sockets (allowing HTTP + WebSocket concurrent use).

---

## 11. Acceptance Criteria / Definition of Done

- [ ] `espf.schema.kdl` defines `<ws_client>` and `<wss_client>` with complete validation attributes and self-contained examples.
- [ ] `espforge-runtime/src/services/` implements `WsClient`, `WssClient`, and `WsSession` using `edge-ws` and `edge-http`.
- [ ] RFC 6455 client masking is automatically applied to all outgoing client frames.
- [ ] Diagnostic codes `E0801`–`E0805` and `W0804` are emitted on configuration errors.
- [ ] Generated `Cargo.toml` activates `ws-client` or `wss-client` features on `espforge-runtime` selectively.
- [ ] User applications interact via `service!(ctx, ws)` using an ergonomic, non-allocating async API.
- [ ] Both `ws_client` and `wss_client` generated projects compile cleanly under `cargo check` for `riscv32imc-unknown-none-elf`.
- [ ] All existing golden tests (HelloWorld, Blink, Button, I2C, SPI, UART, WiFi, HTTP, HTTPS) remain 100% green without regressions.