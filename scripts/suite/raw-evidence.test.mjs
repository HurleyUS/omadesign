import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { hash, fingerprint, validate } from './contract.mjs';
import { cameraData, frozenCameraEvidence } from './raw-evidence.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
fs.mkdirSync(path.join(root,'.artifacts'),{recursive:true});
const scratch=fs.mkdtempSync(path.join(root,'.artifacts/raw-reader-'));
test.after(()=>fs.rmSync(scratch,{recursive:true,force:true}));

/** Author synthetic camera records and tiny pixel data; no decoder or product binary exists.
 * Args: unique fixture name. Returns pinned metadata, private bindings and record editing helpers.
 */
function fixture(name) {
  const dir=path.join(scratch,name);fs.mkdirSync(dir);
  const git=(...args)=>execFileSync('git',['-C',dir,...args],{stdio:'pipe'}).toString().trim();
  git('init','--quiet');
  const write=(name,bytes)=>{const file=path.join(dir,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,bytes);return file};
  const sourceFile=write('input.cr3','authored synthetic input');
  const sensor=write('sensor.u16le',Buffer.from([0x34,0x12]));
  const rgb=write('rgb.rgb16le',Buffer.from([0,0,1,0,2,0]));
  let fnv=0xcbf29ce484222325n;for(const byte of [0x34,0x12])fnv=BigInt.asUintN(64,(fnv^BigInt(byte))*0x100000001b3n);
  const id='cr3-canon-m50-raw',host=hash('synthetic producer host');
  const tools=Object.fromEntries(['libraw-integration','independent-sensor'].map(id=>[id,{name:id,version:'synthetic-only',platform:'synthetic-node-test',command:[id,'SOURCE'],executable_sha256:hash(id)}]));
  const references=[{id:'libraw-integration',kind:'libraw_integration',representation:'oriented-linear-srgb-rgb16le',file:rgb,fnv:null},{id:'independent-sensor',kind:'independent_sensor',representation:'sensor-u16le-masked',file:sensor,fnv:fnv.toString(16).padStart(16,'0')}].map(r=>({id:r.id,kind:r.kind,representation:r.representation,sha256:fingerprint(r.file).sha256,dimensions:[1,1],tolerance:{mean:0,max:0},toolchain:tools[r.id],sensor_fnv1a64:r.fnv}));
  const entry={id,required:true,source:{sha256:fingerprint(sourceFile).sha256,size_bytes:fs.statSync(sourceFile).size},references,required_reference_kinds:references.map(r=>r.kind),identity:{make:'Canon',model:'EOS M50',orientation:0,camera_wb:[1,1,1,1],output_dimensions:[1,1]},preview_tolerance:{mean:0,max:0}};
  const manifest={cases:[entry,...Array.from({length:79},(_,i)=>({id:`unavailable-${i}`,required:true}))]};
  const hostFunction='fn host_identity() -> String {\n    "synthetic-only".into()\n}\n';
  const sources={'manifest.json':JSON.stringify(manifest),'inspector.mjs':'export function validateManifest(m) { if (m.cases.length !== 80) throw Error("wrong cohort"); return m; }\nexport function extensions() { return ["cr3"]; }\n','raw.rs':'authored synthetic source','qualification.rs':hostFunction,'Cargo.lock':'authored synthetic lock'};
  for(let i=0;i<6;i++)sources[`transitive-${i}.rs`]='authored synthetic source';
  for(const [file,bytes] of Object.entries(sources))write(file,bytes);
  git('add','.');git('-c','user.name=Suite Reader','-c','user.email=reader@invalid','-c','core.hooksPath=/dev/null','commit','--quiet','-m','Synthetic camera reader data');
  const revision=git('rev-parse','HEAD'),tree=git('rev-parse','HEAD^{tree}');
  const sourcePins=Object.entries(sources).map(([file,bytes])=>({path:file,sha256:hash(bytes),bytes:Buffer.byteLength(bytes),blob:git('rev-parse',`${revision}:${file}`)}));
  const pinSource=file=>({root:'raw-camera',...sourcePins.find(p=>p.path===file)});
  const record=(id,value,immutable=false)=>{const bytes=typeof value==='string'?value:JSON.stringify(value);const file=write(immutable?`${hash(bytes)}.json`:`${id}.json`,bytes);return {root:'records',path:path.basename(file),...fingerprint(file)}};
  const binary={sha256:hash('synthetic recorded binary'),bytes:42,live_observation:{sha256:hash('synthetic recorded binary'),bytes:42,matches_producer:true}};
  const exe=path.join(dir,'product-executable-does-not-exist');
  const cargo_args=['test','--locked','--lib','--','formats::raw::qualification::raw_camera_corpus','--ignored','--exact','--nocapture','--test-threads=1'];
  const compile={schema:1,repository:'omadesign',revision,source_tree:tree,dirty_before:false,dirty_after:false,exit_code:0,signal:null,receipt_errors:[],cargo_args,executed_cargo_args:['test','--message-format=json-render-diagnostics',...cargo_args.slice(1)],helper_sha256:hash('synthetic helper'),receipt_helper_sha256:hash('synthetic receipt module'),config_sha256:hash('synthetic config'),executables:[{path:exe,sha256:binary.sha256,size:binary.bytes}]};
  const original=record('original',compile,true),workflow=record('workflow',compile,true);
  const stats={samples:3,mean:0,max:0,channel_mean:[0,0,0],channel_max:[0,0,0]};
  const checks=['normalized_preview','normalized_full','png_u16_codec_decode','tif_u16_codec_decode','normalized_preview','normalized_full','settings_exposure_crop_rotation_reopen'].map(check=>({check,dimensions:[1,1],stats,channel_bits:16,pixel_sha256:hash('synthetic pixels'),owned_source_sha256:entry.source.sha256}));
  const receipt={schema_version:1,case:id,required:true,checked:true,status:'passed',errors:[],source_before_sha256:entry.source.sha256,source_after_sha256:entry.source.sha256,manifest_sha256:pinSource('manifest.json').sha256,runner:{revision,sha256:binary.sha256,cargo_lock_sha256:hash(sources['Cargo.lock']),host_sha256:host},checks,required_reference_kinds:entry.required_reference_kinds,reference_tools:tools,observed:{...entry.identity,dimensions:[1,1]},comparisons:references.map(r=>({reference:r.id,kind:r.kind,representation:r.representation,reference_sha256:r.sha256,passed:true,stats:{...stats,samples:r.representation==='sensor-u16le-masked'?1:3}})),resources:{measurement:'Synthetic process lifetime HWM, not decoder peak',samples:[{stage:'before',admitted_bytes:0},{stage:'released',admitted_bytes:0}]}};
  const result=record('result',receipt),outer=record('outer',receipt);
  const summary={command:'run',mode:'required',selected:1,checked:1,passed:1,failed:0,skipped:0,success:true,qualification_executed:true,manifest_sha256:receipt.manifest_sha256};
  const summaryPin=record('summary',summary);
  const packet={schema_version:1,producer_revision:revision,source_tree:tree,source_pins:sourcePins.map(({blob,...p})=>p),test_executable:binary,runner:receipt.runner,compiler_receipt:{sha256:workflow.sha256},case_receipt:{id,sha256:result.sha256,bytes:result.bytes,checked:true,status:'passed'},required_reference_kinds:entry.required_reference_kinds,reference_tools:tools,reference_contracts:references.map(({sensor_fnv1a64,...r})=>r)};
  const stable=[sourceFile,rgb,sensor].map(file=>({path:file,...fingerprint(file),expected_sha256:fingerprint(file).sha256,matches:true}));
  const executables=Object.fromEntries(references.map(r=>[r.id,path.join(dir,`${r.id}-executable-does-not-exist`)]));
  stable.push(...references.map(r=>({path:executables[r.id],sha256:r.toolchain.executable_sha256,expected_sha256:r.toolchain.executable_sha256,matches:true})));
  const coordinator={m50:{revision,result,original_compiler_receipt:original,workflow_compiler_receipt:workflow,summary:{value:summary},stable_files:stable}};
  const producer={case:id,case_count:80,host_sha256:host,host_contract:{source:'qualification.rs',algorithm:'sha256(trimmed-hostname)',function_sha256:hash(hostFunction)},inspector_imports:[],packet:record('packet',packet),coordinator:record('coordinator',coordinator),original_compiler:original,workflow_compiler:workflow,receipt:result,outer,summary:summaryPin,log:record('log',`test formats::raw::qualification::raw_camera_corpus ... RAW_QUALIFICATION ${JSON.stringify(receipt)}\ntest result: ok. 1 passed; 0 failed; 0 ignored;\nCargo receipt: ${path.join(dir,workflow.path)}\n`),qualification_boundary:'Synthetic only; never product proof'};
  const cohort={id:'raw-camera',type:'photo03',required:true,source_revision:revision,source_tree:tree,source_pins:sourcePins,manifest:pinSource('manifest.json'),inspector:pinSource('inspector.mjs'),advertised:pinSource('raw.rs'),producer};
  const bindings={cases:{[id]:{source:sourceFile,references:{'libraw-integration':rgb,'independent-sensor':sensor},executables}}};
  const edit=(field,mutate)=>{const value=JSON.parse(fs.readFileSync(path.join(dir,producer[field].path)));mutate(value);producer[field]=record(field,value)};
  return {cohort,roots:{'raw-camera':dir,records:dir},bindings,edit,sourceFile,sensor,rgb,exe};
}

