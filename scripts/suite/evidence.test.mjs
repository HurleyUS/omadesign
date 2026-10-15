import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { fingerprint, source } from './contract.mjs';
import { ownerEvidence } from './owner-evidence.mjs';
import { buildBinding } from './build.mjs';
import { hash } from './contract.mjs';

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

test('owner proofs reject altered artifacts, old source and absent acceptance checks',()=>{
  const current=source(root);
  const binary=fingerprint(process.execPath);
  const artifact=path.join(scratch,'proof.txt');
  fs.writeFileSync(artifact,'explicit synthetic validator proof, not a product receipt');
  const packet={version:1,id:'UI-03',product:'omadesign',...current,binary_sha256:binary.sha256,host_sha256:'5'.repeat(64),status:'passed',checks:[{id:'UI-03',status:'passed',artifact:{path:artifact,...fingerprint(artifact)}}]};
  const file=path.join(scratch,'UI-03.omadesign.json');
  const requirements=[{id:'UI-03',product:'omadesign',checks:['UI-03']}];
  const local={directory:scratch,roots:{omadesign:root},builds:{omadesign:{...current,binary:process.execPath,...binary}}};
  const compile={schema:1,repository:'omadesign',revision:current.revision,source_tree:current.tree,dirty_before:false,dirty_after:false,exit_code:0,signal:null,receipt_errors:[],executables:[{path:process.execPath,sha256:binary.sha256,size:binary.bytes}]};
  const compileBytes=JSON.stringify(compile)+'\n';
  const compileFile=path.join(scratch,`${hash(compileBytes)}.json`);
  fs.writeFileSync(compileFile,compileBytes);
  local.builds.omadesign.compiler_receipt={path:compileFile,sha256:hash(compileBytes)};
  const check=p=>{fs.writeFileSync(file,JSON.stringify(p));return ownerEvidence(requirements,local)[0].status};
  assert.equal(check(packet),'passed');
  assert.equal(check({...packet,revision:'0'.repeat(40)}),'unqualified');
  assert.equal(check({...packet,checks:[]}),'unqualified');
  fs.writeFileSync(artifact,'changed proof');
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
  const run=p=>{fs.writeFileSync(receipt,JSON.stringify(p));const result=spawnSync(process.execPath,[path.join(root,'scripts/suite/ui-evidence.mjs'),requestFile],{encoding:'utf8'});assert.notEqual(result.status,0);return JSON.parse(fs.readFileSync(path.join(scratch,'result.json'))).checks};
  assert.equal(run({...packet,binary_sha256:'4'.repeat(64)}).editability.status,'unavailable');
  const valid=run(packet);
  assert.equal(valid.editability.status,'passed');
  assert.equal(valid.appearance.status,'unavailable');
  fs.rmSync(artifact);
  assert.equal(run(packet).editability.status,'unavailable');
});
