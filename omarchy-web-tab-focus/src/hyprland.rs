use std::env;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crate::browser::BrowserSpec;
use crate::json::Json;

#[derive(Debug, Clone)]
pub struct HyprWindow {
    pub address: String,
    pub title: String,
    pub focus_history_id: i64,
}

fn hypr_socket_path() -> Option<PathBuf> {
    let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    if sig.is_empty() {
        return None;
    }
    let runtime = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = PathBuf::from(&runtime)
        .join("hypr")
        .join(&sig)
        .join(".socket.sock");
    if path.exists() {
        return Some(path);
    }
    let legacy = PathBuf::from("/tmp/hypr").join(&sig).join(".socket.sock");
    if legacy.exists() {
        return Some(legacy);
    }
    None
}

fn hypr_request(cmd: &str) -> io::Result<String> {
    let path = hypr_socket_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HYPRLAND_INSTANCE_SIGNATURE socket not found",
        )
    })?;
    let mut stream = UnixStream::connect(&path)?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(cmd.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

pub fn list_browser_windows(spec: &BrowserSpec) -> Vec<HyprWindow> {
    let Ok(raw) = hypr_request("j/clients") else {
        return Vec::new();
    };
    let Ok(Json::Array(clients)) = Json::parse(&raw) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for c in clients {
        let class = c.get("class").and_then(Json::as_str).unwrap_or("");
        if !spec.matches_hypr_class(class) {
            continue;
        }
        let Some(address) = c.get("address").and_then(Json::as_str) else {
            continue;
        };
        let title = c.get("title").and_then(Json::as_str).unwrap_or("");
        let focus_history_id = c
            .get("focusHistoryID")
            .and_then(Json::as_i64)
            .unwrap_or(i64::MAX);
        out.push(HyprWindow {
            address: address.to_string(),
            title: title.to_string(),
            focus_history_id,
        });
    }
    out.sort_by_key(|w| w.focus_history_id);
    out
}

pub fn focus_window(address: &str) -> bool {
    let lua_cmd = format!(
        "dispatch hl.dsp.focus({{ window = \"address:{}\" }})",
        address
    );
    if let Ok(resp) = hypr_request(&lua_cmd) {
        if resp.trim() == "ok" {
            return true;
        }
    }
    let fallback_cmd = format!("dispatch focuswindow address:{address}");
    matches!(hypr_request(&fallback_cmd), Ok(r) if r.trim() == "ok")
}

fn title_matches(window_title: &str, tab_title: &str) -> bool {
    if tab_title.is_empty() {
        return false;
    }
    if window_title == tab_title {
        return true;
    }
    if let Some(rest) = window_title.strip_prefix(tab_title) {
        if rest.starts_with(" - ") || rest.starts_with(" – ") || rest.starts_with(" — ") {
            return true;
        }
    }
    window_title.contains(tab_title)
}

pub fn pick_matching_window<'a>(
    windows: &'a [HyprWindow],
    target_title: &str,
) -> Option<&'a HyprWindow> {
    if windows.is_empty() {
        return None;
    }
    if windows.len() == 1 {
        return Some(&windows[0]);
    }
    if !target_title.is_empty() {
        if let Some(w) = windows.iter().find(|w| {
            w.title == target_title
                || w.title
                    .strip_prefix(target_title)
                    .is_some_and(|r| r.starts_with(" - ") || r.starts_with(" — "))
        }) {
            return Some(w);
        }
        if let Some(w) = windows.iter().find(|w| title_matches(&w.title, target_title)) {
            return Some(w);
        }
    }
    None
}

/// Focus the Hyprland window belonging to `spec` that owns the activated/created tab.
pub fn focus_browser_for_tab(
    spec: &BrowserSpec,
    target_title: &str,
    prev_title: &str,
    newly_created_window: bool,
) -> bool {
    let initial = list_browser_windows(spec);
    if initial.len() == 1 && !newly_created_window {
        return focus_window(&initial[0].address);
    }

    if initial.is_empty() {
        let deadline = Instant::now() + Duration::from_millis(2000);
        while Instant::now() < deadline {
            let wins = list_browser_windows(spec);
            if let Some(w) = wins.first() {
                return focus_window(&w.address);
            }
            thread::sleep(Duration::from_millis(30));
        }
        return false;
    }

    let deadline = Instant::now() + Duration::from_millis(350);
    loop {
        let wins = list_browser_windows(spec);
        if let Some(w) = pick_matching_window(&wins, target_title) {
            return focus_window(&w.address);
        }
        if Instant::now() >= deadline {
            if let Some(w) = pick_matching_window(&wins, prev_title) {
                return focus_window(&w.address);
            }
            if let Some(w) = wins.first() {
                return focus_window(&w.address);
            }
            return false;
        }
        thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_exact_window_among_multiple() {
        let wins = vec![
            HyprWindow {
                address: "0x1".into(),
                title: "Inbox - Gmail - Chromium".into(),
                focus_history_id: 0,
            },
            HyprWindow {
                address: "0x2".into(),
                title: "(5) Home / X - Brave Origin".into(),
                focus_history_id: 3,
            },
        ];
        let picked = pick_matching_window(&wins, "(5) Home / X").unwrap();
        assert_eq!(picked.address, "0x2");
    }
}
