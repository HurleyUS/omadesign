import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { hash, fingerprint } from './contract.mjs';
import { producerEvidence } from './producer-evidence.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
fs.mkdirSync(path.join(root,'.artifacts'),{recursive:true});
const scratch = fs.mkdtempSync(path.join(root,'.artifacts/producer-reader-'));
test.after(()=>fs.rmSync(scratch,{recursive:true,force:true}));

/** Author tiny synthetic records for the reader; these do not represent product execution.
 * Args: unique name. Returns private roots, reviewed pins and a record editor.
 */
function fixture(name) {
  const dir = path.join(scratch,name);
  fs.mkdirSync(dir,{recursive:true});
  const git = (...args)=>execFileSync('git',['-C',dir,...args],{stdio:'pipe'}).toString().trim();
  git('init','--quiet');
  fs.writeFileSync(path.join(dir,'source.txt'),'Original synthetic reader source, no product proof.');
  fs.mkdirSync(path.join(dir,'tests/fixtures/audio-delivery'),{recursive:true});
  fs.writeFileSync(path.join(dir,'tests/fixtures/audio-delivery/matrix.json'),'{}');
  git('add','.');
  git('-c','user.name=Suite Reader','-c','user.email=reader@invalid','-c','core.hooksPath=/dev/null','commit','--quiet','-m','Synthetic reader data');
  let revision = git('rev-parse','HEAD');
  const tree = git('rev-parse','HEAD^{tree}');
  const index = {version:1,assets:{},runs:{},groups:{delivery_cases:[],routing_cases:[],editorial_artifacts:[],caption_artifacts:[]},routes:[]};
  const put = (id,value,immutable=false) => {
    const bytes = typeof value === 'string' ? value : JSON.stringify(value);
    const relative = immutable ? `${hash(bytes)}.json` : `${id}.json`;
    fs.writeFileSync(path.join(dir,relative),bytes);
    index.assets[id] = {root:'records',path:relative,...fingerprint(path.join(dir,relative))};
    return path.join(dir,relative);
  };
  const file = id => path.join(dir,index.assets[id].path);
  const edit = (id,mutate) => {const value=JSON.parse(fs.readFileSync(file(id)));mutate(value);put(id,value,id.endsWith('.compiler') || id.endsWith('.clippy'));};
  for (const id of ['helper.serialized-cargo.mjs','helper.cargo-receipt.mjs','helper.config','helper.build-queue.mjs','tool.ffprobe','tool.ffmpeg','tool.xmllint','tool.7z']) put(id,`synthetic ${id}`);
  const toolFields = {helper_sha256:index.assets['helper.serialized-cargo.mjs'].sha256,receipt_helper_sha256:index.assets['helper.cargo-receipt.mjs'].sha256,config_sha256:index.assets['helper.config'].sha256,queue_helper_sha256:index.assets['helper.build-queue.mjs'].sha256};
  const required = Array.from({length:16},(_,i)=>({id:`required-${i}`,test:`synthetic::required_${i}`,source:'source.txt',source_sha256:hash(fs.readFileSync(path.join(dir,'source.txt')))}));
  const optional = [{id:'optional-sparse',test:'synthetic::optional'}];
  const routingTests = Array.from({length:3},(_,i)=>({id:`routing-${i}`,test:`synthetic::routing_${i}`,required:true}));
  const counts = {'delivery':[44,0,1],'routing.failed7cf':[81,7,0],'routing.failedd72':[46,1,0],'routing.ui':[4,0,0],editorial:[6,0,0],'captions.ui':[15,0,0],'captions.formats':[19,0,0]};
  const compilers = {};
  for (const [id,[passed,failed,ignored]] of Object.entries(counts)) {
    if (id !== 'captions.formats') git('-c','user.name=Suite Reader','-c','user.email=reader@invalid','-c','core.hooksPath=/dev/null','commit','--quiet','--allow-empty','-m',id);
    const runRevision = git('rev-parse','HEAD');
    const product = id.startsWith('routing') || id==='delivery' ? 'omatainer' : 'editbay';
    const executable = {path:path.join(dir,'target','must-never-be-opened'),sha256:hash(`not a real binary: ${id}`),size:1,target:'synthetic-test'};
    const cargo_args = ['test','--locked','--','--test-threads=1'];
    const r = {...toolFields,schema:1,repository:product,revision:runRevision,source_tree:tree,dirty_before:false,dirty_after:false,exit_code:failed?101:0,signal:null,receipt_errors:[],cargo_args,executed_cargo_args:['test','--message-format=json-render-diagnostics',...cargo_args.slice(1)],executables:[executable]};
    compilers[id] = r;
    put(id+'.compiler',r,true);
    const tests = id==='delivery' ? [...required,...optional] : id==='routing.failed7cf' ? routingTests : [];
    const log = `     Running unittests src/main.rs (${executable.path})\n${tests.map(t=>`test ${t.test} ... ok\n`).join('')}test result: ${failed?'FAILED':'ok'}. ${passed} passed; ${failed} failed; ${ignored} ignored;\nCargo receipt: ${file(id+'.compiler')}\n`;
    put(id+'.log',log);
    index.runs[id] = {product,revision:runRevision,tree,compiler:id+'.compiler',log:id+'.log',exit_code:r.exit_code,summary:product==='omatainer'?'last':'all',counts:{passed,failed,ignored},executables:[{sha256:executable.sha256,bytes:1,target:executable.target}],sources:{'source.txt':required[0].source_sha256}};
  }
  revision = index.runs.delivery.revision;
  for (const [id,run] of [['editorial.clippy','editorial'],['captions.clippy','captions.ui']]) put(id,{...compilers[run],cargo_args:['clippy','--locked'],executed_cargo_args:['clippy','--locked'],executables:[]},true);
  const delivery = {qualified_revision:revision,helper_receipt:file('delivery.compiler'),raw:file('delivery.log'),binary:[{sha256:compilers.delivery.executables[0].sha256}],matrix_at_qualified_revision:hash('{}'),required_cases:required,optional_cases:optional,per_case_receipts:[]};
  for(let i=0;i<8;i++) {
    const id=`delivery.case.${i}`;
    const tool={path:'/usr/bin/ffmpeg',sha256:index.assets['tool.ffmpeg'].sha256};
    put(id,{schema:1,status:i===3?'characterized':'passed',source_revision:revision,strict_agreement_qualified:false,...(i===3?{input_sha256:hash('missing generated input'),input_bytes:42,report:{meter:{sha256:compilers.delivery.executables[0].sha256},decoded_pcm_sha256:hash('synthetic pcm'),decoder:tool,probe:tool},independent_reference:{tool}}:{})});
    index.groups.delivery_cases.push(id);
    delivery.per_case_receipts.push({path:file(id),sha256:index.assets[id].sha256});
  }
  put('delivery',delivery);
  const matrix={candidate_source:'f'.repeat(40),qualified_runs:[{source:index.runs['routing.ui'].revision,receipt:file('routing.ui.compiler')}],actual_run:{source:index.runs['routing.failed7cf'].revision,case_files:[]},cases:routingTests};
  for(let i=0;i<12;i++) {
    const id=`routing.case.${i}`;
    put(id,{schema:1,status:'pass',maximum_residual:0,fixed_wet_tails:true,fresh_process:true,encoded_wav:true,native_archive:true,wet_tail_l1:0.1,callback_allocations:0,callback_frees:0,live_offline_null:true,physical_proof:false});
    index.groups.routing_cases.push(id);
    matrix.actual_run.case_files.push({sha256:index.assets[id].sha256});
  }
  put('matrix',matrix);
  put('routing',{product_source:matrix.candidate_source,actual_proof:['routing.failed7cf','routing.failedd72','routing.ui'].map(id=>({source:index.runs[id].revision,...index.runs[id].counts,binary_sha256:compilers[id].executables[0].sha256}))});
  const videoRoutes = Array.from({length:8},(_,i)=>({id:`video-${i}`,commit:i<2?index.runs.editorial.revision:index.runs['captions.ui'].revision,generator_id:i<2?'editbay.editorial.group.v1':i<4?'editbay.captions.group.v1':`pending-${i}`}));
  put('video',{schema:'editbay.producer-routes.v1',required_routes:videoRoutes,current_first_layer_result:{revision:index.runs.editorial.revision},current_caption_result:{revision:index.runs['captions.ui'].revision},arabic_reference:{path:'source.txt',tool_sha256:hash('x'),font_sha256:hash('x'),tool_dependency_hashes:1},font_cases:{required:{path:'source.txt',sha256:required[0].source_sha256}}});
  put('captions',{status:'passed',tests:34});
  const add = (group,id,value) => {put(id,value);index.groups[group].push(id);};
  const results=[];
  const member='editorial.input.inputs-synthetic.media.json',native='editorial.input.inputs-synthetic.native.editbay';
  add('editorial_artifacts',member,{synthetic:'original authored input'});
  add('editorial_artifacts',native,{synthetic:'original authored document'});
  for(const [id,relative] of [[member,'media.json'],[native,'native.editbay']]) {fs.renameSync(file(id),path.join(dir,relative));index.assets[id].path=relative;}
  add('editorial_artifacts','editorial.input.inputs-synthetic.fixture.json',{schema:'editbay.editorial.fixture.v1',license:'MIT',native_project:file(native),members:[{path:file(member),sha256:index.assets[member].sha256,bytes:index.assets[member].bytes}]});
  for(let i=0;i<12;i++) {const name=`export-${i}`;add('editorial_artifacts',`editorial.output.${name}`,'synthetic export');results.push({file:name,status:'passed',sha256:index.assets[`editorial.output.${name}`].sha256});}
  add('editorial_artifacts','editorial.format-inspection-9a718c3137e3.json',{status:'passed',results,pending:[]});
  add('editorial_artifacts','editorial.inspect-binary-editorial.mjs','synthetic oracle');
  add('editorial_artifacts','editorial.fcpxml-1.10.reference.dtd','synthetic reference');
  add('editorial_artifacts','editorial.fcpxml-dtd-reference.json',{dtd_sha256:index.assets['editorial.fcpxml-1.10.reference.dtd'].sha256});
  const semantics=[];
  for(let i=0;i<4;i++){const id=`editorial.output.fixture-${i}.aaf.loss.json`;add('editorial_artifacts',id,'synthetic loss');semantics.push({status:'passed',file:`export-${i}`,sha256:results[i].sha256,fixture:`fixture-${i}`,format:'aaf',loss_report_sha256:index.assets[id].sha256});}
  add('editorial_artifacts','editorial.binary-editorial-inspection-20261015204249627.json',{status:'passed',results:semantics,inspector_sha256:index.assets['editorial.inspect-binary-editorial.mjs'].sha256});
  fs.writeFileSync(path.join(dir,'synthetic'),'x');
  const dependency = {path:'/usr/synthetic',sha256:hash('x'),bytes:1};
  add('caption_artifacts','captions.arabic-pango-toolchain.json',{tool:dependency,font:dependency,font_license:dependency,libraries:[dependency]});
  const sidecars=[];
  for(const ext of ['srt','vtt']){const id=`captions.range.${ext}`;add('caption_artifacts',id,'00:00:00.003 --> 00:00:02.000\nBefore\nCafé 世界');sidecars.push({path:`range.${ext}`,sha256:index.assets[id].sha256,time_base:'1/1000',packets:[{start_ms:0,duration_ms:1,text:'Before'},{start_ms:3,duration_ms:1997,text:'Café 世界'}]});}
  add('caption_artifacts','captions.caption-independent.json',{status:'passed',files:sidecars});
  index.routes=[...required.map(c=>({...c,generator_id:'delivery',family:'delivery',required:true,revision})),{...optional[0],generator_id:'delivery',family:'delivery',required:false,revision},...routingTests.map(c=>({...c,generator_id:'routing.failed7cf',family:'routing',revision:index.runs['routing.failed7cf'].revision})),...videoRoutes.map(c=>({id:c.id,generator_id:c.generator_id,family:'video',required:true,revision:c.commit}))];
  return {index,roots:{records:dir,omatainer:dir,editbay:dir,system:dir},file,put,edit,git};
}

