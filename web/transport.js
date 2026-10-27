// SPDX-License-Identifier: GPL-3.0-only
import { native, nativeRequest, saveSession, clearSession, onlineOrigin } from './platform.js';

export class ApiError extends Error {
  constructor(message, status) { super(message); this.status = status; }
}

export function isTransientError(error) {
  return !(error instanceof ApiError) || error.status === 0 || [408, 429].includes(error.status) || error.status >= 500;
}

export async function request(path, { method = 'GET', data } = {}) {
  if (native) {
    const response = await nativeRequest(path, method, data);
    if (response.status === 204) return null;
    if (response.status < 200 || response.status >= 300) {
      if (response.status === 401 && path !== '/mobile/exchange') await clearSession();
      throw new ApiError(response.data?.error || `Request failed (${response.status}).`, response.status);
    }
    if (!response.data || typeof response.data !== 'object') throw new ApiError('Online play is unavailable here.', response.status);
    if (path === '/me' && response.data.user === null) await clearSession();
    return response.data;
  }
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15_000);
  try {
    const response = await fetch(`/api/v1${path}`, {
      method, credentials: 'same-origin', signal: controller.signal,
      ...(data === undefined ? {} : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) }),
    });
    if (response.status === 204) return null;
    const body = await response.json().catch(() => null);
    if (!response.ok) throw new ApiError(body?.error || `Request failed (${response.status}).`, response.status);
    if (body === null) throw new ApiError('Online play is unavailable here.', response.status);
    return body;
  } finally { clearTimeout(timeout); }
}

export async function guestSession() {
  if (!native) return request('/guest', { method: 'POST' });
  const session = await request('/mobile/guest', { method: 'POST' });
  await saveSession(session.token);
  return session.user;
}
export async function logoutSession() {
  await request('/logout', { method: 'POST' });
  if (native) await clearSession();
}
export async function streamURL(id) {
  const path = native ? `/mobile/games/${id}/stream` : `/games/${id}/stream`;
  const url = new URL(`/api/v1${path}`, onlineOrigin());
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  if (native) {
    const { ticket } = await request(`/mobile/games/${id}/ticket`, { method: 'POST' });
    url.searchParams.set('ticket', ticket);
  }
  return url;
}
