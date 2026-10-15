import { spawn } from 'node:child_process';
import fs from 'node:fs';

/** Run a bounded process group and reap its leader. Args: executable, argv, environment/cwd/deadline. Returns exit and resource-independent output status; descendant groups are always killed. */
export function runProcess(executable, args, { cwd, env, deadline_ms, output_bytes = 65536 }) {
  return new Promise(resolve => {
    const child = spawn(executable, args, { cwd, env, detached: true, stdio: ['ignore','pipe','pipe'] });
    let count = 0, output = '', reason = null, closed = false;
    let parent_rss_kib = 0, worker_rss_kib = 0;
    const sample = () => {
      const visited = new Set();
      const rss = pid => {
        if (!pid || visited.has(pid)) return 0;
        visited.add(pid);
        try {
          const status=fs.readFileSync(`/proc/${pid}/status`,'utf8');
          let total=Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0);
          for (const child of fs.readFileSync(`/proc/${pid}/task/${pid}/children`,'utf8').trim().split(/\s+/).filter(Boolean)) total+=rss(Number(child));
          return total;
        } catch {return 0;}
      };
      if (child.pid) {
        try {parent_rss_kib=Math.max(parent_rss_kib,Number(fs.readFileSync(`/proc/${child.pid}/status`,'utf8').match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0));}catch {}
        worker_rss_kib=Math.max(worker_rss_kib,rss(child.pid));
      }
    };
    const sampling=setInterval(sample,20);
    const kill = () => { if (child.pid) { try { process.kill(-child.pid, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') reason ??= 'process cleanup failed'; } } };
    const cancel = () => { reason = 'cancelled'; kill(); };
    process.once('SIGTERM', cancel);
    process.once('SIGINT', cancel);
    const timer = setTimeout(() => { reason = 'deadline exceeded'; kill(); }, deadline_ms);
    const collect = bytes => { count += bytes.length; if (count > output_bytes) { reason = 'output limit exceeded'; kill(); } else output += bytes; };
    child.stdout.on('data', collect);
    child.stderr.on('data', collect);
    child.on('error', () => { reason = 'executable unavailable'; });
    child.on('exit', () => kill());
    child.on('close', (code, signal) => {
      if (closed) return;
      closed = true;
      clearTimeout(timer);
      clearInterval(sampling);
      process.removeListener('SIGTERM', cancel);
      process.removeListener('SIGINT', cancel);
      kill();
      resolve({ code, signal, reason, output, memory:{parent_rss_kib:parent_rss_kib || null,process_tree_rss_kib:worker_rss_kib || null,sampling_ms:20,boundary:'sampled live RSS; may miss short peaks; not allocation-budget or benchmark proof'} });
    });
  });
}
