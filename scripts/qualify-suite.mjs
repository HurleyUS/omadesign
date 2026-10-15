#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
import { asset, canonical, contained, fingerprint, gate, hash, libraries, select, source, validate } from './suite/contract.mjs';
import { runProcess } from './suite/process.mjs';
import { rawEvidence } from './suite/raw-evidence.mjs';
import { ownerEvidence } from './suite/owner-evidence.mjs';
import { verifyCompilerReceipt } from './suite/build.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [command, ...args] = process.argv.slice(2);
const options = {};
const allowedOptions=new Set(['index','roots','lane','base','builds','out','config','inside','diff-product','product','raw-bindings','raw-receipts','raw-corpus','owner-receipts']);
for (let i = 0; i < args.length; i += 2) {
  if (!args[i].startsWith('--') || !allowedOptions.has(args[i].slice(2)) || args[i+1] === undefined || options[args[i].slice(2)] !== undefined) throw Error('Use known named --option VALUE pairs');
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
  const diffRoot=options['diff-product']?roots[options['diff-product']]:root;
  if (!diffRoot) throw Error('affected diff product root unavailable');
  const raw = execFileSync('git', ['-C', diffRoot, 'diff', '--no-renames', '--name-only', '-z', `${options.base}...HEAD`]);
  changed = raw.toString().split('\0').filter(Boolean);
}
const selection = ['product','baseline'].includes(lane) ? { ids:index.fixtures.filter(f=>f.product === options.product && (lane!=='baseline' || f.cohort!=='ui')).map(f=>f.id), reason:`${lane} headless developer lane; does not qualify a full candidate` } : select(index, lane, changed,options['diff-product'] ?? 'omadesign');
const fixtures = index.fixtures.filter(f => selection.ids.includes(f.id));
const cohorts = (index.cohorts ?? []).filter(c=>lane==='candidate' || (lane==='affected' && selection.ids.length>0 && (options['diff-product'] ?? 'omadesign')==='omadesign' && (selection.full || changed.some(p=>c.affected.some(s=>p.startsWith(s))))));
/** Verify the enclosing shared lock rather than trusting an internal CLI marker.
 * Args: optional canonical config. Returns nothing or throws before any fixture work.
 */
