# espforge — cheat sheet

> Fast, declarative firmware generation for ESP32 with Rust.
> `cheat.sh`-style quick reference for everyday workflows and examples.

---

## 1. Quick Start (From Zero to Built Firmware)

```bash
# 1. Discover built-in examples
espforge examples

# 2. Scaffold a new project from an example
espforge new blink my-app

# 3. Enter your project
cd my-app

# 4. Verify everything is valid without writing files
espforge check

# 5. Build your firmware (generates project into build/ and runs cargo build)
espforge build
```

---

## 2. Listing Examples (`espforge examples`)

List all embedded examples, grouped by category with their descriptions.

```bash
espforge examples
```

#### Output
```text
01.Basics
  blink                 BLINK EXAMPLE
  blink_async           ASYNC BLINK EXAMPLE
  helloworld            HELLO WORLD EXAMPLE
  minimal               MINIMAL EXAMPLE

02.Digital
  button                BUTTON EXAMPLE
  button_async          ASYNC BUTTON EXAMPLE

04.Communication
  i2c_example           I2C EXAMPLE
  i2c_example_async     I2C EXAMPLE (ASYNC)
  i2c_scanner           I2C SCANNER
  spi_example           SPI EXAMPLE
  spi_example_async     SPI EXAMPLE (ASYNC)
  uart_example          UART EXAMPLE
  uart_example_async    UART EXAMPLE (ASYNC)

05.Networking
  http_client           HTTP CLIENT EXAMPLE
  https_client          HTTPS CLIENT EXAMPLE
  ntp                   NTP CLIENT EXAMPLE
  wifi_station          WIFI STATION EXAMPLE
  ws_client             WEBSOCKET CLIENT EXAMPLE
  wss_client            SECURE WEBSOCKET CLIENT EXAMPLE

06.Displays
  ili9341               ILI9341 TFT EXAMPLE
  ssd1306               SSD1306 OLED EXAMPLE

Create a project:  espforge new <example> <dir>
```

---

## 3. Creating a Project (`espforge new`)

Create a runnable source project from any example name.

```bash
# Scaffold by short name
espforge new blink my-firmware

# Scaffold by full category key (also supported)
espforge new 01.Basics/blink my-firmware

# Scaffold network or display examples
espforge new wifi_station wifi-demo
espforge new ili9341 display-demo
```

### What `new` creates:
```text
my-firmware/
├── blink.espf       <-- Project hardware & pin configuration (edit this)
├── src/
│   └── app.rs       <-- Your application logic: setup() and forever() (edit this)
└── diagram.json     <-- Wokwi simulation diagram (if the example includes one)
```

The example is copied whole (including a custom Wokwi chip, where the example
has one). The `.espf` keeps the example's title banner and drops the
`ESPF FILE FORMAT` appendix, which documents the format rather than the project.
The destination directory must be absent or empty; `new` never merges.

> **Important**:
> Always edit `src/app.rs` in your project root, **not** inside `build/`.
> The `build/` folder is regenerated on every build.

---

## 4. Validating Projects (`espforge check`)

Check hardware pin assignments, peripherals, and application files without writing anything to disk.

`check` does not generate the crate: run `espforge build` to prove the project
compiles on your toolchain.

```bash
# Check the project in the current directory (finds the .espf file automatically)
espforge check

# Check a specific directory or file
espforge check ./my-firmware
espforge check my-firmware/blink.espf

# Check an embedded example directly with target overrides
espforge check --example blink --chip esp32s3
espforge check --example helloworld --runtime embassy
```

---

## 5. Generating & Building (`espforge gen` & `espforge build`)

### `espforge gen` (Generate Cargo crate only)
```bash
# Generate inside current project directory (outputs to ./build)
espforge gen

# Generate for a specific ESPF file to a custom folder
espforge gen blink.espf --out dist

# Generate directly from an example without creating a source folder
espforge gen --example blink --out /tmp/blink-build
```

### `espforge build` (Generate Cargo crate + run `cargo build`)
```bash
# Build current project in debug mode
espforge build

# Build with optimizations (cargo build --release)
espforge build --release

# Build for a specific chip target
espforge build --chip esp32s3

# Build with an alternate runtime (blocking or embassy)
espforge build --runtime embassy
```

---

## 6. Common Recipes & Overrides

### Override chip target on the fly
You do not need to rewrite your `.espf` file to target a different ESP32 chip:
```bash
espforge build --chip esp32s3
espforge build --chip esp32c3
```

### Override serial console output
```bash
espforge build --console jtag-serial
espforge build --console uart
```

### Pass custom build options
```bash
# Disable memory allocator:
espforge build --option alloc=false

# Fill default options automatically:
espforge build --defaults true
```

---

## 7. Command Cheat Sheet

| Command | Action |
| --- | --- |
| `espforge examples` | List all available examples and short names |
| `espforge new <example> <dir>` | Scaffold a new source project |
| `espforge check [path]` | Validate configuration and app logic without writing files |
| `espforge gen [path] [--out <dir>]` | Generate the Cargo project (stops before compilation) |
| `espforge build [--release]` | Generate the project and run `cargo build` in `build/` |
| `espforge build --chip <chip>` | Build while overriding the target chip (e.g. `esp32s3`) |
| `espforge --help` | Display CLI help and options |

