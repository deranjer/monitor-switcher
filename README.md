# Monitor Switcher

A lightweight Windows tray app that switches your monitors' active input (DisplayPort/HDMI/DVI/etc.) via DDC/CI, triggered by global hotkeys or the tray menu. Useful if you have monitors shared between multiple PCs and want a one-key/one-click way to flip all of them to the same source at once ("profiles").

Native Win32 GUI (no Electron/webview, no GPU rendering context) — idles around 16MB working set / ~2.6MB committed memory.

## Features

- **Profiles**: define named groups of per-monitor input assignments (e.g. "Gaming PC" -> Monitor 1: DP-1, Monitor 2: HDMI-2) and apply them all in one action.
- **Global hotkeys**: bind a hotkey to each profile to switch inputs from anywhere, without opening the window.
- **Tray icon**: left-click to open the window; right-click for a menu listing all profiles plus Settings/Quit.
- **Identify Monitors**: shows a numbered overlay badge on each monitor so you can tell which is which in the Monitors tab.
- **Switch verification**: after sending a DDC/CI input-switch command, reads back the monitor's state to confirm it actually switched (not just that the command was acknowledged). Can be disabled per-monitor for monitors whose read-back is unreliable.
- **Manual VCP override**: lets you enter a raw VCP input code by hand, for monitors whose advertised capability list doesn't match reality.
- **Autostart**: optional launch at Windows login.
- **Single-instance guarded**: launching a second copy just exits instead of creating a duplicate tray icon.

## Requirements

- Windows 10/11
- Monitor(s) that support DDC/CI input switching (must be enabled in the monitor's own OSD menu, if it has a toggle for it)

## Installing

Download the latest `monitor-switcher.exe` (or the zip) from the [Releases](../../releases) page and run it — no installer needed.

## Building from source

Requires a recent stable Rust toolchain.

```
cargo build --release
```

The binary is produced at `target/release/monitor-switcher.exe`.

## Usage

1. Launch the app. It starts minimized to the tray by default.
2. Open the window from the tray icon (left-click, or right-click -> Open Settings).
3. **Monitors tab**: click "Detect" to enumerate connected DDC/CI monitors. Use "Identify" to show which physical monitor is which.
4. **Profiles tab**: add a profile, assign an input source per monitor, and optionally bind a hotkey.
5. Apply a profile via its hotkey, or via the tray menu / Profiles tab.

Configuration is stored at `%APPDATA%\monitor-switcher\config\config.json`.

### Command-line flags

- `--tray` — start minimized to the tray regardless of the saved setting.
- `--debug-monitors` — print all detected monitors, their VCP 0x60 input capabilities, and any EDID hardware info to the console (no GUI). Useful for diagnosing "monitor not detected" issues.
- `--debug-set-input <index> <hex>` — directly set one monitor (by the index shown in `--debug-monitors`) to a raw VCP input code, e.g. `--debug-set-input 0 0x0F`.

## Troubleshooting

- **A monitor doesn't respond to switching, or switches to the wrong input**: some monitors advertise inaccurate VCP capability codes in their firmware. Use a tool like [ControlMyMonitor](https://www.nirsoft.net/utils/control_my_monitor.html) to find the VCP code that actually works, then enter it via the "Manual override" checkbox on that monitor's assignment row in the Profiles tab.
- **A monitor reports "FAILED" after switching but the switch actually worked**: its DDC read-back doesn't reliably reflect its true state. Disable "Verify switches" for that monitor in the Monitors tab.
- DDC/CI must be enabled in the monitor's on-screen menu — some monitors ship with it off.

## License

MIT — see [LICENSE](LICENSE).
