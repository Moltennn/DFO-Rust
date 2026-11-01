/*
 * The page-side probe. Injected before any game script; posts JSON over the
 * shell's message channel (`window.webkit.messageHandlers.dfo`), which the
 * WebKitGTK shim prints.
 *
 * It never touches the game's own canvas: a throwaway 1x1 canvas answers the
 * adapter question, so the game's WebGL2 context keeps its own attributes.
 */
(() => {
  /* ---- shell-side input fixes -----------------------------------------
   *
   * Alt+Enter: the Electron shell's View menu had this accelerator; the page
   * itself only has the touch button. WebKit's default handler fullscreens
   * the top-level window when the page asks.
   *
   * Movement gain: under Wayland, WebKit's pointer lock receives raw relative
   * motion, while the desktop game (Electron on X11) sees the compositor's
   * pointer acceleration. Scale the locked deltas so the same hand motion
   * turns at the same speed. DFO_MOUSE_GAIN=1 restores raw input.
   */
  const mouseGain = () => {
    const value = Number(globalThis.__dfoMouseGain);
    return Number.isFinite(value) && value > 0 && value <= 8 ? value : 1;
  };
  window.addEventListener(
    'mousemove',
    (event) => {
      if (event.__dfoRaw) return;
      const gain = mouseGain();
      if (gain === 1 || !document.pointerLockElement) return;
      try {
        Object.defineProperty(event, 'movementX', {
          value: event.movementX * gain,
          configurable: true,
        });
        Object.defineProperty(event, 'movementY', {
          value: event.movementY * gain,
          configurable: true,
        });
      } catch {
        /* leave the event alone */
      }
    },
    true,
  );

  // One pointer-lock request per gesture. The game asks with
  // unadjustedMovement (which WebKit rejects) and retries after the rejection;
  // a second request from the shell on the same click made WebKit answer two
  // permission requests, and the resulting lock could be nominal only: the
  // compositor never confined the pointer, so movement came from cursor
  // deltas (grid-like, edge-limited).
  const requestPointerLock = Element.prototype.requestPointerLock;
  let lockPending = null;
  Element.prototype.requestPointerLock = function requestPlain() {
    if (document.pointerLockElement === this) return Promise.resolve();
    if (lockPending) return lockPending;
    lockPending = requestPointerLock
      .call(this)
      .finally(() => {
        lockPending = null;
      });
    return lockPending;
  };

  // Keep every CSS cursor (including the game's custom images) off while the
  // pointer is locked, so a hovered element cannot bring a cursor back.
  const lockCursorCss = () => {
    const style = document.createElement('style');
    style.textContent =
      'html[data-dfo-locked][data-dfo-locked], html[data-dfo-locked][data-dfo-locked] *{cursor:none !important}';
    document.head.appendChild(style);
    const sync = () =>
      document.documentElement.toggleAttribute(
        'data-dfo-locked',
        !!document.pointerLockElement,
      );
    document.addEventListener('pointerlockchange', sync);
    sync();
  };
  if (document.head) lockCursorCss();
  else window.addEventListener('DOMContentLoaded', lockCursorCss, { once: true });

  document.addEventListener('pointerlockchange', () =>
    post('lock', {
      locked: !!document.pointerLockElement,
      at: Math.round(performance.now()),
    }),
  );
  document.addEventListener('pointerlockerror', () =>
    post('lockerror', { at: Math.round(performance.now()) }),
  );

  const post = (kind, data) => {
    const message = JSON.stringify({ kind, ...data });
    try {
      window.webkit?.messageHandlers?.dfo?.postMessage(message);
    } catch {
      /* the channel is gone (page closing) */
    }
  };

  /* ---- raw evdev mouse ------------------------------------------------
   *
   * Wayland's relative-pointer stream is quantised to surface pixels; the
   * shell's evdev reader has the mouse's own counts (about 2.5x more for the
   * same hand motion). While locked, drop WebKit's deltas and dispatch the raw
   * ones once per frame. Menus keep WebKit events; raw is only for the look.
   */
  let rawActive = false;
  let rawSocket = null;
  let rawDx = 0;
  let rawDy = 0;
  let rawButtons = 0;
  let rawFrame = 0;
  const flushRaw = () => {
    rawFrame = 0;
    if (!rawActive || !document.pointerLockElement || !document.hasFocus()) {
      rawDx = 0;
      rawDy = 0;
      return;
    }
    const dx = rawDx;
    const dy = rawDy;
    const buttons = rawButtons;
    rawDx = 0;
    rawDy = 0;
    if (dx === 0 && dy === 0) return;
    const event = new MouseEvent('mousemove', {
      bubbles: true,
      cancelable: true,
      buttons,
      clientX: Math.round(innerWidth / 2),
      clientY: Math.round(innerHeight / 2),
    });
    Object.defineProperty(event, 'movementX', { value: dx });
    Object.defineProperty(event, 'movementY', { value: dy });
    event.__dfoRaw = true;
    document.pointerLockElement.dispatchEvent(event);
    if (rawDx || rawDy) rawFrame = requestAnimationFrame(flushRaw);
  };
  const connectRaw = () => {
    const socket = new WebSocket(`ws://${location.host}/__input`);
    rawSocket = socket;
    socket.onmessage = (message) => {
      if (!rawActive) return;
      let delta;
      try {
        delta = JSON.parse(message.data);
      } catch {
        return;
      }
      rawDx += Number(delta.dx) || 0;
      rawDy += Number(delta.dy) || 0;
      rawButtons = Number(delta.buttons) || 0;
      if (!rawFrame && document.pointerLockElement && document.hasFocus()) {
        rawFrame = requestAnimationFrame(flushRaw);
      }
    };
    socket.onerror = () => {
      rawActive = false;
      rawSocket = null;
    };
    socket.onclose = () => {
      rawActive = false;
      rawSocket = null;
    };
  };
  fetch('/__input/status', { cache: 'no-store' })
    .then((response) => response.json())
    .then((status) => {
      post('raw', status);
      if (status.enabled && status.available) {
        rawActive = true;
        connectRaw();
      }
    })
    .catch(() => {
      /* no raw input: the compositor stream and the gain stay in charge */
    });
  window.addEventListener(
    'mousemove',
    (event) => {
      if (!rawActive || !document.pointerLockElement || event.__dfoRaw) return;
      event.stopImmediatePropagation();
    },
    true,
  );


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
