import { test, expect } from '@playwright/test';

for (const rows of [2, 4]) {
  test(`4x${rows}_uses_four_columns_and_keeps_routing_after_reload_and_undo`, async ({ page }) => {
    await page.goto('/');
    await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
    await page.getByRole('button', { name: `4 × ${rows}`, exact: true }).click();
    await expect(page.locator('#board-count')).toHaveText(`${4 * rows} boards`);
    await expect(page.getByRole('button', { name: '8 × 8', exact: true })).toHaveCount(0);
    const bounds = await page.locator('.macro-cell').evaluateAll(elements => elements.map(element => ({
      x: element.getBoundingClientRect().x, y: element.getBoundingClientRect().y,
    })));
    expect(new Set(bounds.map(value => value.x)).size).toBe(4);
    expect(new Set(bounds.map(value => value.y)).size).toBe(rows);
    await page.locator('[data-square="e2"]').click();
    await page.locator('[data-square="e4"]').hover();
    await expect(page.locator('#route-title')).toHaveText(`e4 → Board C${rows === 2 ? 1 : 2}`);
    await page.locator('[data-square="e4"]').click();
    await expect(page.locator('#focus-title')).toHaveText(`Board C${rows === 2 ? 1 : 2}`);
    await page.locator('[data-square="e7"]').click();
    await page.locator('[data-square="e5"]').click();
    await expect(page.locator('#focus-title')).toHaveText(`Board C${rows === 2 ? 2 : 3}`);
    await page.reload();
    await expect(page.locator('#ply-count')).toHaveText('2 moves');
    await expect(page.getByRole('button', { name: `4 × ${rows}`, exact: true })).toHaveAttribute('aria-pressed', 'true');
    await page.locator('#undo').click();
    await expect(page.locator('#ply-count')).toHaveText('1 move');
    await expect(page.locator('#focus-title')).toHaveText(`Board C${rows === 2 ? 1 : 2}`);
    const download = page.waitForEvent('download');
    await page.locator('#export').click();
    expect((await download).suggestedFilename()).toBe(`nested-4x${rows}-ply1.json`);
  });
}

test('an_old_64_board_save_still_opens_and_its_new_match_uses_eight_boards', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('nested-chess-v1', JSON.stringify({ mode: 'local', level: '2', save: JSON.stringify({
      version: 3, size: 8, first_player: 'white', initial_fens: Array(64).fill('rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'),
      finish_rule: 'majority', en_passant_rule: 'local', events: [],
    }) }));
  });
  await page.goto('/');
  await expect(page.locator('#board-count')).toHaveText('64 boards');
  await page.getByRole('button', { name: 'New match', exact: true }).click();
  await expect(page.locator('#board-count')).toHaveText('8 boards');
  await expect(page.getByRole('button', { name: '4 × 2', exact: true })).toHaveAttribute('aria-pressed', 'true');
});

test('eight_board_online_invitations_keep_the_shape_for_both_players', async ({ page, playwright, baseURL }) => {
  const peer = await playwright.request.newContext({ baseURL });
  try {
    await page.goto('/');
    await page.getByRole('button', { name: 'Online', exact: true }).click();
    await page.locator('#online-size').selectOption('4x2');
    await page.getByRole('button', { name: 'Create invitation', exact: true }).click();
    await expect(page.locator('#online-connection')).toContainText('You play');
    const id = new URL(page.url()).searchParams.get('match');
    expect((await peer.post('/api/v1/guest', { headers: { Origin: baseURL } })).ok()).toBeTruthy();
    const response = await peer.post(`/api/v1/games/${id}/join`, { headers: { Origin: baseURL } });
    expect(response.ok()).toBeTruthy();
    const actual = await response.json();
    expect([actual.state.columns, actual.state.rows, actual.state.boards.length]).toEqual([4, 2, 8]);
    expect(actual.info.rows).toBe(2);
    await expect(page.locator('#turn-label')).toHaveText('White to move');
    await expect(page.locator('#board-count')).toHaveText('8 boards');
    await page.reload();
    await expect(page.locator('#board-count')).toHaveText('8 boards');
    await expect(page.locator('#online-connection')).toContainText('You play');
  } finally { await peer.dispose(); }
});
