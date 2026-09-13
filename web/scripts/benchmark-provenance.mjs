import { execFileSync } from 'node:child_process';

function output(program, args, cwd) {
  try {
    return execFileSync(program, args, { cwd, encoding: 'utf8', timeout: 5000, maxBuffer: 65536, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  } catch {
    return null;
  }
}

export function benchmarkProvenance(cwd) {
  const commit = output('git', ['rev-parse', '--verify', 'HEAD'], cwd);
  const status = output('git', ['status', '--porcelain', '--untracked-files=normal'], cwd);
  return {
    source: { commit, dirty: status === null ? null : status !== '' },
    toolchain: {
      rustc: output('rustc', ['--version', '--verbose'], cwd),
      cargo: output('cargo', ['--version'], cwd),
      node: process.version,
    },
  };
}
