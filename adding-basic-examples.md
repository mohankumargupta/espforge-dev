To add `minimal` and `blink_async` under `01.Basics`, create the following directories and files inside `espforge-examples/src/examples/01.Basics/`.

---

### 1. `01.Basics/minimal`

#### `espforge-examples/src/examples/01.Basics/minimal/example.espf.txt`
```
====================
MINIMAL EXAMPLE
====================

# espforge
  <project name="minimal" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals

# components

# devices

# services
```

#### `espforge-examples/src/examples/01.Basics/minimal/app/rust/app.rs`
```rust
#[allow(unused_variables)]
use crate::Context;

pub fn setup(ctx: &mut Context) {}

pub fn forever(ctx: &mut Context) {}
```

#### `espforge-examples/src/examples/01.Basics/minimal/example.toml`
```toml
chips    = ["esp32c3", "esp32s3"]
runtimes = ["blocking"]
```

---

### 2. `01.Basics/blink_async`

#### `espforge-examples/src/examples/01.Basics/blink_async/example.espf.txt`
```
====================
ASYNC BLINK EXAMPLE
====================

# espforge
  <project name="blink_async" />
  <chip type="esp32c3" />
  <runtime type="embassy" />

# peripherals
  <gpio id="gpio18" pin="18" direction="output" />

# components
  <led id="red_led" gpio="$gpio18" />

# devices

# services
```

#### `espforge-examples/src/examples/01.Basics/blink_async/app/rust/app.rs`
```rust
use crate::{component, Context};
use embassy_executor::Spawner;

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("blink async example");
}

pub async fn forever(ctx: &'static Context) {
    let led = component!(ctx, red_led);
    led.toggle();
    ctx.delay.delay_ms(1000).await;
}
```

#### `espforge-examples/src/examples/01.Basics/blink_async/diagram.json`
```json
{
  "version": 1,
  "author": "espforge",
  "editor": "wokwi",
  "parts": [
    { "type": "{{ board }}", "id": "esp", "top": 0, "left": 0 },
    { "type": "wokwi-led", "id": "led1", "top": -40, "left": 96, "attrs": { "color": "red", "gpio": "{{ red_led.pin }}" } }
  ],
  "connections": [
    [ "led1:A", "esp:{{ red_led.pin }}", "green", [] ],
    [ "led1:C", "esp:GND.10", "black", [] ]
  ]
}
```

#### `espforge-examples/src/examples/01.Basics/blink_async/example.toml`
```toml
chips    = ["esp32c3", "esp32s3"]
runtimes = ["embassy"]
```

---

### Verification

Because `espforge-examples` uses `include_dir!("$CARGO_MANIFEST_DIR/src/examples")`, adding these directories automatically integrates them into the embedded example catalog. You can verify them with:

```bash
just list-examples
cargo test -p espforge_examples
```

And generate firmware projects from them:
```bash
cargo run -p espforge -- gen --example 01.Basics/minimal --out /tmp/minimal
cargo run -p espforge -- gen --example 01.Basics/blink_async --out /tmp/blink_async
```

