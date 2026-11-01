/*
 * DFO Online - a Rust shell experiment.
 *
 * One window, one local HTTP server, no Electron. The server exists because
 * the game is already a web app that the mod loader serves over HTTP; using a
 * real origin also sidesteps WebKitGTK's custom-scheme quirks (module workers,
 * module preload, fetch). The shell's job is the window and the ARENA2 folder.
 */

mod placement;
mod server;
mod store;

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
use wry::{WebViewBuilderExtUnix, WebViewExtUnix};

/* The page-side probe: reports the engine, the GL adapter and the frame rate
 * over the IPC channel, so a run leaves a record of what actually happened. */
const PROBE: &str = include_str!("probe.js");

/*
 * WebKitGTK keeps requestAnimationFrame near 60 Hz with a setting that has no
 * Rust binding in the 2.0 crate (WebKitFeature API, 2.42+). The C symbols are
 * stable; reach them directly. Identifier and default were read from the
 * installed library: "PreferPageRenderingUpdatesNear60FPS", default true.
 */
#[cfg(target_os = "linux")]
mod webkit_features {
    use std::ffi::{c_char, c_int, c_void};

    use webkit2gtk::WebViewExt;
    use webkit2gtk::glib::object::ObjectType;

    unsafe extern "C" {
        fn webkit_settings_get_all_features() -> *mut c_void;
        fn webkit_feature_list_find(list: *mut c_void, identifier: *const c_char) -> *mut c_void;
        fn webkit_feature_list_unref(list: *mut c_void);
        fn webkit_settings_set_feature_enabled(
            settings: *mut c_void,
            feature: *mut c_void,
            enabled: c_int,
        );
    }

    /// Turn a WebKit feature off by identifier. False when this build has no
    /// such feature (the list is per-build; never assume one exists).
    pub fn disable(view: &webkit2gtk::WebView, identifier: &str) -> bool {
        let Some(settings) = view.settings() else {
            return false;
        };
        let Ok(identifier) = std::ffi::CString::new(identifier) else {
            return false;
        };
        unsafe {
            let list = webkit_settings_get_all_features();
            if list.is_null() {
                return false;
            }
            let feature = webkit_feature_list_find(list, identifier.as_ptr());
            let found = !feature.is_null();
            if found {
                webkit_settings_set_feature_enabled(
                    settings.as_ptr() as *mut c_void,
                    feature,
                    0,
                );
            }
            webkit_feature_list_unref(list);
            found
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = server::Config::from_args();

    let url = match cfg.external_url.clone() {
        Some(url) => url,
        None => {
            let addr = server::spawn(cfg.clone()).map_err(|e| {
                std::io::Error::other(format!("{e} - is another shell already running?"))
            })?;
            let base = format!("http://{addr}");
            // tools/ctl.mjs finds the running shell here (dev only).
            let _ = std::fs::write(std::env::temp_dir().join("dfo-shell-url"), &base);
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

    // The feature toggle must land before the game's document runs, so the
    // page is loaded after the webview exists when the flag is set.
    let start_url: &str = if cfg.no_near_60_fps { "about:blank" } else { &url };
    let builder = WebViewBuilder::new()
        .with_url(start_url)
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

    if cfg.no_near_60_fps {
        #[cfg(target_os = "linux")]
        {
            let found = webkit_features::disable(
                &webview.webview(),
                "PreferPageRenderingUpdatesNear60FPS",
            );
            eprintln!("[dfo-shell] PreferPageRenderingUpdatesNear60FPS off: {found}");
        }
        webview.load_url(&url)?;
    }

    *handle.borrow_mut() = Some(webview);
    placement::start();

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
