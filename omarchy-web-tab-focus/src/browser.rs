use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserFamily {
    Chromium,
    Firefox,
}

#[derive(Debug, Clone, Copy)]
pub struct BrowserSpec {
    pub slug: &'static str,
    pub desktop_ids: &'static [&'static str],
    pub exec_candidates: &'static [&'static str],
    pub hypr_classes: &'static [&'static str],
    pub flags_file: Option<&'static str>,
    pub family: BrowserFamily,
}

pub const BROWSERS: &[BrowserSpec] = &[
    BrowserSpec {
        slug: "brave-origin",
        desktop_ids: &["brave-origin.desktop"],
        exec_candidates: &["brave-origin"],
        hypr_classes: &["brave-origin"],
        flags_file: Some("brave-origin-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "brave",
        desktop_ids: &["brave-browser.desktop", "brave.desktop"],
        exec_candidates: &["brave"],
        hypr_classes: &["brave-browser", "brave"],
        flags_file: Some("brave-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "chromium",
        desktop_ids: &["chromium.desktop"],
        exec_candidates: &["chromium"],
        hypr_classes: &["chromium"],
        flags_file: Some("chromium-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "chrome",
        desktop_ids: &["google-chrome.desktop"],
        exec_candidates: &["google-chrome-stable", "google-chrome"],
        hypr_classes: &["google-chrome"],
        flags_file: Some("chrome-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "edge",
        desktop_ids: &["microsoft-edge.desktop", "microsoft-edge-stable.desktop"],
        exec_candidates: &["microsoft-edge-stable", "microsoft-edge"],
        hypr_classes: &["microsoft-edge"],
        flags_file: Some("microsoft-edge-stable-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "vivaldi",
        desktop_ids: &["vivaldi-stable.desktop", "vivaldi.desktop"],
        exec_candidates: &["vivaldi-stable", "vivaldi"],
        hypr_classes: &["vivaldi-stable", "vivaldi"],
        flags_file: Some("vivaldi-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "helium",
        desktop_ids: &["helium.desktop"],
        exec_candidates: &["helium"],
        hypr_classes: &["helium"],
        flags_file: Some("helium-flags.conf"),
        family: BrowserFamily::Chromium,
    },
    BrowserSpec {
        slug: "firefox",
        desktop_ids: &["firefox.desktop"],
        exec_candidates: &["firefox"],
        hypr_classes: &["firefox"],
        flags_file: None,
        family: BrowserFamily::Firefox,
    },
    BrowserSpec {
        slug: "zen",
        desktop_ids: &["zen.desktop", "zen-browser.desktop"],
        exec_candidates: &["zen-browser", "zen"],
        hypr_classes: &["zen", "zen-alpha", "zen-twilight", "zen-browser"],
        flags_file: None,
        family: BrowserFamily::Firefox,
    },
];

/// All Chromium-family NativeMessagingHosts profile roots matching Omarchy's conventions.
pub const CHROMIUM_NMH_DIRS: &[&str] = &[
    "chromium",
    "google-chrome",
    "google-chrome-beta",
    "google-chrome-unstable",
    "BraveSoftware/Brave-Browser",
    "BraveSoftware/Brave-Browser-Beta",
    "BraveSoftware/Brave-Browser-Nightly",
    "BraveSoftware/Brave-Origin",
    "BraveSoftware/Brave-Origin-Beta",
    "BraveSoftware/Brave-Origin-Nightly",
    "microsoft-edge",
    "microsoft-edge-dev",
    "vivaldi",
];

impl BrowserSpec {
    pub fn resolve_exec(&self) -> &'static str {
        for cand in self.exec_candidates {
            if command_in_path(cand) {
                return cand;
            }
        }
        self.exec_candidates[0]
    }

    pub fn matches_hypr_class(&self, class: &str) -> bool {
        self.hypr_classes
            .iter()
            .any(|c| class.eq_ignore_ascii_case(c))
    }
}

pub fn find_by_slug(name: &str) -> Option<&'static BrowserSpec> {
    let lower = name.trim().to_ascii_lowercase();
    BROWSERS.iter().find(|b| {
        b.slug == lower
            || b.desktop_ids.iter().any(|d| *d == lower)
            || b.exec_candidates.iter().any(|e| *e == lower)
    })
}

pub fn find_by_desktop_id(desktop_id: &str) -> Option<&'static BrowserSpec> {
    let lower = desktop_id.trim().to_ascii_lowercase();
    BROWSERS
        .iter()
        .find(|b| b.desktop_ids.iter().any(|d| *d == lower))
        .or_else(|| {
            if lower.starts_with("brave-origin") {
                find_by_slug("brave-origin")
            } else if lower.starts_with("brave") {
                find_by_slug("brave")
            } else if lower.starts_with("chromium") {
                find_by_slug("chromium")
            } else if lower.starts_with("google-chrome") || lower.starts_with("chrome") {
                find_by_slug("chrome")
            } else if lower.starts_with("microsoft-edge") || lower.starts_with("edge") {
                find_by_slug("edge")
            } else if lower.starts_with("vivaldi") {
                find_by_slug("vivaldi")
            } else if lower.starts_with("helium") {
                find_by_slug("helium")
            } else if lower.starts_with("firefox") {
                find_by_slug("firefox")
            } else if lower.starts_with("zen") {
                find_by_slug("zen")
            } else {
                None
            }
        })
}

/// Detect the active default browser via Omarchy's `omarchy-cmd-default-browser`.
pub fn detect_default_browser() -> &'static BrowserSpec {
    if let Ok(out) = Command::new("omarchy-cmd-default-browser").output() {
        if out.status.success() {
            let desktop = String::from_utf8_lossy(&out.stdout);
            if let Some(spec) = find_by_desktop_id(desktop.trim()) {
                return spec;
            }
        }
    }
    // Fallback to first installed browser in priority order.
    for spec in BROWSERS {
        if spec.exec_candidates.iter().any(|c| command_in_path(c)) {
            return spec;
        }
    }
    &BROWSERS[0]
}

fn command_in_path(cmd: &str) -> bool {
    if cmd.contains('/') {
        return Path::new(cmd).is_file();
    }
    let Ok(path_var) = env::var("PATH") else {
        return false;
    };
    for dir in path_var.split(':') {
        let dir = if dir.is_empty() { "." } else { dir };
        if Path::new(dir).join(cmd).is_file() {
            return true;
        }
    }
    false
}

pub fn classify_exe_path(exe_lower: &str) -> Option<&'static str> {
    if exe_lower.contains("brave-origin") {
        Some("brave-origin")
    } else if exe_lower.contains("brave") {
        Some("brave")
    } else if exe_lower.contains("chromium") {
        Some("chromium")
    } else if exe_lower.contains("google/chrome") || exe_lower.ends_with("/chrome") {
        Some("chrome")
    } else if exe_lower.contains("msedge") || exe_lower.contains("microsoft-edge") {
        Some("edge")
    } else if exe_lower.contains("vivaldi") {
        Some("vivaldi")
    } else if exe_lower.contains("helium") {
        Some("helium")
    } else if exe_lower.contains("zen") {
        Some("zen")
    } else if exe_lower.contains("firefox") {
        Some("firefox")
    } else {
        None
    }
}

fn parent_pid_of(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the closing ')' of comm: state(0) ppid(1) ...
    let after_comm = stat.rsplit_once(')')?.1;
    let mut parts = after_comm.split_whitespace();
    let _state = parts.next()?;
    parts.next()?.parse().ok()
}

/// Detect which browser spawned this native messaging host process by walking ancestors in /proc.
pub fn detect_parent_browser_slug() -> String {
    let mut pid = std::process::id();
    for _ in 0..5 {
        let Some(ppid) = parent_pid_of(pid) else {
            break;
        };
        if ppid <= 1 {
            break;
        }
        if let Ok(exe) = fs::read_link(format!("/proc/{ppid}/exe")) {
            let exe_lower = exe.to_string_lossy().to_ascii_lowercase();
            if let Some(slug) = classify_exe_path(&exe_lower) {
                return slug.to_string();
            }
        }
        if let Ok(cmd) = fs::read(format!("/proc/{ppid}/cmdline")) {
            let cmd_lower = String::from_utf8_lossy(&cmd).to_ascii_lowercase();
            if let Some(slug) = classify_exe_path(&cmd_lower) {
                return slug.to_string();
            }
        }
        pid = ppid;
    }
    "default".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserProcessState {
    NotRunning,
    RunningWithExtension,
    RunningWithoutExtension,
}

/// Inspect `/proc` to see if the main browser process for `spec` is currently running,
/// and whether it was launched with our extension path in `--load-extension`.
pub fn inspect_browser_process(spec: &BrowserSpec, ext_marker: &str) -> BrowserProcessState {
    let Ok(entries) = fs::read_dir("/proc") else {
        return BrowserProcessState::NotRunning;
    };

    let mut running = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let proc_dir = entry.path();
        let Ok(exe) = fs::read_link(proc_dir.join("exe")) else {
            continue;
        };
        let exe_lower = exe.to_string_lossy().to_ascii_lowercase();
        if classify_exe_path(&exe_lower) != Some(spec.slug) {
            continue;
        }
        let Ok(cmd_bytes) = fs::read(proc_dir.join("cmdline")) else {
            continue;
        };
        if cmd_bytes.is_empty() {
            continue;
        }
        let args: Vec<String> = cmd_bytes
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();

        // Skip Chromium helper processes (renderer, gpu, zygote, utility, crashpad).
        if args.iter().any(|a| a.starts_with("--type=")) {
            continue;
        }
        running = true;
        if args
            .iter()
            .any(|a| a.starts_with("--load-extension=") && a.contains(ext_marker))
        {
            return BrowserProcessState::RunningWithExtension;
        }
    }

    if running {
        BrowserProcessState::RunningWithoutExtension
    } else {
        BrowserProcessState::NotRunning
    }
}

pub fn config_home() -> PathBuf {
    if let Ok(dir) = env::var("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config")
}

pub fn data_home() -> PathBuf {
    if let Ok(dir) = env::var("XDG_DATA_HOME") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/share")
}

pub fn extension_dir() -> PathBuf {
    data_home().join("omarchy-web-tab-focus/extension")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_all_omarchy_browser_binaries() {
        assert_eq!(
            classify_exe_path("/opt/brave-origin-bin/brave"),
            Some("brave-origin")
        );
        assert_eq!(classify_exe_path("/opt/brave-bin/brave"), Some("brave"));
        assert_eq!(
            classify_exe_path("/usr/lib/chromium/chromium"),
            Some("chromium")
        );
        assert_eq!(
            classify_exe_path("/opt/google/chrome/chrome"),
            Some("chrome")
        );
        assert_eq!(
            classify_exe_path("/opt/microsoft/msedge/msedge"),
            Some("edge")
        );
        assert_eq!(classify_exe_path("/usr/lib/firefox/firefox"), Some("firefox"));
        assert_eq!(classify_exe_path("/opt/zen-browser/zen"), Some("zen"));
    }

    #[test]
    fn maps_desktop_ids() {
        assert_eq!(
            find_by_desktop_id("brave-origin.desktop").map(|b| b.slug),
            Some("brave-origin")
        );
        assert_eq!(
            find_by_desktop_id("chromium.desktop").map(|b| b.slug),
            Some("chromium")
        );
        assert_eq!(
            find_by_desktop_id("google-chrome.desktop").map(|b| b.slug),
            Some("chrome")
        );
        assert_eq!(
            find_by_desktop_id("zen.desktop").map(|b| b.slug),
            Some("zen")
        );
    }
}
