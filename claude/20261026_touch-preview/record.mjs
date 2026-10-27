import { chromium, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';

const output = new URL('./', import.meta.url).pathname;
const temporary = new URL('../../scratch/claude/20261026_touch-preview/', import.meta.url).pathname;
await mkdir(temporary, { recursive: true });
const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true,
  deviceScaleFactor: 1, colorScheme: 'light',
  recordVideo: { dir: temporary, size: { width: 390, height: 844 } },
});
const page = await context.newPage();
await page.goto(process.env.UI_BASE_URL || 'http://localhost:4184');
await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
await page.evaluate(() => window.scrollTo(0, 96));
await page.evaluate(() => {
  const caption = document.createElement('div');
  caption.id = 'demo-caption';
  caption.style.cssText = 'position:fixed;bottom:16px;left:20px;right:20px;padding:12px;background:#222;color:white;border-radius:8px;text-align:center;font:16px system-ui;z-index:9999;pointer-events:none';
  document.body.append(caption);
  const finger = document.createElement('div');
  finger.style.cssText = 'position:fixed;width:32px;height:32px;border:2px solid #227be6;background:#227be644;border-radius:50%;transform:translate(-50%,-50%);z-index:9999;pointer-events:none;display:none';
  document.body.append(finger);
  document.addEventListener('pointerdown', event => {
    finger.style.left = `${event.clientX}px`;
    finger.style.top = `${event.clientY}px`;
    finger.style.display = 'block';
  });
  document.addEventListener('pointerup', () => { finger.style.display = 'none'; });
});
const caption = text => page.locator('#demo-caption').evaluate((element, text) => { element.textContent = text; }, text);
await caption('Select a piece');
await page.waitForTimeout(800);
await page.locator('[data-square="e2"]').tap();
await page.waitForTimeout(800);
await caption('Hold e4 to preview the next board');
const bounds = await page.locator('[data-square="e4"]').boundingBox();
const client = await context.newCDPSession(page);
await client.send('Input.dispatchTouchEvent', {
  type: 'touchStart', touchPoints: [{ x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 }],
});
await expect(page.locator('#route-title')).toHaveText('e4 → Board B1');
await page.waitForTimeout(1800);
await page.screenshot({ path: `${temporary}held.png` });
await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
await caption('Release: no move made');
await expect(page.locator('#ply-count')).toHaveText('0 moves');
await page.waitForTimeout(1500);
await caption('Tap e4 to make the move');
await page.waitForTimeout(700);
await page.locator('[data-square="e4"]').tap();
await expect(page.locator('#ply-count')).toHaveText('1 move');
await page.waitForTimeout(1500);
const video = page.video();
await context.close();
await video.saveAs(`${output}route-preview.webm`);
await browser.close();
console.log(`${output}route-preview.webm`);
