import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
fs.mkdirSync(`${root}.artifacts`, { recursive: true });
test('nightly preparation preserves exact public tags and chooses the next daily counter', () => {
  const directory = fs.mkdtempSync(`${root}.artifacts/version-`);
  try {
    fs.mkdirSync(`${directory}/scripts`);
    fs.copyFileSync(`${root}scripts/version.mjs`, `${directory}/scripts/version.mjs`);
    fs.writeFileSync(`${directory}/Cargo.toml`, '[package]\nname = "omadesign"\nversion = "0.6.3"\n');
    fs.writeFileSync(`${directory}/Cargo.lock`, '[[package]]\nname = "omadesign"\nversion = "0.6.3"\n');
    fs.mkdirSync(`${directory}/site/src`, { recursive: true });
    fs.writeFileSync(`${directory}/site/src/release.ts`, 'export const RELEASE_VERSION = "0.6.3";\nexport const NIGHTLY_VERSION = "0.5.5-nightly.2";\n');
    const git = (...args) => execFileSync('git', args, { cwd: directory, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    const run = (...args) => execFileSync(process.execPath, ['scripts/version.mjs', ...args], { cwd: directory, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
    git('init', '-q');
    git('add', '.');
    git('-c', 'user.name=Release QA', '-c', 'user.email=release@omadesign.app', 'commit', '-qm', 'fixture');
    assert.equal(run('nightly', '20261011', '0001'), '0.6.3-nightly.20261011.0001');
    assert.equal(run('get'), '0.6.3-nightly.20261011.0001');
    assert.equal(fs.readFileSync(`${directory}/site/src/release.ts`, 'utf8'), 'export const RELEASE_VERSION = "0.6.3";\nexport const NIGHTLY_VERSION = "0.6.3-nightly.20261011.0001";\n');
    assert.match(fs.readFileSync(`${directory}/Cargo.lock`, 'utf8'), /version = "0.6.3-nightly.20261011.1"/);
    git('tag', 'v0.6.3-nightly.20261011.0001');
    git('tag', 'v0.6.3-nightly.20261011.0009');
    assert.equal(run('nightly', '20261011'), '0.6.3-nightly.20261011.0010');
    const before = fs.readFileSync(`${directory}/Cargo.toml`, 'utf8');
    for (const args of [['nightly', '20260230'], ['nightly', '20261011', '0000'], ['nightly', '20261011', '10000'], ['nightly', '20261011', '0001']]) {
      assert.throws(() => run(...args));
      assert.equal(fs.readFileSync(`${directory}/Cargo.toml`, 'utf8'), before);
    }
    assert.equal(run('nightly', '20261012'), '0.6.3-nightly.20261012.0001');
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});
