import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { asset, canonical, gate, hash, select, validate } from './contract.mjs';
import { runProcess } from './process.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
fs.mkdirSync(path.join(root,'.artifacts'), { recursive:true });
const scratch = fs.mkdtempSync(path.join(root,'.artifacts/suite-contract-'));
const f = { id:'layered', product:'omadesign', input:{root:'omadesign',path:'input',sha256:hash('source'),bytes:6}, reference:{root:'omadesign',path:'reference',sha256:hash('oracle'),bytes:6}, provenance:{license:'MIT',source:'authored',authoring:'spec 1'},losses:[],affected:['src/formats/psd'],required:['appearance','editability','undo','save_reopen'],required_tiers:['headless'],adapter:{path:'adapter.mjs'},limits:{deadline_ms:1000,max_input_bytes:1024} };
const index = { version:1, fixtures:[f], candidate:['layered'] };
const binding = { source:{revision:'1'.repeat(40),tree:'2'.repeat(40),dirty:false}, binary:{sha256:'3'.repeat(64)} };
const pass = { fixture:f.id,contract:hash(canonical(f)),binding,tier:'headless',started_at:'2026-10-15T12:00:00.000Z',status:'passed',checks:Object.fromEntries(f.required.map(k=>[k,{status:'passed',artifacts:[{sha256:hash(k)}]}])) };

test.after(() => fs.rmSync(scratch,{recursive:true,force:true}));

test('pins reject changed bytes, absent input/reference and escaping symlinks', () => {
  const roots = {omadesign:scratch};
  fs.writeFileSync(path.join(scratch,'input'),'source');
  fs.writeFileSync(path.join(scratch,'reference'),'oracle');
  assert.equal(asset(f.input,roots).sha256,f.input.sha256);
  fs.writeFileSync(path.join(scratch,'input'),'broken');
  assert.throws(()=>asset(f.input,roots),/SHA/);
  fs.rmSync(path.join(scratch,'reference'));
  assert.throws(()=>asset(f.reference,roots));
  assert.throws(()=>asset(f.input,{}),/unavailable/);
  fs.symlinkSync(path.join(root,'Cargo.toml'),path.join(scratch,'escape'));
  assert.throws(()=>asset({...f.input,path:'escape'},roots),/escapes/);
});

test('cohort, reference and duplicate contract failures are closed', () => {
  assert.equal(validate(index),index);
  for (const bad of [{...index,candidate:[]},{...index,candidate:['unknown']},{...index,fixtures:[f,f]},{...index,fixtures:[{...f,reference:null}]}]) assert.throws(()=>validate(bad));
});

test('affected lane includes rename/deletion old paths, shared changes and unknown mappings', () => {
  const other = {...f,id:'vector',affected:['src/formats/svg']};
  const i = {...index,fixtures:[f,other],candidate:[f.id,other.id]};
  assert.deepEqual(select(i,'affected',['src/formats/psd.rs','src/formats/svg_new.rs']).ids,['layered','vector']);
  for (const p of ['scripts/suite/process.mjs','src/new_module.rs']) assert.equal(select(i,'affected',[p]).ids.length,2);
  assert.deepEqual(select(i,'affected',[]).ids,[]);
  assert.equal(gate([],[],{}).passed,false);
});

test('one regression fails regardless of another fixture improving; newer failures survive old passes', () => {
  const other = {...f,id:'second'};
  const second = {...pass,fixture:other.id,contract:hash(canonical(other))};
  const regression = {...pass,started_at:'2026-10-15T13:00:00.000Z',status:'failed'};
  assert.equal(gate([f,other],[pass,second,regression],{layered:binding,second:binding}).passed,false);
  assert.equal(gate([f],[pass],{layered:binding}).passed,true);
  assert.equal(gate([f],[pass,{...regression,started_at:pass.started_at}],{layered:binding}).passed,false);
});

test('new source, binary or fixture expectations keep old passes historical; missing artifact and evidence tier never pass', () => {
  for (const current of [{...binding,source:{...binding.source,revision:'4'.repeat(40)}},{...binding,binary:{sha256:'5'.repeat(64)}}]) {
    const verdict = gate([f],[pass],{layered:current});
    assert.equal(verdict.passed,false);
    assert.equal(verdict.historical,1);
  }
  assert.equal(gate([{...f,losses:['new loss']}],[pass],{layered:binding}).passed,false);
  assert.equal(gate([f],[{...pass,checks:{...pass.checks,appearance:{status:'passed',artifacts:[]}}}],{layered:binding}).passed,false);
  assert.equal(gate([{...f,required_tiers:['headless','human']}],[pass],{layered:binding}).passed,false);
});

test('process groups time out, bound output and terminate descendants', async () => {
  const pidFile = path.join(scratch,'descendant.pid');
  const code = `const fs=require('fs');const c=require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'inherit'});fs.writeFileSync(${JSON.stringify(pidFile)},String(c.pid));setInterval(()=>{},1000)`;
  const result = await runProcess(process.execPath,['-e',code],{cwd:scratch,env:process.env,deadline_ms:300});
  assert.equal(result.reason,'deadline exceeded');
  const pid = Number(fs.readFileSync(pidFile,'utf8'));
  const stat = fs.existsSync(`/proc/${pid}/stat`) ? fs.readFileSync(`/proc/${pid}/stat`,'utf8') : '';
  assert.ok(!stat || stat.split(' ')[2] === 'Z','descendant must be stopped');
  const flood = await runProcess(process.execPath,['-e','process.stdout.write("x".repeat(65536));setInterval(()=>{},1000)'],{cwd:scratch,env:process.env,deadline_ms:1000,output_bytes:256});
  assert.equal(flood.reason,'output limit exceeded');
  assert.ok(flood.output.length<=256);
});
