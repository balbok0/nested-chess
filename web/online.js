// SPDX-License-Identifier: GPL-3.0-only
import { request } from './session.js';

const $ = id => document.getElementById(id);
const CLOCKS = { none: null, '15+10': { seconds: 900, increment: 10 }, '10+5': { seconds: 600, increment: 5 }, '30+0': { seconds: 1800, increment: 0 } };
const LOGIN_ERRORS = new Map([['guest_matches', 'Finish your guest matches before switching accounts.'], ['identity_in_use', 'That login already belongs to another account. Sign in to that account instead.']]);

export function setupOnline({ enter, current, leave, onerror }) {
  let account = null, open = [], cursor = null;
  const error = value => { if (!$('online').open) { onerror(value); return; } $('online-error').textContent = value.message || String(value); $('online-error').hidden = false; };
  const clear = () => { $('online-error').hidden = true; };
  async function run(action) {
    clear();
    try { await action(); } catch (value) { error(value); }
  }
  async function refresh() {
    account = await request('/me');
    $('online-account').textContent = account.user ? `${account.user.handle}${account.user.guest ? ' · Guest' : ` · ${Math.round(account.user.rating)}${account.user.rated_games < 20 ? '?' : ''}`}` : 'Sign in or play as a guest.';
    $('online-logout').hidden = !account.user || account.user.guest;
    $('online-profile').hidden = !account.user;
    $('online-handle').value = account.user?.handle || '';
    $('online-queue').disabled = !account.user || account.user.guest;
    $('online-rated').disabled = !account.user || account.user.guest;
    if ($('online-rated').disabled) $('online-rated').checked = false;
    applyRatedSettings();
    $('online-providers').replaceChildren();
    for (const [id, label] of [['google', 'Google'], ['github', 'GitHub'], ['microsoft', 'Microsoft']]) {
      const button = document.createElement('button'); button.type = 'button'; button.className = 'outline-button';
      const linked = account.identities.includes(id);
      button.textContent = linked ? `${label} linked` : `${account.user && !account.user.guest ? 'Link' : 'Sign in with'} ${label}`;
      button.disabled = linked || !account.providers.some(provider => provider.id === id);
      button.onclick = () => { void run(async () => {
        if (account.user && !account.user.guest) {
          const result = await request(`/auth/${id}/link`, { method: 'POST' }); location.assign(result.url);
        } else location.assign(`/auth/${id}`);
      }); };
      $('online-providers').append(button);
    }
    $('online-signin-note').textContent = account.providers.length ? 'Sign in for rated matches. Your first 20 rated games are provisional.' : 'Sign-in is currently unavailable. Guest matches are unrated.';
    open = account.user ? await request('/games') : [];
    $('online-open').replaceChildren();
    for (const info of open) {
      const row = document.createElement('div'); row.className = 'online-game-row';
      const button = document.createElement('button'); button.className = 'text-button';
      const opponent = info.white?.id === account.user.id ? info.black : info.white;
      button.textContent = `${info.status === 'waiting' ? 'Waiting for opponent' : opponent?.handle || 'Match'} · ${info.size} × ${info.size}`;
      button.onclick = () => { void run(() => load(info.id)); }; row.append(button);
      if (info.status === 'waiting') {
        const cancel = document.createElement('button'); cancel.className = 'text-button'; cancel.textContent = 'Cancel';
        cancel.onclick = () => { void run(async () => {
          await request(`/games/${info.id}/commands`, { method: 'POST', data: { request_id: crypto.randomUUID(), expected_seq: info.seq, action: { kind: 'resign' } } }); await refresh();
        }); }; row.append(cancel);
      }
      $('online-open').append(row);
    }
    $('online-open-section').hidden = open.length === 0;
    $('online-history').replaceChildren(); cursor = null;
    $('online-history-more').hidden = true;
    if ($('online-history-section').open) await history();
  }
  async function ensureGuest() {
    if (!account?.user) { await request('/guest', { method: 'POST' }); await refresh(); }
  }
  async function load(id) {
    await ensureGuest();
    const snapshot = await request(`/games/${encodeURIComponent(id)}/join`, { method: 'POST' });
    await enter(snapshot); $('online').close();
  }
  async function history() {
    if (!account?.user) { $('online-history').textContent = 'No online matches yet.'; return; }
    const rows = await request(`/history${cursor === null ? '' : `?before=${cursor}`}`);
    for (const row of rows) {
      const info = row.info, white = info.white.id === account.user.id;
      const result = info.result === 'draw' ? 'Draw' : info.result === (white ? 'white' : 'black') ? 'Won' : 'Lost';
      const opponent = white ? info.black : info.white;
      const button = document.createElement('button'); button.className = 'online-game-row';
      const text = document.createElement('span'); text.textContent = `${result} vs ${opponent.handle} · ${info.size} × ${info.size} · ${new Date(info.created_ms).toLocaleDateString()}`;
      const rating = document.createElement('span');
      if (row.rating_after !== null) { const delta = Math.round(row.rating_after - row.rating_before); rating.textContent = `${Math.round(row.rating_after)} (${delta >= 0 ? '+' : ''}${delta})`; }
      else rating.textContent = 'Unrated';
      button.append(text, rating); button.onclick = () => { void run(async () => { const snapshot = await request(`/games/${info.id}`); await enter(snapshot); $('online').close(); }); };
      $('online-history').append(button);
    }
    if (!$('online-history').children.length) $('online-history').textContent = 'No completed online matches yet.';
    cursor = rows.at(-1)?.cursor ?? cursor;
    $('online-history-more').hidden = rows.length < 20;
  }
  async function show() { $('online').showModal(); await run(refresh); }
  $('online-open-button').onclick = () => { void show(); };
  $('online-close').onclick = () => $('online').close();
  $('online-profile').onsubmit = event => { event.preventDefault(); void run(async () => { await request('/profile', { method: 'PATCH', data: { handle: $('online-handle').value } }); await refresh(); }); };
  $('online-logout').onclick = () => { void run(async () => { leave(); await request('/logout', { method: 'POST' }); await refresh(); }); };
  $('online-create').onsubmit = event => { event.preventDefault(); void run(async () => {
    await ensureGuest();
    const spec = { size: Number($('online-size').value), layout: $('online-layout').value, rated: $('online-rated').checked, clock: CLOCKS[$('online-clock').value] };
    const snapshot = await request('/games', { method: 'POST', data: spec }); await enter(snapshot); $('online').close();
  }); };
  function applyRatedSettings() {
    const rated = $('online-rated').checked;
    if (rated) { $('online-size').value = '2'; $('online-layout').value = 'classical'; $('online-clock').value = '15+10'; }
    for (const id of ['online-size', 'online-layout', 'online-clock']) $(id).disabled = rated;
  }
  $('online-rated').onchange = applyRatedSettings;
  $('online-queue').onclick = () => { void run(async () => { const snapshot = await request('/queue', { method: 'POST' }); await enter(snapshot); $('online').close(); }); };
  $('online-join').onsubmit = event => { event.preventDefault(); void run(async () => {
    const input = $('online-invite').value.trim(); let id = input;
    try { id = new URL(input, location.href).searchParams.get('match') || input; } catch {}
    await load(id);
  }); };
  $('online-history-section').ontoggle = () => { if ($('online-history-section').open && !cursor) void run(history); };
  $('online-history-more').onclick = () => { void run(history); };
  $('online-leave').onclick = leave;
  $('online-resign').onclick = () => { void run(async () => {
    const session = current(); if (session.kind !== 'online') return;
    $('online-confirm-title').textContent = session.info.status === 'waiting' ? 'Cancel invitation?' : 'Resign this match?';
    $('online-confirm').showModal();
  }); };
  $('online-confirm-cancel').onclick = () => $('online-confirm').close();
  $('online-confirm-yes').onclick = () => { $('online-confirm').close(); void run(async () => { await current().send({ kind: 'resign' }); }); };
  $('online-resume').onclick = () => { void current().send({ kind: 'resume' }).catch(error); };
  $('online-copy').onclick = () => { void navigator.clipboard.writeText($('online-link').value).then(() => { $('online-copy').textContent = 'Copied'; }).catch(() => { $('online-link').select(); }); };
  return {
    show,
    async restore(canEnter = () => true) {
      const params = new URLSearchParams(location.search);
      if (params.has('auth_error')) { await show(); error(new Error(LOGIN_ERRORS.get(params.get('auth_error')) || 'Sign-in was not completed. Please try again.')); return; }
      if (params.has('match')) {
        const id = params.get('match');
        try { const snapshot = await request(`/games/${encodeURIComponent(id)}`); if (canEnter()) await enter(snapshot); return; }
        catch { if (canEnter()) { await show(); $('online-invite').value = id; } return; }
      }
      if (params.has('online')) { await show(); return; }
      const id = sessionStorage.getItem('nested-online-match');
      if (id) { try { await refresh(); const snapshot = await request(`/games/${id}`); if (canEnter()) await enter(snapshot); } catch { sessionStorage.removeItem('nested-online-match'); } }
    },
  };
}