test('reader keeps exact old passes, failed whole runs and characterization without opening target binaries',()=>{
  const f=fixture('boundaries');
  const r=producerEvidence(f.index,f.roots);
  assert.equal(r.public_candidate,false);
  assert.equal(r.certified_true_peak_ceiling,false);
  assert.equal(r.runs.find(r=>r.id==='routing.failedd72').status,'failed');
  assert.deepEqual(r.runs.find(r=>r.id==='delivery').counts,{passed:44,failed:0,ignored:1});
  assert.equal(r.cases.find(r=>r.id==='delivery.case.3').status,'characterized');
  assert.equal(r.cases.filter(r=>r.whole_run_status==='failed').length,12);
  assert.equal(r.routes.length,28);
  assert.equal(r.cases.length,20);
  assert.equal(r.runs.filter(r=>r.status==='unavailable').length,0);
  assert.equal(r.runs.every(r=>r.current_candidate_status==='unavailable'),true);
  assert.equal(fs.existsSync(path.join(f.roots.records,'target')),false);
  assert.equal(JSON.stringify(r).includes(f.roots.records),false);
  assert.equal(r.integrity_passed,true,JSON.stringify(r.errors));
  assert.equal(r.cases.find(r=>r.id==='delivery.case.3').input.availability,'unavailable');
  assert.equal(r.generators.find(g=>g.id==='editbay.captions.group.v1').execution_runs.length,2);
  assert.equal(r.routes.filter(r=>r.id.startsWith('video-') && r.status==='unavailable').length,4);
});

