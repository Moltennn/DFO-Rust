/*
 * The page-side probe. Injected before any game script; posts JSON over
 * window.ipc.postMessage, which the Rust side prints.
 *
 * It never touches the game's own canvas: a throwaway 1x1 canvas answers the
 * adapter question, so the game's WebGL2 context keeps its own attributes.
 */
(() => {
  const post = (kind, data) => {
    try {
      window.ipc.postMessage(JSON.stringify({ kind, ...data }));
    } catch {
      /* the channel is gone (page closing) */
    }
  };

  /* ---- the daggerShell storage bridge --------------------------------
   *
   * The page's storage seam speaks localStorage's five words synchronously.
   * Reads come from the map seeded here; writes go through a synchronous
   * XHR so a disk failure still throws at the caller, as the Electron
   * preload's sendSync did. The files are the same ones under userData.
   */
  const sync = (method, url, body) => {
    const xhr = new XMLHttpRequest();
    xhr.open(method, url, false);
    if (body !== undefined) xhr.setRequestHeader('content-type', 'application/json');
    xhr.send(body);
    return xhr;
  };

  let seed = null;
  try {
    const response = sync('GET', '/__store/all');
    if (response.status === 200) seed = JSON.parse(response.responseText);
  } catch {
    /* no file store: the page keeps its browser localStorage */
  }

  if (seed) {
    const map = new Map(Object.entries(seed.values));
    let keys = seed.keys.slice();
    let scanned = performance.now();
    const indexAdd = (key) => {
      if (!keys.includes(key)) keys.push(key);
    };
    const indexDrop = (key) => {
      const at = keys.indexOf(key);
      if (at !== -1) keys.splice(at, 1);
    };
    // The shell rescans its index every 2 s so a save copied into an open
    // Saves folder appears; length() is the only place that pays for it.
    const refresh = () => {
      const now = performance.now();
      if (now - scanned < 2000) return;
      scanned = now;
      try {
        const response = sync('GET', '/__store/keys');
        if (response.status === 200) keys = JSON.parse(response.responseText).keys;
      } catch {
        /* keep the index we have */
      }
    };

    window.daggerShell = {
      platform: 'linux',
      versions: { app: '', electron: undefined },
      savesPath: seed.saves_path,
      relockPointer: () => {},
      onUpdateReady: () => {},
      framesLifted: false,
      gpuTrail: () => {},
      storage: {
        length() {
          refresh();
          return keys.length;
        },
        key(i) {
          return keys[i] ?? null;
        },
        getItem(k) {
          const key = String(k);
          if (map.has(key)) return map.get(key);
          try {
            const response = sync('GET', '/__store/get?key=' + encodeURIComponent(key));
            if (response.status !== 200) return null;
            const value = JSON.parse(response.responseText).value;
            map.set(key, value);
            return value;
          } catch {
            return null;
          }
        },
        setItem(k, v) {
          const key = String(k);
          const value = String(v);
          const response = sync(
            'POST',
            '/__store/set',
            JSON.stringify({ key, value }),
          );
          if (response.status !== 200) {
            throw new Error(`storage write failed (${response.status}): ${response.responseText}`);
          }
          map.set(key, value);
          indexAdd(key);
        },
        removeItem(k) {
          const key = String(k);
          const response = sync('POST', '/__store/remove', JSON.stringify({ key }));
          if (response.status !== 200) {
            throw new Error(`storage remove failed (${response.status}): ${response.responseText}`);
          }
          map.delete(key);
          indexDrop(key);
        },
      },
    };
  }

  post('storage', { mode: seed ? 'files' : 'localStorage', keys: seed ? seed.keys.length : 0 });

  const glInfo = () => {
    try {
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = 1;
      const gl = canvas.getContext('webgl2');
      if (!gl) return { ok: false };
      const ext = gl.getExtension('WEBGL_debug_renderer_info');
      return {
        ok: true,
        vendor: ext ? gl.getParameter(ext.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
        renderer: ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
        version: gl.getParameter(gl.VERSION),
        maxTex: gl.getParameter(gl.MAX_TEXTURE_SIZE),
      };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  };

  const moduleWorker = () => {
    try {
      const url = URL.createObjectURL(new Blob([''], { type: 'text/javascript' }));
      const worker = new Worker(url, { type: 'module' });
      worker.terminate();
      URL.revokeObjectURL(url);
      return true;
    } catch (e) {
      return String(e);
    }
  };

  const fps = () =>
    new Promise((resolve) => {
      let frames = 0;
      const start = performance.now();
      const tick = () => {
        frames++;
        const now = performance.now();
        if (now - start < 1000) requestAnimationFrame(tick);
        else resolve(Math.round((frames * 1000) / (now - start)));
      };
      requestAnimationFrame(tick);
    });

  const snapshot = (tag) => {
    const canvas = document.querySelector('canvas');
    const nav = performance.getEntriesByType('navigation')[0];
    post('snapshot', {
      tag,
      t: Math.round(performance.now()),
      title: document.title,
      ready: document.readyState,
      ua: navigator.userAgent,
      dpr: devicePixelRatio,
      cores: navigator.hardwareConcurrency,
      canvas: canvas
        ? { w: canvas.width, h: canvas.height, cw: canvas.clientWidth, ch: canvas.clientHeight }
        : null,
      gl: glInfo(),
      offscreen: typeof OffscreenCanvas,
      moduleWorker: moduleWorker(),
      nav: nav
        ? {
            dcl: Math.round(nav.domContentLoadedEventEnd),
            load: Math.round(nav.loadEventEnd),
          }
        : null,
    });
  };

  const report = (tag) => {
    snapshot(tag);
    fps().then((f) => post('fps', { tag, f }));
  };

  window.addEventListener('load', () => {
    snapshot('load');
    setTimeout(() => report('t+3'), 3000);
    setTimeout(() => report('t+8'), 8000);
    setTimeout(() => report('t+20'), 20000);
    setTimeout(() => report('t+40'), 40000);
  });

  window.addEventListener('error', (e) =>
    post('error', { message: String(e.message || e.error || e), src: e.filename, line: e.lineno }),
  );
  window.addEventListener('unhandledrejection', (e) =>
    post('error', { message: 'unhandledrejection: ' + String(e.reason) }),
  );

  // F12 opens the WebKit inspector (wry devtools), as the desktop shell has.
  window.addEventListener(
    'keydown',
    (e) => {
      if (e.key === 'F12') {
        e.preventDefault();
        post('devtools', {});
      }
    },
    true,
  );

  // The experiment's own door: poll the local server for a JS expression to
  // run, and post what it answered. This is how the shell is inspected and
  // driven without touching the mouse. Development only.
  let busy = false;
  setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      const response = await fetch('/__dfo/cmd', { cache: 'no-store' });
      if (response.status === 200) {
        const command = await response.json();
        let value;
        try {
          value = await (0, eval)(command.js);
        } catch (e) {
          value = { error: String(e) };
        }
        let out;
        try {
          out = value === undefined ? 'null' : JSON.stringify(value);
        } catch {
          out = JSON.stringify(String(value));
        }
        await fetch('/__dfo/result', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: out,
        });
      }
    } catch {
      /* server gone, page unloading */
    }
    busy = false;
  }, 400);
})();
