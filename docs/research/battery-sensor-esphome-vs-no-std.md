# Battery sensor (reed / PIR) on Matter over Thread: ESPHome vs this repo's no-std stack

Research date: 2026-11-14. Scope: a battery-powered ESP32-C6 contact sensor (reed switch) or motion sensor (PIR) that speaks Matter over Thread. Two routes are compared: ESPHome, and this repo's no_std Rust Firmware (rs-matter + Embassy + esp-hal).

**Baseline for the no-std analysis: master @ `8c92940`**, which includes PR #12 (Lockstep Set upgrade to esp-hal 1.2 / esp-radio 1.0.0-beta.1) and PR #13. Every version and rev below comes from that commit's `Cargo.lock`. ADR-0005 is the decision this note checks.

Source versions used. Everything here moves fast, so re-check before relying on line numbers.

| Source | Version / snapshot |
|---|---|
| esp-hal | 1.2.2 (crates.io; `esp-rs/esp-hal@915465d`) |
| esp-hal-procmacros | 0.23.0 (`esp-rs/esp-hal@160b107`) |
| esp-rtos | 0.4.0 (`esp-rs/esp-hal@63e97be`) |
| esp-radio | 1.0.0-beta.1 (`esp-rs/esp-hal@f769fcd`) |
| esp-metadata-generated | 0.5.3 |
| embassy-executor / embassy-time | 0.10.0 / 0.5.1 |
| openthread / openthread-sys | 0.4.0, git `esp-rs/openthread@ac97457` (patched in `[patch.crates-io]`) |
| rs-matter-embassy | git `sysgrok/rs-matter-embassy@2a806b0` |
| rs-matter-stack | 0.3.0 (`sysgrok/rs-matter-stack@25113f9`) |
| rs-matter | 0.4.1 (`project-chip/rs-matter@3f3528e`, released 2026-09-17) |
| Upstream heads checked | rs-matter-embassy `252cc60` (2026-11-12), openthread `705c557` (2026-10-22), rs-matter `main` |
| ESPHome | stable 2026.10.2 (tag → `9a78022`), beta 2026.11.0b4 (tag → `b03870b`, 2026-11-12), `dev` @ `c2e8c87` (2026-11-14) |
| connectedhomeip | `master` @ `c55ddad` |
| esp-matter | `main` @ `80fc466` |
| OpenThread (C) | `main` @ `ee2179d` |
| ESP32-C6 datasheet | v1.6 (https://documentation.espressif.com/esp32-c6_datasheet_en.pdf) |

Abbreviations: **SED** = Thread Sleepy End Device (receiver off between data polls). **SSED** = Synchronized SED (Thread 1.2 CSL: the child wakes for scheduled receive windows instead of polling). **ICD** = Matter Intermittently Connected Device. **SIT/LIT** = Short/Long Idle Time ICD. **SII/SAI** = the Session Idle/Active Interval a node advertises.

## Question and short answer

**Can ESPHome build a battery-powered Matter-over-Thread ESP32 sensor?** **No, not with ESPHome itself.** ESPHome core has no Matter component today. The only Matter PR is a draft for Wi-Fi/Ethernet. ESPHome *can* build a battery-friendly **Thread** sensor that talks the ESPHome native API to Home Assistant:
- `openthread` with `device_type: MTD` and `poll_period` makes it a SED (since 2025.11.0).
- The new `power_management` component adds automatic light sleep with 802.15.4 sleep. It is in the 2026.11.0 betas, not in a stable release yet. Its author measured about 105 µA average on an ESP32-C6 SED polling every 180 s.

Matter from an ESPHome build only comes through a third-party external component (`esphome-matter`, "early development", no ICD settings) or by re-exporting from Home Assistant through a community Matter bridge.

**How far do this repo's pinned dependencies get?** **Further than ADR-0005 says, but not to a working Sleepy Device.**

Already available at the pinned versions:
- esp-hal 1.2.2 has light sleep, deep sleep and LP-GPIO (ext1) wake on the C6, plus `#[ram(persistent)]` RTC memory.
- esp-rtos 0.4.0 has an experimental automatic-light-sleep idle hook.
- The pinned openthread exposes `set_link_mode`, `set_poll_period` and the child-timeout setters.
- rs-matter 0.4.1 ships the ICD Management cluster (CIP + LITS + UAT + DSLS), a Check-In sender, opt-in CASE-resumption persistence and opt-in persistent subscriptions.

Still missing:
- The ESP 802.15.4 receiver is never turned off. `Radio::set_sleep` is a no-op TODO, and esp-radio has no sleep API; esp-hal PR #6429 is open.
- rs-matter-embassy @ `2a806b0` never switches OpenThread to SED mode and gives no handle to do it.
- The ICD active/idle control loop exists only in rs-matter `main` (PR #579, after 0.4.1) and rs-matter-embassy HEAD (PR #87). Both are unreleased git heads that the pins exclude.
- Persisted State itself is still `DummyKvBlobStore`.

As it stands, a no-std Device keeps its receiver on: about 74 mA, so hours on batteries, not months.

**ADR-0005:** the core choice (SED + ICD, RAM kept, light sleep between polls) is backed by every working reference found: ESPHome's measurement, Espressif's `icd_app`, and upstream rs-matter-embassy's SIT example. Its "Consequences" bullet is out of date, though, and "SED/SSED" should read "SED" for the C6. Upstream now argues that **deep sleep is the right model for a LIT sensor**, provided everything is persisted. A reed/PIR sensor is exactly that LIT case. See "Implications for ADR-0005".

## ESPHome path

### Matter: not in ESPHome core

- **No `matter` component.** The component directory on `dev` (`c2e8c87`) and in 2026.11.0b4 has `openthread`, `openthread_info`, `deep_sleep` and `zigbee`, but nothing for Matter (`gh api repos/esphome/esphome/contents/esphome/components?ref=dev`, listed 2026-11-14). The long-standing feature request is still open (https://github.com/esphome/feature-requests/issues/1430).
- **The only Matter PR is a draft, and Wi-Fi/Ethernet only.** PR #18499, "[matter] added new component matter over wifi and ethernet", is open and marked draft (updated 2026-11-07). It builds on Espressif's `esp_matter` 1.6.0 and requires `wifi:` or `ethernet:`, with "no Thread radio required" (https://github.com/esphome/esphome/pull/18499). **Status: in a PR, unreleased, and not Thread.**
- **Third-party external component.** `DavidvtWout/esphome-matter` (`486fbd0`) claims Matter over Thread on ESP-IDF builds, requiring ESPHome ≥ 2026.6.0 and the `openthread` component. It also wraps `esp_matter` 1.6.0~2. In its own words it is "still in early-development", with "stability issues, mainly with reconnection after a restart which may take up to a minute" (https://github.com/DavidvtWout/esphome-matter/blob/486fbd057615ca0307df0117f4cc1cf1d57b890e/README.md). Its build script sets no ICD options (`components/matter/__init__.py`, searched for `ICD`/`sleep`: no hits), and its issue tracker has nothing on sleep, battery or ICD. Whether `esp_matter`'s ICD server could be enabled through ESPHome `sdkconfig_options` is **unknown**.
- **Through Home Assistant instead.** An ESPHome Thread device talks the ESPHome native API to Home Assistant. Home Assistant has no built-in Matter *bridge* (export); community add-ons do this. The original `t0bst4r/home-assistant-matter-hub` is archived (https://github.com/t0bst4r/home-assistant-matter-hub), and a fork is active (https://github.com/RiDDiX/home-assistant-matter-hub). In that setup the sensor itself is not a Matter node.

### Thread (`openthread` component)

| Capability | First release | Source |
|---|---|---|
| OpenThread on ESP-IDF (C6, H2) | **2025.6.0** | PR #7506, https://github.com/esphome/esphome/pull/7506 |
| `device_type: FTD \| MTD` | **2025.7.0** | PR #9272, https://github.com/esphome/esphome/pull/9272 |
| `poll_period` (SED on MTD) | **2025.11.0** | PR #11374, https://github.com/esphome/esphome/pull/11374 |
| `openthread.set_poll_period` action | **2026.7.0** | PR #11766, https://github.com/esphome/esphome/pull/11766 |
| SSED / CSL | **not supported**. In source: `# TODO: Add support for synchronized sleepy end devices (SSED)` | https://github.com/esphome/esphome/blob/2026.11.0b4/esphome/components/openthread/__init__.py#L246 |
| Child timeout / supervision settings | **open PR** #16387 | https://github.com/esphome/esphome/pull/16387 |

(First-release column: checked with `gh api compare <merge commit>...<tag>` against each release tag.)

- **How SED mode works.** A non-zero `poll_period` sets `mRxOnWhenIdle = (poll_period == 0)` and calls `otLinkSetPollPeriod` (https://github.com/esphome/esphome/blob/2026.11.0b4/esphome/components/openthread/openthread.cpp#L384-L410). It is only allowed with `device_type: MTD` (https://github.com/esphome/esphome/blob/2026.11.0b4/esphome/components/openthread/__init__.py#L325-L333).
- **Caveats from the docs** (https://esphome.io/components/openthread/):
  - "esphome.ota" does not work while `poll_period > 0`.
  - The API timeout is 60 s, so a poll period above that needs a keep-alive.
  - Follow-on work is needed for power management / light sleep in ESP-IDF (the docs page predates the power-management PR below).
- **C6 output power** can be set from −15 to 20 dBm (same page). This matters because the datasheet's 802.15.4 TX peak is 305 mA at 20 dBm and 119 mA at 0 dBm (datasheet v1.6, Table 5-9).

### Power management: automatic light sleep (2026.11.0 beta)

- **What landed.** PR #12325, "[power_management] Add the power_management domain with an esp32_power_management platform", merged 2026-10-12. It is in **2026.11.0b1–b4 but not in stable 2026.10.2** (https://github.com/esphome/esphome/pull/12325; docs PR https://github.com/esphome/esphome.io/pull/5736).
- **What it configures.** It sets `CONFIG_PM_ENABLE` and `CONFIG_FREERTOS_USE_TICKLESS_IDLE`, plus `CONFIG_PM_POWER_DOWN_PERIPHERAL_IN_LIGHT_SLEEP` by default on C6/H2. With an OpenThread SED it also disables `CONFIG_LWIP_ND6` and enables `CONFIG_IEEE802154_SLEEP_ENABLE` (https://github.com/esphome/esphome/blob/2026.11.0b4/esphome/components/esp32_power_management/power_management.py#L171-L204).
- **Measured current.** The PR author reports "about 105 µA average over an hour, 70 µA between transmissions" for an ESP32-C6 SED with a 180 s poll period, measured with a Nordic PPK2 (PR #12325 description). This is the contributor's measurement in a PR, not an Espressif figure.
- **Earlier attempt.** A previous try, PR #15722 ("[openthread] Add light sleep support for Thread Sleepy End Devices"), was closed unmerged. It noted that the USB-Serial-JTAG block on C5/C6/H2 holds the BBPLL and blocks light sleep (https://github.com/esphome/esphome/pull/15722).

### Deep sleep

- **The component supports the C6.** `deep_sleep` lists C6 wake pins `[0..7]` and supports both `wakeup_pin` and `esp32_ext1_wakeup` on the C6 (https://github.com/esphome/esphome/blob/2026.11.0b4/esphome/components/deep_sleep/__init__.py#L48-L152). The docs page still says `wakeup_pin` is "Only on ESP32/BK72xx" (https://esphome.io/components/deep_sleep/). The source is newer.
- **Deep sleep together with `openthread`: undocumented.** Neither docs page mentions the combination. A search of esphome/esphome issues and PRs for "deep_sleep openthread" / "deep sleep thread" found nothing relevant.
- **What a wake looks like.** Each wake is a reboot. ESP-IDF's OpenThread keeps its settings in NVS; the ESPHome docs mention NVS in the MTD/FTD note. OpenThread then restores the child role with a single Child Update Request (`Mle::PrevRoleRestorer`, https://github.com/openthread/openthread/blob/ee2179df4f8d0172013a3391dbefb28fed9554d9/src/core/thread/mle.cpp#L4395-L4410). The native API connection to Home Assistant must be rebuilt every time.
- **Unverified:** how quickly ESPHome reaches "connected" after a deep-sleep wake on Thread.

### What ESPHome gives a reed/PIR sensor today

- **Stable (2026.10.2):** a Thread SED to Home Assistant over the ESPHome API, but with no automatic light sleep. The CPU idles awake in modem-sleep, which the datasheet puts at 14–38 mA (Table 5-10). The other option is the `deep_sleep` component with GPIO wake on GPIO0–7, which is untested with Thread.
- **2026.11.0 (beta, stable expected later in November):** SED plus automatic light sleep at about 100 µA. That is the most realistic battery-capable ESP32-C6 Thread sensor found in this research. It is still **not Matter**.

## No-std path (this repo, master @ 8c92940)

### Per-dependency capability table

Link prefixes, written out in full in the table: `esp-rs/esp-hal` at the crate's packaging commit, `esp-rs/openthread@ac97457`, `sysgrok/rs-matter-embassy@2a806b0`, `project-chip/rs-matter@3f3528e`.

| Dependency (pinned) | Capability | Status at pin | Source |
|---|---|---|---|
| esp-hal 1.2.2 | Deep sleep and light sleep on C6. New 1.2 API: the `LowPower` driver with `sleep_deep`, `sleep_deep_with_rejection` and `sleep_light`. Drivers enable their own wake sources, and the timer wake is `set_wakeup_deadline`. | **Supported, `unstable` API** (the repo already enables `unstable`) | https://github.com/esp-rs/esp-hal/blob/915465df59aee91b627fe538f0b442e56c7f4fc9/esp-hal/src/rtc_cntl/sleep/mod.rs#L69-L166 |
| esp-hal 1.2.2 | GPIO wake. In light sleep, any `listen()`ing pin wakes the chip through the digital path. For deep sleep, a pin needs `WakeupConfig::with_low_power_path(true)`. On C6 this is ext1 with one level per pad, **level-triggered only** ("per-pin path ... stays unused until esp-hal supports edge triggers"). | **Supported** (level-only for deep sleep) | https://github.com/esp-rs/esp-hal/blob/915465df59aee91b627fe538f0b442e56c7f4fc9/esp-hal/src/gpio/wakeup/mod.rs#L1-L87, https://github.com/esp-rs/esp-hal/blob/915465df59aee91b627fe538f0b442e56c7f4fc9/esp-hal/src/gpio/wakeup/ext1_v2.rs#L1-L5 |
| esp-hal 1.2.2 | Which pins can deep-wake: only LP pads. LP_GPIO0–7 = GPIO0–7 on the C6 (datasheet §2.3; ESPHome uses the same list). GPIO9/BOOT cannot. | Hardware constraint | Datasheet v1.6 §2.3.4; https://github.com/sysgrok/rs-matter-embassy/blob/252cc60e7865a474b998e47fa0149af0a6648cd6/docs/icd.md |
| esp-hal 1.2.2 | Memory kept through deep sleep: `#[ram(unstable(rtc_fast, persistent))]` statics in the 16 KB `RTC_FAST` region (LP SRAM). | **Supported** | https://github.com/esp-rs/esp-hal/blob/160b10794227eb84805b8676fe188c1110801e9d/esp-hal-procmacros/src/lib.rs#L74-L91, https://github.com/esp-rs/esp-hal/blob/915465df59aee91b627fe538f0b442e56c7f4fc9/esp-hal/ld/esp32c6/memory.x#L39 |
| esp-hal 1.2.2 | The modem power domain stays on in light sleep by default (`pd_modem == false`). No 802.15.4 MAC/BB retention like ESP-IDF's `CONFIG_IEEE802154_SLEEP_ENABLE`. | Radio *state* across light sleep **unknown** | https://github.com/esp-rs/esp-hal/blob/915465df59aee91b627fe538f0b442e56c7f4fc9/esp-hal/src/rtc_cntl/sleep/esp32c6.rs#L228-L247 |
| esp-rtos 0.4.0 | **Automatic light sleep**: `sleep::configure(LPWR)` returns `light_sleep_hook` for `start_with_idle_hook`. It sleeps only when no `WakeLock` is held and the next timer is ≥ `ESP_RTOS_CONFIG_LIGHT_SLEEP_MIN_US` (default 1000 µs) away. Changelog: "ESP32-C6, ESP32-H2: `auto_light_sleep`". | **Supported, documented as experimental**. The Platform currently calls plain `esp_rtos::start` (`platform/src/lib.rs` L153). | https://github.com/esp-rs/esp-hal/blob/63e97be553f435f29171f7ae5f37a41ad737a75a/esp-rtos/src/sleep.rs#L53-L190, https://github.com/esp-rs/esp-hal/blob/63e97be553f435f29171f7ae5f37a41ad737a75a/esp-rtos/src/lib.rs#L66-L80, https://github.com/esp-rs/esp-hal/blob/63e97be553f435f29171f7ae5f37a41ad737a75a/esp-rtos/CHANGELOG.md |
| esp-radio 1.0.0-beta.1 | Wake lock: the shared radio init (BLE/Wi-Fi `RadioRefGuard`) acquires a `WakeLock` and releases it on deinit. `Ieee802154::new` does not take that guard. With BLE dropped after Commissioning (ADR-0002), nothing in esp-radio blocks auto light sleep, as far as the source shows. | Source reading; **contradicted** by rs-matter-embassy HEAD's `docs/icd.md` ("esp-radio holds a wake lock for as long as the radio is initialized"). **Unverified on hardware.** | https://github.com/esp-rs/esp-hal/blob/f769fcd27056b0837febe4209f3bae78ed764561/esp-radio/src/lib.rs#L304-L313, https://github.com/esp-rs/esp-hal/blob/f769fcd27056b0837febe4209f3bae78ed764561/esp-radio/src/ble/controller/mod.rs#L50-L80, https://github.com/esp-rs/esp-hal/blob/f769fcd27056b0837febe4209f3bae78ed764561/esp-radio/src/ieee802154/mod.rs#L107-L135 |
| esp-radio 1.0.0-beta.1 | 802.15.4 receiver off / radio sleep: the driver has `start_receive` and no counterpart. | **Not supported.** esp-hal PR #6429 "802.15.4 - Implement a sleep API" is open; the earlier #6406 (`stop_receive`) was closed. | https://github.com/esp-rs/esp-hal/pull/6429, https://github.com/esp-rs/esp-hal/pull/6406 |
| openthread @ ac97457 | `set_link_mode(rx_on_when_idle, ftd, full_netdata)`, `set_poll_period`, `poll_period`, `set_child_timeout`, child-supervision setters | **Supported in the binding** | https://github.com/esp-rs/openthread/blob/ac97457efc5debb4256bcd3013d4ddbe1ff804f4/openthread/src/lib.rs#L874-L990 |
| openthread @ ac97457 | ESP radio sleep: the runner calls `radio.set_sleep()` when OpenThread wants the receiver off. `EspRadio::set_sleep` is a no-op ("TODO: Upstream in `esp-radio` a `stop_receive`..."), and `AUTO_SLEEP` is not advertised. So a SED would keep its receiver on, and queued frames arrive late. | **Not supported** (TODO) | https://github.com/esp-rs/openthread/blob/ac97457efc5debb4256bcd3013d4ddbe1ff804f4/openthread/src/lib.rs#L1386-L1393, https://github.com/esp-rs/openthread/blob/ac97457efc5debb4256bcd3013d4ddbe1ff804f4/openthread/src/radio/esp.rs#L157-L200 |
| openthread @ ac97457 | SSED (CSL receiver): the build sets `OT_CSL_RECEIVER=OFF` and the CSL transmitter to 0. MTD and FTD are both built. | **Not supported.** The next commit, `705c557` "CSL support (#122)", adds a `csl-receiver` feature, but the ESP radio there still reports no timed receive and `set_sleep` is still a TODO. | https://github.com/esp-rs/openthread/blob/ac97457efc5debb4256bcd3013d4ddbe1ff804f4/openthread-sys/gen/builder.rs#L244-L288, https://github.com/esp-rs/openthread/commit/705c5576ccfd7b417cfa4b23912a13b1b99e01f2 |
| rs-matter-embassy @ 2a806b0 | Creates the `OpenThread` instance inside `ThreadDriverTaskImpl::run` and never calls `set_link_mode` or `set_poll_period`. There is no public hook to reach the instance short of writing our own `wireless::Thread` impl. | **Not exposed** | https://github.com/sysgrok/rs-matter-embassy/blob/2a806b0b1b521293b34a773618499e87402c843c/rs-matter-embassy/src/wireless/thread.rs#L354-L400 |
| rs-matter-embassy @ 2a806b0 | OpenThread settings persistence: `OtPersist` writes **only the SRP ECDSA key** to the KV store. Dataset is stored by the Matter stack; parent/child info stays in RAM. After any reboot, OpenThread does a full attach. | **Partial** (as ADR-0006 says) | https://github.com/sysgrok/rs-matter-embassy/blob/2a806b0b1b521293b34a773618499e87402c843c/rs-matter-embassy/src/ot.rs#L1027-L1067 |
| rs-matter-embassy @ 2a806b0 | BLE and Thread run one after the other in the non-coex driver: `ThreadDriver::run` creates only the 802.15.4 radio. | Supported (matches ADR-0002) | https://github.com/sysgrok/rs-matter-embassy/blob/2a806b0b1b521293b34a773618499e87402c843c/rs-matter-embassy/src/wireless/thread/esp_thread.rs#L44-L80 |
| rs-matter-embassy @ 2a806b0 | Cargo features `case-resumption` and `persistent-subscriptions` pass through to rs-matter. The repo enables neither. | **Available, opt-in** | https://github.com/sysgrok/rs-matter-embassy/blob/2a806b0b1b521293b34a773618499e87402c843c/rs-matter-embassy/Cargo.toml#L222-L225 |
| rs-matter 0.4.1 | **ICD Management cluster** (`IcdMgmtHandler`) with feature bits CIP + LITS + UAT + DSLS: registration, a persisted Check-In counter, `StayActiveRequest` (30 s maximum), and the operating mode flipping SIT↔LIT with registrations. Also `IcdModeConfig` (idle/active durations, threshold, UAT hint) and the `ICD` mDNS TXT key. | **Supported, released** | https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/dm/clusters/icd_mgmt.rs#L18-L138, https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/dm/clusters/icd_mgmt.rs#L718-L741, https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/transport/network/mdns.rs#L129-L144 |
| rs-matter 0.4.1 | **Check-In protocol** sender (`Icd::send_check_in` / `send_one_check_in`, `sc/checkin.rs`). Module doc: "The application drives them when it decides clients should be nudged." | **Supported, but the application drives it** | https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/dm/clusters/icd_mgmt.rs#L586-L640 |
| rs-matter 0.4.1 | ICD active/idle state machine (power mode, fast/slow poll interval feeding the network driver) | **Not in 0.4.1.** Added by rs-matter PR #579 "Simplify the ICD API by running everything on behalf of the user", merged 2026-09-28, 20 commits after v0.4.1. Unreleased. | https://github.com/project-chip/rs-matter/pull/579 |
| rs-matter 0.4.1 | SII/SAI advertised in `BasicInfoConfig` (the Platform sets `sai: Some(500)`, no `sii`) | Supported | https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/dm/clusters/basic_info.rs#L224-L232 |
| rs-matter 0.4.1 | State that survives a reboot: fabrics/ACL/NOC (persisted), the ICD registrations and Check-In counter keys, the `CASE_RESUMPTION_KEY` (behind feature `case-resumption`), and persisted subscriptions re-hydrated at startup with a session opened lazily when the next report is due (behind feature `persistent-subscriptions`) | **Supported, opt-in** | https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/persist.rs#L113-L128, https://github.com/project-chip/rs-matter/blob/3f3528ef5c492b6feb0a2e043264dc47e9da4340/rs-matter/src/im.rs#L1223-L1235 |
| rs-matter-stack 0.3.0 | Root endpoint and handlers: no ICD Management cluster wired in (no `icd` anywhere in `src/`). The Platform would have to add `IcdMgmtHandler` to Endpoint 0 itself. | **Not wired** (feasibility of adding it ourselves not verified) | https://github.com/sysgrok/rs-matter-stack/tree/25113f98577c32e2b22feff3b6eb32a181d16e57 |
| embassy-executor 0.10.0 / embassy-time 0.5.1 | The executor and time driver are esp-rtos's (`esp_rtos::main`), so power management is esp-rtos's idle hook (above). Embassy has nothing ESP-specific of its own. | n/a | — |
| This repo (`platform/src/lib.rs` @ 8c92940) | `DummyKvBlobStore`, random Discriminator and EUI-64 per boot, plain `esp_rtos::start`, `EspThreadDriver` via `EmbassyThread::new` | No persistence, no sleep | `platform/src/lib.rs` L143–L153, L215–L240 |

### What changed with the Lockstep Set upgrade (PR #12)

Before PR #12, master pinned esp-hal 1.1.2, esp-rtos 0.3.0, esp-radio 0.18.0, rs-matter 0.2.0, rs-matter-embassy `f31233a` and openthread `32ed1bc`. The capabilities that changed:
- **esp-hal 1.1 → 1.2.** The sleep API was redesigned. 1.1 had `Rtc::sleep_deep(&[&wake_sources])`; 1.2 has the `LowPower` driver with a wakeup-enable mask owned by each driver, GPIO `WakeupConfig`, and `set_wakeup_deadline`.
- **esp-rtos 0.3 → 0.4.** Automatic light sleep and `WakeLock` arrived (changelog v0.4.0, 2026-08-25). **This invalidates ADR-0005's "esp-rtos has no automatic light sleep"** for the pinned version.
- **rs-matter 0.2.0 → 0.4.1.** 0.2.0 had no `icd_mgmt.rs`; 0.4.1 has the ICD Management cluster, Check-In, CASE resumption and persistent subscriptions. **This invalidates "rs-matter has no ICD Management cluster or check-in protocol."**
- **openthread `32ed1bc` → `ac97457`.** `ac97457` has `set_link_mode` / `set_poll_period`. `EspRadio::set_sleep` is still a TODO.
- **esp-radio 0.18 → 1.0.0-beta.1.** Still no 802.15.4 sleep API. The changelog now notes 802.15.4 + BLE coexistence and "unconditionally raise the pending bit in all ACKs so that SED children are not missing frames" (#6090), which concerns the *parent* role.

### Where upstream already is (not usable without moving pins)

- **rs-matter-embassy HEAD `252cc60`** contains, among other changes:
  - PR #87 "ICD support" (2026-10-22): `EmbassyThread::with_icd(&icd)` makes the node an MTD SED, maps the ICD poll interval onto `otLinkSetPollPeriod`, and sizes the child and supervision timeouts.
  - An OpenThread persister that keeps "the active dataset, the network and parent info, the SLAAC key and the SRP state".
  - Two ESP examples: `trv_battery_thread` (SIT, light sleep) and `temp_sensor_battery_thread` (LIT, deep sleep, GPIO4 user trigger).

  Sources: https://github.com/sysgrok/rs-matter-embassy/pull/87, https://github.com/sysgrok/rs-matter-embassy/blob/252cc60e7865a474b998e47fa0149af0a6648cd6/docs/icd.md. Moving to it means:
  - patching `rs-matter` to git `main` (HEAD's root `Cargo.toml` patches it);
  - **the ESP examples build against `ivmarkov/esp-hal` branch `ieee802154-sleep`** (the esp-hal PR #6429 branch), not a released esp-hal (https://github.com/sysgrok/rs-matter-embassy/blob/252cc60e7865a474b998e47fa0149af0a6648cd6/examples/esp/Cargo.toml).

  So a working ESP Sleepy Device upstream currently needs unreleased esp-hal, rs-matter and openthread.
- **Upstream's own "Current limitations"** (same `icd.md`):
  - "Light sleep does not engage on the ESP32-C6 yet."
  - A deep-sleep wake re-registers with SRP every time.
  - "The BOOT button cannot wake the C6 from deep sleep. Only GPIO0..7 ..."

### What state must persist across a deep sleep, and what a wake costs

| State | Where it lives at 8c92940 | Needed for a deep-sleep LIT sensor | Notes |
|---|---|---|---|
| Fabrics, NOC, ACL | `DummyKvBlobStore` (lost) | Yes | Persisted State is the next step (ADR-0006). |
| Thread dataset | `DummyKvBlobStore` (lost) | Yes | Stored by the Matter stack once a real store is used. |
| OpenThread parent/child info, frame counters | RAM (ADR-0006, `OtPersist` SRP-key-only) | Yes, for a cheap re-attach | Without it, each wake does a full MLE attach (Parent Request/Response, Child ID Request/Response) instead of one Child Update Request (OpenThread `PrevRoleRestorer`, https://github.com/openthread/openthread/blob/ee2179df4f8d0172013a3391dbefb28fed9554d9/src/core/thread/mle.cpp#L4395-L4410). **This contradicts ADR-0006's choice to keep OpenThread settings in RAM.** Frame counters also need to survive, or the parent may drop frames as replays (standard OpenThread behaviour; not checked here). |
| SRP ECDSA key | Persisted by `OtPersist` | Yes | Upstream notes one SRP update per wake. |
| ICD registrations, Check-In counter | rs-matter keys (store lost) | Yes (LIT) | rs-matter 0.4.1 keys exist. |
| CASE sessions | RAM only; `case-resumption` off | Yes, otherwise each controller needs a full CASE (Sigma1-3) | Opt-in feature. |
| Subscriptions | RAM only; `persistent-subscriptions` off | Yes, otherwise the controller must notice and re-subscribe | Opt-in feature; resumption opens a session lazily (im.rs L1223-L1235). |
| Small app state (last reed level, wake count) | — | Optional | `#[ram(unstable(rtc_fast, persistent))]`, 16 KB. |

Flash wear: every wake that changes OpenThread settings, the CASE resumption cache or the Check-In counter writes flash. rs-matter batches the Check-In counter by an "epoch" (icd_mgmt.rs `Icd::new`). The total write rate for a sensor waking every few minutes was **not quantified**.

### What is missing for a real reed/PIR battery sensor (no-std)

The work runs in this order. Items 1 and 2 are independent of sleep.

1. **Persisted State** (already planned, ADR-0006/0008): a real `KvBlobStore`, stable Discriminator and EUI-64 (ADR-0007).
2. **ICD on Endpoint 0**: add `IcdMgmtHandler` and an `Icd` with an `IcdModeConfig`. For a contact/occupancy sensor that is LIT, with `ActiveModeThreshold` ≥ 5000 ms (connectedhomeip `kMinLitActiveModeThreshold`, https://github.com/project-chip/connectedhomeip/blob/c55ddad4480afad31f249af5356c16e8b53416c6/src/app/icd/server/ICDConfigurationData.h#L179). At 0.4.1 the active/idle loop and Check-In timing are ours to write. Moving rs-matter to the post-#579 code removes that work, but needs a git rs-matter.
3. **Thread SED mode**: call `set_link_mode(false, false, false)` plus `set_poll_period` / `set_child_timeout` on the OpenThread instance. rs-matter-embassy @ `2a806b0` gives no access, so this means our own `wireless::Thread` impl, a fork, or moving to HEAD (`with_icd`).
4. **Radio sleep**: `esp-radio` 802.15.4 sleep (esp-hal PR #6429) *and* `EspRadio::set_sleep` in openthread calling it. Without this, SED mode saves nothing: the receiver stays on at about 74 mA (Table 5-9, RX peak).
5. **MCU sleep**: choose one of these.
   - (a) **Light sleep (SIT, or LIT with RAM kept):** switch the Platform to `esp_rtos::start_with_idle_hook(..., sleep::configure(LPWR).light_sleep_hook)`. Check on hardware that the 802.15.4 driver survives light sleep with the modem domain on, and whether `power_down_peripherals`-style savings (35 µA vs 180 µA floor, Table 5-11) are reachable without ESP-IDF's sleep retention.
   - (b) **Deep sleep (LIT):** persist everything in the table above, enable `case-resumption` and `persistent-subscriptions`, `LowPower::sleep_deep` with `set_wakeup_deadline` = IdleModeDuration, and the reed/PIR pin as an ext1 wake source on GPIO0–7.
6. **Sensor wake input**:
   - Reed: ext1 on the C6 is level-triggered, so before each deep sleep arm the level *opposite* to the current contact state, or the sleep is rejected or ends at once (esp-hal `WakeupConfig` docs, `sleep_deep_with_rejection`).
   - PIR: wake on high.
   - Pull resistors: the internal pull-up is about 45 kΩ (datasheet Table 5-4, R_PU typ), roughly 73 µA at 3.3 V while a reed holds the line low. A closed door would then cost more than the whole SED budget. Use a high-value external pull-up (estimate: 1 MΩ ≈ 3.3 µA) or a normally-open contact orientation.
   - Light sleep accepts any `listen()`ing pin (edge triggers).
7. **Commissioning on battery**: ADR-0010's "commissioning window open until commissioned" means BLE advertising indefinitely on an uncommissioned battery Device. A Sleepy Device needs a bounded window or a button trigger. This is not researched further here.

### Rough battery numbers (estimates unless marked measured)

ESP32-C6 datasheet v1.6, typical:
- 802.15.4 RX 74 mA; TX 119 mA at 0 dBm, 305 mA at 20 dBm (Table 5-9).
- Modem-sleep, CPU idle: 14–28 mA (Table 5-10).
- Light sleep: 180 µA with peripheral clocks off; 35 µA with peripherals powered down.
- Deep sleep: 7 µA, with RTC timer and LP memory on (Table 5-11).

| Scenario | Average current | 2×AA (~2500 mAh) | Basis |
|---|---|---|---|
| This repo today (receiver always on) | ~75 mA | ~1.4 days | Estimate from datasheet RX |
| SED, radio off, CPU awake (ESPHome stable; no-std with radio sleep but no light sleep) | ~15–30 mA | ~4–7 days | Estimate from modem-sleep table |
| SED + auto light sleep, 180 s poll (ESPHome 2026.11 beta) | ~105 µA | ~2.7 years (before self-discharge and regulator losses) | **Measured** by the PR #12325 author |
| SIT, 15 s poll, light sleep (rs-matter-embassy upstream, C6) | ~230 µA | ~1.2 years | **Measured** per upstream `icd.md` (sleep floor 50 µA + wakes 180 µA) |
| LIT deep sleep, 1 wake/h + events | ~10–60 µA | years | **Estimate only.** 7 µA floor plus an unmeasured per-wake cost: boot + Child Update + CASE resumption + report, guessed at 1–4 s at 30–60 mA. Very sensitive to that guess: 12 wakes/h at the high end would be about 600 µA. |

Wake-on-event latency is not the problem for these sensors. A SED can transmit at any time, and the poll period only limits downlink (controller → device). That is why a contact/occupancy sensor fits LIT.

## Comparison

| | ESPHome (2026.10.2 stable / 2026.11.0 beta) | This repo, no-std (master @ 8c92940) |
|---|---|---|
| Speaks Matter | **No** in core. Only the external `esphome-matter` (early development) or an HA→Matter bridge. | **Yes** (rs-matter, commissioned over BLE) |
| Thread SED | **Yes**: MTD + `poll_period` (2025.11.0+) | Binding has it (`set_link_mode`/`set_poll_period`), but rs-matter-embassy @ 2a806b0 doesn't use or expose it |
| Thread SSED (CSL) | No (TODO in source) | No (`OT_CSL_RECEIVER=OFF` at pin; no ESP timed receive) |
| Radio really off between polls | Yes (ESP-IDF driver) | **No** (`EspRadio::set_sleep` TODO; no esp-radio sleep API) |
| Automatic light sleep | 2026.11.0 beta (`power_management`), ~105 µA measured | esp-rtos 0.4.0 hook exists (experimental); unused by the Platform; radio interplay unverified |
| Deep sleep + GPIO wake (C6) | `deep_sleep`, GPIO0–7, ext1 | esp-hal 1.2.2 `LowPower` + `WakeupConfig`, GPIO0–7, level-only |
| Matter ICD (cluster, Check-In, LIT/SIT) | n/a (no Matter). `esphome-matter`: no ICD settings found | rs-matter 0.4.1: cluster + Check-In sender **released**; control loop only on git `main` |
| State across deep sleep | OT settings in NVS (ESP-IDF); API reconnect each wake | Nothing persisted yet. OT parent info RAM-only at pin. CASE resumption / persistent subscriptions opt-in, off. |
| Commissioning | API/YAML credentials; no Matter commissioning in core | Matter over BLE, then Thread (ADR-0002) |
| Battery feasibility today | **Months to years** as a Thread/HA sensor (2026.11+) | **Days at best** until radio sleep lands |
| Effort to a Matter battery sensor | Not possible in core. External component plus unexplored ICD config. | Large: items 1–7 above, several needing unreleased upstream code (esp-hal #6429, rs-matter post-0.4.1, rs-matter-embassy HEAD) |
| Risk | Not Matter, so it fails the requirement. External component immature. | Upstream churn; ESP light-sleep and 802.15.4 interplay unproven; flash wear for deep-sleep LIT |

For reference only (rejected in ADR-0001): Espressif's C++ stack already ships a SIT/LIT ICD example for the ESP32-C6. It defaults to SIT with a 5 s slow poll and offers LIT with a 20 s slow poll and 600 s idle mode (https://github.com/espressif/esp-matter/blob/80fc466602f7382fbd8574b523da284d0b9cb060/examples/icd_app/README.md).

## Implications for ADR-0005

1. **The decision itself holds for a SIT-style device.** For anything a controller must reach within seconds, "SED + ICD, keep RAM, light sleep between polls" is what ESPHome measured (about 100 µA), what Espressif's `icd_app` does, and what upstream rs-matter-embassy recommends for SIT ("Light sleep wins", `icd.md`).
2. **The "deep sleep rejected" rationale is too broad for sensors.** ADR-0005 assumes each wake needs "a full Thread re-attach and a new Matter session". That is true only without persistence. With OpenThread's settings persisted, re-attach is one Child Update Request. With CASE resumption plus persistent subscriptions, the session and reports resume in one round trip. Upstream's own guidance is deep sleep for LIT, with "everything persisted". A door/window contact or PIR is a textbook LIT device. **Suggested amendment:** keep light-sleep SED as the default Sleepy Device model, and record LIT-with-deep-sleep as an allowed variant for event-driven sensors once Persisted State covers OpenThread settings, CASE resumption and subscriptions. That variant also requires revisiting ADR-0006's "OpenThread settings stay in RAM".
3. **"SED/SSED" should read "SED" for the ESP32-C6.** Neither path has CSL on the C6. In the no-std stack the CSL receiver is compiled out at the pin, and the ESP radio has no timed receive even at openthread HEAD. ESPHome has SSED as a TODO.
4. **The Consequences bullet is stale at 8c92940.** Suggested replacement facts:
   - rs-matter 0.4.1 *has* the ICD Management cluster and Check-In (the control loop is post-0.4.1, on git).
   - esp-rtos 0.4.0 *has* experimental automatic light sleep.
   - The openthread binding *exposes* link mode and poll period, but rs-matter-embassy @ 2a806b0 does not use or expose them (HEAD does, via `with_icd`).
   - ESP 802.15.4 radio sleep is still missing in esp-radio (esp-hal PR #6429 open) and in openthread's `EspRadio::set_sleep`. **This is the real blocker.**
5. **Hardware constraint worth recording** (affects Board wiring for battery Devices): only GPIO0–7 can wake the C6 from deep sleep, and the BOOT button (GPIO9, used for Factory Reset in ADR-0006) cannot.

## Open questions and next steps

Open questions (unverified):
1. Does esp-radio 1.0.0-beta.1 block auto light sleep while only the 802.15.4 driver is alive? The source says no; upstream `icd.md` says yes. Test on the Test Board: start with `light_sleep_hook` and measure.
2. Does the 802.15.4 driver keep working across an esp-hal light sleep with the modem domain powered (`pd_modem == false`)? Without ESP-IDF's 802.15.4 sleep retention, it may need re-init.
3. What does one deep-sleep wake actually cost (boot, OT Child Update, SRP update, CASE resumption, report) on a C6? This decides LIT deep sleep vs LIT light sleep.
4. Do Home Assistant's Matter server, Apple Home and Google Home register as LIT Check-In clients? If none does, a LIT device operates as SIT (≤ 15 s slow poll, per connectedhomeip `kSITPollingThreshold`, https://github.com/project-chip/connectedhomeip/blob/c55ddad4480afad31f249af5356c16e8b53416c6/src/app/icd/server/ICDConfigurationData.h#L76-L92), which caps the battery win. Not researched.
5. Can `esphome-matter` enable `esp_matter`'s ICD server through ESPHome `sdkconfig_options`? Unknown.
6. How much flash wear does a deep-sleep LIT sensor cause, given OpenThread settings, the CASE resumption cache and counters written per wake?

Next steps (local work, in order):
1. Finish Persisted State (already first per ADR-0005).
2. Measure on the Test Board: the current Device's average current, then the same with `start_with_idle_hook(light_sleep_hook)`. This answers questions 1–2 cheaply, with no upstream change.
3. Prototype SED mode behind the Platform. Either write a minimal own `wireless::Thread` impl that calls `set_link_mode`/`set_poll_period`, or evaluate a move to rs-matter-embassy HEAD together with rs-matter git (a Lockstep Set change; see the `upgrade-lockstep-set` skill).
4. Track esp-hal PR #6429 (802.15.4 sleep) and the openthread `EspRadio::set_sleep` follow-up. Until both land in releases (or are carried locally), radio sleep is the hard blocker.
5. Amend ADR-0005 as described above (SED-only on the C6; a LIT deep-sleep variant; corrected Consequences), and revisit ADR-0006/0010 for battery Devices.
