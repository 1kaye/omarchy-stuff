use std::collections::{BTreeMap, HashMap};
use std::fs::{self, Permissions};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::browser::detect_parent_browser_slug;
use crate::json::Json;
use crate::protocol::{
    ensure_socket_dir, read_native_message, socket_path_for, write_native_message,
};

type ResponseSlot = Arc<(Mutex<Option<Json>>, Condvar)>;

struct HostState {
    next_id: u64,
    pending: HashMap<u64, ResponseSlot>,
}

struct SharedHost {
    state: Mutex<HostState>,
    stdout_lock: Mutex<()>,
    alive: AtomicBool,
}

impl SharedHost {
    fn new() -> Self {
        Self {
            state: Mutex::new(HostState {
                next_id: 1,
                pending: HashMap::new(),
            }),
            stdout_lock: Mutex::new(()),
            alive: AtomicBool::new(true),
        }
    }

    fn send_to_extension(&self, mut req: Json) -> Result<Json, String> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("native host shutting down".to_string());
        }
        let slot: ResponseSlot = Arc::new((Mutex::new(None), Condvar::new()));
        let req_id = {
            let mut st = self.state.lock().map_err(|_| "lock poisoned")?;
            let id = st.next_id;
            st.next_id += 1;
            st.pending.insert(id, Arc::clone(&slot));
            id
        };

        if let Some(map) = req.as_object_mut() {
            map.insert("id".to_string(), Json::Number(req_id as f64));
        } else {
            self.remove_pending(req_id);
            return Err("request must be a JSON object".to_string());
        }

        let write_res = {
            let _guard = self.stdout_lock.lock().map_err(|_| "stdout lock poisoned")?;
            let mut stdout = io::stdout().lock();
            write_native_message(&mut stdout, &req)
        };
        if let Err(e) = write_res {
            self.remove_pending(req_id);
            return Err(format!("failed writing to extension: {e}"));
        }

        // Allow up to 6s so cold-start session restore settle loops in JS finish cleanly.
        let (lock, cvar) = &*slot;
        let guard = lock.lock().map_err(|_| "slot lock poisoned")?;
        let (guard, timeout) = cvar
            .wait_timeout_while(guard, Duration::from_secs(6), |resp| {
                resp.is_none() && self.alive.load(Ordering::SeqCst)
            })
            .map_err(|_| "condvar wait poisoned")?;

        self.remove_pending(req_id);

        if let Some(resp) = guard.as_ref() {
            Ok(resp.clone())
        } else if timeout.timed_out() {
            Err("extension response timed out".to_string())
        } else {
            Err("extension disconnected".to_string())
        }
    }

    fn deliver_response(&self, msg: Json) {
        let Some(id) = msg.get("id").and_then(Json::as_u64) else {
            return;
        };
        let slot = {
            let Ok(mut st) = self.state.lock() else {
                return;
            };
            st.pending.remove(&id)
        };
        if let Some(slot) = slot {
            let (lock, cvar) = &*slot;
            if let Ok(mut guard) = lock.lock() {
                *guard = Some(msg);
                cvar.notify_all();
            }
        }
    }

    fn remove_pending(&self, id: u64) {
        if let Ok(mut st) = self.state.lock() {
            st.pending.remove(&id);
        }
    }

    fn shutdown(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut st) = self.state.lock() {
            for (_, slot) in st.pending.drain() {
                let (_, cvar) = &*slot;
                cvar.notify_all();
            }
        }
    }
}

fn bind_socket_replacing_stale(path: &Path) -> io::Result<(UnixListener, (u64, u64))> {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    let listener = UnixListener::bind(path)?;
    let _ = fs::set_permissions(path, Permissions::from_mode(0o600));
    let meta = fs::metadata(path)?;
    Ok((listener, (meta.dev(), meta.ino())))
}

fn cleanup_owned_socket(path: &Path, owned_id: (u64, u64)) {
    if let Ok(meta) = fs::metadata(path) {
        if (meta.dev(), meta.ino()) == owned_id {
            let _ = fs::remove_file(path);
        }
    }
}

fn handle_client(mut stream: UnixStream, host: Arc<SharedHost>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(7)));

    let mut line = String::new();
    let read_res = {
        let mut reader = BufReader::new(&stream);
        reader.read_line(&mut line)
    };

    let response = match read_res {
        Ok(0) => return,
        Ok(_) => match Json::parse(line.trim()) {
            Ok(req) => match host.send_to_extension(req) {
                Ok(resp) => resp,
                Err(err) => error_json(&err),
            },
            Err(err) => error_json(&format!("invalid json: {err}")),
        },
        Err(err) => error_json(&format!("read error: {err}")),
    };

    let mut payload = response.to_json_string();
    payload.push('\n');
    let _ = stream.write_all(payload.as_bytes());
    let _ = stream.flush();
}

fn error_json(msg: &str) -> Json {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Json::Bool(false));
    map.insert("error".to_string(), Json::Str(msg.to_string()));
    Json::Object(map)
}

pub fn run_host(browser_override: Option<&str>) -> i32 {
    if let Err(e) = ensure_socket_dir() {
        eprintln!("omarchy-web-tab-focus host: failed creating socket dir: {e}");
        return 1;
    }
    let slug = browser_override
        .map(ToOwned::to_owned)
        .unwrap_or_else(detect_parent_browser_slug);
    let sock_path = socket_path_for(&slug);

    let (listener, owned_id) = match bind_socket_replacing_stale(&sock_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "omarchy-web-tab-focus host: failed to bind {}: {e}",
                sock_path.display()
            );
            return 1;
        }
    };

    let host = Arc::new(SharedHost::new());

    let accept_host = Arc::clone(&host);
    thread::spawn(move || {
        for incoming in listener.incoming() {
            if !accept_host.alive.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(stream) = incoming {
                let h = Arc::clone(&accept_host);
                thread::spawn(move || handle_client(stream, h));
            }
        }
    });

    let mut stdin = io::stdin().lock();
    while let Ok(Some(msg)) = read_native_message(&mut stdin) {
        host.deliver_response(msg);
    }

    host.shutdown();
    cleanup_owned_socket(&sock_path, owned_id);
    0
}
