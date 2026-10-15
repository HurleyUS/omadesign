import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
import { fingerprint, source } from './contract.mjs';
import { ownerEvidence } from './owner-evidence.mjs';
import { buildBinding } from './build.mjs';
import { hash } from './contract.mjs';
import { cameraChecks } from './raw-evidence.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
fs.mkdirSync(path.join(root,'.artifacts'),{recursive:true});
const scratch=fs.mkdtempSync(path.join(root,'.artifacts/suite-evidence-'));
test.after(()=>fs.rmSync(scratch,{recursive:true,force:true}));

test('locked compiler artifacts bind the original bytes, never a later reused target or a zero-artifact run',()=>{
  const record={schema:1,repository:'omadesign',revision:'1'.repeat(40),source_tree:'2'.repeat(40),dirty_before:false,dirty_after:false,exit_code:0,signal:null,receipt_errors:[],executables:[{path:path.join(scratch,'already-overwritten-or-missing'),sha256:'3'.repeat(64),size:123,profile:{test:true}}]};
  const config={repositories:{omadesign:{worktree:root}}};
  const save=r=>{const bytes=JSON.stringify(r)+'\n';const file=path.join(scratch,`${hash(bytes)}.json`);fs.writeFileSync(file,bytes);return file};
  const binding=buildBinding(config,save(record));
  assert.equal(binding.sha256,record.executables[0].sha256);
  assert.equal(binding.test_execution_proven,false);
  assert.throws(()=>buildBinding(config,save({...record,executables:[]})),/exactly one/);
  assert.throws(()=>buildBinding(config,save({...record,dirty_after:true})),/Clean successful/);
  const corrupt=save(record);fs.appendFileSync(corrupt,' ');
  assert.throws(()=>buildBinding(config,corrupt),/integrity/);
});

test('camera packets require every pinned comparison/tool and actual sixteen-bit workflow math',()=>{
  const stats={samples:6,mean:0,max:0,channel_mean:[0,0,0],channel_max:[0,0,0]};
  const references=['integration','independent-a','independent-b'].map((id,i)=>({id,kind:i?'independent_render':'libraw_integration',representation:'oriented-linear-srgb-rgb16le',sha256:hash(id),dimensions:[2,1],tolerance:{mean:0,max:0},toolchain:{name:id,version:'1',executable_sha256:hash(`tool-${id}`),platform:'synthetic-node-test',command:['synthetic-test-only']}}));
  const entry={references,required_reference_kinds:['libraw_integration','independent_render'],preview_tolerance:{mean:0,max:0}};
  const inspected={references:references.map(r=>({id:r.id,reference_sha256:r.sha256,executable_sha256:r.toolchain.executable_sha256}))};
  const receipt={source_before_sha256:hash('synthetic-source'),reference_tools:Object.fromEntries(references.map(r=>[r.id,r.toolchain])),comparisons:references.map(r=>({reference:r.id,kind:r.kind,representation:r.representation,reference_sha256:r.sha256,passed:true,stats})),checks:[
    ...['png_u16_codec_decode','tif_u16_codec_decode'].map(check=>({check,channel_bits:16,pixel_sha256:hash('synthetic-baseline'),stats})),
    {check:'settings_exposure_crop_rotation_reopen',pixel_sha256:hash('synthetic-edit'),owned_source_sha256:hash('synthetic-source'),stats},
    ...['normalized_preview','normalized_full'].flatMap(check=>[{check,stats},{check,stats}]),
  ]};
  assert.equal(cameraChecks(entry,inspected,receipt),true);
  const mutations=[
    p=>{p.comparisons.pop()},
    p=>{p.comparisons.push(p.comparisons[0])},
    p=>{p.comparisons[0].stats.samples=0},
    p=>{p.comparisons[0].stats.samples=1},
    p=>{p.comparisons[0].stats.max=0.1},
    p=>{p.comparisons[0].reference_sha256=hash('changed-reference')},
    p=>{p.reference_tools.integration.executable_sha256=hash('changed-tool')},
    p=>{p.reference_tools.integration.command=['changed-recipe']},
    p=>{p.checks[0].channel_bits=8},
    p=>{p.checks[1].check='tiff_u16_codec_decode'},
    p=>{p.checks[2].owned_source_sha256=hash('changed-source')},
    p=>{p.checks.pop()},
  ];
  for(const mutate of mutations) {
    const packet=structuredClone(receipt);
    mutate(packet);
    assert.equal(cameraChecks(entry,inspected,packet),false);
  }
});

