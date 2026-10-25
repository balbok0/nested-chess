import test from 'node:test';
import assert from 'node:assert/strict';
import { apiOrigin, appLink, invitationId, pkceChallenge, randomProof, APP_SCHEME } from '../web/mobile-protocol.js';

for (const { input, expected } of [
  { input: 'https://chess.example.com/', expected: 'https://chess.example.com' },
  { input: 'http://localhost:4180', expected: 'http://localhost:4180' },
  { input: 'http://127.0.0.1:4180/', expected: 'http://127.0.0.1:4180' },
  { input: 'http://[::1]:4180', expected: 'http://[::1]:4180' },
]) test(`mobile server origin ${input}`, () => assert.equal(apiOrigin(input), expected));

for (const input of ['http://chess.example.com', 'https://user:secret@chess.example.com', 'https://chess.example.com/api', 'https://chess.example.com?redirect=elsewhere', 'https://chess.example.com/#fragment', 'file:///server', 'not_a_url']) {
  test(`reject unsupported mobile server ${input}`, () => assert.throws(() => apiOrigin(input)));
}

const id = '12345678-1234-1234-1234-123456789abc';
for (const input of [id, `https://chess.example.com/?match=${id}`, `${APP_SCHEME}://match/${id}`]) {
  test(`invitation accepts ${input}`, () => assert.equal(invitationId(input), id));
}
for (const input of ['', 'https://chess.example.com/', `${APP_SCHEME}://auth?code=x`, 'not_a_match']) {
  test(`invitation rejects ${input}`, () => assert.throws(() => invitationId(input)));
}

test('app authentication callback carries only a handoff code and matching state', () => {
  assert.deepEqual(appLink(`${APP_SCHEME}://auth?code=${'a'.repeat(64)}&state=${'s'.repeat(43)}`), { kind: 'auth', code: 'a'.repeat(64), state: 's'.repeat(43) });
});
test('app match links select the specified match', () => assert.deepEqual(appLink(`${APP_SCHEME}://match/${id}`), { kind: 'match', id }));
for (const input of [`https://auth?code=${'a'.repeat(64)}&state=${'s'.repeat(43)}`, `${APP_SCHEME}://auth/other?code=${'a'.repeat(64)}&state=${'s'.repeat(43)}`, `${APP_SCHEME}://auth?code=short&state=${'s'.repeat(43)}`, `${APP_SCHEME}://match/${id}#auth`, `${APP_SCHEME}://user@match/${id}`, `${APP_SCHEME}://match/${id}?code=anything`]) {
  test(`reject invalid app link ${input}`, () => assert.throws(() => appLink(input)));
}
test('app PKCE matches RFC 7636 S256 example', async () => {
  assert.equal(await pkceChallenge('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk'), 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM');
});
test('app random proof supplies 32 bytes encoded as base64url', () => {
  assert.match(randomProof(), /^[A-Za-z0-9_-]{43}$/);
});
