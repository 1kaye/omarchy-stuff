# omarchy-stuff

Personal tools, scripts, and customizations for [Omarchy Linux](https://omarchy.org/).

## Tools

### `omarchy-web-tab-focus`
Multi-browser Native Messaging host + CLI (written in pure Rust, zero external dependencies) paired with a lightweight Manifest V3 extension so Hyprland keybindings can jump straight to an already-open tab (including background tabs and restored sessions) across workspaces instead of opening duplicates.

- Supports Chromium-family browsers in Omarchy (`brave-origin`, `brave`, `chromium`, `chrome`, `edge`, `vivaldi`, `helium`) with graceful fallback for `firefox` and `zen`.
- Automatically follows Omarchy's default browser (`omarchy-cmd-default-browser`).
- Safe across `omarchy update` (keeps everything in `~/.local` / `~/.config`, plus installs an Omarchy `post-update` hook).

#### Install / Reinstall
```bash
./omarchy-web-tab-focus/install.sh
```

#### Example Hyprland Bindings (`~/.config/hypr/bindings.lua`)
```lua
o.rebind("SUPER + SHIFT + E", "Email", "omarchy-web-tab-focus focus 'https://mail.google.com/mail/u/0/#inbox'")
o.rebind("SUPER + SHIFT + ALT + E", "New email", "omarchy-web-tab-focus open 'https://mail.google.com/mail/u/0/#inbox?compose=new'")
o.rebind("SUPER + SHIFT + W", "WhatsApp", "omarchy-web-tab-focus focus 'https://web.whatsapp.com/'")
o.rebind("SUPER + SHIFT + X", "X", "omarchy-web-tab-focus focus 'https://x.com/'")
o.rebind("SUPER + SHIFT + C", "Chat gpt", "omarchy-web-tab-focus focus 'https://chatgpt.com/'")
```

#### Useful Commands
```bash
omarchy-web-tab-focus status          # Check active default browser connection
omarchy-web-tab-focus sync            # Re-sync browser flags and NativeMessagingHosts
omarchy-web-tab-focus focus <url> [--match <host-or-host/path>] [--browser <slug>]
omarchy-web-tab-focus open <url> [--browser <slug>]
```
