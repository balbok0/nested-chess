import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';

async function registered(client, baseURL, rating) {
  const response = await client.post('/api/v1/guest', { headers: { Origin: baseURL } });
  expect(response.ok()).toBeTruthy();
  const user = await response.json();
  const container = process.env.TEST_POSTGRES_CONTAINER;
  if (!container) throw new Error('Set TEST_POSTGRES_CONTAINER to the real test database container.');
  const env = new Map(JSON.parse(execFileSync('docker', ['inspect', '--format', '{{json .Config.Env}}', container], { encoding: 'utf8' })).map(value => {
    const split = value.indexOf('='); return [value.slice(0, split), value.slice(split + 1)];
  }));
  execFileSync('docker', ['exec', '-i', container, 'psql', '-U', env.get('POSTGRES_USER') || 'postgres', '-d', env.get('POSTGRES_DB') || 'postgres', '-v', 'ON_ERROR_STOP=1', '-v', `player=${user.id}`, '-v', `rating=${rating}`], {
    input: "UPDATE users SET guest=false WHERE id=:'player'; UPDATE ratings SET rating=:'rating'::double precision WHERE user_id=:'player' AND pool='classical-2x2-15+10';",
    encoding: 'utf8',
  });
}

async function search(page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Online', exact: true }).click();
  await page.getByRole('button', { name: 'Find rated opponent', exact: true }).click();
  await expect(page.locator('#turn-label')).toHaveText('Finding an opponent');
  await expect(page.locator('#online-share')).not.toBeVisible();
  return new URL(page.url()).searchParams.get('match');
}

async function finish(client, baseURL) {
  const response = await client.get('/api/v1/games');
  for (const info of await response.json()) {
    const ended = await client.post(`/api/v1/games/${info.id}/commands`, {
      headers: { Origin: baseURL },
      data: { request_id: randomUUID(), expected_seq: info.seq, action: { kind: 'resign' } },
    });
    expect(ended.ok(), await ended.text()).toBeTruthy();
  }
}

test('widening_searches_join_the_same_live_match_and_old_links_resume_it', async ({ page, browser, baseURL }, testInfo) => {
  const peer = await browser.newContext({ ...testInfo.project.use, baseURL });
  try {
    await registered(page.request, baseURL, 1500);
    await registered(peer.request, baseURL, 1700);
    const other = await peer.newPage();
    const first = await search(page);
    const second = await search(other);
    await expect(page.locator('#turn-detail')).toContainText('Within 150 Elo');
    await expect(other.locator('#turn-detail')).toContainText('Within 150 Elo');
    await expect(page.locator('#turn-label')).toHaveText('White to move', { timeout: 25_000 });
    await expect(other.locator('#turn-label')).toHaveText('White to move');
    const id = new URL(page.url()).searchParams.get('match');
    expect(new URL(other.url()).searchParams.get('match')).toBe(id);
    for (const player of [page, other]) {
      await expect(player.locator('#online-connection')).toContainText('You play');
      expect(await player.evaluate(() => localStorage.getItem('nested-online-match'))).toBe(id);
    }
    const white = (await page.locator('#online-connection').textContent()).includes('You play White') ? page : other;
    await white.locator('[data-square="e2"]').click();
    await white.locator('[data-square="e4"]').click();
    await expect(page.locator('#ply-count')).toHaveText('1 move');
    await expect(other.locator('#ply-count')).toHaveText('1 move');
    const redirected = first !== id ? page : other;
    await redirected.goto(`/?match=${first !== id ? first : second}`);
    await expect(redirected.locator('#ply-count')).toHaveText('1 move');
    expect(new URL(redirected.url()).searchParams.get('match')).toBe(id);
  } finally {
    await finish(page.request, baseURL);
    await finish(peer.request, baseURL);
    await peer.close();
  }
});

test('cancelling_a_search_keeps_later_players_waiting', async ({ page, browser, baseURL }, testInfo) => {
  const peer = await browser.newContext({ ...testInfo.project.use, baseURL });
  try {
    await registered(page.request, baseURL, 1500);
    await registered(peer.request, baseURL, 1500);
    await search(page);
    await page.getByRole('button', { name: 'Cancel search', exact: true }).click();
    await expect(page.locator('#online-confirm-title')).toHaveText('Cancel search?');
    await page.locator('#online-confirm-yes').click();
    await expect(page.locator('#online-resign')).not.toBeVisible();
    const other = await peer.newPage();
    await search(other);
    await expect(other.locator('#turn-detail')).toContainText('Within 150 Elo');
  } finally {
    await finish(page.request, baseURL);
    await finish(peer.request, baseURL);
    await peer.close();
  }
});

test('a_cancel_search_confirmation_cannot_resign_a_match_found_while_it_was_open', async ({ page, browser, baseURL }, testInfo) => {
  const peer = await browser.newContext({ ...testInfo.project.use, baseURL });
  try {
    await registered(page.request, baseURL, 1500);
    await registered(peer.request, baseURL, 1500);
    await search(page);
    await page.getByRole('button', { name: 'Cancel search', exact: true }).click();
    await expect(page.locator('#online-confirm-title')).toHaveText('Cancel search?');
    const matched = await peer.request.post('/api/v1/queue', { headers: { Origin: baseURL } });
    expect(matched.ok()).toBeTruthy();
    await expect(page.locator('#turn-label')).toHaveText('White to move');
    await page.locator('#online-confirm-yes').click();
    await expect(page.locator('#error')).toContainText('The match changed');
    const games = await page.request.get('/api/v1/games');
    expect((await games.json()).map(info => info.status)).toEqual(['active']);
  } finally {
    await finish(page.request, baseURL);
    await finish(peer.request, baseURL);
    await peer.close();
  }
});
