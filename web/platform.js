// SPDX-License-Identifier: GPL-3.0-only
import { Capacitor, CapacitorHttp, SystemBars, SystemBarsStyle } from '@capacitor/core';
import { apiOrigin, appLink, pkceChallenge, randomProof } from './mobile-protocol.js';

export const native = Capacitor.isNativePlatform();
export const botsAvailable = !native && import.meta.env.MODE !== 'mobile';
const configuredOrigin = import.meta.env.VITE_API_ORIGIN;
let storagePromise;
export async function setNativeAppearance(theme) {
  if (native) await SystemBars.setStyle({ style: theme.endsWith('dark') ? SystemBarsStyle.Dark : SystemBarsStyle.Light });
}
let saves = Promise.resolve();
export async function hydrateLocal(keys) {
  if (!native) return;
  const { Preferences } = await import('@capacitor/preferences');
  for (const key of keys) {
    const { value } = await Preferences.get({ key });
    if (value !== null) localStorage.setItem(key, value);
  }
}
export function saveLocal(key, value) {
  if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value);
  if (!native) return Promise.resolve();
  const result = saves.then(async () => {
    const { Preferences } = await import('@capacitor/preferences');
    if (value === null) await Preferences.remove({ key }); else await Preferences.set({ key, value });
  });
  saves = result.catch(() => {});
  return result;
}
async function storage() {
  if (!native) throw new Error('Native session storage is unavailable in a browser.');
  if (!storagePromise) storagePromise = (async () => {
    const { SecureStorage, KeychainAccess } = await import('@aparajita/capacitor-secure-storage');
    await SecureStorage.setKeyPrefix(`nested-chess:${onlineOrigin()}:`);
    await SecureStorage.setSynchronize(false);
    await SecureStorage.setDefaultKeychainAccess(KeychainAccess.whenUnlockedThisDeviceOnly);
    return SecureStorage;
  })();
  return storagePromise;
}
export function onlineOrigin() {
  if (!native) return location.origin;
  if (!configuredOrigin) throw new Error('Online play needs a server address configured for this app build.');
  return apiOrigin(configuredOrigin);
}
export function rememberedMatch() {
  const id = localStorage.getItem('nested-online-match') || (!native ? sessionStorage.getItem('nested-online-match') : null);
  if (id && !native) { localStorage.setItem('nested-online-match', id); sessionStorage.removeItem('nested-online-match'); }
  return id;
}
export function rememberMatch(id) {
  if (!native) sessionStorage.removeItem('nested-online-match');
  return saveLocal('nested-online-match', id);
}
export async function saveSession(token) { await (await storage()).set('session', token); }
export async function clearSession() { await (await storage()).remove('session'); }
export async function nativeRequest(path, method, data) {
  const token = await (await storage()).get('session');
  return CapacitorHttp.request({
    url: `${onlineOrigin()}/api/v1${path}`, method,
    headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), ...(data === undefined ? {} : { 'Content-Type': 'application/json' }) },
    ...(data === undefined ? {} : { data }),
    responseType: 'json', connectTimeout: 15_000, readTimeout: 15_000, disableRedirects: true,
  });
}

export async function nativeLogin(provider, linking, request) {
  const verifier = randomProof(), state = randomProof();
  const challenge = await pkceChallenge(verifier);
  const result = await request(`/mobile/auth/${provider}`, { method: 'POST', data: { challenge, state, linking } });
  const url = new URL(result.url);
  if (url.origin !== onlineOrigin() || url.pathname !== `/auth/mobile/${provider}`) throw new Error('Invalid sign-in address.');
  await (await storage()).set('login', { verifier, state, expires: Date.now() + 600_000 });
  const { Browser } = await import('@capacitor/browser');
  await Browser.open({ url: url.href });
}

export async function initializeMobile({ request, onlogin, onmatch, onback, onerror }) {
  if (!native) return false;
  document.documentElement.dataset.native = Capacitor.getPlatform();
  const { App } = await import('@capacitor/app');
  let handling = false;
  async function handle(url) {
    if (handling) return;
    handling = true;
    try {
      const link = appLink(url);
      if (link.kind === 'match') { await onmatch(link.id); return; }
      const secure = await storage();
      const handoff = await pkceChallenge(link.code);
      if (await secure.get('last-handoff') === handoff) return;
      const pending = await secure.get('login');
      if (!pending || pending.expires <= Date.now() || pending.state !== link.state) throw new Error('Sign-in did not match this app. Please try again.');
      let result;
      try { result = await request('/mobile/exchange', { method: 'POST', data: { code: link.code, verifier: pending.verifier } }); }
      catch (error) {
        if (error.status >= 400 && error.status < 500) { await secure.set('last-handoff', handoff); await secure.remove('login'); }
        throw error;
      }
      await saveSession(result.token);
      await secure.set('last-handoff', handoff);
      await secure.remove('login');
      const { Browser } = await import('@capacitor/browser');
      await Browser.close();
      await onlogin();
    } catch (error) { onerror(error); }
    finally { handling = false; }
  }
  await App.addListener('appUrlOpen', ({ url }) => { void handle(url); });
  await App.addListener('appStateChange', ({ isActive }) => window.dispatchEvent(new CustomEvent('native-state', { detail: { active: isActive } })));
  if (Capacitor.getPlatform() === 'android') await App.addListener('backButton', () => {
    const dialog = [...document.querySelectorAll('dialog[open]')].at(-1);
    if (dialog) dialog.close();
    else if (!onback()) void App.minimizeApp();
  });
  document.addEventListener('click', event => {
    const link = event.target.closest('a[href]');
    if (!link || !/^https?:$/.test(new URL(link.href).protocol) || new URL(link.href).origin === location.origin) return;
    event.preventDefault();
    void import('@capacitor/browser').then(({ Browser }) => Browser.open({ url: link.href })).catch(onerror);
  });
  const launch = await App.getLaunchUrl();
  if (launch?.url) { await handle(launch.url); return true; }
  return false;
}

export async function copyText(text) {
  if (!native) return navigator.clipboard.writeText(text);
  const { Clipboard } = await import('@capacitor/clipboard');
  await Clipboard.write({ string: text });
}
export async function shareInvite(url) {
  const { Share } = await import('@capacitor/share');
  await Share.share({ title: 'Nested chess', url });
}
export async function shareSave(data) {
  const [{ Filesystem, Directory, Encoding }, { Share }] = await Promise.all([import('@capacitor/filesystem'), import('@capacitor/share')]);
  const { uri } = await Filesystem.writeFile({ path: 'nested-chess.json', directory: Directory.Cache, encoding: Encoding.UTF8, data });
  await Share.share({ title: 'Nested chess match', files: [uri] });
}
