use std::ffi::{CString, c_char};

use gtk4::glib::translate::FromGlibPtrNone;
use gtk4::prelude::*;

use dfo_shell::{server, state_dir};

unsafe extern "C" {
    fn dfo_webview_new(url: *const c_char, probe_js: *const c_char) -> *mut gtk4::ffi::GtkWidget;
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
            // tools/ctl.mjs and tools/enter.mjs find the shell here.
            let state = state_dir();
            let _ = std::fs::create_dir_all(&state);
            let _ = std::fs::write(state.join("url"), &base);
            eprintln!("[dfo-shell-gtk4] server {base}");
            format!("{base}/play/index.html")
        }
    };

    if cfg.serve_only {
        eprintln!("[dfo-shell-gtk4] serving {url} (no window)");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    eprintln!("[dfo-shell-gtk4] loading {url}");

    let app = gtk4::Application::builder()
        .application_id("org.moltenn.DfoRust")
        .build();

    let url = CString::new(url)?;
    let probe = CString::new(dfo_shell::initialization_script())?;
    let width = cfg.width as i32;
    let height = cfg.height as i32;

    app.connect_activate(move |app| {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("DFO Online (Rust GTK4)")
            .default_width(width)
            .default_height(height)
            .build();

        let view = unsafe { dfo_webview_new(url.as_ptr(), probe.as_ptr()) };
        let view: gtk4::Widget = unsafe { gtk4::Widget::from_glib_none(view) };
        window.set_child(Some(&view));

        // Alt+Enter toggles the window, in the capture phase so the page never
        // sees the key (and pointer lock is not disturbed).
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let win = window.clone();
        keys.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gtk4::gdk::Key::Return
                && state.contains(gtk4::gdk::ModifierType::ALT_MASK)
            {
                if win.is_fullscreen() {
                    win.unfullscreen();
                } else {
                    win.fullscreen();
                }
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        window.add_controller(keys);

        window.present();
    });

    // GTK's own option parser rejects the shell's flags, which
    // server::Config::from_args has already consumed; pass it no arguments.
    app.run_with_args::<&str>(&[]);
    Ok(())
}