test('required references, retained native inputs and tool/font bytes cannot be omitted or substituted',()=>{
  const checks=[
    f=>fs.rmSync(f.file('editorial.fcpxml-1.10.reference.dtd')),
    f=>fs.rmSync(f.file('editorial.input.inputs-synthetic.media.json')),
    f=>f.edit('editorial.input.inputs-synthetic.fixture.json',r=>{r.members[0].sha256=hash('other input')}),
    f=>fs.writeFileSync(path.join(f.roots.system,'synthetic'),'wrong reference tool/font'),
    f=>f.edit('captions.caption-independent.json',r=>{r.files[0].packets[1].duration_ms=1998}),
    f=>f.put('captions.range.srt','wrong timing/Unicode'),
    f=>f.edit('delivery.case.3',r=>{delete r.independent_reference}),
  ];
  for(const [i,mutate] of checks.entries()) {
    const f=fixture(`reference-${i}`);mutate(f);
    const r=producerEvidence(f.index,f.roots);
    assert.equal(r.integrity_passed,false,`reference ${i}`);
    assert.equal(r.public_candidate,false);
    assert.ok(r.errors.some(e=>e.id === (i===6?'delivery':'video')));
    assert.equal(r.routes.length,28);
  }
});

test('missing/corrupt records, compiler/source/tool/count regressions and promoted characterization fail closed',()=>{
  const checks=[
    f=>fs.rmSync(f.file('delivery.case.0')),
    f=>fs.appendFileSync(f.file('delivery.case.0'),' '),
    f=>f.edit('delivery.compiler',r=>{r.dirty_after=true}),
    f=>f.edit('delivery.compiler',r=>{r.executables[0].sha256=hash('replaced binary')}),
    f=>f.edit('delivery.compiler',r=>{r.executed_cargo_args.push('changed argument')}),
    f=>f.edit('delivery.compiler',r=>{r.source_tree='f'.repeat(40)}),
    f=>fs.appendFileSync(f.file('helper.config'),'changed'),
    f=>f.put('delivery.log',fs.readFileSync(f.file('delivery.log'),'utf8').replace('44 passed','0 passed')),
    f=>f.put('delivery.log',fs.readFileSync(f.file('delivery.log'),'utf8').replace(' ... ok',' ... FAILED')),
    f=>f.edit('delivery',r=>{r.qualified_revision='f'.repeat(40)}),
    f=>{f.edit('delivery.case.3',r=>{r.status='passed'});f.edit('delivery',r=>{r.per_case_receipts[3].sha256=f.index.assets['delivery.case.3'].sha256})},
    f=>{f.index.runs.delivery.sources['source.txt']=hash('different source')},
  ];
  for(const [i,mutate] of checks.entries()) {
    const f=fixture(`rejection-${i}`);mutate(f);
    const r=producerEvidence(f.index,f.roots);
    assert.equal(r.integrity_passed,false,`mutation ${i}`);
    assert.equal(r.public_candidate,false);
    assert.ok(r.errors.some(e=>e.id==='delivery'),`delivery rejection ${i}`);
    assert.equal(r.routes.filter(r=>r.family==='delivery' || r.generator_id==='delivery').every(r=>r.status==='unavailable'),true);
    assert.equal(r.cases.length,20);
  }
});

