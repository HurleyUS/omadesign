import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';

export const tiers = ['headless', 'native-window', 'device', 'physical', 'human'];
export const dimensions = ['appearance', 'editability', 'undo', 'save_reopen', 'failure', 'preservation'];
const sha = /^[a-f0-9]{64}$/;
const id = /^[a-z0-9][a-z0-9_.-]*$/;

/** Hash bytes. Args: buffer or text. Returns a SHA-256 hex digest. */
export function hash(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }

/** Encode stable JSON. Args: a JSON value. Returns sorted, compact JSON. */
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
  return JSON.stringify(value);
}

/** Reject paths outside a root, including symlinks. Args: root and relative path. Returns the real filename. */
export function contained(root, relative) {
  if (typeof relative !== 'string' || !relative || path.isAbsolute(relative) || relative.split(/[\\/]/).includes('..')) throw Error('unsafe relative path');
  const base = fs.realpathSync(root);
  const target = fs.realpathSync(path.join(base, relative));
  if (!target.startsWith(`${base}${path.sep}`)) throw Error('path escapes root');
  return target;
}

/** Validate the public ledger. Args: parsed index. Returns the index or throws. */
export function validate(index) {
  if (index.version !== 1 || !Array.isArray(index.fixtures) || !index.fixtures.length || !Array.isArray(index.candidate)) throw Error('invalid suite index');
  const ids = new Set();
  for (const f of index.fixtures) {
    if (!id.test(f.id) || ids.has(f.id) || !['omadesign','omatainer','editbay'].includes(f.product)) throw Error('invalid or duplicate fixture');
    ids.add(f.id);
    if (!f.provenance?.license || !f.provenance?.source || !f.provenance?.authoring || !Array.isArray(f.losses) || !Array.isArray(f.affected) || !f.affected.length) throw Error(`${f.id}: provenance/loss/mapping missing`);
    if (!Array.isArray(f.required) || !f.required.length || new Set(f.required).size !== f.required.length || f.required.some(x => !dimensions.includes(x))) throw Error(`${f.id}: invalid required checks`);
    if (!Array.isArray(f.required_tiers) || !f.required_tiers.length || f.required_tiers.some(x => !tiers.includes(x))) throw Error(`${f.id}: invalid evidence tiers`);
    if (!Number.isSafeInteger(f.limits?.deadline_ms) || f.limits.deadline_ms < 1 || f.limits.deadline_ms > 300000 || !Number.isSafeInteger(f.limits?.max_input_bytes) || f.limits.max_input_bytes < 1) throw Error(`${f.id}: invalid limits`);
    for (const asset of [f.input, f.reference].filter(Boolean)) {
      if (!id.test(asset.root) || !sha.test(asset.sha256) || !Number.isSafeInteger(asset.bytes) || asset.bytes < 1 || typeof asset.path !== 'string' || path.isAbsolute(asset.path) || asset.path.split(/[\\/]/).includes('..')) throw Error(`${f.id}: unpinned/unsafe asset`);
    }
    if (!f.input || (f.required.includes('appearance') && !f.reference)) throw Error(`${f.id}: required input/reference missing from contract`);
    if (!f.adapter?.path || f.adapter.path.startsWith('/') || f.adapter.path.split('/').includes('..')) throw Error(`${f.id}: invalid adapter`);
  }
  if (!index.candidate.length || new Set(index.candidate).size !== index.candidate.length || index.candidate.some(x => !ids.has(x))) throw Error('candidate cohort must be fixed, nonempty and known');
  return index;
}

/** Select fixtures. Args: index, candidate or affected lane, all old/new changed paths. Returns IDs and an explicit selection reason. */
export function select(index, lane, changed = []) {
  validate(index);
  if (lane === 'candidate') return { ids: [...index.candidate], reason: 'complete pinned candidate cohort' };
  if (lane !== 'affected') throw Error('choose affected or candidate');
  if (!changed.length) return { ids: [], reason: 'zero changed paths; no qualification performed' };
  const all = index.fixtures.map(f => f.id);
  const shared = ['scripts/suite/', 'scripts/qualify-suite.mjs', 'qualification/suite/', 'Cargo.', '.github/', 'build.rs', 'src/document.rs', 'src/project.rs', 'src/compositor', 'src/import.rs'];
  if (changed.some(p => shared.some(s => p.startsWith(s)))) return { ids: all, reason: 'shared qualification or product dependency changed' };
  const selected = new Set();
  for (const p of changed) {
    const matching = index.fixtures.filter(f => f.affected.some(prefix => p.startsWith(prefix)));
    if (!matching.length) return { ids: all, reason: 'unmapped change; full affected lane required' };
    matching.forEach(f => selected.add(f.id));
  }
  return { ids: [...selected], reason: 'mapped old and new changed paths' };
}

/** Read source identity without trusting an adapter. Args: Git checkout. Returns commit, tree and every dirty path, including untracked files. */
export function source(root) {
  const git = (...args) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 }).trim();
  return { revision: git('rev-parse','HEAD'), tree: git('rev-parse','HEAD^{tree}'), dirty: Boolean(git('status','--porcelain=v1','--untracked-files=all')) };
}

/** Verify pinned asset bytes. Args: asset and local root bindings. Returns public hash/size metadata, never the private path. */
export function asset(asset, roots) {
  if (!roots[asset.root]) throw Error(`${asset.root}: root unavailable`);
  const file = contained(roots[asset.root], asset.path);
  const st = fs.statSync(file);
  if (!st.isFile() || st.size !== asset.bytes) throw Error('asset byte length mismatch');
  const digest = hash(fs.readFileSync(file));
  if (digest !== asset.sha256) throw Error('asset SHA-256 mismatch');
  return { sha256: digest, bytes: st.size };
}

/** Evaluate each required fixture and tier. Args: selected fixtures, immutable receipts and independently measured current bindings. Returns per-fixture verdicts and historical counts. */
export function gate(fixtures, receipts, current) {
  let historical = 0;
  const rows = [];
  for (const f of fixtures) {
    const binding = current[f.id];
    const matching = receipts.filter(r => {
      if (r.fixture !== f.id) return false;
      const same = binding && canonical(r.binding) === canonical(binding) && r.contract === hash(canonical(f));
      if (!same) historical++;
      return same;
    });
    for (const tier of f.required_tiers) {
      const candidates = matching.filter(r => r.tier === tier).sort((a,b) => b.started_at.localeCompare(a.started_at));
      const latestTime = candidates[0]?.started_at;
      const latest = candidates.filter(r => r.started_at === latestTime);
      const ok = latest.length > 0 && latest.every(r => r.status === 'passed' && f.required.every(k => r.checks?.[k]?.status === 'passed' && r.checks[k].artifacts?.length > 0));
      rows.push({ fixture: f.id, tier, status: ok ? 'passed' : 'unqualified', reason: ok ? 'all required checks passed' : candidates[0]?.reason ?? 'required current receipt unavailable' });
    }
  }
  return { passed: rows.length > 0 && rows.every(r => r.status === 'passed'), rows, historical };
}
