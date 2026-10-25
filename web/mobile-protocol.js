// SPDX-License-Identifier: GPL-3.0-only
export const APP_SCHEME = 'io.github.balbok0.nestedchess';
const MATCH_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function apiOrigin(value) {
  const url = new URL(value);
  if (url.username || url.password || url.pathname !== '/' || url.search || url.hash
      || !(url.protocol === 'https:' || (url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)))) {
    throw new Error('The online server must be an HTTPS origin (HTTP is allowed for localhost).');
  }
  return url.origin;
}

export function appLink(value) {
  const url = new URL(value);
  if (url.protocol !== `${APP_SCHEME}:` || url.username || url.password || url.port || url.hash) throw new Error('Invalid app link.');
  if (url.hostname === 'auth' && !url.pathname) {
    const code = url.searchParams.get('code'), state = url.searchParams.get('state');
    if (/^[0-9a-f]{64}$/.test(code || '') && /^[A-Za-z0-9_-]{43}$/.test(state || '')) return { kind: 'auth', code, state };
  }
  if (url.hostname === 'match' && !url.search && MATCH_ID.test(url.pathname.slice(1))) return { kind: 'match', id: url.pathname.slice(1) };
  throw new Error('Invalid app link.');
}

export function invitationId(value) {
  let id = value.trim();
  try {
    const url = new URL(id);
    id = url.protocol === `${APP_SCHEME}:` ? appLink(id).id : url.searchParams.get('match');
  } catch { /* A bare match ID is accepted below. */ }
  if (!MATCH_ID.test(id || '')) throw new Error('Enter an invitation link or match ID.');
  return id;
}

export function randomProof() {
  return base64url(crypto.getRandomValues(new Uint8Array(32)));
}
function base64url(bytes) {
  return btoa(String.fromCharCode(...bytes)).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
}
export async function pkceChallenge(verifier) {
  return base64url(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))));
}
