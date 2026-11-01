#!/usr/bin/env node
/*
 * Walk the game from the intro to a Test Room character, then measure.
 * The no-mouse equivalent of `dfo enter`: it clicks by visible text inside
 * the page, over the control channel.
 *
 *   node tools/enter.mjs
 */
import fs from 'node:fs';

import { URL_FILE } from './state.mjs';

const baseFile = URL_FILE;
let base;
try {
  base = fs.readFileSync(baseFile, 'utf8').trim();
} catch {
  console.error(`no ${baseFile} - is the shell running?`);
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const BUTTONS = `[...document.querySelectorAll("button,[role=button],a")]`;

/** Queue one expression; wait for the page's answer. */
async function run(js) {
  const put = await fetch(`${base}/__dfo/cmd`, { method: 'POST', body: js });
  if (!put.ok) throw new Error(`queue failed: ${put.status} ${await put.text()}`);
  for (let i = 0; i < 200; i++) {
    await sleep(100);
    const answers = await (await fetch(`${base}/__dfo/result`)).json();
    if (answers.length) return answers[answers.length - 1];
  }
  throw new Error('no answer from the page');
}

const has = (re) =>
  run(`!!${BUTTONS}.find(e => new RegExp(${JSON.stringify(re)}, "i").test((e.textContent || "").trim()))`);

/** Click the first button whose text matches; returns its text or null. */
const click = (re) =>
  run(`(() => {
    const el = ${BUTTONS}.find(e => new RegExp(${JSON.stringify(re)}, "i").test((e.textContent || "").trim()));
    if (!el) return null;
    el.click();
    return (el.textContent || "").trim();
  })()`);

async function waitFor(re, ms = 30000) {
  const started = Date.now();
  while (Date.now() - started < ms) {
    if (await has(re)) return true;
    await sleep(400);
  }
  return false;
}

const step = async (label, re) => {
  if (!(await waitFor(re, 30000))) {
    console.error(`[enter] could not find "${label}"`);
    process.exit(1);
  }
  const text = await click(re);
  console.log(`[enter] clicked ${label}: ${text}`);
};

if (await has('skip intro')) {
  console.log(`[enter] clicked Skip intro: ${await click('skip intro')}`);
}
await step('Offline', 'offline');
await step('Test Room', 'test room');
await step('a preset character', 'nord warrior');

// The world is up when the game's canvas has been resized past its default.
const started = Date.now();
let ready = false;
while (Date.now() - started < 120000) {
  ready = await run(`document.querySelector("canvas")?.width > 300`);
  if (ready) break;
  await sleep(500);
}
console.log(`[enter] world ${ready ? 'ready' : 'NOT ready'} after ${((Date.now() - started) / 1000).toFixed(1)}s`);

// The Test Room opens on the pause menu; Resume if it is there.
if (await has('^resume$')) console.log(`[enter] clicked Resume: ${await click('^resume$')}`);
await sleep(1000);

const report = await run(`(async () => {
  const canvas = document.querySelector("canvas");
  const gl = canvas && canvas.getContext("webgl2");
  const ext = gl && gl.getExtension("WEBGL_debug_renderer_info");
  const t0 = performance.now();
  let frames = 0;
  const times = [];
  let last = t0;
  await new Promise((resolve) => {
    const tick = () => {
      const now = performance.now();
      times.push(now - last);
      last = now;
      frames++;
      if (now - t0 < 3000) requestAnimationFrame(tick);
      else resolve();
    };
    requestAnimationFrame(tick);
  });
  times.sort((a, b) => a - b);
  return {
    title: document.title,
    fps: Math.round((frames * 1000) / (performance.now() - t0)),
    medianMs: +times[Math.floor(times.length / 2)].toFixed(2),
    worstMs: +times[times.length - 1].toFixed(2),
    canvas: canvas ? { w: canvas.width, h: canvas.height } : null,
    gl: gl ? {
      vendor: ext ? gl.getParameter(ext.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
      renderer: ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
    } : null,
    pointerLocked: !!document.pointerLockElement,
  };
})()`);
console.log('[enter] report');
console.log(JSON.stringify(report, null, 2));
