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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use axum::body::{Body, Bytes};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, Request, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

use crate::input::{self, Delta, RawInput};
use crate::store::Store;

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub arena2: Option<PathBuf>,
    pub external_url: Option<String>,
    pub port: u16,
    pub width: f64,
    pub height: f64,
    pub mods: Option<PathBuf>,
    pub proxy: Option<String>,
    pub raw_input: bool,
    pub no_launcher: bool,
    pub serve_only: bool,
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
            mods: std::env::var_os("DFO_MODS_DIR").map(PathBuf::from),
            proxy: None,
            raw_input: true,
            no_launcher: false,
            serve_only: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                "--root" => cfg.root = PathBuf::from(value(&mut args, "--root")),
                "--arena2" => cfg.arena2 = Some(PathBuf::from(value(&mut args, "--arena2"))),
                "--url" => cfg.external_url = Some(value(&mut args, "--url")),
                "--port" => cfg.port = number(&mut args, "--port"),
                "--width" => cfg.width = number(&mut args, "--width"),
                "--height" => cfg.height = number(&mut args, "--height"),
                "--mods" => cfg.mods = Some(PathBuf::from(value(&mut args, "--mods"))),
                "--no-mods" => cfg.mods = None,
                "--no-raw-input" => cfg.raw_input = false,
                "--serve-only" => cfg.serve_only = true,
                "--no-launcher" => cfg.no_launcher = true,
                other => {
                    eprintln!("unknown argument: {other}\n");
                    print_usage();
                    std::process::exit(2);
                }
            }
        }
        cfg
    }
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("{flag} needs a value\n");
        print_usage();
        std::process::exit(2);
    })
}

fn number<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> T
where
    T: std::str::FromStr,
{
    let raw = value(args, flag);
    raw.parse().unwrap_or_else(|_| {
        eprintln!("bad {flag}: {raw}\n");
        print_usage();
        std::process::exit(2);
    })
}

fn print_usage() {
    println!(
        "dfo-shell - an experimental GTK4 shell for DFO Online

USAGE:
  dfo-shell [OPTIONS]

OPTIONS:
  --width N --height N   window content size (default 1280x800)
  --root DIR             the extracted build served as the game (default game/dist)
  --arena2 DIR           the Daggerfall ARENA2 folder (default: config.json's arena2Path)
  --port N               local origin port (default 47565)
  --url URL              load an existing page instead of the local origin
  --mods DIR             serve the game through the DFO-Mods loader (DFO_MODS_DIR)
  --no-mods              ignore DFO_MODS_DIR and the loader
  --no-launcher          boot straight into the game
  --serve-only           the origin without a window
  -h, --help             print this

State lives in $DFO_STATE_DIR (default ~/.local/share/dfo-rust)."
    );
}

/// `<userData>/config.json`, the file the Electron shell keeps next to `Prefs/`
/// and `Saves/`.
fn config_json_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/Daggerfall JavaScript/config.json"))
}

/// `<userData>/config.json`'s arena2Path, the key the Electron shell keeps.
fn default_arena2() -> Option<PathBuf> {
    let text = std::fs::read_to_string(config_json_path()?).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("arena2Path")?.as_str().map(PathBuf::from)
}

/// The ARENA2 folder in `dir`, or in the usual places below it (the player may
/// pick the Daggerfall install rather than the data folder itself).
pub fn find_arena2(dir: &Path) -> Option<PathBuf> {
    let looks = |candidate: &Path| {
        candidate.join("MAPS.BSA").is_file() && candidate.join("BLOCKS.BSA").is_file()
    };
    [
        dir.to_path_buf(),
        dir.join("ARENA2"),
        dir.join("DAGGER/ARENA2"),
        dir.join("DF/DAGGER/ARENA2"),
    ]
    .into_iter()
    .find(|candidate| looks(candidate))
}

/// Write `arena2Path` back to the shared config, keeping the other keys.
pub fn save_arena2(dir: &Path) -> std::io::Result<()> {
    let path = config_json_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no ~/.config directory")
    })?;
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}));
    json["arena2Path"] = json!(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&json)?)?;
    std::fs::rename(tmp, path)
}

/// The live ARENA2 path, so the launcher can point the server at a new folder
/// without a restart.
pub type Arena2 = Arc<RwLock<Option<PathBuf>>>;

#[derive(Clone, Default)]
struct Control {
    queue: Arc<Mutex<VecDeque<(u64, String)>>>,
    results: Arc<Mutex<VecDeque<serde_json::Value>>>,
    next: Arc<AtomicU64>,
}

#[derive(Clone)]
struct AppState {
    arena2: Arena2,
    raw: Arc<RawInput>,
    proxy: Option<String>,
    client: reqwest::Client,
    control: Control,
    store: Arc<Store>,
    write_lock: Arc<Mutex<()>>,
}

