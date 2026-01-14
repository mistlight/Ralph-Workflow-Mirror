// Playwright script to capture baseline screenshots
import { chromium } from 'playwright';

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

    const baseUrl = 'file:///Users/mistlight/Projects/Ralph-Pages/index.html';

    for (const viewport of viewports) {
        console.log(`Capturing ${viewport.name}...`);

        await page.setViewportSize({ width: viewport.width, height: viewport.height });
        await page.goto(baseUrl);

        // Wait for animations to complete
        await page.waitForTimeout(2000);

        // Scroll to capture full page
        await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
        await page.waitForTimeout(500);

        // Capture full page screenshot
        await page.screenshot({
            path: `.screenshots/baseline/${viewport.name}-full.png`,
            fullPage: true
        });

        // Go back to top and capture hero section
        await page.evaluate(() => window.scrollTo(0, 0));
        await page.waitForTimeout(500);

        await page.screenshot({
            path: `.screenshots/baseline/${viewport.name}-hero.png`
        });

        console.log(`  ✓ Captured ${viewport.name}`);
    }

    await browser.close();
    console.log('\nBaseline screenshots complete!');
})();
