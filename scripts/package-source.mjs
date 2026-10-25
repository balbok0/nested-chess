import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';

const output = 'web/vendor/source';
mkdirSync(output, { recursive: true });
copyFileSync('third_party/fairy-stockfish/source.tar.gz', `${output}/fairy-stockfish.tar.gz`);
copyFileSync('third_party/fairy-stockfish/README.md', `${output}/fairy-stockfish.README.md`);
writeFileSync(`${output}/fairy-stockfish.sha256`, readFileSync('third_party/fairy-stockfish/source.sha256', 'utf8').replace('source.tar.gz', 'fairy-stockfish.tar.gz'));
execFileSync('tar', [
  '--sort=name', '--mtime=@0', '--owner=0', '--group=0', '--numeric-owner',
  '--exclude=web/vendor', '--exclude=web/pkg', '--exclude=__pycache__', '--exclude=*.pyc',
  '-czf', `${output}/nested-chess.tar.gz`,
  '.gitignore', 'Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json',
  'pyproject.toml', 'vite.config.js', 'LICENSE', 'LICENSE-MIT', 'LICENSING.md',
  'THIRD_PARTY.md', 'README.md', 'rules.md', 'crates', 'web', 'scripts', 'variants', 'tests',
  'docs', 'third_party',
], { stdio: 'inherit' });
