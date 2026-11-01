use std::ffi::{CString, c_char};
use std::path::Path;
use std::rc::Rc;

use gtk4::glib::translate::FromGlibPtrNone;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label, Orientation};

use dfo_shell::{mods, server, state_dir, store};

unsafe extern "C" {
    fn dfo_webview_new(url: *const c_char, probe_js: *const c_char) -> *mut gtk4::ffi::GtkWidget;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = server::Config::from_args();

    // The loader patches the store's build; the shell keeps the origin so the
    // store and the control channel stay on the same address.
    let _mods = match (cfg.external_url.is_none(), cfg.mods.clone()) {
        (true, Some(dir)) => match mods::start(&dir) {
            Ok(server) => {
                eprintln!("[dfo-shell] mods: {} ({})", server.base, dir.display());
                cfg.proxy = Some(server.base.clone());
                Some(server)
            }
            Err(e) => {
                eprintln!("[dfo-shell] mods: {e}");
                eprintln!("[dfo-shell] serving the stock game");
                None
            }
        },
        _ => None,
    };

    let (url, arena2) = match cfg.external_url.clone() {
        Some(url) => (url, None),
        None => {
            let (addr, arena2) = server::spawn(cfg.clone()).map_err(|e| {
                std::io::Error::other(format!("{e} - is another shell already running?"))
            })?;
            let base = format!("http://{addr}");
            // tools/ctl.mjs and tools/enter.mjs find the shell here.
            let state = state_dir();
            let _ = std::fs::create_dir_all(&state);
            let _ = std::fs::write(state.join("url"), &base);
            eprintln!("[dfo-shell] server {base}");
            (format!("{base}/play/index.html"), Some(arena2))
        }
    };

    if cfg.serve_only {
        eprintln!("[dfo-shell] serving {url} (no window)");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }

    let launcher = !cfg.no_launcher && arena2.is_some();
    eprintln!(
        "[dfo-shell] {}",
        if launcher {
            "launcher".to_string()
        } else {
            format!("loading {url}")
        }
    );

    let version = game_version(&cfg.root);
    let app = gtk4::Application::builder()
        .application_id("org.moltenn.DfoRust")
        .build();

    let url = Rc::new(CString::new(url)?);
    let probe = Rc::new(CString::new(dfo_shell::initialization_script())?);
    let width = cfg.width as i32;
    let height = cfg.height as i32;

    app.connect_activate(move |app| {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("DFO Online")
            .default_width(width)
            .default_height(height)
            .build();

        add_fullscreen_shortcut(&window);

        if launcher {
            let arena2 = arena2.clone().expect("launcher needs the origin");
            window.set_child(Some(&launcher_page(
                &window,
                version.as_deref(),
                arena2,
                Rc::clone(&url),
                Rc::clone(&probe),
            )));
        } else {
            open_game(&window, &url, &probe);
        }

        window.present();
    });

    // GTK's own option parser rejects the shell's flags, which
    // server::Config::from_args has already consumed; pass it no arguments.
    app.run_with_args::<&str>(&[]);
    Ok(())
}

/// The game's build tag from `game/game-seed.json`, next to the served `dist/`.
fn game_version(root: &Path) -> Option<String> {
    let seed = root.parent()?.join("game-seed.json");
    let text = std::fs::read_to_string(seed).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(json.get("version")?.as_str()?.to_string())
}

