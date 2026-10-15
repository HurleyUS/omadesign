#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { fingerprint, source } from './contract.mjs';

const [binary, out] = process.argv.slice(2);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
if (!out?.startsWith('/home/') || !binary?.startsWith('/home/')) throw Error('Explicit project-local executable and receipts directory required');
fs.mkdirSync(out,{recursive:true});
const current = source(root);
if (current.dirty) throw Error('Source must be committed and clean');
const tools = {};
for (const id of ['rsvg-convert','pdftoppm']) {
  const filename = execFileSync('which',[id],{encoding:'utf8'}).trim();
  tools[id] = {path:filename,...fingerprint(filename),version_args:id==='pdftoppm'?['-v']:['--version']};
}
const builds = path.join(out,'builds.json');
fs.writeFileSync(builds,JSON.stringify({omadesign:{...current,binary,...fingerprint(binary),runtime_root:root,driver:'libtest',tools,origin:'current_exe of serialized Cargo test; library/test evidence only'}}));
const result = spawnSync(process.execPath,[path.join(root,'scripts/qualify-suite.mjs'),'run','--lane','baseline','--product','omadesign','--builds',builds,'--out',out,'--inside','locked'],{stdio:'inherit',env:process.env});
process.exitCode=result.status ?? 1;
