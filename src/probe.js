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

})();
