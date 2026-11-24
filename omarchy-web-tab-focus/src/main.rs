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
  omarchy-web-tab-focus focus <url> [--match <host-or-host/path>] [--browser <slug>]
  omarchy-web-tab-focus open <url> [--browser <slug>]
  omarchy-web-tab-focus status [--browser <slug>]
  omarchy-web-tab-focus sync
  omarchy-web-tab-focus --host [--browser <slug>]"
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if let Some(first) = args.first() {
        if first == "--host" || first.starts_with("chrome-extension://") {
            let mut browser_override: Option<&str> = None;
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--browser" {
                    browser_override = args.get(i + 1).map(String::as_str);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            process::exit(host::run_host(browser_override));
        }
    }

    let code = match args.first().map(String::as_str) {
        Some("focus") => {
            let Some(url) = args.get(1) else {
                print_usage();
                process::exit(2);
            };
            let mut match_pattern: Option<&str> = None;
            let mut browser_override: Option<&str> = None;
            let mut i = 2;
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
            cli::run_focus(url, match_pattern, browser_override, false)
        }
        Some("open") => {
            let Some(url) = args.get(1) else {
                print_usage();
                process::exit(2);
            };
            let mut browser_override: Option<&str> = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--browser" {
                    browser_override = args.get(i + 1).map(String::as_str);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cli::run_focus(url, None, browser_override, true)
        }
        Some("status") => {
            let mut browser_override: Option<&str> = None;
            if args.get(1).map(String::as_str) == Some("--browser") {
                browser_override = args.get(2).map(String::as_str);
            }
            cli::run_status(browser_override)
        }
        Some("sync") => sync::run_sync(),
        Some("-h") | Some("--help") | None => {
            print_usage();
            0
        }
        Some(other) => {
            eprintln!("unknown command: {other}");
            print_usage();
            2
        }
    };

    process::exit(code);
}
