// SPDX-License-Identifier: GPL-3.0-only
import { ApiError, isTransientError, request, guestSession, logoutSession } from './transport.js';
import { native, nativeLogin, rememberedMatch, rememberMatch, saveLocal, onlineOrigin, copyText, shareInvite } from './platform.js';
import { APP_SCHEME, invitationId } from './mobile-protocol.js';

const $ = id => document.getElementById(id);
const CLOCKS = { none: null, '15+10': { seconds: 900, increment: 10 }, '10+5': { seconds: 600, increment: 5 }, '30+0': { seconds: 1800, increment: 0 } };
const CREATION_KEY = 'nested-online-creation';
const LOGIN_ERRORS = new Map([['guest_matches', 'Finish your guest matches before switching accounts.'], ['identity_in_use', 'That login already belongs to another account. Sign in to that account instead.']]);

export function setupOnline({ enter, current, leave, onerror }) {
  let account = null, open = [], cursor = null, running = false;
  const pendingCreation = () => {
    const value = localStorage.getItem(CREATION_KEY);
    if (!value) return null;
    const creation = JSON.parse(value);
    return creation.origin === onlineOrigin() && (!account?.user || creation.owner === account.user.id) ? creation : null;
  };
  const error = value => { if (!$('online').open) { onerror(value); return; } $('online-error').textContent = value.message || String(value); $('online-error').hidden = false; };
  const clear = () => { $('online-error').hidden = true; };
  async function run(action) {
    if (running) return;
    running = true;
    $('online-controls').disabled = true;
    $('online-controls').setAttribute('aria-busy', 'true');
    clear();
    try { await action(); } catch (value) { error(value); }
    finally {
      running = false;
      $('online-controls').disabled = false;
      $('online-controls').setAttribute('aria-busy', 'false');
      applyRatedSettings();
    }
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
        if (native) await nativeLogin(id, Boolean(account.user && !account.user.guest), request);
        else if (account.user && !account.user.guest) {
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
      button.textContent = `${info.status === 'waiting' ? info.queue_started_ms != null ? 'Finding an opponent' : 'Waiting for opponent' : opponent?.handle || 'Match'} · ${info.size} × ${info.rows ?? info.size}`;
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
    if (!account?.user) { await guestSession(); await refresh(); }
  }
  async function load(id) {
    cancelRecovery();
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
      const text = document.createElement('span'); text.textContent = `${result} vs ${opponent.handle} · ${info.size} × ${info.rows ?? info.size} · ${new Date(info.created_ms).toLocaleDateString()}`;
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
  $('online-logout').onclick = () => { void run(async () => { leave(); await logoutSession(); await refresh(); }); };
  $('online-create').onsubmit = event => { event.preventDefault(); void run(async () => {
    await ensureGuest();
    cancelRecovery();
    let creation = pendingCreation();
    if (!creation) {
      const [columns, rows] = $('online-size').value.split('x').map(Number);
      creation = { owner: account.user.id, origin: onlineOrigin(), spec: {
        request_id: crypto.randomUUID(), size: columns, rows, layout: $('online-layout').value,
        rated: $('online-rated').checked, clock: CLOCKS[$('online-clock').value],
      } };
      await saveLocal(CREATION_KEY, JSON.stringify(creation));
    }
    try {
      const snapshot = await request('/games', { method: 'POST', data: creation.spec });
      await enter(snapshot);
      await saveLocal(CREATION_KEY, null);
      $('online').close();
    } catch (value) {
      if (!isTransientError(value)) await saveLocal(CREATION_KEY, null);
      throw value;
    }
  }); };
  function applyRatedSettings() {
    const creation = pendingCreation();
    if (creation) {
      $('online-size').value = `${creation.spec.size}x${creation.spec.rows ?? creation.spec.size}`;
      $('online-layout').value = creation.spec.layout;
      $('online-clock').value = Object.keys(CLOCKS).find(key => JSON.stringify(CLOCKS[key]) === JSON.stringify(creation.spec.clock)) || 'none';
      $('online-rated').checked = creation.spec.rated;
    }
    $('online-create-submit').textContent = creation ? 'Retry invitation' : 'Create invitation';
    $('online-create-status').textContent = creation ? 'Invitation not confirmed. Retry to recover the same match.' : '';
    $('online-rated').disabled = Boolean(creation) || !account?.user || account.user.guest;
    const rated = $('online-rated').checked;
    if (rated) { $('online-size').value = '2x2'; $('online-layout').value = 'classical'; $('online-clock').value = '15+10'; }
    for (const id of ['online-size', 'online-layout', 'online-clock']) $(id).disabled = rated || Boolean(creation);
  }
  $('online-rated').onchange = applyRatedSettings;
  $('online-queue').onclick = () => { void run(async () => { const snapshot = await request('/queue', { method: 'POST' }); await enter(snapshot); $('online').close(); }); };
  $('online-join').onsubmit = event => { event.preventDefault(); void run(async () => {
    await load(invitationId($('online-invite').value));
  }); };
  $('online-history-section').ontoggle = () => { if ($('online-history-section').open && !cursor) void run(history); };
  $('online-history-more').onclick = () => { void run(history); };
  $('online-leave').onclick = leave;
  $('online-resign').onclick = () => { void run(async () => {
    const session = current(); if (session.kind !== 'online') return;
    const { id, status } = session.info;
    $('online-confirm-title').textContent = session.info.status === 'waiting' ? session.info.queue_started_ms != null ? 'Cancel search?' : 'Cancel invitation?' : 'Resign this match?';
    $('online-confirm-yes').onclick = () => { $('online-confirm').close(); void run(async () => {
      if (current() !== session || session.info.id !== id || session.info.status !== status) {
        throw new Error('The match changed. Check it before confirming again.');
      }
      await session.send({ kind: 'resign' });
    }); };
    $('online-confirm').showModal();
  }); };
  $('online-cancel-search').onclick = () => $('online-resign').click();
  $('online-confirm-cancel').onclick = () => $('online-confirm').close();
  $('online-resume').onclick = () => { void current().send({ kind: 'resume' }).catch(error); };
  $('online-copy').onclick = () => { void copyText($('online-link').value).then(() => { $('online-copy').textContent = 'Copied'; }).catch(() => { $('online-link').select(); }); };
  $('online-share-native').hidden = !native;
  $('online-share-native').onclick = () => { void run(() => shareInvite($('online-link').value)); };
  const invited = new URLSearchParams(location.search).get('match');
  if (!native && invited) {
    try { $('online-open-app').href = `${APP_SCHEME}://match/${invitationId(invited)}`; $('online-open-app').hidden = false; } catch {}
  }

  let recovery = null, recoveryTimer = null, recoveryBusy = false, recoveryGeneration = 0, recoveryDelay = 500;
  function cancelRecovery() {
    recoveryGeneration += 1;
    recovery = null;
    clearTimeout(recoveryTimer);
    $('match-recovery').hidden = true;
    document.documentElement.dataset.restoring = 'false';
  }
  function recoveryMessage(title, detail, account = false) {
    $('match-recovery').hidden = false;
    document.documentElement.dataset.restoring = 'true';
    $('recovery-title').textContent = title;
    $('recovery-detail').textContent = detail;
    $('recovery-account').hidden = !account;
  }
  async function resumeStored() {
    if (!recovery || recoveryBusy) return;
    recoveryBusy = true;
    $('recovery-retry').disabled = true;
    const ticket = recoveryGeneration, target = recovery;
    try {
      let snapshot;
      if (target.creation) {
        const me = await request('/me');
        if (me.user?.id !== target.creation.owner) throw new ApiError('Sign in to the account that created this invitation.', 401);
        snapshot = await request('/games', { method: 'POST', data: target.creation.spec });
      } else snapshot = await request(`/games/${encodeURIComponent(target.id)}`);
      if (ticket !== recoveryGeneration) return;
      if (!target.canEnter()) { cancelRecovery(); return; }
      await enter(snapshot);
      if (target.creation) await saveLocal(CREATION_KEY, null);
      cancelRecovery();
    } catch (value) {
      if (ticket !== recoveryGeneration) return;
      if (isTransientError(value)) {
        recoveryMessage('Reconnecting to your match…', 'Connection unavailable. Retrying automatically. Timed matches keep running.');
        clearTimeout(recoveryTimer);
        recoveryTimer = setTimeout(() => { void resumeStored(); }, recoveryDelay);
        recoveryDelay = Math.min(recoveryDelay * 2, 8000);
      } else if (value.status === 404 && !target.creation) {
        if (target.saved) await rememberMatch(null);
        cancelRecovery();
        onerror(new Error('That online match is no longer available.'));
      } else if (target.invitation && [401,403].includes(value.status)) {
        cancelRecovery();
        await show();
        $('online-invite').value = target.id;
      } else {
        recoveryMessage('Your match is saved.', value.message, [401,403].includes(value.status));
      }
    } finally {
      recoveryBusy = false;
      $('recovery-retry').disabled = false;
      if (recovery && ticket !== recoveryGeneration) void resumeStored();
    }
  }
  async function restore(canEnter = () => true) {
    cancelRecovery();
    if (!canEnter()) return;
    const params = new URLSearchParams(location.search);
    if (params.has('auth_error')) {
      await show();
      error(new Error(LOGIN_ERRORS.get(params.get('auth_error')) || 'Sign-in was not completed. Please try again.'));
      return;
    }
    const creation = pendingCreation();
    const saved = rememberedMatch();
    const id = params.get('match') || saved;
    if (creation || id) {
      recovery = { id, creation, canEnter, invitation: params.has('match'), saved: id === saved };
      recoveryDelay = 500;
      recoveryMessage('Reconnecting to your match…', 'Getting the latest position. Timed matches keep running.');
      await resumeStored();
      return;
    }
    if (params.has('online')) await show();
  }
  $('recovery-retry').onclick = () => { clearTimeout(recoveryTimer); void resumeStored(); };
  $('recovery-local').onclick = cancelRecovery;
  $('recovery-account').onclick = () => { void show(); };
  window.addEventListener('online', () => { if (recovery) { clearTimeout(recoveryTimer); void resumeStored(); } });
  window.addEventListener('native-state', ({detail}) => {
    if (!recovery) return;
    if (detail.active) void resumeStored(); else clearTimeout(recoveryTimer);
  });
  return {
    show, restore,
    async loggedIn() {
      leave();
      if (pendingCreation() || rememberedMatch()) { await restore(); if (current().kind === 'online') { $('online').close(); return; } }
      await show();
    },
    async invitation(id) { cancelRecovery(); await run(async () => { await refresh(); await load(id); }); },
  };
}

export function renderOnline(session) {
  const online = session?.kind === 'online'; $('online-match').hidden = !online; $('online-actions').hidden = !online;
  if (!online) return;
  const info = session.info;
  $('online-white').textContent = info.white?.handle || 'Open seat'; $('online-black').textContent = info.black?.handle || 'Open seat';
  $('online-label-white').textContent = session.seat === 'white' ? 'White · You' : 'White';
  $('online-label-black').textContent = session.seat === 'black' ? 'Black · You' : 'Black';
  $('online-white').title = info.white?.handle || 'Open seat';
  $('online-black').title = info.black?.handle || 'Open seat';
  const connection = session.connectionIssue || (session.pending ? 'Confirming move…' : !session.connected ? 'Reconnecting…' : `You play ${session.seat === 'white' ? 'White' : 'Black'} · ${info.rated ? 'Rated' : 'Unrated'}`);
  if ($('online-connection').textContent !== connection) $('online-connection').textContent = connection;
  $('online-retry').hidden = session.connected;
  $('online-retry').onclick = () => { void session.recover(); };
  const now = Date.now() + session.serverOffset;
  const searching = info.status === 'waiting' && info.queue_started_ms != null;
  if (searching) {
    const seconds = Math.floor(Math.max(0, now - info.queue_started_ms) / 1000);
    const gap = Math.min(600, 150 + 50 * Math.floor(seconds / 15));
    $('turn-label').textContent = 'Finding an opponent';
    $('turn-detail').textContent = `Within ${gap} Elo · ${seconds}s waiting`;
  } else if (info.matched_id) {
    $('turn-label').textContent = 'Opponent found';
    $('turn-detail').textContent = 'Joining your match…';
  }
  for (const color of ['white', 'black']) {
    const element = $(`online-clock-${color}`), stored = info[`${color}_ms`];
    element.hidden = stored === null;
    if (stored === null) continue;
    const seconds = Math.ceil(Math.max(0, stored - (info.turn === color && info.running_since_ms !== null ? Math.max(0, now - info.running_since_ms) : 0)) / 1000);
    element.textContent = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
    element.classList.toggle('running', info.status === 'active' && info.turn === color);
  }
  $('online-share').hidden = info.status !== 'waiting' || searching;
  const url = new URL(native ? onlineOrigin() : location.href); url.search = ''; url.hash = ''; url.searchParams.set('match', info.id);
  if ($('online-link').value !== url.href) { $('online-link').value = url.href; $('online-copy').textContent = 'Copy invite'; }
  $('online-resume').hidden = info.status !== 'paused';
  $('online-resume').disabled = !session.connected || info[`ready_${session.seat}`];
  $('online-resume').textContent = info[`ready_${session.seat}`] ? 'Ready' : 'Resume match';
  $('online-cancel-search').hidden = !searching;
  $('online-cancel-search').disabled = !session.connected || Boolean(session.pending);
  $('online-resign').hidden = searching || ['finished', 'cancelled'].includes(info.status);
  $('online-resign').disabled = !session.connected || Boolean(session.pending);
  $('online-resign').textContent = info.status === 'waiting' ? searching ? 'Cancel search' : 'Cancel invitation' : 'Resign';
}
