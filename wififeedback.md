# wifi.md: Wi-Fi in espforge

> Design for plan.md **Phase 4**. It replaces the Phase 4 text there ("`wifi` element",
> "`Services` as a real emitted struct", `ble-trouble`).
>
> **Status:** revision 3, after review. The porting half is **implemented on this branch**
> (section 3); the shape change (section 4) is the next commit. Section 12 is what is left
> of the old prerequisite list, and it is short on purpose.
>
> **What changed in this revision:**
> - The work is **split in two commits** so the legacy golden keeps working as an oracle for
>   the half where it is still true (section 2).
> - The prerequisite list went from five items to **none outstanding**: the `wifi` option
>   landed with the porting half, and the two proposals that were prerequisites move to the
>   commits that can actually use them (sections 4.4, 12).
> - The validation table now says which rules are **ported** and which are **new**, because
>   the proposed `auth` model deliberately breaks one legacy rule (section 6.1).
> - `EmitCx::cargo_feature` and `EmitCx::needs_heap` were dropped: with the `wifi` option
>   landed they would have no caller, and the phase gate says not to add a seam before the
>   phase that uses it (section 7.1).
> - The `main()` startup contract is **typed**, not stringly (section 8.3).
> - The `env:` syntax is a decision to take in this phase, not after it (section 8.4).

---

## 1. Why one `<wifi>` element is wrong

"Wi-Fi" is several things at different layers:

- a **radio**: the `WIFI` hardware block, and which chips have it;
- a **link**: station or access point, SSID, authentication;
- an **interface**: something with an IP stack, which Wi-Fi shares with Ethernet;
- **protocols** on top: NTP, MQTT, HTTP, sockets.

One element in `# services` folds all four together, hides the hardware claim from conflict
detection, and can't be extended to Ethernet, ESP-NOW, BLE or Thread. ESPF's five sections
already are layers, so Wi-Fi is spread across them. (Today `wifi` in `# services` exists only
as a special case in the legacy tree.)

## 2. How the work is sequenced, and why

The legacy `05.Networking.wifi_station.esp32c3.embassy` golden is the only full description
we have of a working Wi-Fi project, and it is about to become unusable: the shape changes
(`Services::new` becomes a component plus a startup hook) and so does the input (the example
uses `service!`, not `component!`).

So the change is split at the point where the oracle stops being true:

| | Commit A: the porting half — **done** | Commit B: the shape change — **next** |
|---|---|---|
| Runtime | `wifi` feature, `Wifi`/`WifiConfig`/`WifiError` | — |
| `Cargo.toml` | the radio dependency block | — |
| `main()` | the 72 KiB heap | `EmitCx::startup` statements |
| Schema | `option "wifi"` | `<wifi>`, `<wifi_station>` |
| `generated.rs` | unchanged | `WifiStation` field, no `Services` |
| Oracle | **byte-identical** to the golden's `[dependencies]` and heap line | baselined once, reviewed |

Everything that the golden can still prove is proven in commit A (section 3.5). Everything it
cannot is baselined in commit B, once.

The two things that used to be prerequisites are not, and each moves to the commit that can
use it:

- **Duplicate-id-across-sections is an error** (today `i2c_example_async` and `i2c_scanner`
  use `i2c0` for both a peripheral and a component, and only work because the lookups are
  separate). It is a real bug, but no Wi-Fi element needs it: the ids here are `radio` and
  `home`, which do not collide. Making it an error re-baselines two phase-3 cells, which is
  exactly what the phase gate ("all earlier goldens stay byte-identical") forbids in a phase
  that is not about ids. **Its own commit.**
