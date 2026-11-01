/*
 * The local origin the page runs on.
 *
 * It does what the Electron shell's dagger:// handler does, minus the asar:
 *   /play/...            the built site (game/dist)
 *   /arena2/NAME         the player's own Daggerfall data
 *   /play/arena2/NAME    the same, as the page fetches it relative to /play/
 *
 * ARENA2 is not bundled (it is the player's copy of Daggerfall), so the folder
 * comes from the game's own config.json - the same key the Electron shell
 * writes - or from --arena2.
 *
 * /__dfo/... is the experiment's own door: a command queue the injected probe
 * polls, so the page can be inspected and driven without touching the mouse
 * (tools/ctl.mjs). Development only.
 */

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tower_http::services::ServeDir;

use crate::store::Store;

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub arena2: Option<PathBuf>,
    pub external_url: Option<String>,
    pub port: u16,
    pub width: f64,
    pub height: f64,
}

impl Config {
    pub fn from_args() -> Self {
        let mut cfg = Config {
            root: PathBuf::from("game/dist"),
            arena2: default_arena2(),
            external_url: None,
            // Pinned, not random: WebKitGTK keys localStorage and IndexedDB by
            // origin, and an origin carries the port. A random port would give
            // the game a fresh profile every launch.
            port: 47565,
            width: 1280.0,
            height: 800.0,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--root" => cfg.root = PathBuf::from(args.next().expect("--root needs a path")),
                "--arena2" => cfg.arena2 = args.next().map(PathBuf::from),
                "--url" => cfg.external_url = args.next(),
                "--port" => {
                    cfg.port = args
                        .next()
                        .expect("--port needs a number")
                        .parse()
                        .expect("bad --port")
                }
                "--width" => {
                    cfg.width = args
                        .next()
                        .expect("--width needs a number")
                        .parse()
                        .expect("bad --width")
                }
                "--height" => {
                    cfg.height = args
                        .next()
                        .expect("--height needs a number")
                        .parse()
                        .expect("bad --height")
                }
                other => panic!("unknown argument: {other}"),
            }
        }
        cfg
    }
}

/// `<userData>/config.json`'s arena2Path, the key the Electron shell keeps.
fn default_arena2() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let cfg = PathBuf::from(home).join(".config/Daggerfall JavaScript/config.json");
    let text = std::fs::read_to_string(cfg).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("arena2Path")?.as_str().map(PathBuf::from)
}

#[derive(Clone, Default)]
struct Control {
    queue: Arc<Mutex<VecDeque<(u64, String)>>>,
    results: Arc<Mutex<VecDeque<serde_json::Value>>>,
    next: Arc<AtomicU64>,
}

#[derive(Clone)]
struct AppState {
    arena2: Option<PathBuf>,
    control: Control,
    store: Arc<Store>,
    write_lock: Arc<Mutex<()>>,
}

pub fn spawn(cfg: Config) -> std::io::Result<SocketAddr> {
    let port = cfg.port;
    let (tx, rx) = std::sync::mpsc::channel::<Result<SocketAddr, String>>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let app = router(cfg);
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                Ok(listener) => listener,
                Err(e) => {
                    tx.send(Err(format!("cannot bind 127.0.0.1:{port}: {e}")))
                        .ok();
                    return;
                }
            };
            tx.send(Ok(listener.local_addr().expect("local addr"))).ok();
            axum::serve(listener, app).await.expect("serve");
        });
    });
    match rx.recv() {
        Ok(Ok(addr)) => Ok(addr),
        Ok(Err(message)) => Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, message)),
        Err(_) => Err(std::io::Error::other("server thread died")),
    }
}

