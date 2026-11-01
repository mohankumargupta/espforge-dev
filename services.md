Here is the complete, self-contained specification document `services.md`. You can commit this directly into your repository.

---

# `services.md`

> **Design and Implementation Specification**  
> **Topic:** Unifying and Declarativizing `# services` via `service_recipe`  
> **Status:** Draft / Ready for Implementation  
> **Precedents:** `newdriverdesign.md` (the device recipe tier), `recipe.md` (the component recipe tier)

---

## 1. Problem Statement & Motivation

In previous phases, `espforge` eliminated cross-crate coupling when adding components and devices by introducing recipes:
- **`# components`** uses `recipe` in `espf.schema.kdl`, allowing GPIO elements (`<led>`, `<button>`) to be declared purely in schema data without a dedicated `.rs` file in `espforge-elements`.
- **`# devices`** uses `device_recipe` in `drivers.schema.kdl`, allowing drivers (`<ssd1306>`, `<ili9341>`, `<scd4x>`) to be added by editing only `espforge-drivers`, completely freeing compiler crates from per-device edits.

However, **`# services`** (`<http_client>`, `<https_client>`, `<ws_client>`, `<wss_client>`, `<ntp>`) still suffers from the exact legacy problem that `# devices` once had:

1. **Repetitive Hand-Written Elements in `espforge-elements`**:
   - `src/services/http_client.rs`
   - `src/services/https_client.rs`
   - `src/services/ws_client.rs`
   - `src/services/wss_client.rs`
   - `src/services/ntp.rs`  
   All 5 files implement identical validation logic: check if ID is a valid Rust ident, check if runtime is `embassy`, verify that `network="$name"` exists in `LowerCx`, bounds-check buffer sizes, and push a struct into `cx`.
2. **Rigid Model Macro (`service_enum!`)**:
   Adding or modifying any service requires updating `service_enum!` and adding custom struct variants to `espforge-elements/src/model.rs`.
3. **Hardcoded Feature Inspection in `espforge-emit`**:
   `espforge-emit/src/codegen/cargo.rs` maintains an explicit `Clients` struct:
   ```rust
   pub struct Clients {
       pub http: bool,
       pub https: bool,
       pub ws: bool,
       pub wss: bool,
       pub ntp: bool,
   }
   ```
   And `espforge-elements/src/model.rs` maintains manual inspection methods on `Board`:
   ```rust
   board.uses_http()
   board.uses_https()
   board.uses_ws()
   board.uses_wss()
   board.uses_ntp()
   ```

Adding a new network service (e.g., `mqtt_client`, `coap_client`, `dns_sd`) currently requires editing:
1. `espforge-runtime` (runtime implementation & feature)
2. `espforge-schema` (`espf.schema.kdl`)
3. `espforge-elements` (hand-written `Element` + `model.rs` enum + predicate method)
4. `espforge-emit` (`cargo.rs` struct + `runtime_dependency` mapping + `mod.rs`)

---

## 2. Target Workflow: Adding a Service

Adding a new service (e.g. `mqtt_client`) should require editing **only two places**:

1. **`espforge-runtime`**: Implement the client and add its feature flag in `Cargo.toml`.
2. **`espforge-schema/src/espf.schema.kdl`**: Declare the element and its `service_recipe`.

**Zero edits** in:
- `espforge-elements`
- `espforge-emit`
- `espforge-core`

---

## 3. The `service_recipe` Schema Design

A service in `espforge` represents software running over an established network interface. It is characterized by:
- A `network` reference pointing to a network component (e.g. `<wifi_station>`).
- A target feature to enable on `espforge-runtime`.
- A configuration struct and constructor in `espforge-runtime`.
- Optional constraints (e.g. `requires_embassy`, buffer floors, heap requirements).

### 3.1 KDL Grammar

In `espforge-schema/src/espf.schema.kdl`:

```kdl
element "ntp" section="services" {
    summary "Async SNTP client for wall-clock time synchronization"
    description "Queries NTP/SNTPv4 servers (RFC 5905) over an established network interface using sntpc."
    service_recipe {
        network "network"
        feature "ntp"
        type "espforge_runtime::NtpClient"
        new "espforge_runtime::NtpClient::new"
        config "espforge_runtime::NtpConfig"
        field "server"
        field "port"
        field "timeout_ms" as="timeout_ms as u64"
    }
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
    example "minimal" #"<ntp id="clock" network="$home" />"#
}
```

For services with crypto/TLS requirements and extra constructor arguments (such as `https_client`):

