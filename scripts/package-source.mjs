import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';

const output = 'web/vendor/source';
mkdirSync(output, { recursive: true });
copyFileSync('third_party/fairy-stockfish/source.tar.gz', `${output}/fairy-stockfish.tar.gz`);
copyFileSync('third_party/fairy-stockfish/README.md', `${output}/fairy-stockfish.README.md`);
writeFileSync(`${output}/fairy-stockfish.sha256`, readFileSync('third_party/fairy-stockfish/source.sha256', 'utf8').replace('source.tar.gz', 'fairy-stockfish.tar.gz'));
execFileSync(process.platform === 'darwin' ? 'gtar' : 'tar', [
  '--sort=name', '--mtime=@0', '--owner=0', '--group=0', '--numeric-owner',
  '--exclude=web/vendor', '--exclude=web/pkg', '--exclude=__pycache__', '--exclude=*.pyc',
  '--exclude=mobile/android/.gradle', '--exclude=mobile/android/.idea', '--exclude=mobile/android/local.properties',
  '--exclude=mobile/android/build', '--exclude=*.jks', '--exclude=*.keystore', '--exclude=*.mobileprovision',
  '--exclude=mobile/android/*/build', '--exclude=mobile/android/app/src/main/assets/public',
  '--exclude=mobile/android/capacitor-cordova-android-plugins', '--exclude=mobile/ios/App/App/public',
  '--exclude=mobile/ios/App/DerivedData', '--exclude=mobile/ios/App/CapApp-SPM/.build',
  '-czf', `${output}/nested-chess.tar.gz`,
  '.gitignore', '.gitattributes', 'Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json',
  'pyproject.toml', 'vite.config.js', 'playwright.config.mjs', 'LICENSE', 'LICENSE-MIT', 'LICENSING.md',
  'THIRD_PARTY.md', 'README.md', 'crates', 'web', 'scripts', 'variants', 'tests',
  'docs', 'third_party', 'benchmarks',
  'Dockerfile', '.dockerignore', 'compose.yaml', 'deploy', '.env.server.example',
  'capacitor.config.json', 'mobile', '.github', '.env.mobile.example',
], { stdio: 'inherit' });