- **The layer-rank rule** ("a `ref to=` target must be in the same or a lower-ranked
  section") has no consumer in phase 4: `# services` stays empty until `<ntp>` does something.
  Rule and check land with the first element that can violate them, and with the
  same-section cycle check for `component → component` references. **Its own commit.**

Rank, once needed:

| Rank | Section | Wi-Fi example |
|---|---|---|
| 0 | `# espforge` | `<options>`; the derived `wifi` option |
| 1 | `# peripherals` | `<wifi>`: the radio |
| 2 | `# components` | `<wifi_station>`, later `<wifi_ap>`, `<espnow>`, `<ble>` |
| 3 | `# devices` | An external module (ESP-AT, W5500). **Not** the on-chip radio |
| 4 | `# services` | `<ntp>`, `<mqtt>`, `<http_server>` |

## 3. The porting half (done)

Commits on this branch, with the tests that hold them:

**3.1 The `wifi` option.** `espf.schema.kdl` gains

```kdl
option "wifi" display_name="Enable Wi-Fi via the esp-radio crate." derived=#true {
    requires "alloc" "unstable-hal" "embassy"
    help "esp-radio comes with no stability guarantees at this time. Wi-Fi also allocates a heap."
}
```

with **no** `compatible` chip list (section 3.2 explains why). `DerivedInputs` now carries the
`# peripherals` nodes instead of the `# services` ones, and the derivation is "a `<wifi>`
peripheral exists", so the value follows the radio rather than wherever a station is written.

`requires` is not decoration: it is the only mechanism that turns `alloc` and `unstable-hal`
back **on** for a user who wrote `<options alloc="false" unstable-hal="false" />`. `alloc` is
load-bearing — it is what emits the `esp-alloc` dependency at all, and the `esp-alloc` feature
on `esp-rtos` and `esp-radio`, and it is the heap the radio allocates from. `unstable-hal` is
kept because the legacy option required it and the whole radio stack is unstable; if it turns
out that nothing in the `wifi` path needs esp-hal's unstable API, dropping it is a one-line
schema change plus a rebuild to prove it. Tests: `derived_flags_follow_runtime_and_peripherals`,
`the_wifi_flag_says_where_the_radio_is`, `requires_error_when_user_turned_the_required_flag_off`.

**3.2 The chip gate.** `chips::has_wifi(name)` answers from `esp-metadata`'s symbol table
(`Chip::contains("soc_has_wifi")`) — verified in the pinned `esp-metadata-generated` 0.5.3,
where `esp32h2` and `esp32p4` are the only chips without `soc_has_wifi`. A new chip is
therefore correct without editing Rust, which is what the legacy 9-chip `compatible` list
could not manage. Tests: `the_radio_follows_the_chip` (c3/s3/esp32 yes, h2/p4 no),
`an_unknown_chip_has_no_radio`.

**3.3 The radio dependency block.** `cargo.rs` emits `embedded-io`, `embedded-io-async`,
`embassy-net`, `smoltcp` and `esp-radio`, adds the `esp-radio` feature to `esp-rtos`, and asks
`espforge-runtime` for features `["embassy", "wifi"]`. Test:
`wifi_brings_the_radio_stack_verbatim` compares the **whole** rendered `[dependencies]` table
against `legacy/espforge/tests/golden/05.Networking.wifi_station.esp32c3.embassy/Cargo.toml`,
so the `esp-rtos` feature, the `edge-net` comment and the long `smoltcp` feature list are all
pinned. Nothing in the block is driven by a hand-kept list.

**3.4 The heap.** `rust.rs::needs_heap(opts)` emits `esp_alloc::heap_allocator!(size: 72 *
1024);` in the embassy `main`, between `esp_hal::init` and `PeripheralRegistry::new`, spelled
exactly as the golden spells it. Test: `wifi_installs_the_heap_the_radio_needs` (line and
position), plus `a_project_without_a_radio_allocates_nothing`.

**3.5 The runtime.** `espforge-runtime` gains a `wifi` feature that **implies** `embassy` and
enables the optional `esp-radio`, `embassy-executor` and `embassy-net`; `Wifi`, `WifiConfig`
and `WifiError` are ported to `src/hw/wifi.rs`, gated `#[cfg(feature = "wifi")]` rather than
on `embassy`, so an embassy project without a radio never pulls in the stack. Verified by
cross-compiling the feature alone:

```
cargo check -p espforge-runtime --features wifi,esp-hal/esp32c3,esp-radio/esp32c3,esp-radio/unstable \
    --target riscv32imc-unknown-none-elf
```

The chip features come from the generated project, as for `esp-hal`; `requires-unstable` is
what the runtime declares, and the generated `Cargo.toml` supplies `unstable`.

**3.6 What is deliberately not here.** No `<wifi>` element, no `<wifi_station>`, no example,
no golden, no `EmitCx::startup`. Each needs a shape decision, and each belongs to commit B.

## 4. The shape change (next)

**4.1 `<wifi>`, the radio, in `# peripherals`.**

```kdl
element "wifi" section="peripherals" {
    summary "The on-chip Wi-Fi radio"
    description "Claims the WIFI hardware block. One per chip."
    attr "id" kind="ident" required=#true
    example "minimal" #"<wifi id="radio" />"#
    see_also "components/wifi_station"
}
```

No pins, so no pin claims. It claims the peripheral name `WIFI` through
`ResourceGraph::claim_peripheral`: a second `<wifi>` is the existing peripheral clash (E0704);
a chip with no radio is a located error from `<wifi>`'s own lowering, using the `has_wifi`
gate from 3.2. `country`, channel and band are **not** in this block (section 10).

**4.2 `<wifi_station>`, in `# components`.**

```kdl
element "wifi_station" section="components" {
    summary "Connect to an existing Wi-Fi network"
    attr "id" kind="ident" required=#true
    attr "wifi" kind="ref" to="wifi" required=#true
    attr "ssid" kind="str" required=#true
    attr "auth" kind="choice" default="wpa2" { choices "open" "wpa2" }
    attr "password" kind="str" {
        help "Literal, or env:NAME to read NAME at build time. Warns: the value is compiled into the firmware. Required unless auth=\"open\"."
    }
    example "home network" #"..."#
    see_also "peripherals/wifi"
}
```

`auth` has only `open` and `wpa2` because the legacy generator only emitted those two; `wpa3`
arrives with the esp-radio call that sets it (section 10).

Not recipe-shaped (strings and a choice, no pin), so it is a hand-written `Element` in
`components/wifi_station.rs`, and the recipe-tier test does not apply to it.

Two attributes the schema cannot express, checked in lowering: an SSID length and a password
length. There is no length `range` kind, and adding one for two attributes is not worth a
schema feature.

**4.3 The one-station-per-radio rule**, like I2C: a second `<wifi_station>` on one `<wifi>` is
E0617, the same shape as E0611/E0612. Two radios on one chip are already impossible (the
peripheral claim in 4.1).

**4.4 Not yet, and why not now.**

- **The `network=` reference** (`<ntp network="$home">` pointing at a station or a future
  Ethernet device): `AttrKind::Ref { to: Vec<String> }` already holds several tags and the
  loader already collects **repeated** `to=`, so the form is repeated properties, not a comma
  list. It waits for the second target to exist, and it brings the layer-rank rule with it
  (section 2). A test that repeated `to=` yields every target belongs with the first element
  that has two, so the upgrade risk is covered where it matters.
- **`Services` as a real emitted struct:** empty until a service does something. The generated
  `Services {}` stays exactly as it is.

## 5. Example `.espf`

```
# espforge
  <project name="wifi_station" />
  <chip type="esp32c3" />
  <runtime type="embassy" />

# peripherals
  <wifi id="radio" />

# components
  <wifi_station id="home" wifi="$radio" ssid="my-network"
                password="env:WIFI_PASSWORD" />

# devices

# services
```

App side (embassy): `component!(ctx, home)` gives a handle with connection state and the
network stack for sockets — `component!(ctx, home).stack()`, then `wait_connected().await`.

The remote legacy example is not the model to copy: `legacy/.../05.Networking/wifi/http_example`
is stale and too deeply nested, and phase 5 removes it.

## 6. Lowering rules and diagnostics

One code per message; the numbers continue the legacy ranges (peripherals to E0514,
components to E0621). The range test in `codes.rs` grows to cover them.

| Situation | Where | Code | New? |
|---|---|---|---|
| Chip has no Wi-Fi radio (`esp32h2 has no Wi-Fi radio`), pointing at `<wifi>` | `<wifi>` lower | E0514 | new |
| Second `<wifi>` | `<wifi>` lower | E0704 | existing |
| `wifi="$x"` names no `<wifi>` | station lower | E0616 | new |
| Second station on one radio | station lower | E0617 | new |
| `ssid` empty or over 32 bytes | station lower | E0618 | new rule |
| Password required but missing, or not 8 to 63 characters | station lower | E0619 | new rule |
| `auth="open"` with a password | station lower | E0620 | new rule |
| `password="env:NAME"` with an invalid variable name | station lower | E0621 | new rule |
| Station under the `blocking` runtime | options | E0307 (`requires embassy`) | existing |

E0616 is deliberately not a reworded E0613: that code's message names I2C.

### 6.1 "The legacy validations" is the wrong description

The legacy rules (`legacy/espforge-hardware/src/services/wifi.rs`) are:

- `ssid`: **1 to 32 bytes**;
- `password`: **empty or 8 to 63 characters** — where empty means "open network", and the
  password is optional;
- neither may contain a **backslash** (the values were pasted straight into Rust string
  literals).

The proposed model is not a port of those:

- `auth` defaults to `wpa2`, so a password is **required** unless `auth="open"`. A legacy-style
  empty password becomes an error, not an open network. That is a deliberate format change and
  it should be described as one.
- The backslash rule cannot survive `env:NAME` at all: what reaches the literal is the
  environment variable's *name*, validated as an identifier, not its value. Keeping "no
  backslash" for the `ssid` and for a literal password is still worth it, because those do
  reach string literals verbatim; the escape is still on the writer.
- SSID length is counted in bytes in legacy. Say bytes, and say it in the message.

So: three of these are ports (SSID length, password length, no backslash), one is a new rule
(a password is refused with `auth="open"`), and the `env:` name check is new. Each gets a test.

## 7. Runtime features and Cargo

**The `wifi` feature** is landed (3.5). What commit B adds is not a feature but the *caller*:
a station's `Lowered::emit` has nothing to say about Cargo.

The trigger for both the dependency block and the heap is the resolved `wifi` **option**, and
that stays true in the final design: the option is derived from a `<wifi>` peripheral's
presence, which is exactly the fact the block depends on, and it is the fact the resolver can
already point an error at.

### 7.1 Why `EmitCx::cargo_feature` and `EmitCx::needs_heap` are not in this phase

The previous revision added both, to replace the legacy `opts.is_on("wifi") ||
opts.is_on("ble-trouble")` checks. With the option landed they have **no caller**, and the
phase gate's own risk row ("add `once`, `cargo_feature`, `needs_heap` only in the phase that
first uses them") says what to do about that: don't add them.

- The dependency block is fully described by one derived fact, and the element that causes it
  is the same element that causes the option. A feature list collected from components would
  be a second way to say the same thing, and the two could disagree (a `<wifi>` that lowers but
  never collects, or the reverse) with no test able to say which is right.
- The heap has one size and one trigger. `needs_heap(bytes)` matters when a *second* element
  wants a different size (BLE), and it should take the number then, so two elements cannot
  silently get whichever came first.

Where they come back: `cargo_dep` for devices in phase 5 (a per-device crate and version that
no option can carry), and a second heap size with the Bluetooth element. Both are additions to
`EmitCx` in the commit that has a caller.

**Do not copy versions from this document.** `espforge-emit`'s emitters are the source, and
the golden test in 3.3 is the oracle for the whole block.

## 8. Codegen: what commit B has to move

### 8.1 The ordering problem

In the embassy `main.rs`, `Components::new(&mut registry)` runs **before** `esp_rtos::start`,
because it borrows the whole registry before `TIMG0` and `FROM_CPU_INTR0` are moved out. The
radio needs the scheduler and the heap running. So the station must not bring up the radio in
`Components::new`.

### 8.2 The split

1. **Construct** (in `Components::new`, unchanged position): `WifiStation::new(config)` stores
   SSID, auth and password. It takes **no peripheral and uses no `unsafe`**.
2. **Start** (a new hook in `main()`, after `esp_rtos::start`): move `registry.peripherals.WIFI`
   **by value** into `components.home.start(wifi, spawner)`. By-value is sound because
   `claim_peripheral("WIFI")` guarantees nothing else took it. `start` takes `&mut self`,
   since `components` is a local in `main` at that point.

`Components` is moved into `Context` and then into a `StaticCell` afterwards, so `start` must
park the network stack, its resources and the spawned tasks' state in `'static` storage rather
than borrowing from `self`. The app-facing handle is `Copy` (like `I2cBus`), with interior
mutability only where a task awaits through it — `Wifi` already is that type.

### 8.3 The plumbing is "one hook", but not one function

`main()` ignores the board today, and `EmitParts` is built inside `generated()`. A component
cannot be asked to `emit` twice, so the `EmitCx` is collected **once**, in the Rust backend's
entry, and the parts are passed to both `rust::generated` (fields, inits, prelude, safety) and
`rust::main` (startup). `cargo::manifest` keeps reading the resolved options and does not
change (section 7).

- `EmitCx::startup(&mut self, statement: TokenStream)`: statements placed in `main()` after
the scheduler starts, in first-use order; `EmitParts` gains `startup`.
- The startup tokens name locals `main()` defines (`registry`, `spawner`, `components`). Make
  that **typed** rather than a documented sentence plus a test: hand the idents in
  (`struct MainCtx { registry: Ident, spawner: Ident, components: Ident }`) so a typo is a
  compile error, not a test failure. A test can only protect the reviewers who remember to
  run it; the type protects everyone.
- The blocking `main()` emits no startup statements, and `<wifi_station>` cannot reach it
  anyway (`wifi` requires `embassy`).

### 8.4 Passwords

`password="env:NAME"` generates a compile-time `env!("NAME")`. The secret is not in the
`.espf`, but it **is** in the built firmware; the attribute's `help` says both. A literal
password is allowed and gives a `Severity::Warning`, because the `.espf` is meant to be
committed.

**The `env:` syntax is decided here, not after the first release.** It is the only thing in
this design that fixes the file format, and "fix it later" is how it ships. `env:NAME` with a
`[A-Za-z_][A-Za-z0-9_]*` name is the proposal (E0621).

### 8.5 The `# services` section text goes stale in this phase

`espf.schema.kdl` still says `summary "Software services: wifi, ntp"` and `description
"Services are software, not hardware: nothing in them refers to a peripheral."` After commit B
Wi-Fi is not a service, and after `<ntp>` the second sentence is wrong too. The summary loses
`wifi` in commit B; the description is rewritten to "services refer to components or devices,
never peripherals" when the first service lands. It is two lines, and both belong to the
commit that makes them untrue.

## 9. The oracle

**Commit A keeps it.** The `[dependencies]` table and the heap line are byte-identical to the
legacy `wifi_station` golden, and two tests compare them directly (3.3, 3.4). Those two tests
read `legacy/` at test time, so if the golden is ever edited they fail rather than drift.

**Commit B gives it up, once, deliberately.** The shape changes and the input changes
(`service!` → `component!`), so the new golden is baselined in one reviewed commit, exactly as
plan.md says for a deliberate re-baseline. What commit A proved is not re-proved; it is
preserved, because the same emitters produce both.

The parts of the legacy tree that stay useful as a **reference** (not a diff) after commit B:
`legacy/espforge/tests/golden/05.Networking.wifi_station.esp32c3.embassy/src/generated.rs`
(the `Wifi::start` call shape and its doc comment) and `.../src/app.rs` (what the app side of a
station looks like).

## 10. Deferred

Each is a new element or attribute, added with its lowering — no attribute enters the schema
before something lowers it.

- `country` (needs the esp-radio call identified) and `wpa3`; power and band settings;
  dual-band (esp32c5).
- Channel, hidden SSIDs, static IP.
- `<wifi_ap>`, `<espnow>` (rank 2 components over the same `<wifi id="radio">`).
- Multiple remembered networks (dotted keys and `+=`, as the legacy example had).
- `<ntp>` beyond a stub, `<mqtt>`, `<http_server>`; ping. These bring the `network=`
  reference and the layer-rank rule (section 4.4).
- Bluetooth (`<bluetooth>` peripheral, `<ble>` component) and Zigbee/Thread
  (`<ieee802154>`); `ble-trouble` leaves this phase. When BLE lands, the radio block's `radio`
  condition gains its second caller and `coex` comes back.
- Ethernet via a W5500 device on an `spi_component`, as a second `network=` target.
- Matter, as a service on a `network=` reference.

## 11. What is left in this phase

Ordered; each line ends green and committed.

1. **`<wifi>` + `<wifi_station>` schema blocks, and the two elements** (4.1, 4.2, 4.3), with
   the corpus subset in the table in section 6: radio-less chip, duplicate radio, unknown
   reference, second station, and the four value rules.
2. **`EmitCx::startup`**, the typed `MainCtx`, and the once-only collection shared by
   `generated()` and `main()` (8.3). Nothing else is added to `EmitCx` in this phase (7.1).
3. **The example** (`05.Networking/wifi_station`, `.espf`, `app.rs`, `example.toml`) and the
   golden, baselined once with the diff reviewed (section 9).
4. **The `# services` summary** (8.5).
5. **`wifi_station` in the embassy compile-check cell** of `generated_projects_compile`, next
   to `button_async` and the async buses.

## 12. Prerequisites

None outstanding.

- The `wifi` option, `DerivedInputs`, and the chip gate: **done** (3.1, 3.2).
- Multi-target `Ref`, and the loader's refusal to enforce "services refer to nothing": already
  in place from earlier phases.
- An embassy cell in the compile-check CI job: already in place, and step 5 above adds Wi-Fi
  to it.
- Duplicate ids and the layer-rank rule: **not prerequisites**; they are their own commits
  (section 2).

## 13. Replacement text for plan.md Phase 4

> ### Phase 4: Wi-Fi
>
> - **Examples:** `05.Networking/wifi_station`.
> - **Two commits.** First the porting half with the legacy golden as an oracle: the `wifi`
>   option, the `has_wifi` chip gate, the radio dependencies in `Cargo.toml`, the heap in
>   `main()`, and `Wifi`/`WifiConfig`/`WifiError` behind a runtime `wifi` feature that implies
>   `embassy`. Then the shape change: `<wifi>` peripheral (claims `WIFI`), `<wifi_station>`
>   component (SSID, `open`/`wpa2`, `env:` passwords), the `EmitCx` startup/cargo-feature/heap
>   seams, the example, and one reviewed golden baseline.
> - **Gate:** `wifi_station` generates and its embassy cell compiles; the `[dependencies]`
>   table and the heap line are still byte-identical to the legacy golden; no
>   `status="planned"` anywhere.
> - **Not in this phase:** the `Services` struct and `<ntp>` until they do something; the
>   duplicate-id error and the layer-rank rule (their own commits); Bluetooth and
>   `ble-trouble`.

## 14. Open decisions

1. **Literal-password warning:** warn, refuse, or allow silently. Proposal: warn.
2. **`env:` syntax:** `env:NAME`. Decide in this phase (8.4).
3. **Where the radio lives on `ResourceGraph`:** a small separate map (radio id → peripheral
   name, `used_by`), not a `Bus` variant. `Bus::id` and `check_bus_gpio_declarations` are
   exhaustive matches, a pinless radio in a pin-bus enum is conceptually wrong, and
   `board_json` would have to decide what a pinless bus serialises as.
4. **Whether `<wifi>`'s radio-less-chip error is E0514 or something the element can share**
   with a future `<bluetooth>` (same shape, different radio). One code per message today.