test('reviewed cohort is fixed; shared caption/editorial generators have one execution group each',()=>{
  const index=JSON.parse(fs.readFileSync(path.join(root,'qualification/suite/owner-sources.json')));
  assert.equal(index.routes.length,29);
  for(const id of ['editbay.editorial.group.v1','editbay.captions.group.v1']) assert.equal(index.routes.filter(r=>r.generator_id===id).length,2);
  assert.equal(index.routes.filter(r=>r.family==='video.followon').length,5);
  assert.equal(index.video_followon.runs.length,5);
  assert.equal(index.runs['captions.ui'].counts.passed+index.runs['captions.formats'].counts.passed,34);
  assert.equal(index.runs.editorial.counts.passed,6);
  assert.throws(()=>producerEvidence({...index,routes:[]}),/cohort incomplete/);
});

/** Make tiny authored video observations, including absent executable paths.
 * Args: fixture name. Returns private records and editors; never product proof.
 */
function videoFixture(name) {
  const f=fixture(name), {index,put,file}=f;
  const publicIndex=JSON.parse(fs.readFileSync(path.join(root,'qualification/suite/owner-sources.json')));
  const conf=index.video_followon={version:1,runs:publicIndex.video_followon.runs,refs:{},support:{},generators:publicIndex.video_followon.generators,unavailable:publicIndex.video_followon.unavailable};
  const revision=index.runs['captions.ui'].revision, tree=index.runs['captions.ui'].tree;
  for(const [id,passed,ignored]of [['scopes.numeric',14,0],['scopes.viewer',1,0],['multicam.core',12,0],['multicam.app',10,1],['multicam.native',1,0]]) {
    const compiler=JSON.parse(fs.readFileSync(file('captions.ui.compiler')));
    compiler.cargo_args=['test','--locked','-p',id,'--','--test-threads=1'];
    compiler.executed_cargo_args=['test','--message-format=json-render-diagnostics',...compiler.cargo_args.slice(1)];
    put(id+'.compiler',compiler,true);
    put(id+'.log',`Running synthetic (${compiler.executables[0].path})\ntest result: ok. ${passed} passed; 0 failed; ${ignored} ignored;\nCargo receipt: ${file(id+'.compiler')}\n`);
    index.runs[id]={...structuredClone(index.runs['captions.ui']),compiler:id+'.compiler',log:id+'.log',counts:{passed,failed:0,ignored},cargo_args:compiler.cargo_args};
  }
  for(const prefix of ['scopes','multicam']) {
    const id=prefix+'.cli',compiler=JSON.parse(fs.readFileSync(file('captions.ui.compiler')));
    compiler.cargo_args=['build','--locked','-p','synthetic-cli'];
    compiler.executed_cargo_args=['build','--message-format=json-render-diagnostics',...compiler.cargo_args.slice(1)];
    compiler.executables[0].target={name:'synthetic-cli'};
    put(id,compiler,true);
    conf.support[id]={asset:id,revision,tree,cargo_args:compiler.cargo_args,executables:[{sha256:compiler.executables[0].sha256,bytes:1,target:'synthetic-cli'}]};
  }
  const ref=(key,value)=>{const id='video.followon.'+key;put(id,value);conf.refs[key]=id;return {path:file(id),sha256:index.assets[id].sha256,bytes:index.assets[id].bytes};};
  const cli=prefix=>({build_receipt:file(prefix+'.cli'),source_revision:revision,fingerprint:{sha256:conf.support[prefix+'.cli'].executables[0].sha256,bytes:1}});
  const owner={project_id:'synthetic',revision:1};
  const report=(frame,white)=>({identity:{frame,composition:'synthetic',version:owner},settings:{reference_white_nits:white},valid_pixels:256,sampled_pixels:256,statistics:{linear_rgb:[frame/4,.75-frame/4,.125].map(n=>({minimum:n,maximum:n})),luminance_nits:Object.fromEntries(['minimum','maximum'].map(k=>[k,(.2126*frame/4+.7152*(.75-frame/4)+.0722*.125)*white]))}});
  const input=ref('viewer.input','synthetic native input');
  const viewer={schema:'editbay.native-viewer-scopes.v1',status:'passed',cli:cli('scopes'),native_project:input.path,composition:'synthetic',owner:{version:owner},paused_before:report(0,100),paused_same_frame_new_settings:report(0,200),first_picture_enabled_after_unscoped_preparation:{old_report:null,accepted_report:report(0,203)},prepared_frames:[0,1,2,3].map(frame=>({frame,queued_report:report(frame,200),accepted_refreshed_report:report(frame,203)})),worker_joined:true,stale_owner:{cancelled:true,discarded:true},stale_serial_rejections:1,prepared_queue:{prepared:4,queued:4,discarded:0},display_completions:{events:[0,1,2,3].map(frame=>({frame})),summary:{unique_completed:4,observed_sound_end_us:null}}};
  const independent_reports=[0,1].map(i=>ref('scopes.report.'+i,{report:{sampled_pixels:12,valid_pixels:10}}));
  const independent_packet=ref('scopes.independent',{status:'passed',cases:independent_reports.map(r=>({file:r.path,ranges:'passed',clipping:'passed',all_five_density_grids:'passed'}))});
  const a={schema:1,revision,source_tree:tree,runtime_packet:{...ref('viewer.packet',viewer),native_project:input},compiler_receipts:{viewer:{path:file('scopes.viewer.compiler')},numeric:{path:file('scopes.numeric.compiler')}},raw_logs:{viewer:{path:file('scopes.viewer.log')}},exact_scope_evidence:{independent_packet,independent_reports},limits:['Synthetic offscreen evidence only']};
  const assets=[0,1,2].map(i=>ref('delivery.input.'+i,'synthetic media/font '+i));
  const preservation=Object.fromEntries(['cancel_before_publication','existing_master','existing_sidecar','original_camera_compositions','save_reopen','source_hashes','undo_redo'].map(k=>[k,true]));
  const masters=['fixed','all','follow','captioned'].map((name,i)=>{
    const numeric_oracle={pictures:10,mono_samples:20000,maximum_color_error_bytes:0,maximum_pcm_error:0,changed_caption_pixels:i===3?30:0};
    const master=ref('delivery.'+name+'.master','synthetic master '+name), native_project=ref('delivery.'+name+'.native_project','synthetic project '+name);
    const receipt=ref('delivery.'+name+'.receipt',{numeric_oracle,receipt:{file_sha256:master.sha256,file_bytes:master.bytes,pixel_sha256:hash('pixels'),decoded_pixel_sha256:hash('pixels'),pcm_sha256:hash('pcm'),decoded_pcm_sha256:hash('pcm'),profile:{width:16,height:16,frames:10,sample_rate:48000,channels:['FC']}}});
    return {name,master,native_project,receipt,numeric_oracle};
  });
  const program_scopes=[0,9].map((frame,i)=>{const linear={identity:{frame},statistics:{linear_rgb:(i?[0,1,0]:[1,0,0]).map(n=>({minimum:n,maximum:n})),luminance_nits:{minimum:i?71.52:21.26,maximum:i?71.52:21.26}}};return {frame,linear,same_frame_nits:{identity:{frame},statistics:{luminance_nits:{minimum:linear.statistics.luminance_nits.minimum*2,maximum:linear.statistics.luminance_nits.maximum*2}}}};});
  const delivery={schema:1,status:'passed',cli:cli('multicam'),policy_deliveries:masters.slice(0,3).map(m=>({master:m.master.path,native:m.native_project.path,numeric_oracle:m.numeric_oracle})),captioned_numeric_oracle:masters[3].numeric_oracle,input_assets:assets,preservation,program_scopes};
  const b={schema:1,revision,source_tree:tree,compiler:{file:{path:file('multicam.native.compiler')}},raw_test:{log:{path:file('multicam.native.log')},native_packet:ref('delivery.packet',delivery),wrapper:ref('delivery.wrapper',{exit_code:0,signal:null,revision,args:index.runs['multicam.native'].cargo_args,log:file('multicam.native.log')})},inputs:assets,masters,independent_inspection:{file:ref('delivery.independent',{status:'passed',revision,tool:{sha256:hash('recorded only; do not open executable')},cases:masters.map(m=>({...m.master,video:{codec_name:'png',nb_read_frames:'10'},audio:{codec_name:'pcm_f32le',duration_ts:20000}}))})},preservation:{current_sentinels:[ref('delivery.sentinel.0','owner master'),ref('delivery.sentinel.1','owner sidecar')]},vtt:ref('delivery.vtt','WEBVTT\n\n00:00:00.125 --> 00:00:00.250\nI\n\n'),limitations:['Synthetic producer observations only']};
  index.routes=index.routes.slice(0,24).concat(publicIndex.routes.filter(r=>r.family==='video.followon').map(r=>({...r,revision})));
  const required_routes=index.routes.filter(r=>r.family==='video.followon').map(r=>({id:r.id,commit:revision}));
  f.edit('video',p=>{p.required_routes.splice(4,4,...required_routes.filter(r=>r.id!=='editbay.multicam.caption-delivery.cpu.v1'));});
  put('video.followon.hub',{current_scope_result:{revision},current_multicam_result:{revision},prepared_viewer_scopes_route:{commit:revision},required_routes,prepared_runtime_route:{commit:revision}});
  put('video.followon.e7',a);put('video.followon.7f',b);
  return {...f,inspect:()=>producerEvidence(index,f.roots,{videoOnly:true})};
}

