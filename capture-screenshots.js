// Unified Playwright script to capture screenshots
// Usage: node capture-screenshots.js [baseline|final]
import { chromium } from 'playwright';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Get output directory from command line argument (default: 'baseline')
const outputDir = process.argv[2] || 'baseline';

(async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext();
    const page = await context.newPage();

    // Set viewport sizes for different breakpoints
    const viewports = [
        { name: 'desktop-1920x1080', width: 1920, height: 1080 },
        { name: 'laptop-1366x768', width: 1366, height: 768 },
        { name: 'tablet-768x1024', width: 768, height: 1024 },
        { name: 'mobile-375x667', width: 375, height: 667 }
    ];

    // Use relative path for portability
    const baseUrl = 'file://' + join(__dirname, 'index.html');

    for (const viewport of viewports) {
        console.log(`Capturing ${viewport.name}...`);

        await page.setViewportSize({ width: viewport.width, height: viewport.height });
        await page.goto(baseUrl);

        // Wait for animations to complete - wait for hero animation to finish
        await page.waitForSelector('#hero h1', { state: 'attached', timeout: 5000 });
        // Wait for character animation to complete (the longest animation)
        await page.waitForFunction(() => {
            const heroTitle = document.querySelector('#hero h1');
            return heroTitle && heroTitle.textContent.length > 0;
        }, { timeout: 5000 });
        // Additional wait for any CSS transitions to finish
        await page.waitForTimeout(500);

        // Scroll to capture full page
        await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
        // Wait for any scroll-triggered animations
        await page.waitForTimeout(500);

        // Capture full page screenshot
        await page.screenshot({
            path: `.screenshots/${outputDir}/${viewport.name}-full.png`,
            fullPage: true
        });

        // Go back to top and capture hero section
        await page.evaluate(() => window.scrollTo(0, 0));
        // Wait for scroll to complete
        await page.waitForTimeout(300);

        await page.screenshot({
            path: `.screenshots/${outputDir}/${viewport.name}-hero.png`
        });

        console.log(`  ✓ Captured ${viewport.name}`);
    }

    await browser.close();
    console.log(`\n${outputDir.charAt(0).toUpperCase() + outputDir.slice(1)} screenshots complete!`);
})();