test('bounded data stream checks SHA/size/stat and byte FNV against an independent known vector',()=>{
  const file=path.join(scratch,'fnv.txt');fs.writeFileSync(file,'hello');
  const expected={sha256:hash('hello'),bytes:5};
  assert.equal(cameraData(file,expected,'a430d84680aabd0b').fnv1a64,'a430d84680aabd0b');
  assert.throws(()=>cameraData(file,expected,'a430d84680aabd0c'),/hash\/FNV\/stat/);
  fs.writeFileSync(file,'jello');assert.throws(()=>cameraData(file,expected),/hash\/FNV\/stat/);
  assert.throws(()=>cameraData(file,{...expected,bytes:6}),/size\/type/);
});

test('one camera/mode joins original executable records to actual data while79 remain unqualified',async()=>{
  const f=fixture('positive'),r=await frozenCameraEvidence(f.cohort,f);
  assert.equal(r.integrity_passed,true,JSON.stringify(r.errors));
  assert.deepEqual(r.counts,{required:80,qualified:1,unqualified:79});
  assert.equal(r.passed,false);assert.equal(r.public_candidate,false);
  assert.equal(r.rows.filter(r=>r.status==='passed').length,1);
  assert.equal(r.rows.find(r=>r.status==='passed').checks.length,7);
  assert.equal(r.rows.find(r=>r.status==='passed').references[1].fnv1a64,'07ee9e07b4b1c883');
  assert.equal(fs.existsSync(f.exe),false);
  assert.equal(Object.values(f.bindings.cases['cr3-canon-m50-raw'].executables).some(p=>fs.existsSync(p)),false);
  assert.equal(JSON.stringify(r).includes(f.roots.records),false);
});