test('video-only joins five original runs; deduplicates two multicam routes and one four-master execution',()=>{
  const f=videoFixture('video-boundaries');
  const r=f.inspect();
  assert.equal(r.integrity_passed,true,JSON.stringify(r.errors));
  assert.equal(r.runs.length,5);
  assert.equal(r.routes.length,5);
  assert.equal(r.generators.length,4);
  assert.deepEqual(r.generators.find(g=>g.id==='editbay.multicam.group.v1').execution_runs,['multicam.core','multicam.app']);
  assert.equal(r.video_observations.delivery.original_tests,1);
  assert.equal(r.video_observations.delivery.masters.length,4);
  assert.equal(r.runs.find(r=>r.id==='multicam.app').counts.ignored,1);
  assert.equal(r.node_sha256,null);
  assert.equal(r.public_candidate,false);
  assert.equal(r.routes.every(r=>r.current_candidate_status==='unavailable'),true);
  assert.equal(r.verified_artifacts.some(a=>a.id.startsWith('tool.')||a.id.startsWith('delivery.case')),false);
  assert.equal(JSON.stringify(r).includes(f.roots.records),false);
  assert.equal(fs.existsSync(path.join(f.roots.records,'target')),false);
  const combined=producerEvidence(f.index,f.roots);
  assert.equal(combined.integrity_passed,true,JSON.stringify(combined.errors));
  assert.equal(combined.routes.length,29);
  assert.equal(combined.cases.length,20);
  assert.equal(combined.runs.find(r=>r.id==='routing.failedd72').status,'failed');
});

