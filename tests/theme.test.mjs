import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveTheme } from '../web/theme.js';

for (const { name, preference, prefersDark, expected } of [
  { name: 'system light uses Classic', preference: 'system', prefersDark: false, expected: 'classic' },
  { name: 'system dark uses Classic Dark', preference: 'system', prefersDark: true, expected: 'classic-dark' },
  { name: 'explicit Classic overrides system dark', preference: 'classic', prefersDark: true, expected: 'classic' },
  { name: 'explicit Classic Dark overrides system light', preference: 'classic-dark', prefersDark: false, expected: 'classic-dark' },
  { name: 'explicit Light remains available', preference: 'light', prefersDark: true, expected: 'light' },
  { name: 'explicit Dark remains available', preference: 'dark', prefersDark: false, expected: 'dark' },
]) {
  test(name, () => assert.equal(resolveTheme(preference, prefersDark), expected));
}
