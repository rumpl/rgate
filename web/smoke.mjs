import { chromium } from 'playwright';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const fixture = fileURLToPath(new URL('../examples/full-adder.rgate', import.meta.url));
const browser = await chromium.launch({
    executablePath: process.env.CHROME_PATH || undefined,
    headless: true,
    args: ['--enable-unsafe-webgpu', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
});
try {
    const page = await browser.newPage({ viewport: { width: 1240, height: 820 }, acceptDownloads: true });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('dialog', dialog => dialog.accept());
    await page.goto(process.env.RGATE_WEB_URL || 'http://127.0.0.1:8080');
    await page.waitForSelector('body[data-rgate-ready=true]', { timeout: 45000 });
    await page.waitForTimeout(3000);
    assert.equal(await page.locator('canvas').count(), 1);
    const save = async () => {
        const pending = page.waitForEvent('download');
        await page.mouse.click(83, 45);
        const download = await pending;
        return JSON.parse(await readFile(await download.path(), 'utf8'));
    };
    assert.equal((await save()).circuit.modules[0].gates.length, 12);
    await page.mouse.click(500, 500);
    await page.keyboard.press('a');
    await page.mouse.click(700, 460);
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
    assert.equal((await save()).circuit.modules[0].gates.length, 13);
    await page.keyboard.press('Space');
    await page.waitForTimeout(600);
    await page.mouse.click(595, 45);
    const chooser = page.waitForEvent('filechooser');
    await page.mouse.click(55, 45);
    await (await chooser).setFiles(fixture);
    await page.waitForTimeout(500);
    assert.equal((await save()).circuit.modules[0].gates.length, 12);
    await page.mouse.click(358, 14);
    await page.waitForTimeout(150);
    await page.mouse.click(395, 40);
    await page.mouse.click(750, 480);
    await page.keyboard.press('Escape');
    await page.waitForTimeout(150);
    assert.equal((await save()).circuit.modules[0].gates.length, 13);
    assert.deepEqual(errors, []);
    console.log('PASS: browser rendering, placement, simulation, upload/download, Make menu');
} finally {
    await browser.close();
}
