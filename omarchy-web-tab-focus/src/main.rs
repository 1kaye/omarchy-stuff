mod browser;
mod cli;
mod host;
mod hyprland;
mod json;
mod protocol;
mod sync;

use std::env;
use std::process;

fn print_usage() {
    eprintln!(
        "Usage:
  omarchy-web-tab-focus <url> [--match <host-or-host/path>] [--browser <slug>]
  omarchy-web-tab-focus open <url> [--browser <slug>]
  omarchy-web-tab-focus status [--browser <slug>]
  omarchy-web-tab-focus sync
  omarchy-web-tab-focus --host [--browser <slug>]"
    );
}

#[derive(Debug, PartialEq, Eq)]
enum CliAction<'a> {
    Focus {
        url: &'a str,
        match_pattern: Option<&'a str>,
        browser_override: Option<&'a str>,
    },
    Open {
        url: &'a str,
        browser_override: Option<&'a str>,
    },
    Status {
        browser_override: Option<&'a str>,
    },
    Sync,
    Help,
    UsageError,
    Unknown(&'a str),
}

fn parse_focus_flags<'a>(args: &'a [String], start: usize) -> (Option<&'a str>, Option<&'a str>) {
    let mut match_pattern: Option<&'a str> = None;
    let mut browser_override: Option<&'a str> = None;
    let mut i = start;
    while i < args.len() {
        if args[i] == "--match" {
            match_pattern = args.get(i + 1).map(String::as_str);
            i += 2;
        } else if args[i] == "--browser" {
            browser_override = args.get(i + 1).map(String::as_str);
            i += 2;
        } else {
            i += 1;
        }
    }
    (match_pattern, browser_override)
}

fn parse_browser_flag<'a>(args: &'a [String], start: usize) -> Option<&'a str> {
    let mut browser_override: Option<&'a str> = None;
    let mut i = start;
    while i < args.len() {
        if args[i] == "--browser" {
            browser_override = args.get(i + 1).map(String::as_str);
            i += 2;
        } else {
            i += 1;
        }
    }
    browser_override
}

fn parse_cli<'a>(args: &'a [String]) -> CliAction<'a> {
    match args.first().map(String::as_str) {
        Some("focus") => {
            let Some(url) = args.get(1).map(String::as_str) else {
                return CliAction::UsageError;
            };
            let (match_pattern, browser_override) = parse_focus_flags(args, 2);
            CliAction::Focus {
                url,
                match_pattern,
                browser_override,
            }
        }
        Some("open") => {
            let Some(url) = args.get(1).map(String::as_str) else {
                return CliAction::UsageError;
            };
            let browser_override = parse_browser_flag(args, 2);
            CliAction::Open {
                url,
                browser_override,
            }
        }
        Some("status") => CliAction::Status {
            browser_override: parse_browser_flag(args, 1),
        },
        Some("sync") => CliAction::Sync,
        Some("-h") | Some("--help") | None => CliAction::Help,
        Some(url) if url.contains("://") => {
            let (match_pattern, browser_override) = parse_focus_flags(args, 1);
            CliAction::Focus {
                url,
                match_pattern,
                browser_override,
            }
        }
        Some(other) => CliAction::Unknown(other),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if let Some(first) = args.first() {
        if first == "--host" || first.starts_with("chrome-extension://") {
            let browser_override = parse_browser_flag(&args, 1);
            process::exit(host::run_host(browser_override));
        }
    }

    let code = match parse_cli(&args) {
        CliAction::Focus {
            url,
            match_pattern,
            browser_override,
        } => cli::run_focus(url, match_pattern, browser_override, false),
        CliAction::Open {
            url,
            browser_override,
        } => cli::run_focus(url, None, browser_override, true),
        CliAction::Status { browser_override } => cli::run_status(browser_override),
        CliAction::Sync => sync::run_sync(),
        CliAction::Help => {
            print_usage();
            0
        }
        CliAction::UsageError => {
            print_usage();
            2
        }
        CliAction::Unknown(other) => {
            eprintln!("unknown command: {other}");
            print_usage();
            2
        }
    };

    process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn parses_direct_url_as_focus() {
        let args = s(&[
            "https://mail.google.com/mail/u/0/#inbox",
            "--match",
            "mail.google.com",
            "--browser",
            "brave",
        ]);
        assert_eq!(
            parse_cli(&args),
            CliAction::Focus {
                url: "https://mail.google.com/mail/u/0/#inbox",
                match_pattern: Some("mail.google.com"),
                browser_override: Some("brave"),
            }
        );
    }

    #[test]
    fn parses_legacy_focus_alias_and_subcommands() {
        let legacy = s(&["focus", "https://x.com/"]);
        assert_eq!(
            parse_cli(&legacy),
            CliAction::Focus {
                url: "https://x.com/",
                match_pattern: None,
                browser_override: None,
            }
        );

        let open = s(&["open", "https://x.com/", "--browser", "chromium"]);
        assert_eq!(
            parse_cli(&open),
            CliAction::Open {
                url: "https://x.com/",
                browser_override: Some("chromium"),
            }
        );

        let status = s(&["status", "--browser", "brave-origin"]);
        assert_eq!(
            parse_cli(&status),
            CliAction::Status {
                browser_override: Some("brave-origin"),
            }
        );

        assert_eq!(parse_cli(&s(&["sync"])), CliAction::Sync);
        assert_eq!(parse_cli(&s(&["typo"])), CliAction::Unknown("typo"));
    }
}