```kdl
element "https_client" section="services" {
    summary "Async TLS 1.3 encrypted HTTPS client over an established network interface"
    service_recipe {
        network "network"
        feature "https-client"
        type "espforge_runtime::HttpsClient"
        new "espforge_runtime::HttpsClient::new"
        config "espforge_runtime::HttpsClientConfig"
        heap_bytes 98304 // 96 * 1024
        field "rx_buffer_size" as="rx_buffer_size as usize" min=4096
        field "tx_buffer_size" as="tx_buffer_size as usize"
        field "timeout_ms" as="timeout_ms as u64"
        arg "insecure"
    }
    attr "id" kind="ident" required=#true
    attr "network" kind="ref" to="wifi_station" required=#true
    attr "rx_buffer_size" kind="u32" default="4096"
    attr "tx_buffer_size" kind="u32" default="2048"
    attr "timeout_ms" kind="u32" default="15000"
    attr "insecure" kind="bool" default="false"
    example "minimal" #"<https_client id="client" network="$home" />"#
}
```

### 3.2 Recipe Attributes & Properties

| Field | Meaning |
|---|---|
| `network "<attr>"` | The name of the `ref` attribute pointing to the network interface. |
| `feature "<name>"` | The feature flag to activate on `espforge-runtime`. |
| `type "<path>"` | The Rust type of the service field on `Services`. |
| `new "<path>"` | The constructor function path. |
| `config "<path>"` | *(Optional)* The configuration struct name passed to `new`. If omitted, arguments are passed directly to `new`. |
| `heap_bytes <int>` | *(Optional)* Minimum heap size in bytes needed in `main.rs` (e.g. 96 KiB for TLS). |
| `requires_embassy` | Defaults to `#true`. Validates that `runtime == "embassy"`. |
| `field "<attr>"` | Maps an attribute to a named field in the `config` struct. Supports `min=<int>` for buffer checks and `as="..."` for type casts. |
| `arg "<attr>"` | Passes an attribute directly as a positional argument to `new` after the stack and config. |

---

## 4. AST Lowering & Board IR (`espforge-elements`)

### 4.1 Data-Driven `Service` Struct in `model.rs`

Replace `service_enum!` and individual service structs in `espforge-elements/src/model.rs` with a single data-driven `Service` struct (mirroring `Device`):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub id: Ident,
    pub tag: &'static str,
    pub network: Ident,
    pub feature: String,
    pub ty: String,
    pub new: String,
    pub config: Option<String>,
    pub heap_bytes: Option<u32>,
    pub config_fields: Vec<(String, String)>,
    pub extra_args: Vec<String>,
}

impl Service {
    pub fn id(&self) -> &Ident {
        &self.id
    }
}
```

### 4.2 Generic `Lowered` Implementation for `Service`

```rust
impl crate::element::Lowered for Service {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut crate::element::EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let ty: TokenStream = self.ty.parse().expect("valid service type");
        let new: TokenStream = self.new.parse().expect("valid constructor path");
        let stack = network_stack(&self.network);

        cx.service_field(quote! {
            pub #name: #ty,
        });

