/*
 * DFO Online - a Rust shell experiment.
 *
 * One window, one local HTTP server, no Electron. The server exists because
 * the game is already a web app that the mod loader serves over HTTP; using a
 * real origin also sidesteps WebKitGTK's custom-scheme quirks (module workers,
 * module preload, fetch). The shell's job is the window and the ARENA2 folder.
 */

mod server;

use std::cell::RefCell;
use std::rc::Rc;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use wry::WebViewBuilder;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

/* The page-side probe: reports the engine, the GL adapter and the frame rate
 * over the IPC channel, so a run leaves a record of what actually happened. */
const PROBE: &str = include_str!("probe.js");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = server::Config::from_args();

    let url = match cfg.external_url.clone() {
        Some(url) => url,
        None => {
            let addr = server::spawn(cfg.clone()).map_err(|e| {
                std::io::Error::other(format!("{e} - is another shell already running?"))
            })?;
            let base = format!("http://{addr}");
            eprintln!("[dfo-shell] server {base}");
            format!("{base}/play/index.html")
        }
    };
    eprintln!("[dfo-shell] loading {url}");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("DFO Online (Rust shell)")
        .with_inner_size(LogicalSize::new(cfg.width, cfg.height))
        .build(&event_loop)?;

    let handle: Rc<RefCell<Option<wry::WebView>>> = Rc::new(RefCell::new(None));
    let ipc_handle = handle.clone();

    let builder = WebViewBuilder::new()
        .with_url(&url)
        .with_background_color((17, 17, 17, 255))
        .with_initialization_script(PROBE)
        .with_ipc_handler(move |req| {
            let body = String::from_utf8_lossy(req.body().as_bytes());
            if let Ok(ev) = serde_json::from_str::<serde_json::Value>(&body) {
                let kind = ev.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
                if kind == "devtools" {
                    if let Some(w) = ipc_handle.borrow().as_ref() {
                        w.open_devtools();
                    }
                    return;
                }
                eprintln!("[probe] {body}");
            }
        });

    // Linux + Wayland: a WebView must be built into the GTK widget, not from a
    // raw window handle (wry's UnsupportedWindowHandle). tao's window already
    // holds a GtkBox; the webview goes in that, or GTK refuses the second child.
    #[cfg(target_os = "linux")]
    let webview = builder.build_gtk(window.default_vbox().expect("tao default vbox"))?;
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window)?;

    *handle.borrow_mut() = Some(webview);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}
