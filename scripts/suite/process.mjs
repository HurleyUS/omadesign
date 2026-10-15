import { spawn } from 'node:child_process';

/** Run a bounded process group and reap its leader. Args: executable, argv, environment/cwd/deadline. Returns exit and resource-independent output status; descendant groups are always killed. */
export function runProcess(executable, args, { cwd, env, deadline_ms, output_bytes = 65536 }) {
  return new Promise(resolve => {
    const child = spawn(executable, args, { cwd, env, detached: true, stdio: ['ignore','pipe','pipe'] });
    let count = 0, output = '', reason = null, closed = false;
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
      process.removeListener('SIGTERM', cancel);
      process.removeListener('SIGINT', cancel);
      kill();
      resolve({ code, signal, reason, output });
    });
  });
}
