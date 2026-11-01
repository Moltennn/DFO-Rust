/*
 * DFO Online - a Rust shell experiment.
 *
 * One local HTTP server, no Electron. The server exists because the game is
 * already a web app that the mod loader serves over HTTP; using a real origin
 * also sidesteps WebKitGTK's custom-scheme quirks (module workers, module
 * preload, fetch). The window comes next.
 */

mod server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = server::Config::from_args();
    let addr = server::spawn(cfg)?;
    eprintln!("[dfo-shell] server http://{addr}");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
