import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import os from 'node:os';
import { asset, canonical, contained, fingerprint, hash, source } from './contract.mjs';
import { verifyCompilerReceipt } from './build.mjs';

/** Check observed camera comparisons and workflow math against the pinned producer contract.
 * Args: manifest case, independently inspected references and native packet.
 * Returns false for omitted comparisons, changed tools, zero samples or incomplete workflow evidence.
 */
export function cameraChecks(entry, inspected, receipt) {
  const within=(s,t)=>Number.isSafeInteger(s?.samples) && s.samples>0 && Number.isFinite(s.mean) && Number.isFinite(s.max) && s.mean>=0 && s.mean<=s.max && s.mean<=t.mean && s.max<=t.max && Array.isArray(s.channel_mean) && Array.isArray(s.channel_max) && s.channel_mean.length===3 && s.channel_max.length===3 && s.channel_mean.every((n,i)=>Number.isFinite(n) && Number.isFinite(s.channel_max[i]) && n>=0 && n<=s.channel_max[i] && s.channel_max[i]<=t.max);
  const comparisons=receipt?.comparisons;
  if (!Array.isArray(comparisons) || !comparisons.length) return false;
  const seen=new Set();
  for(const c of comparisons) {
    const ref=entry.references.find(r=>r.id===c.reference);
    const actual=inspected.references.find(r=>r.id===c.reference);
    const tool=receipt.reference_tools?.[c.reference];
    if (!ref || !actual || seen.has(c.reference) || c.kind!==ref.kind || c.representation!==ref.representation || c.reference_sha256!==actual.reference_sha256 || actual.reference_sha256!==ref.sha256 || actual.executable_sha256!==ref.toolchain.executable_sha256 || c.passed!==true || !within(c.stats,ref.tolerance)) return false;
    const expectedTool={name:ref.toolchain.name,version:ref.toolchain.version,executable_sha256:actual.executable_sha256,platform:ref.toolchain.platform,command:ref.toolchain.command};
    if (canonical(tool)!==canonical(expectedTool)) return false;
    const channels=ref.representation==='sensor-u16le-masked'?1:3;
    if (!ref.dimensions || c.stats.samples!==ref.dimensions[0]*ref.dimensions[1]*channels) return false;
    seen.add(c.reference);
  }
  if (!entry.required_reference_kinds.every(kind=>entry.references.some(r=>r.kind===kind) && entry.references.filter(r=>r.kind===kind).every(r=>seen.has(r.id)))) return false;
  const checks=receipt.checks;
  if (!Array.isArray(checks)) return false;
  const transfer={mean:3/65535,max:3/65535};
  const digest=/^[a-f0-9]{64}$/;
  const exports=['png_u16_codec_decode','tif_u16_codec_decode'].map(name=>checks.find(c=>c.check===name));
  if (!exports.every(c=>c?.channel_bits===16 && digest.test(c.pixel_sha256) && within(c.stats,transfer)) || exports[0].pixel_sha256!==exports[1].pixel_sha256) return false;
  const edited=checks.find(c=>c.check==='settings_exposure_crop_rotation_reopen');
  if (!edited || !digest.test(edited.pixel_sha256) || edited.owned_source_sha256!==receipt.source_before_sha256 || !within(edited.stats,transfer)) return false;
  return ['normalized_preview','normalized_full'].every(name=>{
    const renders=checks.filter(c=>c.check===name);
    return renders.length===2 && renders.every(c=>within(c.stats,entry.preview_tolerance));
  });
}

/** Verify PHOTO-03's pinned per-case receipts without decoding or acquiring anything.
 * Args: pinned producer contract and private local roots/builds/bindings/receipt directory.
 * Returns one verdict per required camera case, with only public IDs and observed hashes.
 */
export async function rawEvidence(cohort, { roots, builds, bindings = {}, receipts, corpus }) {
  try {
    for (const pin of [cohort.manifest,cohort.inspector,cohort.advertised]) asset(pin,roots);
    const root = roots[cohort.manifest.root];
    const current = source(root);
    if (current.dirty) throw Error('producer source dirty');
    const inspector = await import(pathToFileURL(contained(root,cohort.inspector.path)));
    const manifest = JSON.parse(fs.readFileSync(contained(root,cohort.manifest.path),'utf8'));
    inspector.validateManifest(manifest,inspector.extensions(fs.readFileSync(contained(root,cohort.advertised.path),'utf8')));
    const required = manifest.cases.filter(c=>c.required);
    if (!required.length) throw Error('zero required camera cases');
    const build = builds[cohort.id];
    const binary = build?.binary ? fingerprint(build.binary) : null;
    let currentBuild=false;
    try {verifyCompilerReceipt(build);currentBuild=binary && binary.sha256===build.sha256 && build.revision===current.revision && build.tree===current.tree;}catch {}
    const rows = [];
    for (const entry of required) {
      const inspected = await inspector.inspectCase(entry,bindings.cases?.[entry.id] ?? {},corpus ?? path.join(root,'.artifacts/unavailable-camera-corpus'));
      let receipt = null, receiptHash = null;
      if (receipts) {
        try {const file = contained(receipts,`${entry.id}.json`);receiptHash=fingerprint(file).sha256;receipt=JSON.parse(fs.readFileSync(file,'utf8'));} catch {}
      }
      let passed = currentBuild && !inspected.errors.length && receipt?.schema_version===1 && receipt.case===entry.id && receipt.required===true && receipt.status==='passed' && receipt.checked===true && receipt.errors?.length===0 && receipt.manifest_sha256===cohort.manifest.sha256 && receipt.runner?.sha256===binary.sha256 && receipt.runner?.revision===current.revision && /^[a-f0-9]{64}$/.test(receipt.runner?.host_sha256) && receipt.runner?.cargo_lock_sha256===fingerprint(path.join(root,'Cargo.lock')).sha256 && receipt.source_before_sha256===inspected.source_before_sha256 && receipt.source_after_sha256===inspected.source_before_sha256 && cameraChecks(entry,inspected,receipt);
      if (passed) {
        const after=await inspector.inspectCase(entry,bindings.cases?.[entry.id] ?? {},corpus ?? path.join(root,'.artifacts/unavailable-camera-corpus'));
        passed=!after.errors.length && after.source_before_sha256===inspected.source_before_sha256 && canonical(after.references)===canonical(inspected.references);
      }
      rows.push({fixture:`${cohort.id}.${entry.id}`,tier:'headless',status:passed?'passed':'unqualified',reason:passed?'required current workflow and independent comparisons passed':inspected.errors.length?'required camera input/reference/executable unavailable or invalid':!currentBuild?'current source-bound camera test executable unavailable':'required current camera receipt/comparison/workflow unavailable or invalid',source_sha256:inspected.source_before_sha256,reference_hashes:inspected.references.map(r=>({id:r.id,reference:r.reference_sha256,executable:r.executable_sha256})),receipt_sha256:receiptHash,producer_host_sha256:receipt?.runner?.host_sha256 ?? null});
    }
    if (canonical(source(root))!==canonical(current) || (binary && fingerprint(build.binary).sha256!==binary.sha256)) throw Error('producer changed during verification');
    return {passed:rows.every(r=>r.status==='passed'),rows,source:current,binary,manifest_sha256:cohort.manifest.sha256,verification_host:{id:hash(os.hostname()),os:os.platform(),arch:os.arch()}};
  } catch { return {passed:false,rows:[{fixture:cohort.id,tier:'headless',status:'unqualified',reason:'pinned camera manifest/inspector/source unavailable or invalid'}]}; }
}
