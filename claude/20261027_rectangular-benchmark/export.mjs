import { readFileSync, writeFileSync } from 'node:fs';
import { chromium } from '@playwright/test';

const data = JSON.parse(readFileSync(new URL('../../web/rectangular-benchmark-results.json', import.meta.url), 'utf8'));
const css = readFileSync(new URL('../../web/style.css', import.meta.url), 'utf8');
const html = readFileSync(new URL('../../web/rectangular-analysis.html', import.meta.url), 'utf8')
  .replace('<link rel="stylesheet" href="./style.css" />', `<style>${css}</style>`)
  .replace("import data from './rectangular-benchmark-results.json';", `const data = ${JSON.stringify(data).replaceAll('<', '\\u003c')};`)
  .replace('href="./"', 'href="https://balbok0.github.io/nested-chess/"')
  .replace('href="./analysis.html"', 'href="https://balbok0.github.io/nested-chess/analysis.html"');
const output = new URL('./report.html', import.meta.url);
writeFileSync(output, html);
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  await page.goto(output.href);
  await page.locator('#results table').first().waitFor();
  await page.locator('script').evaluateAll(elements => elements.forEach(element => element.remove()));
  writeFileSync(output, await page.content());
} finally {
  await browser.close();
}
console.log(output.pathname);