/// Alt+Enter toggles the window, in the capture phase so the page never sees
/// the key (and pointer lock is not disturbed).
fn add_fullscreen_shortcut(window: &gtk4::ApplicationWindow) {
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let win = window.clone();
    keys.connect_key_pressed(move |_, keyval, _, state| {
        if keyval == gtk4::gdk::Key::Return && state.contains(gtk4::gdk::ModifierType::ALT_MASK) {
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
}

/// The front door: version, game files, saves, play. It exists so the shell
/// does not boot into the game before the player has chosen a folder.
fn launcher_page(
    window: &gtk4::ApplicationWindow,
    version: Option<&str>,
    arena2: server::Arena2,
    url: Rc<CString>,
    probe: Rc<CString>,
) -> GtkBox {
    let page = GtkBox::new(Orientation::Vertical, 10);
    page.set_halign(Align::Center);
    page.set_valign(Align::Center);
    page.set_margin_top(32);
    page.set_margin_bottom(32);
    page.set_margin_start(32);
    page.set_margin_end(32);

    let title = Label::new(None);
    title.set_markup("<span size=\"xx-large\" weight=\"bold\">DFO Online</span>");
    page.append(&title);

    let subtitle = Label::new(Some("Rust shell - experimental"));
    subtitle.add_css_class("dim-label");
    page.append(&subtitle);

    let build = Label::new(Some(&match version {
        Some(version) => format!("Game build {version}"),
        None => "Game build unknown".to_string(),
    }));
    build.add_css_class("dim-label");
    page.append(&build);

    let sep = gtk4::Separator::new(Orientation::Horizontal);
    sep.set_margin_top(10);
    sep.set_margin_bottom(10);
    page.append(&sep);

    let path = Label::new(None);
    path.set_selectable(true);
    path.set_wrap(true);
    path.set_justify(gtk4::Justification::Center);
    path.set_max_width_chars(72);
    show_arena2(&path, &arena2);
    page.append(&path);

    let play = Button::with_label("Play");
    play.add_css_class("suggested-action");
    play.set_receives_default(true);
    {
        let window = window.clone();
        play.connect_clicked(move |_| {
            open_game(&window, &url, &probe);
        });
    }
    page.append(&play);

    let choose = Button::with_label("Game files...");
    {
        let window = window.clone();
        let arena2 = arena2.clone();
        let path = path.clone();
        choose.connect_clicked(move |_| choose_arena2(&window, &arena2, &path));
    }
    page.append(&choose);

    let saves = Button::with_label("Open saves folder");
    saves.connect_clicked(|_| {
        let dir = store::root().join("Saves");
        if let Err(e) = std::process::Command::new("xdg-open").arg(&dir).spawn() {
            eprintln!("[dfo-shell] could not open {}: {e}", dir.display());
        }
    });
    page.append(&saves);

    let quit = Button::with_label("Quit");
    {
        let window = window.clone();
        quit.connect_clicked(move |_| window.close());
    }
    page.append(&quit);

    window.set_default_widget(Some(&play));
    play.grab_focus();
    page
}

fn show_arena2(label: &Label, arena2: &server::Arena2) {
    let current = arena2.read().ok().and_then(|guard| guard.clone());
    match current {
        Some(dir) if dir.is_dir() => label.set_text(&format!("ARENA2: {}", dir.display())),
        Some(dir) => label.set_text(&format!("ARENA2 (missing): {}", dir.display())),
        None => label.set_text("No ARENA2 folder chosen."),
    }
}

fn choose_arena2(window: &gtk4::ApplicationWindow, arena2: &server::Arena2, label: &Label) {
    let dialog = gtk4::FileDialog::builder()
        .title("Select the Daggerfall ARENA2 folder")
        .build();
    let window = window.clone();
    let arena2 = arena2.clone();
    let label = label.clone();
    dialog.select_folder(
        Some(&window),
        None::<&gtk4::gio::Cancellable>,
        move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            let Some(dir) = server::find_arena2(&path) else {
                label.set_text("That folder does not contain ARENA2 (MAPS.BSA/BLOCKS.BSA).");
                return;
            };
            if let Err(e) = server::save_arena2(&dir) {
                label.set_text(&format!("Could not save the folder: {e}"));
                return;
            }
            if let Ok(mut guard) = arena2.write() {
                *guard = Some(dir.clone());
            }
            label.set_text(&format!("ARENA2: {}", dir.display()));
        },
    );
}

fn open_game(window: &gtk4::ApplicationWindow, url: &CString, probe: &CString) {
    // The launcher's buttons go away with the child; clear the default first so
    // GTK does not notify a destroyed widget.
    window.set_default_widget(None::<&gtk4::Widget>);
    let view = unsafe { dfo_webview_new(url.as_ptr(), probe.as_ptr()) };
    let view: gtk4::Widget = unsafe { gtk4::Widget::from_glib_none(view) };
    window.set_child(Some(&view));
    view.grab_focus();
}