fn router(cfg: Config) -> Router {
    let files = ServeDir::new(&cfg.root).append_index_html_on_directories(true);
    let state = AppState {
        arena2: cfg.arena2.clone(),
        control: Control::default(),
        store: Arc::new(Store::new(crate::store::root())),
        write_lock: Arc::new(Mutex::new(())),
    };
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/play/") }))
        .route("/arena2/{name}", get(arena2_file))
        .route("/play/arena2/{name}", get(arena2_file))
        .route("/__dfo/cmd", get(control_take).post(control_put))
        .route("/__dfo/result", get(control_results).post(control_result))
        .route("/__store/all", get(store_all))
        .route("/__store/keys", get(store_keys))
        .route("/__store/get", get(store_get))
        .route("/__store/set", axum::routing::post(store_set))
        .route("/__store/remove", axum::routing::post(store_remove))
        .fallback_service(files)
        .with_state(state)
}

async fn arena2_file(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    // The Electron shell's own name gate: one path segment, no traversal.
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return (StatusCode::BAD_REQUEST, "bad name").into_response();
    }
    let Some(dir) = state.arena2.as_ref() else {
        return (StatusCode::NOT_FOUND, "no ARENA2 folder").into_response();
    };
    match tokio::fs::read(dir.join(&name)).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/* --------------------------------------------------------------- control */

/// POST /__dfo/cmd  (body: a JS expression) -> queues it for the page.
async fn control_put(State(state): State<AppState>, body: Bytes) -> Response {
    let js = String::from_utf8_lossy(&body).trim().to_string();
    if js.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty command").into_response();
    }
    let id = state.control.next.fetch_add(1, Ordering::Relaxed);
    state.control.queue.lock().unwrap().push_back((id, js));
    Json(json!({ "id": id })).into_response()
}

/// GET /__dfo/cmd -> the next queued command, or 204.
async fn control_take(State(state): State<AppState>) -> Response {
    let next = state.control.queue.lock().unwrap().pop_front();
    match next {
        Some((id, js)) => Json(json!({ "id": id, "js": js })).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// POST /__dfo/result -> what the page answered.
async fn control_result(State(state): State<AppState>, body: Bytes) -> Response {
    let text = String::from_utf8_lossy(&body).to_string();
    let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    eprintln!("[ctl] {value}");
    state.control.results.lock().unwrap().push_back(value);
    (StatusCode::OK, "ok").into_response()
}

/// GET /__dfo/result -> drains what the page answered since the last call.
async fn control_results(State(state): State<AppState>) -> Response {
    let drained: Vec<_> = state.control.results.lock().unwrap().drain(..).collect();
    Json(drained).into_response()
}

/* --------------------------------------------------------------- store */

/// GET /__store/all -> the whole file store, for the page's boot seed.
async fn store_all(State(state): State<AppState>) -> Response {
    Json(state.store.all()).into_response()
}

/// GET /__store/keys -> just the live index (length/key).
async fn store_keys(State(state): State<AppState>) -> Response {
    Json(json!({ "keys": state.store.keys() })).into_response()
}

#[derive(Deserialize)]
struct GetQuery {
    key: String,
}

/// GET /__store/get?key=... -> { value: string | null }.
async fn store_get(State(state): State<AppState>, Query(q): Query<GetQuery>) -> Response {
    Json(json!({ "value": state.store.get(&q.key) })).into_response()
}

#[derive(Deserialize)]
struct SetBody {
    key: String,
    value: String,
}

/// POST /__store/set -> writes the file; a non-200 is thrown by the page's
/// setItem, which is the shell's "quota exceeded" contract.
async fn store_set(State(state): State<AppState>, Json(body): Json<SetBody>) -> Response {
    let _guard = state.write_lock.lock().unwrap();
    match state.store.set(&body.key, &body.value) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct RemoveBody {
    key: String,
}

/// POST /__store/remove -> deletes the file (absent is a no-op).
async fn store_remove(State(state): State<AppState>, Json(body): Json<RemoveBody>) -> Response {
    let _guard = state.write_lock.lock().unwrap();
    match state.store.remove(&body.key) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
