import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { execFileSync } from 'node:child_process';
import { canonical, contained, fingerprint, hash } from './contract.mjs';

const sha = /^[a-f0-9]{64}$/;
const commit = /^[a-f0-9]{40}$/;
const requireThat = (condition, reason) => { if (!condition) throw Error(reason); };
const git = (root, ...args) => execFileSync('git', ['-C', root, ...args], {maxBuffer:8388608, timeout:10000, stdio:['ignore','pipe','pipe']});

/** Read reviewed owner records without executing producers or opening shared product binaries.
 * Args: tracked owner pins and private roots. Returns source-bound observations and unavailable gates.
 */
export function producerEvidence(index, roots = {}) {
  const errors = [], runs = {}, cases = [], routes = [];
  const cache = new Map();
  const read = (id, json = true) => {
    const pin = index.assets?.[id];
    requireThat(pin && sha.test(pin.sha256) && Number.isSafeInteger(pin.bytes) && pin.bytes > 0 && pin.bytes <= 8388608, `${id}: invalid pin`);
    requireThat(!pin.path.split('/').includes('target'), `${id}: shared product target is not an evidence input`);
    if (!cache.has(id)) {
      let file;
      try { file = contained(roots[pin.root], pin.path); }
      catch { throw Error(`${id}: required bytes unavailable`); }
      requireThat(fs.statSync(file).size === pin.bytes, `${id}: byte count changed`);
      const bytes = fs.readFileSync(file);
      requireThat(hash(bytes) === pin.sha256, `${id}: hash changed`);
      cache.set(id, bytes);
    }
    return json ? JSON.parse(cache.get(id)) : cache.get(id).toString('utf8');
  };
  const filename = id => path.resolve(roots[index.assets[id].root], index.assets[id].path);
  const joined = (id, reported) => requireThat(path.resolve(reported) === filename(id), `${id}: producer reference changed`);
  const attempt = (id, action) => {
    try { return action(); }
    catch (error) { errors.push({id, reason:/^[a-z0-9_.-]+: /.test(error.message) ? error.message : `${id}: required record, source or reference invalid/unavailable`}); return null; }
  };
  const compiler = (id, product, revision, tree) => {
    const r = read(id);
    requireThat(path.basename(filename(id)) === `${index.assets[id].sha256}.json`, `${id}: immutable receipt filename changed`);
    requireThat(r.schema === 1 && r.repository === product && r.revision === revision && r.source_tree === tree && commit.test(revision) && commit.test(tree), `${id}: compiler source changed`);
    requireThat(r.dirty_before === false && r.dirty_after === false && r.signal === null && Array.isArray(r.receipt_errors) && !r.receipt_errors.length, `${id}: dirty or incomplete compiler receipt`);
    requireThat([r.helper_sha256,r.receipt_helper_sha256,r.config_sha256].every(v => sha.test(v)) && (!r.queue_helper_sha256 || sha.test(r.queue_helper_sha256)), `${id}: compiler tool identities missing`);
    for (const [field,asset] of [['helper_sha256','helper.serialized-cargo.mjs'],['receipt_helper_sha256','helper.cargo-receipt.mjs'],['config_sha256','helper.config'],['queue_helper_sha256','helper.build-queue.mjs']]) {
      read(asset,false);
      requireThat(r[field] === index.assets[asset].sha256, `${id}: actual compiler helper/config bytes changed`);
    }
    const args = r.cargo_args;
    requireThat(Array.isArray(args) && args.every(a => typeof a === 'string') && ['test','clippy'].includes(args[0]) && args.slice(0,args.indexOf('--') < 0 ? undefined : args.indexOf('--')).includes('--locked') && !args.includes('--no-run'), `${id}: execution arguments unavailable`);
    const executed = args[0] === 'clippy' ? args : [args[0],'--message-format=json-render-diagnostics',...args.slice(1)];
    requireThat(canonical(r.executed_cargo_args) === canonical(executed), `${id}: executed arguments changed`);
    requireThat(git(roots[product], 'rev-parse', `${revision}^{tree}`).toString().trim() === tree, `${id}: committed Git tree unavailable/changed`);
    return r;
  };
  requireThat(index.version === 1 && Object.keys(index.runs ?? {}).length > 0, 'invalid owner evidence index');
  requireThat(index.routes?.length === 28 && new Set(index.routes.map(r=>r.id)).size === 28 && index.groups?.delivery_cases?.length === 8 && index.groups?.routing_cases?.length === 12, 'owner evidence cohort incomplete');
  const current_sources = {};
  for (const product of ['omatainer','editbay']) attempt(`checkout.${product}`, () => {
    current_sources[product] = {revision:git(roots[product],'rev-parse','HEAD').toString().trim(),tree:git(roots[product],'rev-parse','HEAD^{tree}').toString().trim(),dirty:git(roots[product],'status','--porcelain=v1','--untracked-files=normal').length > 0};
  });
  for (const [id, pin] of Object.entries(index.runs)) {
    const observed = attempt(id, () => {
      const r = compiler(pin.compiler, pin.product, pin.revision, pin.tree);
      requireThat(r.cargo_args[0] === 'test' && r.exit_code === pin.exit_code && [0,101].includes(r.exit_code), `${id}: test exit changed`);
      const executables = r.executables.map(e => ({sha256:e.sha256,bytes:e.size,target:typeof e.target === 'string' ? e.target : e.target?.name}));
      requireThat(executables.length > 0 && executables.every(e => sha.test(e.sha256) && e.bytes > 0) && canonical(executables) === canonical(pin.executables), `${id}: recorded executable identity changed`);
      const log = read(pin.log, false);
      requireThat(log.includes(`Cargo receipt: ${filename(pin.compiler)}`), `${id}: log/compiler join unavailable`);
      const launched = [...log.matchAll(/^\s*Running [^\n]+ \(([^\n]+)\)$/gm)].map(m => m[1]);
      requireThat(launched.length === r.executables.length && r.executables.every(e => launched.includes(e.path)), `${id}: actual test executable invocation missing`);
      const summaries = [...log.matchAll(/^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;/gm)];
      const selected = pin.summary === 'last' ? summaries.slice(-1) : summaries;
      requireThat(selected.length === r.executables.length, `${id}: actual test counts unavailable`);
      const counts = selected.reduce((a,m) => ({passed:a.passed+Number(m[2]),failed:a.failed+Number(m[3]),ignored:a.ignored+Number(m[4])}), {passed:0,failed:0,ignored:0});
      requireThat(counts.passed + counts.failed > 0 && canonical(counts) === canonical(pin.counts) && (counts.failed > 0) === (r.exit_code !== 0), `${id}: zero, failed or changed test counts`);
      for (const [file,digest] of Object.entries(pin.sources)) {
        requireThat(sha.test(digest) && hash(git(roots[pin.product], 'show', `${pin.revision}:${file}`)) === digest, `${id}: committed generator/input changed`);
      }
      return {id, product:pin.product, revision:r.revision, tree:r.source_tree, status:counts.failed ? 'failed' : 'passed_at_source', counts, compiler_sha256:index.assets[pin.compiler].sha256, log_sha256:index.assets[pin.log].sha256, executables, tools:{helper_sha256:r.helper_sha256,receipt_helper_sha256:r.receipt_helper_sha256,queue_helper_sha256:r.queue_helper_sha256 ?? null,config_sha256:r.config_sha256}, source_files:pin.sources, current_candidate_status:'unavailable', reason:'Original exact-source test execution; no current executable replay or complete owner acceptance', tier:'headless', host_binding:'not recorded by compiler receipt'};
    });
    runs[id] = observed ?? {id, revision:pin.revision, status:'unavailable', current_candidate_status:'unavailable'};
  }
  const passedTest = (run, test) => {
    requireThat(runs[run]?.counts, `${run}: required run unavailable`);
    const log = read(index.runs[run].log,false);
    if (log.split('\n').includes(`test ${test} ... ok`)) return;
    const start = log.indexOf(`test ${test} ... \n`);
    const end = start < 0 ? -1 : log.indexOf('\nok\ntest ',start);
    const body = log.slice(start,end);
    requireThat(test.endsWith('::fixed_send_capture_native_fresh_process_and_encoded_export_null_with_wet_tails') && end > start && !body.includes('FAILED') && [...body.matchAll(/^test result: ok\. 1 passed; 0 failed;/gm)].length === 9, `${run}: required route did not execute successfully`);
  };
  const caseFiles = (ids, run, statusKey = 'status') => ids.map(id => {
    const c = read(id);
    requireThat(c.schema === 1 && ['passed','pass','characterized'].includes(c[statusKey]), `${id}: unexpected case result`);
    requireThat(!c.source_revision || c.source_revision === runs[run].revision, `${id}: case source changed`);
    return {id, c, row:{id, case:c.case, run, revision:runs[run].revision, artifact_sha256:index.assets[id].sha256, recorded_status:c[statusKey], status:c[statusKey] === 'characterized' ? 'characterized' : 'passed_at_source', current_candidate_status:'unavailable'}};
  });
  attempt('delivery', () => {
    const packet = read('delivery'), run = runs.delivery;
    requireThat(run.counts && packet.qualified_revision === run.revision, 'delivery: packet source changed');
    joined('delivery.compiler',packet.helper_receipt); joined('delivery.log',packet.raw);
    requireThat(canonical(packet.binary.map(e => e.sha256)) === canonical(run.executables.map(e => e.sha256)), 'delivery: binary/compiler mismatch');
    requireThat(packet.required_cases.length === 16 && packet.optional_cases.length === 1 && packet.per_case_receipts.length === 8, 'delivery: required/optional cohort changed');
    requireThat(hash(git(roots.omatainer,'show',`${run.revision}:tests/fixtures/audio-delivery/matrix.json`)) === packet.matrix_at_qualified_revision, 'delivery: committed required case matrix changed');
    for (const c of [...packet.required_cases,...packet.optional_cases]) {
      passedTest('delivery', c.test);
      if (c.source) requireThat(hash(git(roots.omatainer,'show',`${run.revision}:${c.source}`)) === c.source_sha256, 'delivery: required generator source mismatch');
      routes.push({id:c.id,generator_id:'delivery',required:packet.required_cases.includes(c),revision:run.revision,status:'passed_at_source',current_candidate_status:'unavailable'});
    }
    const files = caseFiles(index.groups.delivery_cases,'delivery');
    requireThat(files.filter(f=>f.row.status==='passed_at_source').length === 7 && files.filter(f=>f.row.status==='characterized').length === 1, 'delivery: characterization must remain distinct');
    for (const [i,{id,c,row}] of files.entries()) {
      requireThat(c.source_revision === run.revision, `${id}: required exact case source missing`);
      joined(id,packet.per_case_receipts[i].path);
      requireThat(packet.per_case_receipts[i].sha256 === row.artifact_sha256, `${id}: packet case hash changed`);
      if (c.input_sha256) {
        requireThat(sha.test(c.input_sha256) && c.input_bytes > 0, `${id}: input identity missing`);
        row.input = {sha256:c.input_sha256,bytes:c.input_bytes,availability:'unavailable',reason:'Generated input bytes not retained in this packet; recorded identity only'};
        const report = c.report;
        requireThat(report && report.meter?.sha256 === run.executables[0].sha256 && sha.test(report.decoded_pcm_sha256), `${id}: measured executable/PCM identity missing`);
        requireThat(report.decoder && report.probe && c.independent_reference?.tool, `${id}: required independent tool missing`);
        for (const tool of [report.decoder,report.probe,c.independent_reference.tool]) {
          const toolId = `tool.${path.basename(tool.path)}`;
          read(toolId,false);
          requireThat(tool.sha256 === index.assets[toolId].sha256, `${id}: independent tool identity changed`);
        }
      }
      if (row.status === 'characterized') requireThat(c.strict_agreement_qualified === false, `${id}: characterization promoted`);
      cases.push(row);
    }
  });
  attempt('routing', () => {
    const packet = read('routing'), matrix = read('matrix');
    requireThat(matrix.qualified_runs.some(r => r.source === runs['routing.ui'].revision && r.receipt.endsWith(`${runs['routing.ui'].compiler_sha256}.json`)), 'routing: qualified UI source missing');
    for (const id of ['routing.failed7cf','routing.failedd72','routing.ui']) {
      const p = packet.actual_proof.find(p=>p.source === runs[id].revision);
      requireThat(p && p.passed === runs[id].counts?.passed && p.failed === runs[id].counts?.failed, 'routing: whole-run status/counts changed');
      requireThat(p.binary_sha256 === runs[id].executables[0].sha256, 'routing: binary/compiler mismatch');
    }
    requireThat(runs['routing.failed7cf'].status === 'failed' && runs['routing.failedd72'].status === 'failed', 'routing: failed whole run promoted');
    requireThat(matrix.actual_run.source === runs['routing.failed7cf'].revision && matrix.actual_run.case_files.length === 12, 'routing: specific-case source changed');
    for (const c of matrix.cases.filter(c=>c.required)) {
      passedTest('routing.failed7cf',c.test);
      routes.push({id:c.id,generator_id:'routing.failed7cf',required:true,revision:runs['routing.failed7cf'].revision,status:'passed_at_source',whole_run_status:'failed',current_candidate_status:'unavailable',declared_candidate_source:matrix.candidate_source});
    }
    for (const [i,{id,c,row}] of caseFiles(index.groups.routing_cases,'routing.failed7cf').entries()) {
      const oracle = i < 3 ? c.maximum_residual === 0 && c.fixed_wet_tails === true : c.fresh_process === true && c.encoded_wav === true && c.native_archive === true && c.wet_tail_l1 > 0 && c.callback_allocations === 0 && c.callback_frees === 0;
      requireThat(matrix.actual_run.case_files[i].sha256 === row.artifact_sha256 && oracle && c.live_offline_null === true && c.physical_proof === false, `${id}: case oracle/identity changed`);
      cases.push({...row,whole_run_status:'failed',scope:'Only this contribution/fixed-send case; no full current stereo or surround acceptance'});
    }
  });
  attempt('video', () => {
    const packet = read('video'), captions = read('captions');
    requireThat(packet.schema === 'editbay.producer-routes.v1' && packet.required_routes.length === 8, 'video: required routes changed');
    requireThat(packet.current_first_layer_result.revision === runs.editorial.revision && packet.current_caption_result.revision === runs['captions.ui'].revision && runs['captions.formats'].revision === runs['captions.ui'].revision, 'video: group source changed');
    requireThat(captions.status === 'passed' && captions.tests === 34 && runs['captions.ui'].counts?.passed + runs['captions.formats'].counts?.passed === 34, 'video: captions grouped count changed');
    const clippy = [['editorial.clippy','editorial'],['captions.clippy','captions.ui']];
    for (const [id,runId] of clippy) {
      const run = runs[runId], r = compiler(id,'editbay',run.revision,run.tree);
      requireThat(r.cargo_args[0] === 'clippy' && r.exit_code === 0 && r.executables.length === 0, `${id}: analysis is not fixture execution`);
    }
    for (const id of [...index.groups.editorial_artifacts,...index.groups.caption_artifacts]) read(id,false);
    for (const id of ['tool.ffprobe','tool.xmllint','tool.7z']) read(id,false);
    for (const id of index.groups.editorial_artifacts.filter(id => id.endsWith('.fixture.json'))) {
      const fixture = read(id);
      requireThat(fixture.schema === 'editbay.editorial.fixture.v1' && fixture.members.length > 0 && fixture.license, `${id}: native input manifest invalid`);
      for (const member of fixture.members) {
        const memberId = id.replace(/fixture\.json$/,path.basename(member.path));
        joined(memberId,member.path);
        requireThat(index.assets[memberId].sha256 === member.sha256 && index.assets[memberId].bytes === member.bytes, `${id}: native source identity changed`);
      }
      joined(id.replace(/fixture\.json$/,'native.editbay'),fixture.native_project);
    }
    const arabic = read('captions.arabic-pango-toolchain.json');
    requireThat(Array.isArray(arabic.libraries) && arabic.libraries.length > 0 && arabic.libraries.length === packet.arabic_reference.tool_dependency_hashes, 'video: reference tool dependencies incomplete');
    for (const dependency of [arabic.tool,arabic.font,arabic.font_license,...arabic.libraries]) {
      requireThat(dependency.path?.startsWith('/usr/') && sha.test(dependency.sha256) && dependency.bytes > 0 && dependency.bytes <= 67108864, 'video: reference tool/font dependency unpinned');
      const actual = fingerprint(contained(roots.system,dependency.path.slice(5)));
      requireThat(actual.sha256 === dependency.sha256 && actual.bytes === dependency.bytes, 'video: reference tool/font dependency changed');
    }
    requireThat(arabic.tool.sha256 === packet.arabic_reference.tool_sha256 && arabic.font.sha256 === packet.arabic_reference.font_sha256, 'video: independent glyph reference identity changed');
    const font = packet.font_cases.required;
    requireThat(index.runs['captions.ui'].sources[font.path] === font.sha256 && sha.test(index.runs['captions.ui'].sources[packet.arabic_reference.path]), 'video: committed caption font/reference binding missing');
    const format = read('editorial.format-inspection-9a718c3137e3.json');
    const binary = read('editorial.binary-editorial-inspection-20261015204249627.json');
    requireThat(format.status === 'passed' && format.results.length === 12 && !format.pending.length && binary.status === 'passed' && binary.results.length === 4, 'video: independent artifact set incomplete');
    for (const r of format.results) requireThat(r.status === 'passed' && index.assets[`editorial.output.${r.file}`]?.sha256 === r.sha256, 'video: export artifact identity changed');
    for (const r of binary.results) requireThat(r.status === 'passed' && index.assets[`editorial.output.${path.basename(r.file)}`]?.sha256 === r.sha256 && index.assets[`editorial.output.${r.fixture}.${r.format}.loss.json`]?.sha256 === r.loss_report_sha256, 'video: semantics/loss artifact identity changed');
    const dtd = read('editorial.fcpxml-dtd-reference.json');
    requireThat(dtd.dtd_sha256 === index.assets['editorial.fcpxml-1.10.reference.dtd'].sha256 && binary.inspector_sha256 === index.assets['editorial.inspect-binary-editorial.mjs'].sha256, 'video: reference/inspector changed');
    const independent = read('captions.caption-independent.json');
    requireThat(independent.status === 'passed' && independent.files.length === 2, 'video: caption independent reference missing');
    for (const r of independent.files) {
      const id = `captions.${path.basename(r.path)}`;
      requireThat(index.assets[id]?.sha256 === r.sha256 && r.time_base === '1/1000' && canonical(r.packets.map(p=>[p.start_ms,p.duration_ms,p.text])) === canonical([[0,1,'Before'],[3,1997,'Café 世界']]), 'video: caption timing/Unicode/reference changed');
      const text = read(id,false).replaceAll('\r','');
      requireThat(text.includes('00:00:00.003 --> 00:00:02.000') || text.includes('00:00:00,003 --> 00:00:02,000'), 'video: actual caption timing changed');
      requireThat(text.includes('Café 世界') && text.includes('Before'), 'video: actual caption Unicode changed');
    }
    for (const r of packet.required_routes) {
      const group = r.generator_id;
      const runId = group === 'editbay.editorial.group.v1' ? 'editorial' : group === 'editbay.captions.group.v1' ? 'captions.ui' : null;
      if (runId) requireThat(r.commit === runs[runId].revision, 'video: route/group source mismatch');
      routes.push({id:r.id,generator_id:group ?? r.id,required:true,revision:r.commit,status:runId ? 'passed_at_source' : 'unavailable',current_candidate_status:'unavailable',expected_losses:r.expected_losses ?? [],reason:runId ? 'Exact grouped software proof; complete product delivery gate remains unavailable' : 'Current scopes/multicam/native-viewer runtime or required build dependency unrun'});
    }
  });
  for (const expected of index.routes) {
    const row = routes.find(r=>r.id === expected.id);
    if (row && !errors.some(e=>e.id === expected.family)) requireThat(row.revision === expected.revision && row.generator_id === expected.generator_id && row.required === expected.required, 'reviewed owner route changed');
    if (!row) routes.push({...expected,status:'unavailable',current_candidate_status:'unavailable',reason:'Required owner packet, artifact, source or reference unavailable/invalid'});
    else if (errors.some(e=>e.id === expected.family)) row.status = 'unavailable';
  }
  for (const [family,ids,run] of [['delivery',index.groups.delivery_cases,'delivery'],['routing',index.groups.routing_cases,'routing.failed7cf']]) for (const id of ids) {
    const row = cases.find(c=>c.id === id);
    if (!row) cases.push({id,run,revision:index.runs[run].revision,status:'unavailable',current_candidate_status:'unavailable'});
    else if (errors.some(e=>e.id === family)) row.status = 'unavailable';
  }
  const generators = [...new Set(routes.map(r=>r.generator_id))].map(id=>({id,routes:routes.filter(r=>r.generator_id===id).map(r=>r.id),execution_runs:id === 'editbay.editorial.group.v1' ? ['editorial'] : id === 'editbay.captions.group.v1' ? ['captions.ui','captions.formats'] : index.runs[id] ? [id] : []}));
  const verified_artifacts = [...cache.keys()].map(id=>({id,sha256:index.assets[id].sha256,bytes:index.assets[id].bytes}));
  return {version:1,tier:'node-only evidence inspection; no product executable launched',host_sha256:hash(os.hostname().trim()),node_sha256:fingerprint(process.execPath).sha256,index_sha256:hash(canonical(index)),current_sources,integrity_passed:errors.length === 0,public_candidate:false,certified_true_peak_ceiling:false,runs:Object.values(runs),cases,routes,generators,verified_artifacts,errors,unavailable:['Complete current owner acceptance and stricter product gates','RAW80: reviewed final inspector pin and real exact-source workflow proof','Common UI fixture/font/theme/appearance references','Caption native burn-in/master QC','Current stereo full cohort and surround','Original execution host binding where absent; no native-window/device/physical/human proof','Editorial inspection and ffprobe original executable hashes absent from their records: retained bytes and numerical/text oracles do not establish codec conformance'],policy:'Pinned old passes and failures remain at their source. Missing inputs/references and characterization cannot pass candidate gates. Future execution must verify actual binary SHA before launch under the global lock.'};
}
