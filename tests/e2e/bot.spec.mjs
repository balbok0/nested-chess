import { test, expect } from '@playwright/test';

for (const rows of [2, 4]) {
  test(`ten_candidate_bot_replies_on_four_by_${rows}`, async ({ page }) => {
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto('/');
    await page.getByRole('button', { name: `4 × ${rows}`, exact: true }).click();
    await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('bot');
    await page.locator('#difficulty').selectOption('1');
    await page.getByRole('button', { name: 'e2 white pawn', exact: true }).click();
    await page.getByRole('button', { name: /^e4 empty/ }).click();
    await expect(page.locator('#ply-count')).toHaveText('2 moves');
    await expect(page.locator('#error')).not.toBeVisible();
    const save = await page.evaluate(() => JSON.parse(JSON.parse(localStorage.getItem('nested-chess-v1')).save));
    expect([save.columns, save.rows]).toEqual([4, rows]);
    expect(save.events[0]).toEqual({ kind: 'move', board: 0, uci: 'e2e4' });
    expect(save.events[1].board).toBe(rows === 2 ? 2 : 6);
    expect(errors).toEqual([]);
  });
}
