#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fingerprint } from './contract.mjs';

const request=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const checks={};
const binding=request.verified_binding;
for (const check of request.fixture.required) {
  const artifacts=[];
  let passed=false;
  try {
    if (check==='appearance') {
      const refs=request.fixture.ui.appearance_references;
      const records=[];
      for (const ref of refs) {
        const capture=request.ui_captures?.[String(ref.scale)];
        if (!capture || !/^[a-f0-9]{64}$/.test(capture.host_sha256) || fingerprint(capture.path).sha256!==ref.sha256 || capture.font_sha256!==request.fixture.ui.font_sha256 || capture.theme_sha256!==request.fixture.ui.theme_sha256) throw Error('Pinned appearance capture unavailable');
        records.push({scale:ref.scale,...fingerprint(capture.path),producer_host_sha256:capture.host_sha256});
      }
      if (!request.fixture.ui.font_sha256 || !request.fixture.ui.theme_sha256 || records.length!==3) throw Error('Actual font/theme/scale binding unavailable');
      fs.writeFileSync(path.join(request.output,'appearance.json'),JSON.stringify({comparison:'exact encoded PNG bytes against pinned reference; conservative equality',captures:records,boundary:'headless appearance only; no functionality/native-window/human proof'}));
      artifacts.push('appearance.json');
      passed=true;
    } else {
      const file=request.ui_receipts?.[check];
      if (!file) throw Error('Required app runtime receipt unavailable');
      const record=JSON.parse(fs.readFileSync(file,'utf8'));
      if (record.check!==check || record.status!=='passed' || !/^[a-f0-9]{64}$/.test(record.host_sha256) || record.revision!==binding.source.revision || record.binary_sha256!==binding.binary.sha256 || record.fixture_sha256!==binding.inputs.input.sha256) throw Error('App receipt binding mismatch');
      const actual=fingerprint(record.artifact.path);
      if (actual.sha256!==record.artifact.sha256 || actual.bytes!==record.artifact.bytes || actual.bytes>4194304) throw Error('App receipt artifact unavailable/corrupt');
      const filename=`${check}.proof`;
      fs.copyFileSync(record.artifact.path,path.join(request.output,filename));
      fs.writeFileSync(path.join(request.output,`${check}.json`),JSON.stringify({check,receipt_sha256:fingerprint(file).sha256,artifact:actual,producer_host_sha256:record.host_sha256,boundary:'exact source/test-executable app assertion; appearance and native-window proof separate'}));
      artifacts.push(filename,`${check}.json`);
      passed=true;
    }
  } catch {}
  checks[check]={status:passed?'passed':'unavailable',artifacts};
}
fs.writeFileSync(path.join(request.output,'result.json'),JSON.stringify({checks}));
process.exitCode=Object.values(checks).every(c=>c.status==='passed')?0:1;
