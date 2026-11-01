#!/usr/bin/env node
/*
 * Drive the running shell's page without touching the mouse.
 *
 *   node tools/ctl.mjs '<js expression>'
 *
 * Reads the shell's base URL from <tmp>/dfo-shell-url, queues the expression
 * (the injected probe evals it and posts the answer back), prints the answer.
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

const js = process.argv.slice(2).join(' ').trim();
if (!js) {
  console.error("usage: node tools/ctl.mjs '<js expression>'");
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const put = await fetch(`${base}/__dfo/cmd`, { method: 'POST', body: js });
if (!put.ok) {
  console.error(`queue failed: ${put.status} ${await put.text()}`);
  process.exit(1);
}

for (let i = 0; i < 50; i++) {
  await sleep(120);
  const res = await fetch(`${base}/__dfo/result`);
  const found = await res.json();
  if (found.length) {
    for (const value of found) console.log(JSON.stringify(value, null, 2));
    process.exit(0);
  }
}
console.error('no answer from the page');
process.exit(1);
