import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';

async function registered(client, baseURL) {
  const response = await client.post('/api/v1/guest', { headers: { Origin: baseURL } });
  expect(response.ok()).toBeTruthy();
  const user = await response.json();
  execFileSync('docker', ['exec', '-i', process.env.TEST_POSTGRES_CONTAINER, 'psql', '-U', 'nested', '-d', 'nested', '-v', 'ON_ERROR_STOP=1', '-v', `player=${user.id}`], {
    input: "UPDATE users SET guest=false WHERE id=:'player';", stdio: ['pipe', 'pipe', 'pipe'],
  });
  return user;
}

test('privacy_policy_is_public_linked_and_readable_without_signing_in', async ({ page }) => {
  await page.goto('/');
  await page.locator('.site-footer').getByRole('link', { name: 'Privacy policy', exact: true }).click();
  await expect(page).toHaveURL(/\/privacy\.html$/);
  await expect(page.getByRole('heading', { name: 'Privacy policy', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Sign-in and account information', exact: true })).toBeVisible();
  await expect(page.locator('main')).toContainText('openid');
  await expect(page.locator('main')).toContainText('No deletion request or email is required');
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(page.viewportSize().width);
  expect((await (await page.request.get('/api/v1/me')).json()).user).toBeNull();
});

test('cancelling_account_deletion_preserves_the_account_and_requires_the_username', async ({ page, baseURL }) => {
  const user = await registered(page.request, baseURL);
  await page.goto('/');
  await expect(page.locator('#account-status')).toHaveText(user.handle);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Delete account', exact: true }).click();
  await expect(page.locator('#delete-account-submit')).toBeDisabled();
  await page.locator('#delete-account-confirmation').fill('wrong_username');
  await expect(page.locator('#delete-account-submit')).toBeDisabled();
  await page.locator('#delete-account-confirmation').fill(user.handle);
  await expect(page.locator('#delete-account-submit')).toBeEnabled();
  await page.getByRole('button', { name: 'Keep account', exact: true }).click();
  await expect(page.locator('#delete-account')).not.toBeVisible();
  expect((await (await page.request.get('/api/v1/me')).json()).user.id).toBe(user.id);
});

test('deleting_an_account_keeps_local_play_and_the_opponents_anonymous_history', async ({ page, browser, baseURL }, testInfo) => {
  const user = await registered(page.request, baseURL);
  const peer = await browser.newContext({ ...testInfo.project.use, baseURL });
  try {
    const headers = { Origin: baseURL };
    expect((await peer.request.post('/api/v1/guest', { headers })).ok()).toBeTruthy();
    await page.goto('/');
    await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
    await page.locator('[data-square="e2"]').click(); await page.locator('[data-square="e4"]').click();
    await expect(page.locator('#ply-count')).toHaveText('1 move');
    const saved = await page.evaluate(() => localStorage.getItem('nested-chess-v1'));
    const created = await page.request.post('/api/v1/games', { headers, data: {} });
    const { info } = await created.json();
    expect((await peer.request.post(`/api/v1/games/${info.id}/join`, { headers })).ok()).toBeTruthy();
    await page.goto(`/?match=${info.id}`);
    await expect(page.locator('#online-connection')).toContainText('You play');
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Delete account', exact: true }).click();
    await page.locator('#delete-account-confirmation').fill(user.handle);
    await page.getByRole('button', { name: 'Delete permanently', exact: true }).click();
    await expect(page.locator('#account-status')).toHaveText('Account deleted');
    await expect(page.locator('#delete-account')).not.toBeVisible();
    await expect(page.locator('#ply-count')).toHaveText('1 move');
    expect(await page.evaluate(() => localStorage.getItem('nested-chess-v1'))).toBe(saved);
    expect(await page.evaluate(() => localStorage.getItem('nested-online-match'))).toBeNull();
    expect(await page.evaluate(() => localStorage.getItem('nested-online-creation'))).toBeNull();
    expect((await (await page.request.get('/api/v1/me')).json()).user).toBeNull();
    const other = await peer.newPage();
    await other.goto('/');
    await other.getByRole('button', { name: 'Online', exact: true }).click();
    await other.locator('#online-history-section summary').click();
    await expect(other.locator('#online-history')).toContainText('Won vs Deleted player');
    await other.locator('#online-history button').click();
    await expect(other.locator('#online-match')).toContainText('Deleted player');
    await expect(other.locator('#turn-label')).not.toHaveText('Loading game…');
    await page.reload();
    await expect(page.locator('#account-status')).toHaveText('Signed out');
    await expect(page.locator('#ply-count')).toHaveText('1 move');
  } finally { await peer.close(); }
});

test('a_failed_deletion_request_keeps_the_confirmation_open_without_claiming_success', async ({ page, context, baseURL }) => {
  const user = await registered(page.request, baseURL);
  await page.goto('/');
  await expect(page.locator('#account-status')).toHaveText(user.handle);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Delete account', exact: true }).click();
  await page.locator('#delete-account-confirmation').fill(user.handle);
  try {
    await context.setOffline(true);
    await page.getByRole('button', { name: 'Delete permanently', exact: true }).click();
    await expect(page.locator('#delete-account-error')).toBeVisible();
    await expect(page.locator('#delete-account')).toBeVisible();
    await expect(page.locator('#account-status')).toHaveText(user.handle);
    await page.getByRole('button', { name: 'Keep account', exact: true }).click();
  } finally { await context.setOffline(false); }
  expect((await (await page.request.get('/api/v1/me')).json()).user.id).toBe(user.id);
});
