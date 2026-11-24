use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::browser::{
    detect_default_browser, find_by_slug, inspect_browser_process, BrowserFamily,
    BrowserProcessState, BrowserSpec, BROWSERS,
};
use crate::hyprland;
use crate::json::Json;
use crate::protocol::{ensure_socket_dir, socket_dir, socket_path_for};

const EXT_MARKER: &str = "omarchy-web-tab-focus/extension";

fn extract_host_from_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or("");
    host.trim().to_ascii_lowercase()
}

fn send_socket_request(browser_slug: &str, req: &Json, read_timeout: Duration) -> Result<Json, String> {
    let path = socket_path_for(browser_slug);
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("cannot connect to {}: {e}", path.display()))?;
    stream
        .set_read_timeout(Some(read_timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;

    let mut line = req.to_json_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut resp_line = String::new();
    let mut reader = BufReader::new(&stream);
    reader
        .read_line(&mut resp_line)
        .map_err(|e| format!("failed reading host response: {e}"))?;
    if resp_line.trim().is_empty() {
        return Err("empty response from native host".to_string());
    }
    Json::parse(resp_line.trim())
}

fn ping_host(browser_slug: &str) -> bool {
    let mut req = BTreeMap::new();
    req.insert("action".to_string(), Json::Str("ping".to_string()));
    matches!(
        send_socket_request(browser_slug, &Json::Object(req), Duration::from_millis(400)),
        Ok(resp) if resp.get("ok").and_then(Json::as_bool) == Some(true)
    )
}

fn wait_for_host_ready(browser_slug: &str, max_wait: Duration) -> bool {
    let deadline = Instant::now() + max_wait;
    while Instant::now() < deadline {
        if ping_host(browser_slug) {
            return true;
        }
        thread::sleep(Duration::from_millis(60));
    }
    false
}

fn spawn_detached(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

fn try_acquire_launch_guard(slug: &str) -> bool {
    let _ = ensure_socket_dir();
    let lock_path = socket_dir().join(format!("{slug}.launching"));
    if let Ok(meta) = fs::metadata(&lock_path) {
        if let Ok(modified) = meta.modified() {
            if SystemTime::now()
                .duration_since(modified)
                .is_ok_and(|age| age > Duration::from_secs(4))
            {
                let _ = fs::remove_file(&lock_path);
            }
        }
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .is_ok()
}

fn release_launch_guard(slug: &str) {
    let lock_path = socket_dir().join(format!("{slug}.launching"));
    let _ = fs::remove_file(lock_path);
}

fn launch_browser_bare(spec: &BrowserSpec) -> bool {
    if !try_acquire_launch_guard(spec.slug) {
        // Another invocation already started launching this browser moments ago.
        return true;
    }
    let exec = spec.resolve_exec();
    let ok = spawn_detached("setsid", &["-f", "uwsm-app", "--", exec])
        || spawn_detached("setsid", &["-f", exec])
        || spawn_detached(exec, &[]);
    if !ok {
        release_launch_guard(spec.slug);
    }
    ok
}

fn launch_browser_with_url(spec: &BrowserSpec, url: &str) {
    let exec = spec.resolve_exec();

    // 1. Try Omarchy's instant SingletonSocket handoff if browser is already running.
    if spec.family == BrowserFamily::Chromium {
        let handoff_ok = Command::new("omarchy-cmd-browser-handoff")
            .args([exec, url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if handoff_ok {
            let wins = hyprland::list_browser_windows(spec);
            if let Some(w) = wins.first() {
                hyprland::focus_window(&w.address);
            }
            return;
        }
    }

    // 2. Otherwise start the browser detached with the URL.
    let launched = spawn_detached("setsid", &["-f", "uwsm-app", "--", exec, url])
        || spawn_detached("setsid", &["-f", exec, url])
        || spawn_detached(exec, &[url]);

    if launched {
        let _ = hyprland::focus_browser_for_tab(spec, "", "", true);
    }
}

fn send_focus_request(
    spec: &BrowserSpec,
    url: &str,
    pattern: &str,
    always_open: bool,
    wait_for_restore: bool,
) -> Result<(), ()> {
    let mut req = BTreeMap::new();
    req.insert(
        "action".to_string(),
        Json::Str(if always_open { "open" } else { "focus_or_open" }.to_string()),
    );
    req.insert("url".to_string(), Json::Str(url.to_string()));
    req.insert("match".to_string(), Json::Str(pattern.to_string()));
    req.insert("wait_for_restore".to_string(), Json::Bool(wait_for_restore));

    let timeout = if wait_for_restore {
        Duration::from_secs(7)
    } else {
        Duration::from_secs(4)
    };

    match send_socket_request(spec.slug, &Json::Object(req), timeout) {
        Ok(resp) if resp.get("ok").and_then(Json::as_bool) == Some(true) => {
            let title = resp.get("title").and_then(Json::as_str).unwrap_or("");
            let prev_title = resp.get("prev_title").and_then(Json::as_str).unwrap_or("");
            let created_window = resp
                .get("created_window")
                .and_then(Json::as_bool)
                .unwrap_or(false);
            hyprland::focus_browser_for_tab(spec, title, prev_title, created_window);
            Ok(())
        }
        Ok(resp) => {
            if let Some(err) = resp.get("error").and_then(Json::as_str) {
                eprintln!("omarchy-web-tab-focus: extension error: {err}");
            }
            Err(())
        }
        Err(_) => Err(()),
    }
}

fn title_matches_host_heuristic(title: &str, pattern: &str) -> bool {
    let lower_title = title.to_ascii_lowercase();
    let host = pattern.split('/').next().unwrap_or(pattern).to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    if lower_title.contains(host) {
        return true;
    }
    // Common short-title sites for non-extension fallback (e.g. Firefox/Zen active tab).
    match host {
        "x.com" | "twitter.com" => lower_title.contains(" / x"),
        "mail.google.com" => lower_title.contains("gmail"),
        "web.whatsapp.com" => lower_title.contains("whatsapp"),
        "chatgpt.com" => lower_title.contains("chatgpt"),
        _ => false,
    }
}

pub fn resolve_browser(browser_override: Option<&str>) -> Result<&'static BrowserSpec, String> {
    if let Some(name) = browser_override {
        find_by_slug(name).ok_or_else(|| format!("unknown browser '{name}'"))
    } else {
        Ok(detect_default_browser())
    }
}

pub fn run_focus(
    url: &str,
    match_pattern: Option<&str>,
    browser_override: Option<&str>,
    always_open: bool,
) -> i32 {
    let spec = match resolve_browser(browser_override) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("omarchy-web-tab-focus: {e}");
            return 2;
        }
    };

    let pattern = match match_pattern {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => extract_host_from_url(url),
    };

    // Fast path: extension host socket is already live.
    if send_focus_request(spec, url, &pattern, always_open, false).is_ok() {
        return 0;
    }

    // For Chromium-family browsers on `focus` (not `always_open`), avoid creating a duplicate tab
    // when the browser is cold-starting or restoring its previous session:
    //   1. If browser is NOT running -> launch bare (NO URL), wait for extension socket, then ask
    //      the extension with `wait_for_restore: true` so restored tabs are reused cleanly!
    //   2. If browser IS running WITH our extension (e.g. just restarted moments ago) -> wait for
    //      the socket to come online instead of opening a duplicate tab!
    //   3. Only if browser is running WITHOUT our extension (or socket never connects) -> fall back
    //      to launching with URL.
    if spec.family == BrowserFamily::Chromium && !always_open {
        match inspect_browser_process(spec, EXT_MARKER) {
            BrowserProcessState::NotRunning => {
                let launched = launch_browser_bare(spec);
                let ready = launched && wait_for_host_ready(spec.slug, Duration::from_secs(8));
                release_launch_guard(spec.slug);
                if ready && send_focus_request(spec, url, &pattern, false, true).is_ok() {
                    return 0;
                }
            }
            BrowserProcessState::RunningWithExtension => {
                if wait_for_host_ready(spec.slug, Duration::from_secs(5))
                    && send_focus_request(spec, url, &pattern, false, true).is_ok()
                {
                    return 0;
                }
            }
            BrowserProcessState::RunningWithoutExtension => {}
        }
    }

    // Non-extension fallback (Firefox/Zen or Chromium without extension loaded):
    // If an existing window's active tab title already matches, focus it instead of duplicating.
    if !always_open {
        let wins = hyprland::list_browser_windows(spec);
        if let Some(w) = wins
            .iter()
            .find(|w| title_matches_host_heuristic(&w.title, &pattern))
        {
            hyprland::focus_window(&w.address);
            return 0;
        }
    }

    launch_browser_with_url(spec, url);
    0
}

pub fn run_status(browser_override: Option<&str>) -> i32 {
    let default_spec = match resolve_browser(browser_override) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("omarchy-web-tab-focus: {e}");
            return 2;
        }
    };

    let default_connected = ping_host(default_spec.slug);
    let default_wins = hyprland::list_browser_windows(default_spec);
    println!(
        "default browser: {} (connected={}, socket={}, windows={})",
        default_spec.slug,
        if default_connected { "yes" } else { "no" },
        socket_path_for(default_spec.slug).display(),
        default_wins.len()
    );

    // Also report any other connected browsers if multiple are open.
    for spec in BROWSERS {
        if spec.slug == default_spec.slug {
            continue;
        }
        if ping_host(spec.slug) {
            let wins = hyprland::list_browser_windows(spec);
            println!(
                "other connected: {} (socket={}, windows={})",
                spec.slug,
                socket_path_for(spec.slug).display(),
                wins.len()
            );
        }
    }

    if default_connected { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_extraction() {
        assert_eq!(
            extract_host_from_url("https://mail.google.com/mail/u/0/#inbox"),
            "mail.google.com"
        );
        assert_eq!(extract_host_from_url("https://x.com/"), "x.com");
        assert_eq!(
            extract_host_from_url("https://web.whatsapp.com/"),
            "web.whatsapp.com"
        );
        assert_eq!(
            extract_host_from_url("https://chatgpt.com/?model=gpt-5"),
            "chatgpt.com"
        );
    }
}