        let init_expr = if let Some(config_name) = &self.config {
            let config_ty: TokenStream = config_name.parse().expect("valid config type");
            let fields: Vec<TokenStream> = self
                .config_fields
                .iter()
                .map(|(field, expr)| {
                    let f = format_ident!("{field}");
                    let val: TokenStream = expr.parse().expect("valid field expression");
                    quote! { #f: #val, }
                })
                .collect();

            let extra: Vec<TokenStream> = self
                .extra_args
                .iter()
                .map(|arg| arg.parse().expect("valid extra arg"))
                .collect();

            quote! {
                #new(
                    #stack,
                    #config_ty {
                        #(#fields)*
                    },
                    #(#extra),*
                )
            }
        } else {
            let extra: Vec<TokenStream> = self
                .extra_args
                .iter()
                .map(|arg| arg.parse().expect("valid arg"))
                .collect();

            quote! {
                #new(#stack, #(#extra),*)
            }
        };

        cx.service_init(quote! {
            #name: #init_expr,
        });
    }
}
```

### 4.3 Generic `ServiceRecipeElement` Interpreter

Create `espforge-elements/src/services/recipe.rs` to lower any schema element with a `service_recipe`:

1. **AST Validation:** Run `generated::validate(tag, node)` to enforce schema constraints.
2. **Identifier Check:** Validate that `id` satisfies `is_rust_ident`.
3. **Runtime Check:** Verify `cx.runtime() == Runtime::Embassy`. If not, emit `codes::SERVICE_REQUIRES_EMBASSY`.
4. **Network Interface Check:** Ensure `cx.has_network_provider(&network_name)`. If not, emit `codes::SERVICE_UNKNOWN_NETWORK`.
5. **Buffer Size Floors:** Inspect any `field` with `min=<limit>`. If the attribute's value is below `min`, emit `codes::SERVICE_BUFFER_TOO_SMALL`.
6. **Insecure TLS Warning:** If an `insecure` attribute is present and evaluates to `true`, emit `codes::SERVICE_INSECURE_WARNING`.
7. **Build IR:** Assemble `Service` and push to `cx.push_service(...)`.

### 4.4 Distributed Registration

In `espforge-elements/src/registry.rs`, add a third recipe door:

```rust
pub fn service_recipes() -> &'static [ServiceRecipeElement] {
    static RECIPES: OnceLock<Vec<ServiceRecipeElement>> = OnceLock::new();
    RECIPES.get_or_init(|| {
        schema()
            .elements
            .iter()
            .filter(|element| element.service_recipe.is_some())
            .map(ServiceRecipeElement::new)
            .collect()
    })
}

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
        .or_else(|| {
            device_recipes()
                .iter()
                .find(|element| element.tag() == tag)
                .map(|element| element as &'static dyn Element)
        })
        .or_else(|| {
            service_recipes()
                .iter()
                .find(|element| element.tag() == tag)
                .map(|element| element as &'static dyn Element)
        })
}
```

---

## 5. Dynamic Code Generation (`espforge-emit`)

### 5.1 Dynamic Features on `Board`

In `espforge-elements/src/model.rs`:

```rust
impl Board {
    /// Distinct service features requested by the board in declaration order.
    pub fn service_features(&self) -> Vec<&str> {
        self.services
            .iter()
            .map(|service| service.feature.as_str())
            .fold(Vec::new(), |mut out, feature| {
                if !out.contains(&feature) {
                    out.push(feature);
                }
                out
            })
    }

    /// Maximum heap required by any service on the board (e.g. TLS scratch).
    pub fn max_service_heap(&self) -> Option<u32> {
        self.services.iter().filter_map(|s| s.heap_bytes).max()
    }
}
```

### 5.2 Dynamic `runtime_dependency` in `codegen/cargo.rs`

Delete `struct Clients`. Update `runtime_dependency` in `espforge-emit/src/codegen/cargo.rs`:

```rust
fn runtime_dependency(
    source: &RuntimeSource,
    embassy: bool,
    wifi: bool,
    board: &Board,
) -> Value {
    let mut table = InlineTable::new();
    if let RuntimeSource::Path(path) = source {
        table.insert("path", Value::from(path.replace('\\', "/")));
    }
    table.insert("version", Value::from(RUNTIME_VERSION));

    let service_features = board.service_features();
    let mut features: Vec<&str> = Vec::new();

    if embassy || wifi || !service_features.is_empty() {
        features.push("embassy");
    }
    if wifi {
        features.push("wifi");
    }

    for feat in service_features {
        if !features.contains(&feat) {
            features.push(feat);
        }
    }

    if !features.is_empty() {
        table.insert("features", Value::Array(array(&features)));
    }
    Value::InlineTable(table)
}
```

### 5.3 Dynamic Heap Sizing in `codegen/rust.rs`

Replace hardcoded service checks in `heap_line`:

```rust
fn heap_line(board: &Board, opts: &ResolvedOptions) -> TokenStream {
    if let Some(heap_size) = board.max_service_heap() {
        let size = Literal::u32_unsuffixed(heap_size);
        quote::quote! { esp_alloc::heap_allocator!(size: #size); }
    } else if opts.is_on("wifi") {
        quote::quote! { esp_alloc::heap_allocator!(size: 72 * 1024); }
    } else {
        quote::quote! {}
    }
}
```

---

## 6. Migration Plan & File Changes

### Step 1: `espforge-schema`
- Extend parser in `espforge-schema/src/lib.rs` to parse `service_recipe`.
- Add `ServiceRecipe` and `ServiceField` structs to `espforge-schema`.
- Annotate `http_client`, `https_client`, `ws_client`, `wss_client`, and `ntp` in `espf.schema.kdl` with `service_recipe`.

### Step 2: `espforge-elements`
- Add `espforge-elements/src/services/recipe.rs`.
- Replace `service_enum!` in `model.rs` with `pub struct Service { ... }`.
- Add `service_recipes()` to `espforge-elements/src/registry.rs`.
- Delete:
  - `espforge-elements/src/services/http_client.rs`
  - `espforge-elements/src/services/https_client.rs`
  - `espforge-elements/src/services/ws_client.rs`
  - `espforge-elements/src/services/wss_client.rs`
  - `espforge-elements/src/services/ntp.rs`

### Step 3: `espforge-emit`
- Remove `struct Clients` from `espforge-emit/src/codegen/cargo.rs`.
- Wire `board.service_features()` into `cargo.rs`.
- Wire `board.max_service_heap()` into `rust.rs`.

### Step 4: Verification
- Run `cargo test --workspace`.
- Verify that all networking golden tests remain 100% byte-identical.
- Verify `cargo test -p espforge --test generated_projects_compile -- --ignored`.