test('video source/helper/command/count and retained input/master/reference mutations fail required routes',()=>{
  const checks=[
    f=>{f.index.runs['scopes.viewer'].sources['source.txt']=hash('wrong generator')},
    f=>fs.appendFileSync(f.file('helper.serialized-cargo.mjs'),'changed'),
    f=>fs.rmSync(f.file('video.followon.delivery.input.0')),
    f=>fs.appendFileSync(f.file('video.followon.delivery.fixed.master'),'changed'),
    f=>fs.rmSync(f.file('video.followon.scopes.report.0')),
    f=>f.edit('multicam.native.compiler',r=>{r.executed_cargo_args.push('changed command')}),
    f=>f.put('multicam.native.log',fs.readFileSync(f.file('multicam.native.log'),'utf8').replace('1 passed','0 passed')),
    f=>f.edit('video.followon.7f',r=>{r.revision='f'.repeat(40)}),
    f=>f.edit('video.followon.delivery.packet',r=>{r.cli.fingerprint.sha256=hash('another CLI')}),
    f=>f.edit('video.followon.viewer.packet',r=>{r.worker_joined=false}),
    f=>{f.index.assets['video.followon.delivery.input.0'].root='system'},
  ];
  for(const [i,mutate]of checks.entries()){
    const f=videoFixture('video-reject-'+i);mutate(f);const r=f.inspect();
    assert.equal(r.integrity_passed,false,'mutation '+i);
    assert.equal(r.routes.every(r=>r.status==='unavailable'),true,'routes '+i);
    assert.equal(r.public_candidate,false);
    assert.ok(r.errors.length>0);
  }
});
