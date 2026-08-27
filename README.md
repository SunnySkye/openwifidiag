# openwifidiag

A terminal-based WiFi diagnostics tool with a rich, colourful TUI. Scans and
monitors near-by wireless networks (SSID, BSSID, channel, band, security,
signal strength) on macOS, Linux, and Windows. Written in Rust on top of
[ratatui](https://ratatui.rs).

## Features

- Live, auto-refreshing table of discovered networks with colour-coded signal
  bars (green → red by RSSI).
- Columns: SSID, signal (bar + dBm), channel, band (2.4/5/6 GHz), BSSID,
  security classification (Open, WEP, WPA, WPA2, WPA3).
- Input via platform-native backends:
  - **macOS**: CoreWLAN (with legacy `airport` fallback when available).
  - **Linux**: `iw` (falls back to `nmcli` when `iw` is unavailable).
  - **Windows**: `netsh wlan show networks mode=bssid`.
- Handy for privilege/permission caveats: prints useful error hints and
  supports `--iface` to select a specific wireless interface.
- `--json` non-interactive mode for scripting.

## Usage

```sh
openwifidiag          # interactive TUI
openwifidiag --json   # one-shot JSON scan

# Options:
#   --interval <secs>   refresh interval (default: 3)
#   --sort <mode>       initial sort: signal|ssid|channel|security
#   --iface <name>      specific interface (e.g. wlan0, en0)
```

Keybindings: `q` quit, `r` refresh, `s` cycle sort, `↑/↓` or `j/k` navigate
selection, `g`/`G` top/bottom.

## Install via npm (recommended)

```sh
npm install -g openwifidiag
openwifidiag
```

The npm package ships platform-specific binary packages under
`optionalDependencies` (darwin-arm64/x64, linux-x64/arm64, win32-x64) and a
tiny JS launcher in `bin/`.

## Permissions

- **Linux**: `iw scan` usually needs `CAP_NET_ADMIN` — run with `sudo`.
  `nmcli` fallback works as a normal user on most distros.
- **macOS**: modern macOS redacts SSIDs/BSSIDs unless **Location Services**
  is enabled for your terminal app. Grant it to see names.
- **Windows**: run from a normal terminal; the WLAN service must be running.

## Platform notes

- The parsers for `iw`/`nmcli`/`netsh` are unit-tested (`cargo test`); the
  CoreWLAN backend is integration-tested on real hardware in CI manually.
- Windows backend relies on English `netsh` field labels — localised output
  can confuse the parser. Native WiFi API replacement is planned.

## Releasing to npm

Releases are automated via GitHub Actions (`.github/workflows/release.yml`):

1. Add an `NPM_TOKEN` secret to the repo (granular access token with publish
   rights for `openwifidiag*`).
2. Bump `version` in `package.json` and `Cargo.toml`.
3. `git tag v<version> && git push --tags`.

The pipeline builds the five platform binaries, assembles the platform
packages via `npm run pack` (which the CI calls explicitly as
`node scripts/pack.js`), and publishes them followed by the root package.

## Project layout

```
src/
  main.rs            CLI entry (clap) + TUI event loop
  app.rs             application state, background scan thread
  ui.rs              ratatui rendering
  model.rs           WifiNetwork, Security, Band
  scanner/
    mod.rs           Scanner trait + platform factory
    parsers.rs       pure parsers (iw/nmcli/netsh/airport) + tests
    linux.rs         iw / nmcli backends
    windows.rs       netsh backend
    macos.rs         CoreWLAN primary, airport fallback
bin/openwifidiag.js  npm launcher
scripts/pack.js      builds ./npm/* platform packages
```

## License

MIT