export function renderOnline(session) {
  const online = session?.kind === 'online'; $('online-match').hidden = !online;
  if (!online) return;
  const info = session.info;
  $('online-white').textContent = info.white?.handle || 'Open seat'; $('online-black').textContent = info.black?.handle || 'Open seat';
  $('online-connection').textContent = session.pending ? 'Confirming move…' : !session.connected ? 'Reconnecting…' : `You play ${session.seat === 'white' ? 'White' : 'Black'} · ${info.rated ? 'Rated' : 'Unrated'}`;
  const now = Date.now() + session.serverOffset;
  for (const color of ['white', 'black']) {
    const element = $(`online-clock-${color}`), stored = info[`${color}_ms`];
    element.hidden = stored === null;
    if (stored === null) continue;
    const seconds = Math.ceil(Math.max(0, stored - (info.turn === color && info.running_since_ms !== null ? Math.max(0, now - info.running_since_ms) : 0)) / 1000);
    element.textContent = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
    element.classList.toggle('running', info.status === 'active' && info.turn === color);
  }
  $('online-share').hidden = info.status !== 'waiting';
  const url = new URL(location.href); url.search = ''; url.hash = ''; url.searchParams.set('match', info.id); $('online-link').value = url.href;
  $('online-resume').hidden = info.status !== 'paused';
  $('online-resume').disabled = !session.connected || info[`ready_${session.seat}`];
  $('online-resume').textContent = info[`ready_${session.seat}`] ? 'Ready' : 'Resume match';
  $('online-resign').hidden = ['finished', 'cancelled'].includes(info.status);
  $('online-resign').disabled = !session.connected || Boolean(session.pending);
  $('online-resign').textContent = info.status === 'waiting' ? 'Cancel invitation' : 'Resign';
}
