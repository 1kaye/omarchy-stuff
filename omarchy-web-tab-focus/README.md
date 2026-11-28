# omarchy-web-tab-focus

Multi-browser Native Messaging host + CLI (written in pure Rust, zero external dependencies) paired with a lightweight Manifest V3 extension so Hyprland keybindings can jump straight to an already-open tab—including background tabs and restored sessions—across workspaces instead of opening duplicates.

## Features

- **Focus existing tabs or open cleanly**: `omarchy-web-tab-focus <url>` switches to an existing matching tab and focuses its Hyprland window; if none exists, it opens `<url>` in the target browser window (or reuses a single blank New Tab on a fresh window).
- **Cold-start & session-restore aware**: When launching a Chromium-family browser from cold start, waits for restored tabs to populate before deciding whether to open a new tab so restored sessions aren't duplicated.
- **Follows Omarchy's default browser**: Automatically detects the active default browser via `omarchy-cmd-default-browser`, or lets you target a specific browser with `--browser <slug>`.
- **Multi-browser support**:
  - **Chromium-family (full extension + Native Messaging support)**: `brave-origin`, `brave`, `chromium`, `chrome`, `edge`, `vivaldi`, `helium`
  - **Firefox-family (graceful fallback via Hyprland window title heuristic + launch)**: `firefox`, `zen`
- **Safe across `omarchy update`**: Installs everything under `~/.local` and `~/.config`, and registers an Omarchy `post-update` hook so `NativeMessagingHosts` and browser flags stay synced automatically.

## Prerequisites

- Omarchy Linux with Hyprland (`HYPRLAND_INSTANCE_SIGNATURE`)
- Rust toolchain (`cargo`) to compile the binary

## Install / Reinstall

From the repository root:

```bash
./omarchy-web-tab-focus/install.sh
```

Or from inside `omarchy-web-tab-focus/`:

```bash
./install.sh
```

The installer:
1. Builds the release binary (`cargo build --release`).
2. Installs the CLI/host binary to `~/.local/bin/omarchy-web-tab-focus`.
3. Installs the Manifest V3 extension to `~/.local/share/omarchy-web-tab-focus/extension`.
4. Runs `omarchy-web-tab-focus sync` to register `NativeMessagingHosts` (`com.user.omarchy_web_tab_focus.json`) and add `--load-extension=...` to existing `~/.config/*-flags.conf` files.
5. Installs an Omarchy `post-update` hook (`omarchy hook install post-update`).

> **Note:** If your Chromium-family browser was already running before the first install, restart it once so it loads `--load-extension` from its `*-flags.conf` file. Ensure `~/.local/bin` is in your `PATH`.

## Usage

### Focus an existing tab or open it (default)

```bash
omarchy-web-tab-focus <url> [--match <host-or-host/path>] [--browser <slug>]
```

- Searches all open tabs in the target browser matching `--match` (or, if omitted, the hostname extracted from `<url>`).
- **Matching rules**:
  - Host only (e.g. `mail.google.com` or `github.com`): matches that host and its subdomains (leading `www.` is ignored).
  - Host + path prefix (e.g. `github.com/my-org`): matches that host/subdomains whose path + query + hash starts with the given path.
  - When multiple tabs match, normal browser windows are preferred over popups/PWAs, then the most recently accessed / active tab.
- If a matching tab is found, activates it and focuses its Hyprland window (even across workspaces).
- If no matching tab is found, opens `<url>` in the most recently focused normal browser window (navigating in place if that window only has a blank New Tab), or creates a new browser window if none is open.

Examples:

```bash
omarchy-web-tab-focus 'https://mail.google.com/mail/u/0/#inbox'
omarchy-web-tab-focus 'https://github.com/org/repo' --match 'github.com/org/repo'
omarchy-web-tab-focus 'https://chatgpt.com/' --browser brave
```

### `open` — Always open a new tab

```bash
omarchy-web-tab-focus open <url> [--browser <slug>]
```

Skips tab matching and always opens `<url>` in a new tab (useful for "compose" or "new chat" shortcuts).

Example:

```bash
omarchy-web-tab-focus open 'https://mail.google.com/mail/u/0/#inbox?compose=new'
```

### `status` — Check browser connection

```bash
omarchy-web-tab-focus status [--browser <slug>]
```

Checks whether the default (or specified) browser's extension is connected to the native messaging host socket, prints the socket path and open Hyprland window count, and lists any other connected browsers. Exits `0` if connected, `1` if not.

### `sync` — Re-sync flags and Native Messaging hosts

```bash
omarchy-web-tab-focus sync
```

Re-writes `NativeMessagingHosts/com.user.omarchy_web_tab_focus.json` across Chromium-family config directories and ensures `--load-extension` in existing `~/.config/*-flags.conf` files points to `~/.local/share/omarchy-web-tab-focus/extension`.

### Supported `--browser` Slugs

- Chromium-family: `brave-origin`, `brave`, `chromium`, `chrome`, `edge`, `vivaldi`, `helium`
- Firefox-family (fallback): `firefox`, `zen`

## Hyprland Keybindings (`~/.config/hypr/bindings.lua`)

```lua
o.rebind("SUPER + SHIFT + E", "Email", "omarchy-web-tab-focus 'https://mail.google.com/mail/u/0/#inbox'")
o.rebind("SUPER + SHIFT + ALT + E", "New email", "omarchy-web-tab-focus open 'https://mail.google.com/mail/u/0/#inbox?compose=new'")
o.rebind("SUPER + SHIFT + W", "WhatsApp", "omarchy-web-tab-focus 'https://web.whatsapp.com/'")
o.rebind("SUPER + SHIFT + X", "X", "omarchy-web-tab-focus 'https://x.com/'")
o.rebind("SUPER + SHIFT + C", "Chat gpt", "omarchy-web-tab-focus 'https://chatgpt.com/'")
```

Reload Hyprland (or run `omarchy-web-tab-focus status`) after updating bindings.

## Troubleshooting

- **`connected=no` in `omarchy-web-tab-focus status`**:
  1. Run `omarchy-web-tab-focus sync`.
  2. Completely quit and reopen your browser so it picks up `--load-extension` from `~/.config/<browser>-flags.conf`.
  3. Confirm the extension **Omarchy Web Tab Focus** is loaded in `chrome://extensions` (or `brave://extensions`).
- **Command not found from Hyprland keybindings**:
  - Make sure `~/.local/bin` is on your session `PATH`, or invoke `~/.local/bin/omarchy-web-tab-focus` directly in your binding.
