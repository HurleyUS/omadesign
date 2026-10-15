#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { buildBinding } from './build.mjs';
import { canonical, hash, source } from './contract.mjs';
import { runProcess } from './process.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const [mode,configFile,product,revision,output]=process.argv.slice(2);
const recipes={
  omatainer:[{args:['test','--locked','--bin','omatainer','--no-run'],targets:{omatainer:['audio-tone-codecs','audio-native-session','audio-groove-members']}}],
  editbay:[
    {args:['test','--locked','-p','editbay-media','--test','delivery','--no-run'],targets:{delivery:['video-fractional-rgba-pcm']}},
    {args:['test','--locked','-p','editbay-core','--test','timeline','--test','archive','--no-run'],targets:{timeline:['video-editable-timeline'],archive:['video-movable-archive']}},
  ],
};
if (!['plan','run','inside'].includes(mode) || !recipes[product] || !/^[a-f0-9]{40}$/.test(revision ?? '')) throw Error('Use serialized-adapters.mjs plan|run CONFIG omatainer|editbay FULL_SHA /home/.../NEW_OUTPUT');
const configBytes=fs.readFileSync(configFile);
const config=JSON.parse(configBytes);
if (!config.lock?.startsWith('/home/') || !config.lock.startsWith(`${path.resolve(config.scratch)}/`)) throw Error('Canonical global build configuration required');
if (mode==='plan') {
  console.log(JSON.stringify({product,revision,recipes:recipes[product],policy:'One outer global lock; helper --inside builds, then actual adapter/test execution before release. Build-only is not execution proof.'},null,2));
} else if (mode==='run') {
  if (!output?.startsWith('/home/') || fs.existsSync(output)) throw Error('A new project-local output directory is required');
  const result=spawnSync('flock',[config.lock,process.execPath,fileURLToPath(import.meta.url),'inside',configFile,product,revision,output],{stdio:'inherit'});
  if(result.error)throw result.error;
  process.exitCode=result.status ?? 1;
} else {
  let pid=process.ppid,held=false;
  for(let n=0;pid>1 && n<32;n++) {
    const args=fs.readFileSync(`/proc/${pid}/cmdline`,'utf8').split('\0').filter(Boolean);
    if(path.basename(args[0])==='flock' && args[1]===config.lock)held=true;
    pid=Number(fs.readFileSync(`/proc/${pid}/status`,'utf8').match(/^PPid:\s+(\d+)/m)?.[1] ?? 0);
  }
  if(!held)throw Error('No existing global lock ancestor; refusing build/execution');
  if (!output?.startsWith('/home/'))throw Error('Project-local output required');
  fs.mkdirSync(output,{recursive:false,mode:0o700});
  const builds={};
  for(const [i,recipe] of recipes[product].entries()) {
    const args=['--inside',configFile,product,revision,...recipe.args];
    const result=await runProcess(process.execPath,[path.join(root,'scripts/serialized-cargo.mjs'),...args],{cwd:root,env:process.env,deadline_ms:3600000,output_bytes:4*1024*1024});
    fs.writeFileSync(path.join(output,`build-${i}.log`),result.output);
    const receiptFile=[...result.output.matchAll(/^Cargo receipt: (.+)$/gm)].at(-1)?.[1].trim();
    if(result.code!==0 || result.reason || !receiptFile)throw Error('Serialized build did not produce a successful immutable compiler receipt');
    const r=JSON.parse(fs.readFileSync(receiptFile,'utf8'));
    if(r.repository!==product || r.revision!==revision || r.config_sha256!==hash(configBytes) || canonical(r.cargo_args)!==canonical(recipe.args))throw Error('Compiler recipe/source/config binding mismatch');
    for(const [target,ids] of Object.entries(recipe.targets)) {
      const artifacts=r.executables.filter(e=>e.target.name===target && e.profile.test);
      if(artifacts.length!==1)throw Error('Expected exactly one compiler-emitted test target');
      const binding=buildBinding(config,receiptFile,artifacts[0].sha256);
      for(const id of ids)builds[id]=binding;
    }
  }
  const runtimeRoot=config.repositories[product].worktree;
  if(source(runtimeRoot).revision!==revision || source(runtimeRoot).dirty)throw Error('Committed runtime fixture checkout changed');
  const rootsFile=path.join(output,'roots.json'),buildsFile=path.join(output,'builds.json');
  fs.writeFileSync(rootsFile,JSON.stringify({[product]:runtimeRoot}),{mode:0o600});
  fs.writeFileSync(buildsFile,JSON.stringify(builds,null,2),{mode:0o600});
  const result=await runProcess(process.execPath,[path.join(root,'scripts/qualify-suite.mjs'),'run','--lane','baseline','--product',product,'--roots',rootsFile,'--builds',buildsFile,'--out',path.join(output,'receipts'),'--config',configFile,'--inside','locked'],{cwd:root,env:process.env,deadline_ms:300000,output_bytes:65536});
  fs.writeFileSync(path.join(output,'adapter.log'),result.output);
  process.stdout.write(result.output);
  process.exitCode=result.reason?1:result.code ?? 1;
}
