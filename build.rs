fn main() {
    // A private patched WebKit lives outside the system prefix; DFO_WEBKIT_PREFIX
    // points the pkg-config probe at it.
    let private = std::env::var("DFO_WEBKIT_PREFIX").ok();
    if let Some(prefix) = private.as_deref() {
        let pkgconfig = format!("{prefix}/lib/pkgconfig");
        let old = std::env::var("PKG_CONFIG_PATH").unwrap_or_default();
        // Safety: this build script is a single-threaded process and nothing
        // reads the environment concurrently before the probes below.
        unsafe {
            std::env::set_var("PKG_CONFIG_PATH", format!("{pkgconfig}:{old}"));
        }
    }

    let gtk = pkg_config::Config::new()
        .probe("gtk4")
        .expect("gtk4 (pkg-config)");
    let webkit = pkg_config::Config::new()
        .probe("webkitgtk-6.0")
        .expect("webkitgtk-6.0 (set DFO_WEBKIT_PREFIX for the private build)");

    // The system WebKitGTK works but caps the page at 60 fps. Say so loudly
    // rather than letting a first build blame the game.
    if private.is_none()
        && !webkit
            .include_paths
            .iter()
            .any(|path| path.to_string_lossy().contains("webkitgtk-patched"))
        && std::env::var_os("DFO_ALLOW_SYSTEM_WEBKIT").is_none()
    {
        println!(
            "cargo:warning=building against the system WebKitGTK, which caps the page at 60 fps."
        );
        println!(
            "cargo:warning=use tools/build.sh for the private patched build, or set DFO_ALLOW_SYSTEM_WEBKIT=1 to accept 60 fps."
        );
    }

    let mut build = cc::Build::new();
    build.file("src/webkit_shim.c");
    for path in gtk.include_paths.iter().chain(webkit.include_paths.iter()) {
        build.include(path);
    }
    build.compile("dfo_webkit_shim");

    println!("cargo:rerun-if-env-changed=DFO_WEBKIT_PREFIX");
    println!("cargo:rerun-if-env-changed=DFO_ALLOW_SYSTEM_WEBKIT");
}
