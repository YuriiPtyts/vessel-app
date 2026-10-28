import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/ui',
  use: {
    browserName: process.env.VESSEL_UI_BROWSER === 'chromium' ? 'chromium' : 'webkit',
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 960, height: 900 },
  },
  webServer: {
    timeout: 120_000,
    command:
      process.env.VESSEL_UI_PREVIEW === '1'
        ? 'npm run build && npm run preview -- --host 127.0.0.1 --port 1420'
        : 'npm run dev',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: !process.env.CI && process.env.VESSEL_UI_PREVIEW !== '1',
  },
  reporter: 'list',
});