function assertBuildLock(configPath) {
  const ancestors=[];
  let pid=process.ppid;
  for(let n=0;pid>1 && n<32;n++) {
    const command=fs.readFileSync(`/proc/${pid}/cmdline`,'utf8').split('\0').filter(Boolean);
    ancestors.push(command);
    const helper=command.findIndex(a=>a.endsWith('/scripts/serialized-cargo.mjs'));
    if (!configPath && helper>=0 && command[helper+1]==='--inside') configPath=command[helper+2];
    pid=Number(fs.readFileSync(`/proc/${pid}/status`,'utf8').match(/^PPid:\s+(\d+)/m)?.[1] ?? 0);
  }
  if (!configPath) throw Error('Internal fixture execution requires the actual global serialized lock');
  const config=read(configPath);
  if (!config.lock?.startsWith('/home/') || !ancestors.some(a=>path.basename(a[0])==='flock' && a[1]===config.lock)) throw Error('Global serialized lock ancestry unavailable');
}
if (command==='run' && fixtures.length===0 && cohorts.length===0) {
  console.log(JSON.stringify({lane,selection,passed:false,status:'not_run',reason:'zero selected fixtures; no qualification performed'},null,2));
  process.exitCode=1;
} else if (command === 'plan') {
  console.log(JSON.stringify({ lane, ...selection, cohorts:cohorts.map(c=>c.id),owner_gates:lane==='candidate'?(index.owner_gates ?? []).map(r=>`${r.id}.${r.product}`):[] }, null, 2));
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
    if (command==='run') assertBuildLock(options.config);
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
        const inputs = { input: asset(f.input, roots), ...(f.reference ? { reference: asset(f.reference, roots) } : {}), dependencies:(f.dependencies ?? []).map(a=>asset(a,roots)) };
        const adapterRoot = roots[f.adapter.root ?? f.product];
        const adapter = contained(adapterRoot, f.adapter.path);
        const adapterSource = source(adapterRoot);
        if (adapterSource.dirty) throw Error('adapter source has dirty tracked or untracked files');
        const build = builds[f.id] ?? builds[f.product];
        let binary = null;
        if (f.adapter.kind !== 'node') {
          if (!build?.binary || build.revision !== actualSource.revision || build.tree !== actualSource.tree) throw Error('current source-bound build unavailable');
          if (!build.runtime_root || canonical(source(build.runtime_root)) !== canonical(actualSource)) throw Error('build runtime fixture checkout unavailable or changed');
          binary = { ...fingerprint(build.binary), driver: build.driver };
          if (binary.sha256 !== build.sha256) throw Error('build executable hash changed');
          if (build.compiler_receipt) verifyCompilerReceipt(build);
          else {
            let pid=process.ppid, live=false;
            for(let n=0;pid>1 && n<32;n++) {
              if (pid===build.live_test_pid && fingerprint(`/proc/${pid}/exe`).sha256===binary.sha256) {live=true;break;}
              pid=Number(fs.readFileSync(`/proc/${pid}/status`,'utf8').match(/^PPid:\s+(\d+)/m)?.[1] ?? 0);
            }
            if (command!=='run' || !live) throw Error('current locked compiler receipt or live Cargo test binding unavailable');
          }
        }
        const tools = [];
        for (const name of f.tools ?? []) {
          const tool = build?.tools?.[name];
          if (!tool?.path) throw Error(`required tool ${name} unavailable`);
          const actual = fingerprint(tool.path);
          if (actual.sha256 !== tool.sha256) throw Error(`required tool ${name} hash changed`);
          const probe = spawnSync(tool.path, tool.version_args ?? ['--version'], {encoding:'utf8',timeout:2000,maxBuffer:32768});
          const version = `${probe.stdout ?? ''}${probe.stderr ?? ''}`.trim();
          if (probe.status !== 0 || probe.error || !version) throw Error(`required tool ${name} version probe failed`);
          tools.push({ id: name, ...actual, version_sha256:hash(version) });
        }
        const nativeLibraries=binary?libraries(build.binary):[];
        const runnerSource=source(root);
        if (runnerSource.dirty) throw Error('product runner source dirty');
        binding = { source: actualSource, runner_source:runnerSource, adapter_source: adapterSource, adapter_sha256: hash(fs.readFileSync(adapter)), binary, inputs, tools, native_libraries:nativeLibraries, runner_sha256: hash(fs.readFileSync(fileURLToPath(import.meta.url))), contract_sha256: hash(fs.readFileSync(new URL('./suite/contract.mjs', import.meta.url))), process_sha256:hash(fs.readFileSync(new URL('./suite/process.mjs',import.meta.url))), node_sha256: fingerprint(process.execPath).sha256, host: { id: hash(os.hostname()), platform: os.platform(), arch: os.arch(), release: os.release() } };
        current[f.id] = binding;
        if (command === 'run') {
          const scratch = fs.mkdtempSync(path.join(out, 'run-'));
          const request = { fixture: f, input: contained(roots[f.input.root], f.input.path), reference: f.reference ? contained(roots[f.reference.root], f.reference.path) : null, output: scratch, binary: build?.binary, driver: build?.driver, tools: build?.tools ?? {}, verified_binding:binding,ui_receipts:build?.ui_receipts,ui_captures:build?.ui_captures };
          fs.writeFileSync(path.join(scratch,'request.json'), JSON.stringify(request));
          const started = performance.now();
          const result = await runProcess(process.execPath, [adapter, path.join(scratch,'request.json')], { cwd: productRoot, env: { ...process.env, TMPDIR: scratch, TMP: scratch, TEMP: scratch }, deadline_ms: f.limits.deadline_ms });
          if (canonical(source(productRoot)) !== canonical(actualSource) || canonical(source(adapterRoot)) !== canonical(adapterSource)) throw Error('product or adapter source changed during run');
          asset(f.input, roots);
          if (f.reference) asset(f.reference, roots);
          for (const dependency of f.dependencies ?? []) asset(dependency,roots);
          if (binary && fingerprint(build.binary).sha256 !== binary.sha256) throw Error('build executable changed during run');
          if (binary && canonical(libraries(build.binary)) !== canonical(nativeLibraries)) throw Error('required native library changed during run');
          if (binary && canonical(source(build.runtime_root)) !== canonical(actualSource)) throw Error('build runtime fixture checkout changed during run');
          for (const tool of tools) if (fingerprint(build.tools[tool.id].path).sha256 !== tool.sha256) throw Error('required tool changed during run');
          fs.writeFileSync(path.join(scratch,'process.log'), result.output);
          const reportFile = path.join(scratch,'result.json');
          const report = fs.existsSync(reportFile) && fs.statSync(reportFile).size <= 1048576 ? read(reportFile) : {};
          const checks = {};
          for (const key of f.required) {
            const check = report.checks?.[key];
            const artifacts = [];
            for (const relative of check?.artifacts ?? []) {
              const file = contained(scratch, relative);
              if (fs.statSync(file).size>4194304) throw Error('required artifact exceeds bounded receipt budget');
              const bytes = fs.readFileSync(file);
              artifacts.push({ path: path.relative(out,file), sha256: hash(bytes), bytes: bytes.length });
            }
            checks[key] = { status: check?.status === 'passed' && artifacts.length ? 'passed' : check?.status==='unavailable'?'unavailable':'failed', artifacts };
          }
          const passed = result.code === 0 && !result.reason && f.required.every(k => checks[k].status === 'passed');
          receipt = { version: 1, fixture: f.id, contract: hash(canonical(f)), tier: 'headless', binding, started_at, duration_ms: Math.round(performance.now()-started), status: passed ? 'passed' : 'failed', reason: passed ? 'required headless checks passed' : result.reason ?? 'adapter exit or required evidence failed', checks, losses: f.losses, evidence: binary ? binary.driver === 'libtest' ? 'headless library/test executable; installed CLI unproven' : 'product CLI executable; installed app unproven' : 'Node-only; no product binary', memory:result.memory,limits:f.limits };
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
    for (const cohort of cohorts) {
      const raw = await rawEvidence(cohort,{roots,builds,bindings:options['raw-bindings']?read(options['raw-bindings']):{},receipts:options['raw-receipts'],corpus:options['raw-corpus']});
      result.passed &&= raw.passed;
      result.rows ??= [];
      result.rows.push(...raw.rows);
      const bytes=canonical({version:1,cohort:cohort.id,checked_at:new Date().toISOString(),...raw});
      fs.writeFileSync(path.join(out,`producer-${hash(bytes)}.json`),bytes,{flag:'wx'});
    }
    if (lane==='candidate') {
      const owners=ownerEvidence(index.owner_gates ?? [],{directory:options['owner-receipts'],roots,builds});
      result.passed &&= owners.length>0 && owners.every(r=>r.status==='passed');
      result.rows ??= [];
      result.rows.push(...owners);
    }
    console.log(JSON.stringify({ lane, selection, ...result, issues }, null, 2));
    process.exitCode = result.passed && issues.length === 0 ? 0 : 1;
  }
} else throw Error('Use plan|verify|run|gate --lane affected|candidate [--base COMMIT] [--roots LOCAL.json] [--builds LOCAL.json] [--out DIR] [--config BUILD-CONFIG]');
