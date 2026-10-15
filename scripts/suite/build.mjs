#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { hash } from './contract.mjs';

/** Convert a locked compiler receipt into a local suite binding without hashing a reused executable later.
 * Args: validated build config, immutable receipt path and optional exact artifact digest.
 * Returns a build binding only; compilation or --no-run never establishes fixture execution.
 */
export function buildBinding(config, receiptPath, artifactSha) {
  const bytes=fs.readFileSync(receiptPath);
  const digest=hash(bytes);
  if (path.basename(receiptPath)!==`${digest}.json`) throw Error('Compiler receipt integrity mismatch');
  const r=JSON.parse(bytes);
  if (r.schema!==1 || !config.repositories[r.repository] || !/^[a-f0-9]{40}$/.test(r.revision) || !/^[a-f0-9]{40}$/.test(r.source_tree) || r.dirty_before!==false || r.dirty_after!==false || r.exit_code!==0 || r.signal || !Array.isArray(r.receipt_errors) || r.receipt_errors.length) throw Error('Clean successful locked compiler receipt required');
  const artifacts=(r.executables ?? []).filter(a=>!artifactSha || a.sha256===artifactSha);
  if (artifacts.length!==1) throw Error('Select exactly one compiler-emitted executable');
  const a=artifacts[0];
  if (!a.path?.startsWith('/home/') || !/^[a-f0-9]{64}$/.test(a.sha256) || !Number.isSafeInteger(a.size) || a.size<=0) throw Error('Invalid compiler artifact binding');
  return {revision:r.revision,tree:r.source_tree,binary:a.path,sha256:a.sha256,runtime_root:config.repositories[r.repository].worktree,driver:a.profile?.test?'libtest':'cli',tools:{},compiler_receipt:{path:receiptPath,sha256:digest,helper_sha256:r.helper_sha256,receipt_helper_sha256:r.receipt_helper_sha256},test_execution_proven:false};
}

/** Check the locked compiler record behind a local binding.
 * Args: build binding. Returns its immutable receipt or throws; never infers old bytes from today's executable.
 */
export function verifyCompilerReceipt(build) {
  const file=build.compiler_receipt?.path;
  if (!file) throw Error('required locked compiler receipt unavailable');
  const bytes=fs.readFileSync(file);
  const digest=hash(bytes);
  if (digest!==build.compiler_receipt.sha256 || path.basename(file)!==`${digest}.json`) throw Error('required compiler receipt integrity mismatch');
  const r=JSON.parse(bytes);
  if (r.schema!==1 || r.revision!==build.revision || r.source_tree!==build.tree || r.dirty_before!==false || r.dirty_after!==false || r.exit_code!==0 || r.signal || r.receipt_errors?.length!==0 || !r.executables?.some(a=>a.path===build.binary && a.sha256===build.sha256 && a.size>0)) throw Error('required compiler executable binding unavailable');
  return r;
}

if (process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const [configPath,receiptPath,output,artifactSha]=process.argv.slice(2);
  if (!output?.startsWith('/home/')) throw Error('Use build.mjs CONFIG LOCKED_RECEIPT /home/.../BUILD.json [EXACT_ARTIFACT_SHA]');
  const configBytes=fs.readFileSync(configPath);
  const config=JSON.parse(configBytes);
  const receipt=JSON.parse(fs.readFileSync(receiptPath));
  if (hash(configBytes)!==receipt.config_sha256) throw Error('Compiler receipt configuration mismatch');
  const record=buildBinding(config,receiptPath,artifactSha);
  fs.mkdirSync(path.dirname(output),{recursive:true});
  fs.writeFileSync(output,JSON.stringify(record,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({revision:record.revision,binary_sha256:record.sha256,test_execution_proven:false}));
}
