#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { fingerprint, source } from './contract.mjs';

const [configPath, product, revision, output, ...cargo] = process.argv.slice(2);
if (!output?.startsWith('/home/') || !/^[a-f0-9]{40}$/.test(revision ?? '') || cargo[0] !== 'test' || !cargo.includes('--locked') || !cargo.includes('--no-run') || !cargo.includes('--message-format=json')) throw Error('Use build.mjs CONFIG PRODUCT FULL_SHA /home/.../BUILD.json test --locked [narrow target] --no-run --message-format=json');
if (product === 'omadesign' && !cargo.includes('--lib')) throw Error('Omadesign qualification builds must use --lib');
if (product === 'omatainer' && (cargo.includes('--lib') || !cargo.includes('--bin'))) throw Error('Omatainer qualification builds must select its binary');
if (product === 'editbay' && !cargo.includes('-p')) throw Error('EditBay qualification builds must select an exact package');
const config = JSON.parse(fs.readFileSync(configPath,'utf8'));
fs.mkdirSync(path.dirname(output),{recursive:true});
const log = fs.openSync(`${output}.build.log`,'wx');
const child = spawn(process.execPath,[fileURLToPath(new URL('../serialized-cargo.mjs',import.meta.url)),configPath,product,revision,...cargo],{stdio:['ignore','pipe',log]});
const artifacts = [];
let pending = '';
child.stdout.on('data', chunk => {
  pending += chunk;
  if (pending.length>16*1024*1024) {child.kill('SIGTERM');throw Error('Cargo JSON output limit exceeded');}
  const lines = pending.split('\n');pending=lines.pop();
  for(const line of lines) {try {const j=JSON.parse(line);if(j.reason==='compiler-artifact' && j.profile?.test && j.executable) artifacts.push(j.executable);}catch {}}
});
const code = await new Promise((resolve,reject)=>{child.on('error',reject);child.on('close',resolve)});
fs.closeSync(log);
if (code !== 0 || artifacts.length !== 1) throw Error('Exact serialized build failed or did not identify one test executable');
const actual = source(config.repositories[product].worktree);
if (actual.revision !== revision || actual.dirty) throw Error('Canonical build source changed; build binding unavailable');
const binary = artifacts[0];
const record = {revision,tree:actual.tree,binary,sha256:fingerprint(binary).sha256,runtime_root:config.repositories[product].worktree,driver:'libtest',tools:{},build:{route:'scripts/serialized-cargo.mjs',args:cargo,log_sha256:fingerprint(`${output}.build.log`).sha256}};
fs.writeFileSync(output,JSON.stringify(record,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({product,revision,binary_sha256:record.sha256}));
