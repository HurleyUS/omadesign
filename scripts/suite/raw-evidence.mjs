import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import os from 'node:os';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
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
export async function rawEvidence(cohort, { roots, builds, bindings = {}, receipts, corpus, producerBinding }) {
  if (cohort.producer) return frozenCameraEvidence(cohort,{roots,bindings,producerBinding});
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

const requireCamera = (condition, reason) => { if (!condition) throw Error(reason); };
const statIdentity = s => [s.dev,s.ino,s.size,s.mtimeNs,s.ctimeNs].map(String).join(':');

/** Stream pinned fixture data without decoding, copying or opening executables.
 * Args: authorized data path, exact SHA/length and optional byte FNV. Returns hashes and stable file identity.
 */
export function cameraData(file, expected, fnvExpected = null) {
  requireCamera(typeof file === 'string' && path.isAbsolute(file) && /^[a-f0-9]{64}$/.test(expected.sha256) && Number.isSafeInteger(expected.bytes) && expected.bytes > 0 && expected.bytes <= 1073741824, 'camera data pin invalid');
  const fd = fs.openSync(file,fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  const digest = crypto.createHash('sha256'), buffer = Buffer.alloc(512*1024);
  let bytes = 0, hi = 0xcbf29ce4, lo = 0x84222325, before, after;
  try {
    before = fs.fstatSync(fd,{bigint:true});
    requireCamera(before.isFile() && before.size === BigInt(expected.bytes), 'camera data size/type changed');
    for (;;) {
      const n = fs.readSync(fd,buffer,0,Math.min(buffer.length,expected.bytes-bytes+1),null);
      if (!n) break;
      bytes += n;
      requireCamera(bytes <= expected.bytes, 'camera data grew while reading');
      digest.update(buffer.subarray(0,n));
      if (fnvExpected) for (let i=0;i<n;i++) {
        const low = (lo ^ buffer[i]) >>> 0;
        const product = low*435;
        hi = (Math.imul(hi,435) + Math.imul(low,256) + Math.floor(product/4294967296)) >>> 0;
        lo = product >>> 0;
      }
    }
    after = fs.fstatSync(fd,{bigint:true});
  } finally { fs.closeSync(fd); }
  const identity = statIdentity(before), sha256 = digest.digest('hex');
  const fnv1a64 = fnvExpected ? hi.toString(16).padStart(8,'0')+lo.toString(16).padStart(8,'0') : null;
  requireCamera(identity === statIdentity(after) && identity === statIdentity(fs.statSync(file,{bigint:true})) && bytes === expected.bytes && sha256 === expected.sha256 && (!fnvExpected || fnv1a64 === fnvExpected), 'camera data hash/FNV/stat changed');
  return {sha256,bytes,fnv1a64,identity};
}

/** Join the one reviewed M50 producer to current input/reference bytes and original executable records.
 * Args: pinned cohort and private roots/bindings. Returns all required rows; never opens a product or tool executable.
 */
export async function frozenCameraEvidence(cohort,{roots,bindings = {},producerBinding}) {
  let rows = [], verified = null;
  const producer = cohort.producer;
  const git = (...args) => execFileSync('git',['-C',roots['raw-camera'],...args],{maxBuffer:1048576,timeout:10000,stdio:['ignore','pipe','pipe']});
  const metadata = pin => {
    requireCamera(pin && pin.bytes > 0 && pin.bytes <= 1048576, 'camera metadata pin invalid');
    const stat = fs.statSync(contained(roots[pin.root],pin.path));
    requireCamera(stat.isFile() && stat.size === pin.bytes, 'camera metadata byte count/type changed');
    asset(pin,roots);
    return JSON.parse(fs.readFileSync(contained(roots[pin.root],pin.path),'utf8'));
  };
  try {
    requireCamera(git('rev-parse',`${cohort.source_revision}^{tree}`).toString().trim() === cohort.source_tree, 'camera producer Git tree changed/unavailable');
    const sources = new Map();
    for (const pin of cohort.source_pins) {
      requireCamera(!path.isAbsolute(pin.path) && !pin.path.split('/').includes('..') && /^[a-f0-9]{40}$/.test(pin.blob), 'camera source pin invalid');
      requireCamera(git('rev-parse',`${cohort.source_revision}:${pin.path}`).toString().trim() === pin.blob, 'camera producer blob changed');
      const bytes = git('cat-file','blob',pin.blob);
      requireCamera(bytes.length === pin.bytes && hash(bytes) === pin.sha256, 'camera producer source hash/size changed');
      sources.set(pin.path,bytes);
    }
    for (const pin of [cohort.manifest,cohort.inspector,cohort.advertised]) requireCamera(hash(sources.get(pin.path)) === pin.sha256 && sources.get(pin.path).length === pin.bytes, 'camera producer manifest/inspector pin mismatch');
    const manifest = JSON.parse(sources.get(cohort.manifest.path));
    const required = manifest.cases.filter(c=>c.required);
    requireCamera(required.length === producer.case_count && required.length === 80, 'camera required cohort changed/empty');
    rows = required.map(entry=>({fixture:`${cohort.id}.${entry.id}`,tier:'headless',status:'unqualified',reason:'Required exact-source camera/mode workflow and reference qualification unavailable; no inference from M50',producer_revision:cohort.source_revision}));
    asset(cohort.inspector,roots);
    const inspectorText = sources.get(cohort.inspector.path).toString();
    requireCamera(canonical([...inspectorText.matchAll(/^import .* from '([^']+)';$/gm)].map(m=>m[1])) === canonical(producer.inspector_imports), 'camera inspector imports changed');
    const inspector = await import(`${pathToFileURL(contained(roots[cohort.inspector.root],cohort.inspector.path))}?suite=${cohort.inspector.sha256}`);
    inspector.validateManifest(manifest,inspector.extensions(sources.get(cohort.advertised.path).toString()));
    const packet = metadata(producer.packet), coordinator = metadata(producer.coordinator).m50;
    if (producerBinding) requireCamera(producerBinding.schema_version === 1 && producerBinding.producer_revision === cohort.source_revision && path.resolve(producerBinding.producer_worktree) === path.resolve(roots['raw-camera']) && path.resolve(producerBinding.public_packet) === contained(roots[producer.packet.root],producer.packet.path) && path.resolve(producerBinding.case_receipt) === contained(roots[producer.receipt.root],producer.receipt.path) && path.resolve(producerBinding.compiler_receipt) === contained(roots[producer.workflow_compiler.root],producer.workflow_compiler.path), 'camera private producer source/record binding changed');
    requireCamera(packet.schema_version === 1 && packet.producer_revision === cohort.source_revision && packet.source_tree === cohort.source_tree && coordinator.revision === cohort.source_revision, 'camera producer packet source mismatch');
    requireCamera(canonical(packet.source_pins) === canonical(cohort.source_pins.map(({blob,...pin})=>pin)), 'camera producer transitive pins changed');
    const hostFunction = sources.get(producer.host_contract.source).toString().match(/fn host_identity\(\)[\s\S]*?\n}\n/)?.[0];
    requireCamera(producer.host_contract.algorithm === 'sha256(trimmed-hostname)' && hostFunction && hash(hostFunction) === producer.host_contract.function_sha256, 'camera producer host contract changed');
    const receipt = metadata(producer.receipt), outer = metadata(producer.outer), summary = metadata(producer.summary);
    requireCamera(producer.case === 'cr3-canon-m50-raw' && receipt.case === producer.case && packet.case_receipt.id === producer.case, 'camera case/mode mismatch');
    const entry = required.find(c=>c.id === producer.case);
    requireCamera(entry && packet.case_receipt.sha256 === producer.receipt.sha256 && packet.case_receipt.bytes === producer.receipt.bytes && coordinator.result.sha256 === producer.receipt.sha256, 'camera case receipt binding changed');
    const nativeOuter = Object.fromEntries(Object.entries(outer).filter(([key])=>key in receipt));
    requireCamera(canonical(nativeOuter) === canonical(receipt), 'camera outer/native receipt mismatch');
    requireCamera(summary.mode === 'required' && summary.command === 'run' && summary.selected === 1 && summary.checked === 1 && summary.passed === 1 && summary.failed === 0 && summary.skipped === 0 && summary.success === true && summary.qualification_executed === true && summary.manifest_sha256 === cohort.manifest.sha256 && canonical(summary) === canonical(coordinator.summary.value), 'camera strict selected/checked result invalid');
    const binary = packet.test_executable;
    requireCamera(binary.live_observation?.sha256 === binary.sha256 && binary.live_observation.bytes === binary.bytes && binary.live_observation.matches_producer === true, 'camera original live executable observation missing');
    const compilerHashes = [];
    for (const [key,pin] of [['original_compiler_receipt',producer.original_compiler],['workflow_compiler_receipt',producer.workflow_compiler]]) {
      const r = metadata(pin), file = contained(roots[pin.root],pin.path);
      requireCamera(r.repository === 'omadesign' && r.executables.length === 1 && r.executables[0].size === binary.bytes && coordinator[key].sha256 === pin.sha256, 'camera original executable record mismatch');
      verifyCompilerReceipt({revision:cohort.source_revision,tree:cohort.source_tree,binary:r.executables[0].path,sha256:binary.sha256,compiler_receipt:{path:file,sha256:pin.sha256}});
      if (producerBinding) requireCamera(producerBinding.test_executable === r.executables[0].path, 'camera private executable record binding changed');
      requireCamera(r.cargo_args.includes('--lib') && !r.cargo_args.includes('--no-run'), 'camera compiler record is build-only');
      if (key === 'workflow_compiler_receipt') requireCamera(canonical(r.cargo_args) === canonical(['test','--locked','--lib','--','formats::raw::qualification::raw_camera_corpus','--ignored','--exact','--nocapture','--test-threads=1']), 'camera workflow execution arguments changed');
      compilerHashes.push(pin.sha256);
    }
    asset(producer.log,roots);
    const log = fs.readFileSync(contained(roots[producer.log.root],producer.log.path),'utf8');
    const testCounts = [...log.matchAll(/^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;/gm)];
    requireCamera(testCounts.length === 1 && testCounts[0][1] === 'ok' && testCounts[0][2] === '1' && testCounts[0][3] === '0' && testCounts[0][4] === '0' && log.includes(`Cargo receipt: ${contained(roots[producer.workflow_compiler.root],producer.workflow_compiler.path)}`) && log.includes('test formats::raw::qualification::raw_camera_corpus ... RAW_QUALIFICATION '), 'camera workflow invocation/count proof missing');
    const logged = [...log.matchAll(/^test formats::raw::qualification::raw_camera_corpus \.\.\. RAW_QUALIFICATION (\{[^\n]+\})$/gm)];
    requireCamera(logged.length === 1 && canonical(JSON.parse(logged[0][1])) === canonical(receipt), 'camera logged/native case proof differs');
    requireCamera(packet.compiler_receipt.sha256 === producer.workflow_compiler.sha256 && packet.runner.sha256 === binary.sha256 && receipt.runner.sha256 === binary.sha256 && receipt.runner.revision === cohort.source_revision && packet.runner.revision === cohort.source_revision && receipt.runner.cargo_lock_sha256 === hash(sources.get('Cargo.lock')), 'camera runner/source/compiler binding changed');
    requireCamera(canonical(packet.runner) === canonical(receipt.runner) && packet.case_receipt.checked === true && packet.case_receipt.status === 'passed', 'camera packet/native runner differs');
    requireCamera(/^[a-f0-9]{64}$/.test(producer.host_sha256) && packet.runner.host_sha256 === producer.host_sha256 && receipt.runner.host_sha256 === producer.host_sha256, 'camera producer host pin mismatch');
    requireCamera(receipt.schema_version === 1 && receipt.status === 'passed' && receipt.checked === true && receipt.required === true && Array.isArray(receipt.errors) && receipt.errors.length === 0 && receipt.manifest_sha256 === cohort.manifest.sha256 && receipt.checks.length === 7, 'camera required workflow incomplete/failed');
    requireCamera(canonical(receipt.required_reference_kinds) === canonical(entry.required_reference_kinds) && canonical(packet.required_reference_kinds) === canonical(entry.required_reference_kinds) && canonical(receipt.reference_tools) === canonical(packet.reference_tools), 'camera required reference/tool set changed');
    const referenceContracts = entry.references.map(({id,kind,representation,sha256,dimensions,tolerance,toolchain})=>({id,kind,representation,sha256,dimensions,tolerance,toolchain}));
    requireCamera(canonical(referenceContracts) === canonical(packet.reference_contracts), 'camera frozen reference contract changed');
    const data = bindings.cases?.[entry.id];
    requireCamera(data && Object.keys(bindings.cases).length === 1, 'camera private M50 binding missing/extra modes');
    const observedData = (file,expected,fnv) => {
      const observation = coordinator.stable_files.find(o=>o.path === file);
      requireCamera(observation && observation.matches === true && observation.sha256 === expected.sha256 && observation.expected_sha256 === expected.sha256 && observation.bytes === expected.bytes, 'camera original data observation missing/mismatched');
      return cameraData(file,expected,fnv);
    };
    const currentInput = observedData(data.source,{sha256:entry.source.sha256,bytes:entry.source.size_bytes});
    requireCamera(receipt.source_before_sha256 === currentInput.sha256 && receipt.source_after_sha256 === currentInput.sha256, 'camera source preservation failed');
    const references = [];
    for (const reference of entry.references) {
      requireCamera(reference.dimensions?.length === 2 && reference.dimensions.every(n=>Number.isSafeInteger(n) && n>0) && reference.toolchain.version && reference.toolchain.platform && reference.toolchain.command.length > 0, 'camera required reference geometry/toolchain missing');
      const channels = reference.representation === 'sensor-u16le-masked' ? 1 : 3;
      const actual = observedData(data.references?.[reference.id],{sha256:reference.sha256,bytes:reference.dimensions[0]*reference.dimensions[1]*channels*2},reference.sensor_fnv1a64);
      const toolObservation = coordinator.stable_files.find(o=>o.path === data.executables?.[reference.id]);
      requireCamera(toolObservation?.matches === true && toolObservation.sha256 === reference.toolchain.executable_sha256 && toolObservation.expected_sha256 === reference.toolchain.executable_sha256, 'camera original reference executable identity missing');
      references.push({id:reference.id,kind:reference.kind,representation:reference.representation,dimensions:reference.dimensions,channels,reference_sha256:actual.sha256,executable_sha256:toolObservation.sha256,bytes:actual.bytes,fnv1a64:actual.fnv1a64,identity:actual.identity});
    }
    requireCamera(cameraChecks(entry,{references},receipt), 'camera mathematical/reference/workflow oracle failed');
    for (const check of receipt.checks) requireCamera(check.dimensions?.length === 2 && check.stats.samples === check.dimensions[0]*check.dimensions[1]*3, 'camera workflow channel/sample count invalid');
    requireCamera(canonical(receipt.observed.dimensions) === canonical(entry.identity.output_dimensions) && receipt.observed.make === entry.identity.make && receipt.observed.model === entry.identity.model && receipt.observed.orientation === entry.identity.orientation && canonical(receipt.observed.camera_wb) === canonical(entry.identity.camera_wb), 'camera observed identity/mode mismatch');
    const samples = receipt.resources.samples;
    requireCamera(samples.length > 1 && samples[0].stage === 'before' && samples.at(-1).stage === 'released' && samples[0].admitted_bytes === samples.at(-1).admitted_bytes, 'camera admitted owners not released');
    requireCamera(statIdentity(fs.statSync(data.source,{bigint:true})) === currentInput.identity && references.every(r=>statIdentity(fs.statSync(data.references[r.id],{bigint:true})) === r.identity), 'camera input/reference changed after reading');
    verified = {packet_sha256:producer.packet.sha256,coordinator_sha256:producer.coordinator.sha256,compiler_receipts:compilerHashes,binary:{sha256:binary.sha256,bytes:binary.bytes,verification:'Original locked records only; no executable opened'},source:{revision:cohort.source_revision,tree:cohort.source_tree},producer_checkout:source(roots['raw-camera']),inspector:{...cohort.inspector,blob:cohort.source_pins.find(p=>p.path === cohort.inspector.path).blob},producer_host_sha256:producer.host_sha256};
    Object.assign(rows.find(r=>r.fixture === `${cohort.id}.${entry.id}`),{status:'passed',reason:'Exact pinned M50 producer proof and current input/reference bytes verified; no new decode or executable read',binding:verified,input:{sha256:currentInput.sha256,bytes:currentInput.bytes},references:references.map(({identity,...ref})=>ref),checks:receipt.checks.map(c=>({id:c.check,dimensions:c.dimensions,samples:c.stats.samples})),comparisons:receipt.comparisons.map(c=>({id:c.reference,kind:c.kind,representation:c.representation,samples:c.stats.samples,passed:c.passed})),resources:{measurement:receipt.resources.measurement,samples},boundary:producer.qualification_boundary});
    return {passed:rows.every(r=>r.status === 'passed'),integrity_passed:true,counts:{required:rows.length,qualified:1,unqualified:rows.length-1},rows,binding:verified,consumer_host_sha256:hash(os.hostname().trim()),consumer_tier:'Node-only source/receipt/fixture-data inspection; no product/tool executable opened',public_candidate:false};
  } catch (error) {
    const reason = /^(camera |[a-z]+: )/.test(error.message) && !error.message.includes('/home/') ? error.message : 'camera required pinned source/metadata/input/reference unavailable or invalid';
    const row = rows.find(r=>r.fixture === `${cohort.id}.${producer.case}`);
    if (row) row.reason = reason;
    return {passed:false,integrity_passed:false,counts:{required:producer.case_count,qualified:0,unqualified:producer.case_count},rows:rows.length ? rows : [{fixture:cohort.id,tier:'headless',status:'unqualified',reason}],errors:[reason],consumer_tier:'Node-only inspection; no executable opened',public_candidate:false};
  }
}
