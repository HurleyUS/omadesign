#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const file = process.argv[2];
const request = JSON.parse(fs.readFileSync(file,'utf8'));
const extension = path.extname(request.input);
let reference = request.reference;
if (extension === '.svg' || extension === '.pdf') {
  const toolId = extension === '.svg' ? 'rsvg-convert' : 'pdftoppm';
  const tool = request.tools[toolId];
  if (!tool?.path) throw Error('Independent renderer required');
  const prefix = path.join(request.output,'reference');
  reference = `${prefix}.png`;
  const args = extension === '.svg' ? ['--format=png','--output',reference,request.reference] : ['-r','72','-singlefile','-png',request.reference,prefix];
  const result = spawnSync(tool.path,args,{encoding:'utf8',timeout:10000,maxBuffer:32768});
  if (result.status !== 0 || result.error || !fs.existsSync(reference)) throw Error('Independent reference rendering failed');
}
const driverRequest = path.join(request.output,'driver.json');
fs.writeFileSync(driverRequest,JSON.stringify({...request,render_reference:reference}));
const args = request.driver === 'libtest' ? ['--exact','formats::suite_qualification::tests::qualification_driver','--ignored','--nocapture'] : ['--qualify-fixture',driverRequest];
const result = spawnSync(request.binary,args,{env:{...process.env,OMA_SUITE_REQUEST:driverRequest},encoding:'utf8',timeout:20000,maxBuffer:32768});
fs.rmSync(driverRequest);
if (result.error) throw result.error;
if (result.status !== 0) process.stderr.write('Product fixture check failed; see result.json and facts.json\n');
process.exitCode = result.status ?? 1;
