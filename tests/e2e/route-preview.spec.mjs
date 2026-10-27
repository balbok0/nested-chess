import { test, expect } from '@playwright/test';

async function local(page) {
  await page.goto('/');
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('nested-chess-v1'))?.mode)).toBe('local');
}

async function touchDestination(page, context) {
  await page.locator('[data-square="e2"]').tap();
  const square = page.locator('[data-square="e4"]');
  const bounds = await square.boundingBox();
  const client = await context.newCDPSession(page);
  const point = { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point] });
  return { client, point };
}

test('holding_a_destination_previews_routing_and_release_keeps_the_position', async ({ page, context }, testInfo) => {
  test.skip(!testInfo.project.use.hasTouch, 'Touch interaction');
  await local(page);
  const saved = await page.evaluate(() => localStorage.getItem('nested-chess-v1'));
  const { client } = await touchDestination(page, context);
  await expect(page.locator('#route-title')).toHaveText('e4 → Board B1');
  await expect(page.locator('[data-board="1"]')).toHaveClass(/next/);
  await expect(page.locator('[data-square="e4"]')).toHaveClass(/route-preview/);
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect(page.locator('.macro-cell.next')).toHaveCount(0);
  await expect(page.locator('#route-title')).toHaveText('From e2');
  await expect(page.locator('[data-square="e2"]')).toHaveClass(/selected/);
  await expect(page.locator('#ply-count')).toHaveText('0 moves');
  expect(await page.evaluate(() => localStorage.getItem('nested-chess-v1'))).toBe(saved);
  await page.locator('[data-square="e4"]').tap();
  await expect(page.locator('#ply-count')).toHaveText('1 move');
  await expect(page.locator('#focus-title')).toHaveText('Board B1');
});

for (const action of ['slide', 'cancel']) {
  test(`${action}_cancels_a_touch_preview_without_moving`, async ({ page, context }, testInfo) => {
    test.skip(!testInfo.project.use.hasTouch, 'Touch interaction');
    await local(page);
    const { client, point } = await touchDestination(page, context);
    await expect(page.locator('#route-title')).toHaveText('e4 → Board B1');
    if (action === 'slide') {
      await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: point.x + 18, y: point.y }] });
      await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    } else {
      await client.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
    }
    await expect(page.locator('.macro-cell.next')).toHaveCount(0);
    await expect(page.locator('.square.route-preview')).toHaveCount(0);
    await expect(page.locator('#ply-count')).toHaveText('0 moves');
    await page.locator('[data-square="e4"]').tap();
    await expect(page.locator('#ply-count')).toHaveText('1 move');
  });
}

test('mouse_hover_and_keyboard_focus_still_preview_routes', async ({ page }, testInfo) => {
  test.skip(testInfo.project.use.hasTouch, 'Mouse and keyboard interaction');
  await local(page);
  await page.locator('[data-square="e2"]').click();
  await page.locator('[data-square="e4"]').hover();
  await expect(page.locator('#route-title')).toHaveText('e4 → Board B1');
  await page.mouse.move(0, 0);
  await expect(page.locator('.macro-cell.next')).toHaveCount(0);
  await page.locator('[data-square="e4"]').focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Shift+Tab');
  await expect(page.locator('#route-title')).toHaveText('e4 → Board B1');
  await page.keyboard.press('Enter');
  await expect(page.locator('#ply-count')).toHaveText('1 move');
});
