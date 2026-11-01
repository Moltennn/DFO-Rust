#!/usr/bin/env node
/*
 * Extract the game build out of an Electron app.asar into a plain tree.
 *
 * The Rust shell has no asar reader and no need for one: a real build ships
 * plain files. This tool is for the move only - it reads the asar the way
 * Electron does (8-byte pickle, JSON directory, then file data) and writes
 * the `dist/` site plus `game-seed.json` under game/.
 *
 *   node tools/extract-build.mjs [app.asar] [outDir]
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..');

function openAsar(file) {
  const fd = fs.openSync(file, 'r');
  const head = Buffer.alloc(16);
  fs.readSync(fd, head, 0, 16, 0);
  const headerSize = head.readUInt32LE(4);
  const jsonSize = head.readUInt32LE(12);
  const json = Buffer.alloc(jsonSize);
  fs.readSync(fd, json, 0, jsonSize, 16);
  const root = JSON.parse(json.toString('utf8'));
  const dataStart = 8 + headerSize;
  const find = (p) => {
    let node = root;
    for (const seg of String(p).split('/').filter(Boolean)) {
      if (!node?.files || !Object.hasOwn(node.files, seg)) return null;
      node = node.files[seg];
    }
    return node || null;
  };
  return {
    find,
    read(p) {
      const e = find(p);
      if (!e || e.files || !Number.isSafeInteger(e.size)) return null;
      const out = Buffer.alloc(e.size);
      if (e.size) fs.readSync(fd, out, 0, e.size, dataStart + Number(e.offset || 0));
      return out;
    },
    close() { fs.closeSync(fd); },
  };
}

function writeTree(asar, from, to) {
  const node = asar.find(from);
  if (!node?.files) throw new Error(`not a directory in the asar: ${from}`);
  let files = 0, bytes = 0;
  const walk = (dir, rel) => {
    for (const [name, entry] of Object.entries(dir.files)) {
      const r = rel ? `${rel}/${name}` : name;
      if (entry.files) { walk(entry, r); continue; }
      const out = path.join(to, r);
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.writeFileSync(out, asar.read(`${from}/${r}`));
      files++; bytes += entry.size;
    }
  };
  walk(node, '');
  return { files, bytes };
}

const asarPath = path.resolve(process.argv[2] || path.join(ROOT, 'vendor/squashfs-root/resources/app.asar'));
const outDir = path.resolve(process.argv[3] || path.join(ROOT, 'game'));
const asar = openAsar(asarPath);
try {
  const pkg = JSON.parse(asar.read('package.json').toString('utf8'));
  const seed = JSON.parse(asar.read('game-seed.json').toString('utf8'));
  console.log(`shell ${pkg.productName} ${pkg.version}, seed ${seed.version}, ${seed.files.length} files in seed index`);
  if (pkg.version !== seed.version) throw new Error('shell and seed versions differ');

  const t0 = Date.now();
  const dist = writeTree(asar, 'dist', path.join(outDir, 'dist'));
  fs.writeFileSync(path.join(outDir, 'game-seed.json'), asar.read('game-seed.json'));
  const launcher = writeTree(asar, 'launcher', path.join(outDir, 'launcher'));
  console.log(`dist: ${dist.files} files, ${(dist.bytes / 1048576).toFixed(1)} MiB -> ${path.join(outDir, 'dist')}`);
  console.log(`launcher: ${launcher.files} files -> ${path.join(outDir, 'launcher')}`);
  console.log(`done in ${((Date.now() - t0) / 1000).toFixed(1)}s`);
} finally {
  asar.close();
}
