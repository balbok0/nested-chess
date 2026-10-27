import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';

async function post(client, baseURL, path, data = {}) {
  const response = await client.post(path, { headers: { Origin: baseURL }, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

async function match(page, playwright, baseURL) {
  const owner = await post(page.request, baseURL, '/api/v1/guest');
  const peer = await playwright.request.newContext({ baseURL });
  const opponent = await post(peer, baseURL, '/api/v1/guest');
  const created = await post(page.request, baseURL, '/api/v1/games', { clock: { seconds: 900, increment: 10 } });
  const joined = await post(peer, baseURL, `/api/v1/games/${created.info.id}/join`);
  return { id: created.info.id, owner, opponent, peer, info: joined.info };
}

async function move(client, baseURL, id, seq, board, uci) {
  return post(client, baseURL, `/api/v1/games/${id}/commands`, {
    request_id: randomUUID(), expected_seq: seq, action: { kind: 'move', board, uci },
  });
}

async function local(page) {
  await page.goto('/');
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
}

async function localMove(page) {
  await page.getByRole('button', { name: 'e2 white pawn', exact: true }).click();
  await page.getByRole('button', { name: /^e4 empty/ }).click();
  await expect(page.locator('#ply-count')).toHaveText('1 move');
}

test('local_reset_requires_confirmation_and_cancel_keeps_the_saved_position', async ({ page }) => {
  await local(page);
  await localMove(page);
  const saved = await page.evaluate(() => localStorage.getItem('nested-chess-v1'));
  await page.getByRole('button', { name: '2 × 2', exact: true }).click();
  await expect(page.locator('#reset-confirm')).not.toBeVisible();
  await expect(page.locator('#ply-count')).toHaveText('1 move');
  await page.getByRole('button', { name: '4 × 4', exact: true }).click();
  await expect(page.locator('#reset-confirm')).toBeVisible();
  await page.locator('#reset-cancel').click();
  await expect(page.locator('#ply-count')).toHaveText('1 move');
  expect(await page.evaluate(() => localStorage.getItem('nested-chess-v1'))).toBe(saved);
  await page.getByRole('button', { name: 'New match', exact: true }).click();
  await page.locator('#reset-yes').click();
  await expect(page.locator('#ply-count')).toHaveText('0 moves');
  await expect(page.locator('#undo')).toBeDisabled();
});

test('confirming_a_different_grid_starts_that_grid', async ({ page }) => {
  await local(page);
  await localMove(page);
  await page.getByRole('button', { name: '4 × 4', exact: true }).click();
  await page.locator('#reset-yes').click();
  await expect(page.locator('#board-count')).toHaveText('16 boards');
  await expect(page.locator('#ply-count')).toHaveText('0 moves');
});

test('online_players_clocks_and_board_fit_the_viewport', async ({ page, playwright, baseURL }, testInfo) => {
  const game = await match(page, playwright, baseURL);
  try {
    await page.goto(`/?match=${game.id}`);
    await expect(page.locator('#online-connection')).toContainText('You play');
    const bounds = await page.evaluate(() => ({
      players: document.querySelector('#online-match').getBoundingClientRect().toJSON(),
      board: document.querySelector('#chessboard').getBoundingClientRect().toJSON(),
      width: innerWidth, height: innerHeight, pageWidth: document.documentElement.scrollWidth,
    }));
    expect(bounds.pageWidth).toBe(bounds.width);
    expect(bounds.players.bottom).toBeLessThanOrEqual(bounds.height);
    expect(bounds.board.bottom).toBeLessThanOrEqual(bounds.height);
    if (testInfo.project.name !== 'desktop') {
      expect(bounds.players.top).toBeLessThanOrEqual(bounds.board.top);
      const undo = await page.locator('#undo').boundingBox();
      expect(undo.height).toBeGreaterThanOrEqual(44);
    }
  } finally { await game.peer.dispose(); }
});

test('invitation_fields_do_not_overlap_and_double_tap_creates_one_invitation', async ({ page, baseURL }) => {
  await local(page);
  await page.getByRole('button', { name: 'Online', exact: true }).click();
  await expect(page.locator('#online-account')).not.toBeEmpty();
  const fields = await page.locator('.online-fields select').evaluateAll(elements => elements.map(e => e.getBoundingClientRect().toJSON()));
  for (let i = 0; i < fields.length; i++) for (let j = i + 1; j < fields.length; j++) {
    const a = fields[i], b = fields[j];
    expect(a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top).toBeTruthy();
  }
  await page.getByRole('button', { name: 'Create invitation', exact: true }).dblclick({ delay: 0 });
  await expect(page.locator('#online-connection')).toContainText('You play');
  const response = await page.request.get('/api/v1/games');
  expect((await response.json()).length).toBe(1);
  const copy = await page.locator('#online-copy').boundingBox();
  expect(copy.y + copy.height).toBeLessThanOrEqual(page.viewportSize().height);
  expect(new URL(await page.locator('#online-link').inputValue()).origin).toBe(baseURL);
});

test('reopening_resumes_the_latest_server_position', async ({ page, context, playwright, baseURL }) => {
  const game = await match(page, playwright, baseURL);
  try {
    await page.goto(`/?match=${game.id}`);
    await expect(page.locator('#online-connection')).toContainText('You play');
    await page.close();
    const white = game.info.white.id === game.owner.id ? context.request : game.peer;
    const black = game.info.black.id === game.owner.id ? context.request : game.peer;
    await move(white, baseURL, game.id, 1, 0, 'e2e4');
    await move(black, baseURL, game.id, 2, 1, 'e7e5');
    const reopened = await context.newPage();
    await reopened.goto('/');
    await expect(reopened.locator('#online-connection')).toContainText('You play');
    await expect(reopened.locator('#ply-count')).toHaveText('2 moves');
    await expect(reopened.locator('#focus-title')).toHaveText('Board B2');
    expect(new URL(reopened.url()).searchParams.get('match')).toBe(game.id);
  } finally { await game.peer.dispose(); }
});

test('an_unconfirmed_move_reconnects_once_and_clears_connection_feedback', async ({ page, context, playwright, baseURL }) => {
  const game = await match(page, playwright, baseURL);
  try {
    const white = game.info.white.id === game.owner.id;
    if (!white) await move(game.peer, baseURL, game.id, 1, 0, 'e2e4');
    await page.goto(`/?match=${game.id}`);
    await expect(page.locator('#online-connection')).toContainText('You play');
    await page.getByRole('button', { name: white ? 'e2 white pawn' : 'e7 black pawn', exact: true }).click();
    await context.setOffline(true);
    await page.getByRole('button', { name: white ? /^e4 empty/ : /^e5 empty/ }).click();
    await expect(page.locator('#online-connection')).toContainText('Move not confirmed');
    await expect(page.locator('#error')).not.toBeVisible();
    await context.setOffline(false);
    await expect(page.locator('#online-connection')).toContainText('You play');
    await expect(page.locator('#ply-count')).toHaveText(white ? '1 move' : '2 moves');
    await expect(page.locator('#error')).not.toBeVisible();
    const result = await page.request.get(`/api/v1/games/${game.id}`);
    expect((await result.json()).state.ply).toBe(white ? 1 : 2);
  } finally { await context.setOffline(false); await game.peer.dispose(); }
});

test('an_unconfirmed_invitation_resumes_the_same_server_match_after_reopen', async ({ page, context, baseURL }) => {
  await local(page);
  await post(page.request, baseURL, '/api/v1/guest');
  await page.getByRole('button', { name: 'Online', exact: true }).click();
  await expect(page.locator('#online-account')).toContainText('Guest');
  await context.setOffline(true);
  await page.getByRole('button', { name: 'Create invitation', exact: true }).click();
  await expect(page.locator('#online-create-status')).toContainText('not confirmed');
  const pending = await page.evaluate(() => JSON.parse(localStorage.getItem('nested-online-creation')));
  const created = await post(page.request, baseURL, '/api/v1/games', pending.spec);
  await page.close();
  await context.setOffline(false);
  const reopened = await context.newPage();
  await reopened.goto('/');
  await expect(reopened.locator('#online-connection')).toContainText('You play');
  expect(new URL(reopened.url()).searchParams.get('match')).toBe(created.info.id);
  expect(await reopened.evaluate(() => localStorage.getItem('nested-online-creation'))).toBeNull();
  const games = await reopened.request.get('/api/v1/games');
  expect((await games.json()).length).toBe(1);
});

test('startup_outage_keeps_the_match_and_resumes_when_the_database_returns', async ({ page, playwright, baseURL }) => {
  const container = process.env.TEST_POSTGRES_CONTAINER;
  if (!container) throw new Error('TEST_POSTGRES_CONTAINER must name the disposable PostgreSQL container.');
  const game = await match(page, playwright, baseURL);
  try {
    await page.goto(`/?match=${game.id}`);
    await expect(page.locator('#online-connection')).toContainText('You play');
    execFileSync('docker', ['stop', container], { stdio: 'pipe' });
    await page.goto('/');
    await expect(page.locator('#match-recovery')).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem('nested-online-match'))).toBe(game.id);
    await page.getByRole('button', { name: 'Play locally', exact: true }).click();
    await expect(page.locator('#chessboard')).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem('nested-online-match'))).toBe(game.id);
    await page.goto('/');
    await expect(page.locator('#match-recovery')).toBeVisible();
    execFileSync('docker', ['start', container], { stdio: 'pipe' });
    await expect(page.locator('#online-connection')).toContainText('You play');
    expect(new URL(page.url()).searchParams.get('match')).toBe(game.id);
    await expect(page.locator('#match-recovery')).not.toBeVisible();
    await expect(page.locator('#error')).not.toBeVisible();
  } finally {
    execFileSync('docker', ['start', container], { stdio: 'pipe' });
    await game.peer.dispose();
  }
});
