#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { asset, canonical, contained, gate, hash, select, source, validate } from './suite/contract.mjs';
import { runProcess } from './suite/process.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [command, ...args] = process.argv.slice(2);
const options = {};
for (let i = 0; i < args.length; i += 2) {
  if (!args[i].startsWith('--') || args[i+1] === undefined || options[args[i].slice(2)] !== undefined) throw Error('Use named --option VALUE pairs');
  options[args[i].slice(2)] = args[i+1];
}
const read = f => JSON.parse(fs.readFileSync(f, 'utf8'));
const indexPath = options.index ?? path.join(root, 'qualification/suite/index.json');
const index = validate(read(indexPath));
const roots = { omadesign: root, ...(options.roots ? read(options.roots) : {}) };
const lane = options.lane ?? 'candidate';
let changed = [];
if (lane === 'affected') {
  if (!options.base) throw Error('Affected selection requires --base COMMIT');
  const raw = execFileSync('git', ['-C', root, 'diff', '--no-renames', '--name-only', '-z', `${options.base}...HEAD`]);
  changed = raw.toString().split('\0').filter(Boolean);
}
const selection = select(index, lane, changed);
const fixtures = index.fixtures.filter(f => selection.ids.includes(f.id));
if (command === 'plan') {
  console.log(JSON.stringify({ lane, ...selection }, null, 2));
} else if (['run','gate','verify'].includes(command)) {
  const builds = options.builds ? read(options.builds) : {};
  const out = path.resolve(options.out ?? path.join(root, '.artifacts/suite'));
  if (!out.startsWith('/home/')) throw Error('Suite outputs must remain under /home');
  fs.mkdirSync(out, { recursive: true });
  if (command === 'run' && options.inside !== 'locked') {
    if (!options.config) throw Error('Run requires --config with the global serialized build lock');
    const config = read(options.config);
    if (!path.isAbsolute(config.lock) || !config.lock.startsWith('/home/') || !config.lock.startsWith(`${path.resolve(config.scratch)}/`)) throw Error('Invalid global build lock');
    const result = await runProcess('flock', [config.lock, process.execPath, fileURLToPath(import.meta.url), command, ...args, '--inside', 'locked'], { cwd: root, env: process.env, deadline_ms: 300000, output_bytes: 65536 });
    process.stdout.write(result.output);
    process.exitCode = result.reason ? 1 : result.code ?? 1;
  } else {
    const current = {}, receipts = [], issues = [];
    for (const f of fixtures) {
      const started_at = new Date().toISOString();
      let binding, receipt;
      try {
        const productRoot = roots[f.product];
        if (!productRoot) throw Error('product root unavailable');
        const actualSource = source(productRoot);
        if (actualSource.dirty) throw Error('product source has dirty tracked or untracked files');
        if (f.input.bytes > f.limits.max_input_bytes) throw Error('input exceeds fixture budget');
        const inputs = { input: asset(f.input, roots), ...(f.reference ? { reference: asset(f.reference, roots) } : {}) };
        const adapterRoot = roots[f.adapter.root ?? f.product];
        const adapter = contained(adapterRoot, f.adapter.path);
        const adapterSource = source(adapterRoot);
        if (adapterSource.dirty) throw Error('adapter source has dirty tracked or untracked files');
        const build = builds[f.product];
        let binary = null;
        if (f.adapter.kind !== 'node') {
          if (!build?.binary || build.revision !== actualSource.revision || build.tree !== actualSource.tree) throw Error('current source-bound build unavailable');
          const bytes = fs.readFileSync(build.binary);
          binary = { sha256: hash(bytes), bytes: bytes.length, driver: build.driver };
          if (binary.sha256 !== build.sha256) throw Error('build executable hash changed');
        }
        const tools = [];
        for (const name of f.tools ?? []) {
          const tool = build?.tools?.[name];
          if (!tool?.path || !tool.version) throw Error(`required tool ${name} unavailable`);
          const bytes = fs.readFileSync(tool.path);
          if (hash(bytes) !== tool.sha256) throw Error(`required tool ${name} hash changed`);
          tools.push({ id: name, sha256: hash(bytes), bytes: bytes.length, version: tool.version });
        }
        binding = { source: actualSource, adapter_source: adapterSource, adapter_sha256: hash(fs.readFileSync(adapter)), binary, inputs, tools, runner_sha256: hash(fs.readFileSync(fileURLToPath(import.meta.url))), contract_sha256: hash(fs.readFileSync(new URL('./suite/contract.mjs', import.meta.url))), node_sha256: hash(fs.readFileSync(process.execPath)), host: { id: hash(os.hostname()), platform: os.platform(), arch: os.arch(), release: os.release() } };
        current[f.id] = binding;
        if (command === 'run') {
          const scratch = fs.mkdtempSync(path.join(out, 'run-'));
          const request = { fixture: f, input: contained(roots[f.input.root], f.input.path), reference: f.reference ? contained(roots[f.reference.root], f.reference.path) : null, output: scratch, binary: build?.binary, driver: build?.driver, tools: build?.tools ?? {} };
          fs.writeFileSync(path.join(scratch,'request.json'), JSON.stringify(request));
          const started = performance.now();
          const result = await runProcess(process.execPath, [adapter, path.join(scratch,'request.json')], { cwd: productRoot, env: { ...process.env, TMPDIR: scratch, TMP: scratch, TEMP: scratch }, deadline_ms: f.limits.deadline_ms });
          if (canonical(source(productRoot)) !== canonical(actualSource) || canonical(source(adapterRoot)) !== canonical(adapterSource)) throw Error('product or adapter source changed during run');
          asset(f.input, roots);
          if (f.reference) asset(f.reference, roots);
          if (binary && hash(fs.readFileSync(build.binary)) !== binary.sha256) throw Error('build executable changed during run');
          for (const tool of tools) if (hash(fs.readFileSync(build.tools[tool.id].path)) !== tool.sha256) throw Error('required tool changed during run');
          fs.writeFileSync(path.join(scratch,'process.log'), result.output);
          const reportFile = path.join(scratch,'result.json');
          const report = fs.existsSync(reportFile) && fs.statSync(reportFile).size <= 1048576 ? read(reportFile) : {};
          const checks = {};
          for (const key of f.required) {
            const check = report.checks?.[key];
            const artifacts = [];
            for (const relative of check?.artifacts ?? []) {
              const file = contained(scratch, relative);
              const bytes = fs.readFileSync(file);
              artifacts.push({ path: path.relative(out,file), sha256: hash(bytes), bytes: bytes.length });
            }
            checks[key] = { status: check?.status === 'passed' && artifacts.length ? 'passed' : 'failed', artifacts };
          }
          const passed = result.code === 0 && !result.reason && f.required.every(k => checks[k].status === 'passed');
          receipt = { version: 1, fixture: f.id, contract: hash(canonical(f)), tier: 'headless', binding, started_at, duration_ms: Math.round(performance.now()-started), status: passed ? 'passed' : 'failed', reason: passed ? 'required headless checks passed' : result.reason ?? 'adapter exit or required evidence failed', checks, losses: f.losses, evidence: binary ? 'product executable' : 'Node-only; no product binary', memory: report.memory ?? { parent_rss_kib: null, worker_rss_kib: null, reason: 'not measured; no memory acceptance claimed' } };
          fs.rmSync(path.join(scratch,'request.json'));
        }
      } catch (error) {
        const reason = /^(product |adapter |input |asset |build |current |required |[a-z]+: root unavailable)/.test(error.message) ? error.message : 'required input, reference, executable or artifact unavailable/invalid';
        issues.push({ fixture: f.id, status: 'unavailable', reason });
        if (command === 'run') receipt = { version: 1, fixture: f.id, contract: hash(canonical(f)), tier: 'headless', binding: binding ?? null, started_at, status: 'unavailable', reason, checks: {}, losses: f.losses };
      }
      if (receipt) {
        const bytes = canonical(receipt);
        fs.writeFileSync(path.join(out, `${hash(bytes)}.json`), bytes, { flag: 'wx' });
      }
    }
    if (command !== 'verify') {
      for (const name of fs.readdirSync(out).filter(n => /^[a-f0-9]{64}\.json$/.test(n))) {
        const bytes = fs.readFileSync(path.join(out,name));
        if (`${hash(bytes)}.json` !== name) throw Error('receipt integrity mismatch');
        const receipt = JSON.parse(bytes);
        if (receipt.version !== 1 || !['passed','failed','unavailable'].includes(receipt.status) || !/^\d{4}-\d\d-\d\dT/.test(receipt.started_at ?? '')) throw Error('invalid receipt');
        for (const check of Object.values(receipt.checks ?? {})) for (const a of check.artifacts ?? []) asset({ ...a, root:'receipts' }, { receipts:out });
        receipts.push(receipt);
      }
    }
    const result = command === 'verify' ? { passed: fixtures.length > 0 && issues.length === 0 } : gate(fixtures,receipts,current);
    console.log(JSON.stringify({ lane, selection, ...result, issues }, null, 2));
    process.exitCode = result.passed && issues.length === 0 ? 0 : 1;
  }
} else throw Error('Use plan|verify|run|gate --lane affected|candidate [--base COMMIT] [--roots LOCAL.json] [--builds LOCAL.json] [--out DIR] [--config BUILD-CONFIG]');
