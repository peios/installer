import { defineConfig } from '@playwright/test';
const port = Number(process.env.RECOVERY_PORT || 7797);
const results = process.env.RECOVERY_RUN || 'candidate';
export default defineConfig({
    testDir: '.', testMatch: 'recovery.spec.mjs', fullyParallel: false, workers: 1, retries: 0,
    timeout: 30000, expect: { timeout: 5000 },
    outputDir: `results/${results}/test-results`,
    reporter: [['line'], ['json', { outputFile: `results/${results}/report.json` }], ['html', { outputFolder: `results/${results}/html`, open: 'never' }]],
    use: {
        baseURL: `http://127.0.0.1:${port}`, browserName: 'chromium', reducedMotion: 'reduce',
        serviceWorkers: 'block', trace: 'retain-on-failure', screenshot: 'only-on-failure',
        launchOptions: process.env.RECOVERY_CHROMIUM ? { executablePath: process.env.RECOVERY_CHROMIUM } : {},
    },
    projects: [
        { name: 'wide-dark', use: { viewport: { width: 1280, height: 800 }, colorScheme: 'dark' } },
        { name: 'wide-light-preference', use: { viewport: { width: 1280, height: 800 }, colorScheme: 'light' } },
        { name: 'narrow-dark', use: { viewport: { width: 390, height: 844 }, colorScheme: 'dark' } },
        { name: 'narrow-light-preference', use: { viewport: { width: 390, height: 844 }, colorScheme: 'light' } },
    ],
    webServer: { command: 'node server.mjs', url: `http://127.0.0.1:${port}/__health`, reuseExistingServer: false, timeout: 10000 },
});
