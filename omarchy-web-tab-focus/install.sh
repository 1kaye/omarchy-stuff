#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BIN_DEST="$HOME/.local/bin/omarchy-web-tab-focus"
EXT_DEST="$HOME/.local/share/omarchy-web-tab-focus/extension"
HOOK_DIR="$(mktemp -d)"
trap 'rm -rf "$HOOK_DIR"' EXIT

cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

mkdir -p "$(dirname "$BIN_DEST")" "$EXT_DEST"
install -m 755 "$SCRIPT_DIR/target/release/omarchy-web-tab-focus" "$BIN_DEST"
rm -f "$EXT_DEST"/background-*.js
install -m 644 "$SCRIPT_DIR/extension/manifest.json" "$EXT_DEST/manifest.json"
install -m 644 "$SCRIPT_DIR/extension/background-2.js" "$EXT_DEST/background-2.js"

# Sync all Chromium-family NativeMessagingHosts & existing ~/.config/*-flags.conf files
"$BIN_DEST" sync

# Install Omarchy post-update hook so future updates/refreshes stay synced automatically
HOOK_SRC="$HOOK_DIR/omarchy-web-tab-focus"
cat >"$HOOK_SRC" <<'EOF'
#!/usr/bin/env bash
if command -v omarchy-web-tab-focus >/dev/null 2>&1; then
  omarchy-web-tab-focus sync >/dev/null 2>&1 || true
elif [[ -x "$HOME/.local/bin/omarchy-web-tab-focus" ]]; then
  "$HOME/.local/bin/omarchy-web-tab-focus" sync >/dev/null 2>&1 || true
fi
EOF
omarchy hook install post-update "$HOOK_SRC"

echo "Installed binary: $BIN_DEST"
echo "Installed extension: $EXT_DEST"
