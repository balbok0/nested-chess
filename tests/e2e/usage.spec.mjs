import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';

const activity = (page, predicate) => page.waitForResponse(response => response.url().endsWith('/api/v1/activity') && response.request().method() === 'POST' && predicate(response.request().postDataJSON()));

test('local_activity_reports_mode_changes_without_creating_an_account_or_sending_moves', async ({ page }) => {
  const initial = activity(page, body => body.mode === 'engine' && !body.playing && body.visible);
  await page.goto('/');
  expect((await initial).status()).toBe(204);
  const switched = activity(page, body => body.mode === 'same_device' && !body.playing);
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
  expect((await switched).status()).toBe(204);
  const playing = activity(page, body => body.mode === 'same_device' && body.playing);
  await page.locator('[data-square="e2"]').click();
  await page.locator('[data-square="e4"]').click();
  const report = await playing;
  expect(report.status()).toBe(204);
  expect(Object.keys(report.request().postDataJSON()).sort()).toEqual(['client_id','game_id','mode','playing','sequence','visible']);
  expect(report.request().postDataJSON().game_id).toBeNull();
  const me = await page.request.get('/api/v1/me');
  expect((await me.json()).user).toBeNull();
  const engine = activity(page, body => body.mode === 'engine' && body.playing);
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('bot');
  expect((await engine).status()).toBe(204);
});

test('disabling_usage_sharing_marks_the_client_inactive_and_persists', async ({ page }) => {
  const initial = activity(page, body => body.visible);
  await page.goto('/'); await initial;
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const stopped = activity(page, body => !body.visible);
  await page.getByRole('checkbox', { name: 'Share usage statistics', exact: true }).uncheck();
  expect((await stopped).status()).toBe(204);
  await page.locator('#settings-close').click();
  const reports = [];
  page.on('request', request => { if (request.url().endsWith('/api/v1/activity')) reports.push(request.postDataJSON()); });
  await page.reload();
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
  await page.locator('[data-square="e2"]').click(); await page.locator('[data-square="e4"]').click();
  await expect(page.locator('#ply-count')).toHaveText('1 move');
  expect(reports).toEqual([]);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: 'Share usage statistics', exact: true })).not.toBeChecked();
});

test('online_activity_reports_the_joined_match_id', async ({ page, playwright, baseURL }) => {
  const peer = await playwright.request.newContext({ baseURL });
  try {
    const headers = { Origin: baseURL };
    expect((await page.request.post('/api/v1/guest', { headers })).ok()).toBeTruthy();
    const response = await page.request.post('/api/v1/games', { headers, data: {} });
    expect(response.ok()).toBeTruthy();
    const created = await response.json();
    expect((await peer.post('/api/v1/guest', { headers })).ok()).toBeTruthy();
    expect((await peer.post(`/api/v1/games/${created.info.id}/join`, { headers })).ok()).toBeTruthy();
    const reported = activity(page, body => body.mode === 'online' && body.game_id === created.info.id && body.playing && body.visible);
    await page.goto(`/?match=${created.info.id}`);
    expect((await reported).status()).toBe(204);
    await expect(page.locator('#online-connection')).toContainText('You play');
  } finally { await peer.dispose(); }
});

test('local_play_continues_when_activity_reporting_cannot_reach_the_server', async ({ page, context }) => {
  await page.goto('/');
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
  try {
    await context.setOffline(true);
    await page.locator('[data-square="e2"]').click(); await page.locator('[data-square="e4"]').click();
    await expect(page.locator('#ply-count')).toHaveText('1 move');
    await expect(page.locator('#error')).not.toBeVisible();
    const resumed = activity(page, body => body.mode === 'same_device' && body.playing && body.visible);
    await context.setOffline(false);
    expect((await resumed).status()).toBe(204);
  } finally { await context.setOffline(false); }
});

test('usage_page_reads_real_statistics_only_for_the_configured_account', async ({ page, browser, baseURL }) => {
  const guest = await page.request.post('/api/v1/guest', { headers: { Origin: baseURL } });
  const user = await guest.json();
  execFileSync('docker', ['exec','-i',process.env.TEST_POSTGRES_CONTAINER,'psql','-U','nested','-d','nested','-v','ON_ERROR_STOP=1','-v',`player=${user.id}`], { input: "UPDATE users SET guest=false WHERE id=:'player';", stdio: ['pipe','pipe','pipe'] });
  const name = `nested-usage-admin-${randomUUID()}`;
  const project = process.env.TEST_COMPOSE_PROJECT || 'nested-chess';
  let outsider;
  try {
    execFileSync('docker', ['compose','-p',project,'run','--rm','--no-deps','-d','--name',name,'--publish','127.0.0.1::4180','-e',`USAGE_ADMIN_IDS=${user.id}`,'app-a'], { stdio: ['ignore','pipe','pipe'] });
    const address = execFileSync('docker',['port',name,'4180/tcp'], { encoding:'utf8' }).trim();
    const origin = `http://localhost:${address.split(':').at(-1)}`;
    await expect.poll(async () => { try { return (await page.request.get(`${origin}/api/v1/ready`)).status(); } catch { return 0; } }).toBe(200);
    const foreground = activity(page, body => body.visible);
    await page.goto('/'); await foreground;
    const stats = await page.request.get(`${origin}/api/v1/usage`);
    expect(stats.status()).toBe(200);
    expect((await stats.json()).signed_in_accounts).toBeGreaterThanOrEqual(1);
    await page.goto(`${origin}/usage.html`);
    await expect(page.locator('#usage-updated')).toContainText('Updated');
    await expect(page.locator('#usage-error')).not.toBeVisible();
    await expect(page.locator('#usage-clients')).toHaveText(/^\d+$/);
    await expect(page.locator('tbody th')).toHaveText(['Online','Vs engine','Same-device two-player']);
    outsider = await browser.newContext();
    const anonymous = await outsider.newPage();
    await anonymous.goto(`${origin}/usage.html`);
    await expect(anonymous.locator('#usage-error')).toContainText('Sign in');
    await expect(anonymous.locator('#usage-clients')).toHaveText('—');
  } finally {
    await outsider?.close();
    execFileSync('docker',['rm','-f',name], { stdio:['ignore','pipe','pipe'] });
  }
});
