import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  outputDir: './scratch/claude/20261026_mobile-ux/playwright',
  workers: 1,
  timeout: 45_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: process.env.UI_BASE_URL || 'http://localhost:4184',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'desktop', use: { viewport: { width: 1440, height: 1000 } } },
    { name: 'phone', use: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true } },
    { name: 'small_phone', use: { viewport: { width: 320, height: 568 }, isMobile: true, hasTouch: true } },
    { name: 'landscape', use: { viewport: { width: 844, height: 390 }, isMobile: true, hasTouch: true } },
    { name: 'tablet', use: { viewport: { width: 768, height: 1024 }, hasTouch: true } },
  ],
});