/// Start the origin. The returned handle is the live ARENA2 path: write to it
/// to point the server at another folder.
pub fn spawn(cfg: Config) -> std::io::Result<(SocketAddr, Arena2)> {
    let port = cfg.port;
    let arena2: Arena2 = Arc::new(RwLock::new(cfg.arena2.clone()));
    let shared = Arc::clone(&arena2);
    let raw = Arc::new(input::start(cfg.raw_input));
    if !raw.available {
        eprintln!("[dfo-shell] raw input unavailable (no readable mouse device)");
    } else {
        eprintln!("[dfo-shell] raw input: {}", raw.devices.join(", "));
    }
    let (tx, rx) = std::sync::mpsc::channel::<Result<SocketAddr, String>>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let app = router(cfg, shared, raw);
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
        Ok(Ok(addr)) => Ok((addr, arena2)),
        Ok(Err(message)) => Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, message)),
        Err(_) => Err(std::io::Error::other("server thread died")),
    }
}

fn router(cfg: Config, arena2: Arena2, raw: Arc<RawInput>) -> Router {
    let files = ServeDir::new(&cfg.root).append_index_html_on_directories(true);
    let proxy = cfg.proxy.clone();
    let state = AppState {
        arena2,
        raw,
        proxy: proxy.clone(),
        client: reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("http client"),
        control: Control::default(),
        store: Arc::new(Store::new(crate::store::root())),
        write_lock: Arc::new(Mutex::new(())),
    };
    let router = Router::new()
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
        .route("/__input/status", get(input_status))
        .route("/__input", get(input_ws));
    let router = if proxy.is_some() {
        // Mods mode: the loader answers the game's files, the shell keeps the
        // store and the control channel.
        router.fallback(proxy_request)
    } else {
        router.fallback_service(files)
    };
    router
        .layer(axum::middleware::from_fn(no_store))
        .with_state(state)
}

/// Nothing here is cacheable: the served build (and whether mods are on) can
/// change between launches, and a cached HTML page keeps the old loader tag
/// out. The mod loader sends no-store too.
async fn no_store(request: Request<Body>, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// GET/HEAD everything the loader owns (the game's files, its boot script and
/// patched chunks) from the DFO-Mods server. Nothing here rewrites the body.
async fn proxy_request(State(state): State<AppState>, request: Request<Body>) -> Response {
    let Some(base) = state.proxy.as_deref() else {
        return (StatusCode::NOT_FOUND, "no upstream").into_response();
    };
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        return (StatusCode::METHOD_NOT_ALLOWED, "read-only upstream").into_response();
    }
    let path = request
        .uri()
        .path_and_query()
        .map(|query| query.as_str())
        .unwrap_or("/");
    let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);
    let mut builder = state.client.request(method, format!("{base}{path}"));
    for (name, value) in request.headers() {
        if matches!(
            name.as_str(),
            "host" | "connection" | "content-length" | "transfer-encoding"
        ) {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    let upstream = match builder.send().await {
        Ok(upstream) => upstream,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("mod loader: {e}")).into_response();
        }
    };
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers: Vec<(axum::http::HeaderName, HeaderValue)> = upstream
        .headers()
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "connection" | "transfer-encoding"))
        .filter_map(|(name, value)| {
            HeaderValue::from_bytes(value.as_bytes())
                .ok()
                .map(|value| (name.clone(), value))
        })
        .collect();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    for (name, value) in headers {
        response.headers_mut().append(name, value);
    }
    response
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
    let Some(dir) = state.arena2.read().ok().and_then(|guard| guard.clone()) else {
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

/* ------------------------------------------------------------- raw input */

/// GET /__input/status -> whether an evdev mouse is readable.
async fn input_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "enabled": state.raw.enabled,
        "available": state.raw.available,
        "devices": state.raw.devices,
    }))
}

/// GET /__input -> WebSocket of raw deltas (device counts, unaccelerated).
async fn input_ws(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    let receiver = state.raw.tx.subscribe();
    upgrade.on_upgrade(move |socket| input_socket(socket, receiver))
}

async fn input_socket(mut socket: WebSocket, mut receiver: broadcast::Receiver<Delta>) {
    loop {
        tokio::select! {
            delta = receiver.recv() => match delta {
                Ok(delta) => {
                    let message = json!({
                        "dx": delta.dx,
                        "dy": delta.dy,
                        "buttons": delta.buttons,
                    })
                    .to_string();
                    if socket.send(Message::Text(message.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_arena2_in_common_places() {
        let root = std::env::temp_dir().join(format!("dfo-arena2-test-{}", std::process::id()));
        let arena = root.join("DF/DAGGER/ARENA2");
        std::fs::create_dir_all(&arena).unwrap();
        std::fs::write(arena.join("MAPS.BSA"), b"x").unwrap();
        std::fs::write(arena.join("BLOCKS.BSA"), b"x").unwrap();

        assert_eq!(find_arena2(&root), Some(arena.clone()));
        assert_eq!(find_arena2(&arena), Some(arena.clone()));
        assert_eq!(find_arena2(&root.join("DF/DAGGER")), Some(arena));

        assert_eq!(find_arena2(&root.join("empty")), None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
