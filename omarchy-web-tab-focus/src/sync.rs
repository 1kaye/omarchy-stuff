use std::env;
use std::fs;
use std::path::Path;

use crate::browser::{config_home, extension_dir, BROWSERS, CHROMIUM_NMH_DIRS};

pub const EXT_ID: &str = "bpefkagbjpmahbligbjajifakoeljbgo";
pub const HOST_NAME: &str = "com.user.omarchy_web_tab_focus";
const OLD_EXT_MARKER: &str = "brave-tab-focus/extension";

fn sync_flags_file(path: &Path, ext_path: &str) -> Result<bool, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
    let mut changed = false;
    let mut found_load_ext = false;

    for line in &mut lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("--load-extension=") {
            found_load_ext = true;
            let mut exts: Vec<String> = rest
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.contains(OLD_EXT_MARKER))
                .map(ToOwned::to_owned)
                .collect();
            if !exts.iter().any(|e| e == ext_path) {
                exts.push(ext_path.to_string());
            }
            let rebuilt = format!("--load-extension={}", exts.join(","));
            if *line != rebuilt {
                *line = rebuilt;
                changed = true;
            }
        }
    }

    if !found_load_ext {
        lines.push(format!("--load-extension={ext_path}"));
        changed = true;
    }

    if changed {
        let mut out = lines.join("\n");
        out.push('\n');
        fs::write(path, out).map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

pub fn run_sync() -> i32 {
    let cfg = config_home();
    let ext_dir = extension_dir();
    let ext_str = ext_dir.to_string_lossy().into_owned();

    let bin_path = env::current_exe()
        .ok()
        .and_then(|p| fs::canonicalize(p).ok())
        .unwrap_or_else(|| {
            let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            Path::new(&home).join(".local/bin/omarchy-web-tab-focus")
        });
    let bin_str = bin_path.to_string_lossy();

    let manifest = format!(
        "{{\n  \"name\": \"{HOST_NAME}\",\n  \"description\": \"Focus or open tabs in Omarchy browsers from Hyprland keybindings\",\n  \"path\": \"{bin_str}\",\n  \"type\": \"stdio\",\n  \"allowed_origins\": [\n    \"chrome-extension://{EXT_ID}/\"\n  ]\n}}\n"
    );

    let mut nmh_count = 0usize;
    for rel in CHROMIUM_NMH_DIRS {
        let dir = cfg.join(rel).join("NativeMessagingHosts");
        if fs::create_dir_all(&dir).is_ok() {
            let file = dir.join(format!("{HOST_NAME}.json"));
            let old_file = dir.join("com.user.brave_tab_focus.json");
            let _ = fs::remove_file(old_file);
            if fs::write(&file, &manifest).is_ok() {
                nmh_count += 1;
            }
        }
    }

    let mut flags_updated = Vec::new();
    let mut flags_ok = Vec::new();
    for spec in BROWSERS {
        let Some(flags_name) = spec.flags_file else {
            continue;
        };
        let path = cfg.join(flags_name);
        if !path.is_file() {
            continue;
        }
        match sync_flags_file(&path, &ext_str) {
            Ok(true) => flags_updated.push(flags_name),
            Ok(false) => flags_ok.push(flags_name),
            Err(e) => {
                eprintln!("omarchy-web-tab-focus sync: failed updating {}: {e}", path.display());
                return 1;
            }
        }
    }

    println!(
        "synced native messaging hosts: {nmh_count}, flags updated: {:?}, flags already synced: {:?}",
        flags_updated, flags_ok
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sync_flags_idempotent_and_cleans_old_marker() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("omarchy-wtf-test-{nonce}.conf"));
        fs::write(
            &path,
            "--ozone-platform=wayland\n--load-extension=/a,/home/u/.local/share/brave-tab-focus/extension\n--no-default-browser-check\n",
        )
        .unwrap();

        let ext = "/home/u/.local/share/omarchy-web-tab-focus/extension";
        assert_eq!(sync_flags_file(&path, ext).unwrap(), true);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("--load-extension=/a,/home/u/.local/share/omarchy-web-tab-focus/extension"));
        assert!(!text.contains("brave-tab-focus"));
        assert_eq!(sync_flags_file(&path, ext).unwrap(), false);
        let _ = fs::remove_file(&path);
    }
}
