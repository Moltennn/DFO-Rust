//! The optional mod loader: start the DFO-Mods server and let the local origin
//! proxy the game's files to it. The loader patches the store's build on each
//! request, so nothing here needs to know about the mods themselves.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A running loader server. Dropping it stops the child.
pub struct ModsServer {
    /// `http://127.0.0.1:PORT`, the base the local origin proxies to.
    pub base: String,
    child: Child,
    // Held open for the child's lifetime: when the shell dies (even by signal),
    // the pipe closes and the loader sees EOF and exits.
    _stdin: std::process::ChildStdin,
}

impl Drop for ModsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const SCRIPT: &str = "import {startServer} from './modloader/lib/serve.mjs';\n\
process.stdin.resume();\n\
process.stdin.on('end', () => process.exit(0));\n\
const s = await startServer({log: console.log});\n\
console.log('DFO_MODS_URL=' + s.url);\n";

/// Start `node` in the DFO-Mods workspace and wait for its server URL.
pub fn start(dir: &Path) -> Result<ModsServer, String> {
    let entry = dir.join("modloader/lib/serve.mjs");
    if !entry.is_file() {
        return Err(format!(
            "{} does not look like the DFO-Mods workspace (no modloader/lib/serve.mjs)",
            dir.display()
        ));
    }

    let log_path = crate::state_dir().join("mods.log");
    let _ = std::fs::create_dir_all(crate::state_dir());
    let log = Arc::new(Mutex::new(
        std::fs::File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?,
    ));

    // The loader reads the build bundled in the AppImage; on Linux there is no
    // installed asar to find, so name one (DFO_APP, the workspace's extracted
    // asar, or the AppImage if it is running and mounted).
    let app = std::env::var_os("DFO_APP")
        .map(PathBuf::from)
        .or_else(|| workspace_source(dir));

    let node = std::env::var("DFO_NODE").unwrap_or_else(|_| "node".into());
    let mut command = Command::new(&node);
    if let Some(app) = &app {
        command.env("DFO_APP", app);
    }
    let mut child = command
        .arg("--input-type=module")
        .arg("-e")
        .arg(SCRIPT)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {node}: {e}"))?;

    let stdin = child.stdin.take().expect("loader stdin");
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        let log = Arc::clone(&log);
        std::thread::spawn(move || forward(stdout, log, Some(tx)));
    }
    if let Some(stderr) = child.stderr.take() {
        let log = Arc::clone(&log);
        std::thread::spawn(move || forward(stderr, log, None));
    }

    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(url) => {
            let base = url
                .trim_end_matches('/')
                .strip_suffix("/play/index.html")
                .unwrap_or_else(|| url.trim_end_matches('/'))
                .to_string();
            Ok(ModsServer {
                base,
                child,
                _stdin: stdin,
            })
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let tail = std::fs::read_to_string(&log_path).unwrap_or_default();
            let tail: Vec<&str> = tail.lines().rev().take(6).collect();
            Err(format!(
                "the mod loader did not start; see {}\n{}",
                log_path.display(),
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            ))
        }
    }
}

/// A readable build for the loader: an extracted `app.asar`, else the AppImage
/// (which only works while it is mounted by the running game).
fn workspace_source(dir: &Path) -> Option<PathBuf> {
    let asar = [
        dir.join("squashfs-root/resources/app.asar"),
        dir.join("extract/squashfs-root/resources/app.asar"),
    ]
    .into_iter()
    .find(|path| path.is_file());
    if asar.is_some() {
        return asar;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .map(|ext| ext.eq_ignore_ascii_case("AppImage"))
                .unwrap_or(false)
        })
}

fn forward<R: std::io::Read>(
    stream: R,
    log: Arc<Mutex<std::fs::File>>,
    url: Option<mpsc::Sender<String>>,
) {
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if let Ok(mut file) = log.lock() {
            let _ = writeln!(file, "{line}");
        }
        if let (Some(sender), Some(found)) = (url.as_ref(), line.strip_prefix("DFO_MODS_URL=")) {
            let _ = sender.send(found.trim().to_string());
        }
    }
}
