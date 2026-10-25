import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const run = (command, args, options = {}) => execFileSync(command, args, { stdio: 'inherit', ...options });
if (execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { encoding: 'utf8' }).trim()) {
  throw new Error('Commit tracked source changes before publishing.');
}
run('npm', ['run', 'build', '--', '--base=/nested-chess/']);
run('npm', ['test']);
const parent = join('scratch', 'claude', `${new Date().toISOString().slice(0, 10).replaceAll('-', '')}_github-pages`);
mkdirSync(parent, { recursive: true });
const checkout = join(parent, `site-${Date.now()}`);
const published = execFileSync('git', ['ls-remote', '--heads', 'origin', 'gh-pages'], { encoding: 'utf8' }).trim();
if (published) {
  run('git', ['fetch', 'origin', 'gh-pages']);
  run('git', ['worktree', 'add', '--detach', checkout, 'FETCH_HEAD']);
} else {
  run('git', ['worktree', 'add', '--detach', checkout, 'HEAD']);
  run('git', ['-C', checkout, 'checkout', '--orphan', 'gh-pages']);
}
try {
  run('git', ['-C', checkout, 'rm', '-rf', '--ignore-unmatch', '.']);
  cpSync('dist', checkout, { recursive: true });
  cpSync('LICENSE', join(checkout, 'LICENSE'));
  writeFileSync(join(checkout, '.nojekyll'), '');
  run('git', ['-C', checkout, 'add', '.']);
  if (spawnSync('git', ['-C', checkout, 'diff', '--cached', '--quiet']).status !== 0) {
    run('git', ['-C', checkout, 'commit', '-m', `Deploy ${execFileSync('git', ['rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim()}`]);
  }
  run('git', ['-C', checkout, 'push', 'origin', 'HEAD:gh-pages']);
} finally {
  run('git', ['worktree', 'remove', '--force', checkout]);
}
