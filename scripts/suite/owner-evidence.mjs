import fs from 'node:fs';
import { contained, fingerprint, source } from './contract.mjs';
import { verifyCompilerReceipt } from './build.mjs';

/** Retain stricter owner gates in addition to the shared candidate cohort.
 * Args: required gate IDs, local owner packets, roots and actual build bindings.
 * Returns per-owner verdicts; old source/binary passes remain historical.
 */
export function ownerEvidence(requirements,{directory,roots,builds}) {
  return requirements.map(requirement=>{
    const row={fixture:`owner.${requirement.id}.${requirement.product}`,tier:'headless',status:'unqualified',reason:'required owner acceptance receipt unavailable'};
    try {
      const file=contained(directory,`${requirement.id}.${requirement.product}.json`);
      const packet=JSON.parse(fs.readFileSync(file,'utf8'));
      row.receipt_sha256=fingerprint(file).sha256;
      const actual=source(roots[requirement.product]);
      const build=builds[`${requirement.id}.${requirement.product}`] ?? builds[requirement.product];
      verifyCompilerReceipt(build);
      if (actual.dirty || packet.revision!==actual.revision || packet.tree!==actual.tree || !build?.binary || build.revision!==actual.revision || build.tree!==actual.tree || packet.binary_sha256!==fingerprint(build.binary).sha256 || packet.binary_sha256!==build.sha256) throw Error('historical source/binary binding');
      if (packet.version!==1 || packet.id!==requirement.id || packet.product!==requirement.product || packet.status!=='passed' || !/^[a-f0-9]{64}$/.test(packet.host_sha256) || !Array.isArray(packet.checks) || !packet.checks.length) throw Error('required owner checks/host binding unavailable');
      const artifacts=[];
      for(const check of packet.checks) {
        const proof=check.artifact;
        const actual=fingerprint(proof.path);
        if (check.status!=='passed' || actual.sha256!==proof.sha256 || actual.bytes!==proof.bytes || actual.bytes===0) throw Error('required owner artifact failed/missing/corrupt');
        artifacts.push({check:check.id,...actual});
      }
      if (!requirement.checks.every(id=>packet.checks.some(c=>c.id===id && c.status==='passed'))) throw Error('required owner acceptance check missing');
      return {...row,status:'passed',reason:'current owner acceptance and artifacts verified',source:actual,binary_sha256:packet.binary_sha256,producer_host_sha256:packet.host_sha256,artifacts};
    } catch {return row;}
  });
}
