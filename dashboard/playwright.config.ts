import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright config for the GHOST ADE dashboard (T-4.10.3).
 *
 * Three device profiles cover the responsive breakpoints defined in
 * +layout.svelte:
 *   - Desktop Chrome  (>1024 px)  full sidebar
 *   - iPad Pro 11     (641–1024)  collapsed sidebar
 *   - iPhone 14       (<640 px)   sidebar hidden, bottom nav
 */
export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: 'http://localhost:4173',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'npm run preview -- --host 127.0.0.1 --port 4173',
    port: 4173,
    reuseExistingServer: true,
  },
  projects: [
    { name: 'Desktop Chrome', use: { ...devices['Desktop Chrome'], browserName: 'chromium' } },
    { name: 'iPhone 14', use: { ...devices['iPhone 14'] } },
    { name: 'iPad Pro 11', use: { ...devices['iPad Pro 11'] } },
  ],
});
