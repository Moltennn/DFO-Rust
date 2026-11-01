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
 */

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use tower_http::services::ServeDir;

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub arena2: Option<PathBuf>,
    pub external_url: Option<String>,
    pub port: u16,
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

#[derive(Clone)]
struct AppState {
    arena2: Option<PathBuf>,
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
    };
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/play/") }))
        .route("/arena2/{name}", get(arena2_file))
        .route("/play/arena2/{name}", get(arena2_file))
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