test('missing/corrupt input or reference never passes and cannot be replaced by producer observations',async()=>{
  for(const [name,mutate] of [['missing',f=>fs.rmSync(f.rgb)],['corrupt',f=>fs.writeFileSync(f.sensor,Buffer.from([0x35,0x12]))],['source',f=>fs.writeFileSync(f.sourceFile,'different source bytes!!')],['binding',f=>delete f.bindings.cases['cr3-canon-m50-raw'].references['independent-sensor']]]) {
    const f=fixture(name);mutate(f);const r=await frozenCameraEvidence(f.cohort,f);
    assert.equal(r.integrity_passed,false,name);assert.equal(r.counts.qualified,0);assert.equal(r.rows.length,80);assert.equal(r.rows.some(r=>r.status==='passed'),false);
  }
});

test('wrong producer, compiler, source/blob pin and host fail closed despite plausible passed packets',async()=>{
  const mutations=[
    f=>f.edit('packet',p=>{p.producer_revision='f'.repeat(40)}),
    f=>f.edit('packet',p=>{p.runner.host_sha256=hash('wrong producer host')}),
    f=>{f.cohort.producer.host_sha256=hash('different expected host')},
    f=>{f.cohort.source_pins.find(p=>p.path==='qualification.rs').sha256=hash('different code')},
    f=>{f.cohort.source_pins.find(p=>p.path==='inspector.mjs').blob='f'.repeat(40)},
    f=>f.edit('workflow_compiler',p=>{p.dirty_after=true}),
    f=>f.edit('workflow_compiler',p=>{p.executables[0].sha256=hash('other binary')}),
    f=>f.edit('receipt',p=>{p.runner.host_sha256=hash('wrong native host')}),
    f=>{fs.appendFileSync(path.join(f.roots.records,f.cohort.producer.packet.path),' ')},
  ];
  for(const [i,mutate] of mutations.entries()) {const f=fixture(`identity-${i}`);mutate(f);const r=await frozenCameraEvidence(f.cohort,f);assert.equal(r.integrity_passed,false,`identity ${i}`);assert.equal(r.counts.qualified,0);assert.equal(r.public_candidate,false);}
});

test('public pins retain870 history and reject missing or unsafe frozen producer bindings',()=>{
  const index=JSON.parse(fs.readFileSync(path.join(root,'qualification/suite/index.json')));
  const history=JSON.parse(fs.readFileSync(path.join(root,'qualification/suite/history.json')));
  assert.equal(validate(index),index);
  assert.equal(index.cohorts[0].producer.case_count,80);
  assert.equal(index.cohorts[0].producer.case,'cr3-canon-m50-raw');
  assert.equal(history.observations.find(o=>o.id==='raw-camera-870-read-only').producer_contract.source_revision,'870ef7a3e63b4d1160a1602d96281368adb6c12d');
  for(const mutate of [c=>{c.source_pins.pop()},c=>{c.source_pins[0].blob='bad'},c=>{c.producer.host_sha256=null},c=>{c.producer.receipt.path='../outside.json'}]) {
    const value=structuredClone(index);mutate(value.cohorts[0]);assert.throws(()=>validate(value),/frozen camera producer/);
  }
});
