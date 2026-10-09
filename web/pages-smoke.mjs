import { chromium } from "playwright";
import assert from "node:assert/strict";

const url = process.env.RGATE_WEB_URL;
assert.ok(url, "RGATE_WEB_URL must point at the hosted application");
const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH || undefined,
  args: [
    "--enable-unsafe-webgpu",
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
  ],
});
try {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const response = await page.goto(url);
  assert.equal(response.status(), 200);
  await page.waitForSelector("body[data-rgate-ready=true]", { timeout: 60000 });
  await page.waitForTimeout(1500);
  assert.equal(await page.locator("canvas").count(), 1);
  await page.mouse.click(600, 600);
  await page.keyboard.press("Space");
  await page.waitForTimeout(500);
  await page.keyboard.press("Space");
  assert.deepEqual(errors, []);
  console.log(
    "PASS: Pages application subpath, WASM loading, rendering and simulation controls",
  );
} finally {
  await browser.close();
}
