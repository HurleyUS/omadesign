#!/usr/bin/env node
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = fileURLToPath(new URL('../', import.meta.url));
process.chdir(root);
const manifest = fs.readFileSync('Cargo.toml', 'utf8');
const current = manifest.match(/^version = "([^"]+)"/m)[1];
function publicVersion(version) {
  return version.replace(/(-nightly\.\d{8}\.)([1-9]\d{0,3})$/, (_, prefix, counter) => prefix + counter.padStart(4, '0'));
}
const [command = 'get', suppliedDate, suppliedCounter] = process.argv.slice(2);
if (command === 'get') {
  console.log(publicVersion(current));
} else if (command === 'nightly') {
  const date = suppliedDate ?? new Intl.DateTimeFormat('en-CA', { timeZone: 'America/New_York', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date()).replaceAll('-', '');
  const iso = date.replace(/^(\d{4})(\d{2})(\d{2})$/, '$1-$2-$3');
  if (!/^\d{8}$/.test(date) || Number.isNaN(Date.parse(iso)) || new Date(iso).toISOString().slice(0, 10) !== iso) throw new Error('Use a real date in YYYYMMDD form');
  const base = current.split(/[-+]/)[0];
  const prefix = `v${base}-nightly.${date}.`;
  const tags = execFileSync('git', ['tag', '--list', `${prefix}*`], { encoding: 'utf8' }).trim().split('\n').filter(Boolean);
  const counters = tags.map(tag => tag.slice(prefix.length)).filter(counter => /^\d{4}$/.test(counter)).map(Number);
  const counter = suppliedCounter === undefined ? Math.max(0, ...counters) + 1 : Number(suppliedCounter);
  if ((suppliedCounter !== undefined && !/^\d{1,4}$/.test(suppliedCounter)) || !Number.isInteger(counter) || counter < 1 || counter > 9999) throw new Error('Nightly counter must be 0001 through 9999');
  const cargoVersion = `${base}-nightly.${date}.${counter}`;
  const version = publicVersion(cargoVersion);
  if (tags.includes(`v${version}`)) throw new Error(`Tag v${version} already exists`);
  fs.writeFileSync('Cargo.toml', manifest.replace(/^version = "[^"]+"/m, `version = "${cargoVersion}"`));
  const lock = fs.readFileSync('Cargo.lock', 'utf8');
  const updatedLock = lock.replace(/(\[\[package\]\]\nname = "omadesign"\nversion = ")[^"]+/, `$1${cargoVersion}`);
  fs.writeFileSync('Cargo.lock', updatedLock);
  const inventory = 'vendor/rust-notices/licenses.json';
  if (fs.existsSync(inventory)) {
    const hash = createHash('sha256').update(updatedLock).digest('hex');
    fs.writeFileSync(inventory, fs.readFileSync(inventory, 'utf8').replace(/"cargo_lock_sha256": "[a-f0-9]{64}"/, `"cargo_lock_sha256": "${hash}"`));
  }
  console.log(version);
} else {
  throw new Error('Usage: node scripts/version.mjs [get | nightly [YYYYMMDD [NNNN]]]');
}
