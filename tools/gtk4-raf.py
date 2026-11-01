#!/usr/bin/env python3
"""GTK4 + WebKitGTK 6.0 rAF probe.

Loads a URL in a GTK4 WebKit view, injects a rAF counter, and prints the
measurements the page posts back over the WebKit message handler. No game
bridge is needed: this asks whether GTK4's frame clock lets WebKit's
requestAnimationFrame follow the 144 Hz output.

    python3 tools/gtk4-raf.py [url]

Start the local origin first if the URL is the game:

    ./tools/run.sh --serve-only

Result (2026-11-01, WebKitGTK 6.0 2.54.2, GTK 4.24.1, 144 Hz DP-1): the
frame clock is 6944 us (144 Hz), the near-60 feature is off, the window is
focused and visible, and rAF is still ~62 fps / 16 ms. The cap is in
WebKitGTK's animation scheduling, not GTK's frame clock:
notes/experiment-3.md.
"""
import json
import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("WebKit", "6.0")
from gi.repository import Gdk, GLib, Gtk, WebKit  # noqa: E402

DEFAULT_URL = (
    "http://127.0.0.1:47565/play/index.html"
    "?exterior&region=Sentinel&loc=Sentinel&shot&class=0"
)

# Runs at document start on every page. Measures two-second rAF windows and
# posts {fps, ms, title, canvas} to Python; the page itself is untouched.
JS = r"""
(() => {
  const send = (data) => {
    try { window.webkit.messageHandlers.dfoFps.postMessage(JSON.stringify(data)); } catch (_) {}
  };
  const measure = () => new Promise((resolve) => {
    let frames = 0;
    const start = performance.now();
    const tick = () => {
      frames++;
      const now = performance.now();
      if (now - start < 2000) requestAnimationFrame(tick);
      else resolve({ fps: (frames * 1000) / (now - start), ms: (now - start) / frames });
    };
    requestAnimationFrame(tick);
  });
  const report = async (tag) => {
    const m = await measure();
    const c = document.querySelector('canvas');
    send({
      tag,
      fps: Math.round(m.fps * 10) / 10,
      ms: Math.round(m.ms * 100) / 100,
      title: document.title,
      canvas: c ? [c.width, c.height] : null,
      dpr: devicePixelRatio,
      focused: document.hasFocus(),
      hidden: document.hidden,
      visibility: document.visibilityState,
      ua: navigator.userAgent,
    });
  };
  addEventListener('load', () => {
    setTimeout(() => report('t+4'), 4000);
    setTimeout(() => report('t+12'), 12000);
    setTimeout(() => report('t+22'), 22000);
  });
})();
"""


class App(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="org.moltenn.Gtk4Raf")
        self.window = None
        self.view = None

    def do_activate(self):
        self.window = Gtk.ApplicationWindow(application=self, title="GTK4 WebKit rAF test")
        self.window.set_default_size(1280, 772)

        manager = WebKit.UserContentManager()
        manager.register_script_message_handler("dfoFps")
        manager.connect("script-message-received::dfoFps", self.on_message)
        manager.add_script(
            WebKit.UserScript.new(
                JS,
                WebKit.UserContentInjectedFrames.TOP_FRAME,
                WebKit.UserScriptInjectionTime.START,
                None,
                None,
            )
        )

        # WebKit keeps rAF near 60 by default. Build the settings first and
        # pass them to the view, so the web process starts with the feature
        # already off (setting it after construction may be too late).
        settings = WebKit.Settings()
        features = WebKit.Settings.get_all_features()
        feature = features.find("PreferPageRenderingUpdatesNear60FPS")
        if feature is not None:
            settings.set_feature_enabled(feature, False)
            print(
                "[gtk4] PreferPageRenderingUpdatesNear60FPS =",
                settings.get_feature_enabled(feature),
                flush=True,
            )

        self.view = WebKit.WebView(settings=settings, user_content_manager=manager)
        url = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_URL
        self.view.load_uri(url)
        self.window.set_child(self.view)
        self.window.present()

        clock = self.window.get_frame_clock()
        if clock is not None:
            # 1 s: before the window is moved; 10 s: after KWin placed it.
            GLib.timeout_add(1000, self.report_clock, clock)
            GLib.timeout_add(10000, self.report_clock, clock)

        # The page reports at t+4, t+12 and t+22; leave time for the last one.
        GLib.timeout_add_seconds(30, self.quit)

    def report_clock(self, clock):
        try:
            info = clock.get_refresh_info(0)
            print("[gtk4] frame clock refresh_info:", info, flush=True)
            timings = clock.get_current_timings()
            if timings is not None:
                print(
                    "[gtk4] frame clock refresh_interval:",
                    timings.get_refresh_interval(),
                    flush=True,
                )
        except Exception as e:
            print("[gtk4] frame clock:", e, flush=True)
        return False

    def on_message(self, _manager, value, *_args):
        try:
            text = value.to_string()
        except Exception:
            text = str(value)
        try:
            print(json.dumps(json.loads(text), sort_keys=True), flush=True)
        except Exception:
            print(text, flush=True)


if __name__ == "__main__":
    sys.exit(App().run([sys.argv[0]]))