test('owner proofs reject altered artifacts, old source and absent acceptance checks',()=>{
  const checkout=path.join(scratch,'source');
  fs.mkdirSync(checkout);
  const git=(...args)=>execFileSync('git',['-C',checkout,...args],{stdio:'pipe'});
  git('init','--quiet');
  fs.writeFileSync(path.join(checkout,'source.txt'),'synthetic owner source');
  git('add','source.txt');
  git('-c','user.name=Suite Test','-c','user.email=suite-test@invalid','-c','core.hooksPath=/dev/null','commit','--quiet','-m','Synthetic owner source');
  const current=source(checkout);
  const binary=fingerprint(process.execPath);
  const artifact=path.join(scratch,'proof.txt');
  fs.writeFileSync(artifact,'explicit synthetic validator proof, not a product receipt');
  const packet={version:1,id:'UI-03',product:'omadesign',...current,binary_sha256:binary.sha256,host_sha256:'5'.repeat(64),status:'passed',checks:[{id:'UI-03',status:'passed',artifact:{path:artifact,...fingerprint(artifact)}}]};
  const file=path.join(scratch,'UI-03.omadesign.json');
  const requirements=[{id:'UI-03',product:'omadesign',checks:['UI-03']}];
  const local={directory:scratch,roots:{omadesign:checkout},builds:{omadesign:{...current,binary:process.execPath,...binary}}};
  const compile={schema:1,repository:'omadesign',revision:current.revision,source_tree:current.tree,dirty_before:false,dirty_after:false,exit_code:0,signal:null,receipt_errors:[],executables:[{path:process.execPath,sha256:binary.sha256,size:binary.bytes}]};
  const compileBytes=JSON.stringify(compile)+'\n';
  const compileFile=path.join(scratch,`${hash(compileBytes)}.json`);
  fs.writeFileSync(compileFile,compileBytes);
  local.builds.omadesign.compiler_receipt={path:compileFile,sha256:hash(compileBytes)};
  const check=p=>{fs.writeFileSync(file,JSON.stringify(p));return ownerEvidence(requirements,local)[0].status};
  assert.equal(check(packet),'passed');
  assert.equal(check({...packet,revision:'0'.repeat(40)}),'unqualified');
  assert.equal(check({...packet,checks:[]}),'unqualified');
  fs.writeFileSync(path.join(checkout,'untracked'),'dirty source');
  assert.equal(check(packet),'unqualified');
  fs.rmSync(path.join(checkout,'untracked'));
  fs.writeFileSync(artifact,'changed proof');
  assert.equal(check(packet),'unqualified');
  fs.writeFileSync(artifact,'explicit synthetic validator proof, not a product receipt');
  fs.writeFileSync(path.join(checkout,'source.txt'),'new source revision');
  git('add','source.txt');
  git('-c','user.name=Suite Test','-c','user.email=suite-test@invalid','-c','core.hooksPath=/dev/null','commit','--quiet','-m','New source');
  assert.equal(check(packet),'unqualified');
});

test('UI semantic proof cannot satisfy appearance and wrong executable or missing artifact never passes',()=>{
  const artifact=path.join(scratch,'ui-proof.txt');
  fs.writeFileSync(artifact,'synthetic semantic validator proof only');
  const receipt=path.join(scratch,'editability.json');
  const packet={check:'editability',status:'passed',revision:'1'.repeat(40),binary_sha256:'2'.repeat(64),fixture_sha256:'3'.repeat(64),host_sha256:'5'.repeat(64),artifact:{path:artifact,...fingerprint(artifact)}};
  const request={fixture:{required:['editability','appearance'],ui:{appearance_references:[]}},verified_binding:{source:{revision:packet.revision},binary:{sha256:packet.binary_sha256},inputs:{input:{sha256:packet.fixture_sha256}}},ui_receipts:{editability:receipt},output:scratch};
  const requestFile=path.join(scratch,'ui-request.json');
  fs.writeFileSync(requestFile,JSON.stringify(request));
  const run=(p,status=1)=>{fs.writeFileSync(requestFile,JSON.stringify(request));fs.writeFileSync(receipt,JSON.stringify(p));const result=spawnSync(process.execPath,[path.join(root,'scripts/suite/ui-evidence.mjs'),requestFile],{encoding:'utf8'});assert.equal(result.status,status);return JSON.parse(fs.readFileSync(path.join(scratch,'result.json'))).checks};
  assert.equal(run({...packet,binary_sha256:'4'.repeat(64)}).editability.status,'unavailable');
  const valid=run(packet);
  assert.equal(valid.editability.status,'passed');
  assert.equal(valid.appearance.status,'unavailable');
  const capture=path.join(scratch,'synthetic-capture.proof');
  fs.writeFileSync(capture,'synthetic pinned appearance bytes; no rendered app proof');
  request.fixture.ui={font_sha256:hash('synthetic-font'),theme_sha256:hash('synthetic-theme'),appearance_references:[1,1.5,2].map(scale=>({scale,sha256:fingerprint(capture).sha256}))};
  request.ui_captures=Object.fromEntries([1,1.5,2].map(scale=>[String(scale),{path:capture,revision:packet.revision,binary_sha256:packet.binary_sha256,fixture_sha256:packet.fixture_sha256,host_sha256:packet.host_sha256,font_sha256:request.fixture.ui.font_sha256,theme_sha256:request.fixture.ui.theme_sha256}]));
  assert.equal(run(packet,0).appearance.status,'passed');
  request.ui_captures['1'].revision='4'.repeat(40);
  assert.equal(run(packet).appearance.status,'unavailable');
  fs.rmSync(artifact);
  assert.equal(run(packet).editability.status,'unavailable');
});
