fn main() {
    // A private patched WebKit lives outside the system prefix; DFO_WEBKIT_PREFIX
    // points the pkg-config probe at it.
    if let Ok(prefix) = std::env::var("DFO_WEBKIT_PREFIX") {
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

    let mut build = cc::Build::new();
    build.file("src/webkit_shim.c");
    for path in gtk.include_paths.iter().chain(webkit.include_paths.iter()) {
        build.include(path);
    }
    build.compile("dfo_webkit_shim");

    println!("cargo:rerun-if-env-changed=DFO_WEBKIT_PREFIX");
}
