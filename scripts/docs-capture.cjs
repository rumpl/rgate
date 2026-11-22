// Genuine GPUI captures. Build first with scripts/build-web.sh and serve target/web.
// Component fixtures: cargo run -p rgate-format --example docs-fixtures -- target/docs-fixtures
// Usage: node scripts/docs-capture.cjs [editor|components|waveforms|lc3|terminal|tiny-vga|led-properties|all]
const { chromium } = require('../web/node_modules/playwright');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const output = path.join(root, 'target/docs-captures');
const settings = {version:1, appearance:'Light', theme:'Classic', sidebar_width:210,
  components_width:205, modules_height:196, bottom_height:176};
const pause = (page, ms = 350) => page.waitForTimeout(ms);
async function openFile(page, file) {
  const chooser = page.waitForEvent('filechooser');
  await page.mouse.click(44,46);
  await (await chooser).setFiles(path.join(root, file));
  await pause(page, 900);
}
async function shot(page, name, clip) {
  await page.mouse.move(1200,980);
  await pause(page);
  await page.screenshot({path:path.join(output, `${name}.png`), ...(clip ? {clip} : {})});
}
async function fresh(browser, preferences = {}) {
  const page = await browser.newPage({viewport:{width:1440,height:1000}, deviceScaleFactor:1});
  page.on('pageerror', error => { throw error; });
  page.on('dialog', dialog => dialog.accept());
  await page.addInitScript(value => localStorage.setItem('rgate.v1.settings.json', JSON.stringify(value)),
    {...settings, ...preferences});
  await page.goto(process.env.RGATE_URL || 'http://localhost:8080');
  await pause(page, 3500);
  return page;
}
async function editor(browser) {
  const page = await fresh(browser);
  await shot(page, 'editor');
  await page.close();
}
async function components(browser) {
  const fixtures = JSON.parse(fs.readFileSync(path.join(root,'target/docs-fixtures/manifest.json')));
  fs.mkdirSync(path.join(output, 'components'), {recursive:true});
  // Persistence retains at most 32 documents; keep fixture batches below that limit.
  for (let offset=0; offset<fixtures.length; offset+=16) {
    const batch = fixtures.slice(offset,offset+16);
    const documents = Object.fromEntries(batch.map(({name}) => [`file:${name}.rgate`, {
      active_module:'main', views:{'module:main':{zoom:2,pan:{x:512,y:350}}}
    }]));
    const page = await fresh(browser, {documents});
    for (const {name,min,max} of batch) {
      await openFile(page, `target/docs-fixtures/${name}.rgate`);
      // Fixed view: canvas origin (211,94), world origin (512,350), scale 2.
      // Extra right padding retains bus labels extending outside gate bounds.
      const margin = 36;
      const clip = {
        x:Math.floor(723+min.x*2-margin), y:Math.floor(444+min.y*2-margin),
        width:Math.ceil((max.x-min.x)*2+margin+100),
        height:Math.ceil((max.y-min.y)*2+margin*2)
      };
      await shot(page, `components/${name}`, clip);
      console.log(`Captured ${name}`);
    }
    await page.close();
  }
}
async function waveforms(browser) {
  const page = await fresh(browser, {bottom_height:320});
  await page.mouse.click(392,79);
  for (const y of [310,430,454]) await page.mouse.click(184,y);
  await page.mouse.click(335,667);
  for (let i=0; i<12; i++) {
    await page.keyboard.press('Tab');
    if (i%2 === 1) await page.mouse.click(416,225);
  }
  await page.mouse.click(260,693); // Fit retained traces.
  await page.mouse.click(700,790);
  await page.keyboard.down('Shift');
  await page.mouse.click(950,790);
  await page.keyboard.up('Shift');
  await shot(page, 'waveforms');
  await page.close();
}
async function ledProperties(browser) {
  const page = await fresh(browser);
  await openFile(page, 'target/docs-fixtures/led.rgate');
  await page.mouse.dblclick(723,445);
  await shot(page, 'led-properties');
  await page.close();
}
async function lc3(browser) {
  const page = await fresh(browser, {modules_height:280});
  await openFile(page, 'examples/lc3.rgate');
  await shot(page, 'lc3');
  await page.close();
}
async function terminal(browser) {
  const page = await fresh(browser, {modules_height:280});
  await openFile(page, 'examples/lc3.rgate');
  await page.mouse.click(392,79);
  await page.mouse.click(604,46); // Advance one clock in reset.
  await pause(page);
  await page.mouse.click(467,215); // Release the root RESET_N switch.
  await page.mouse.click(604,46);
  await pause(page);
  await page.mouse.dblclick(975,309);
  await pause(page);
  for (let i=0; i<450; i++) {
    await page.mouse.click(539,158);
    await pause(page,40);
  }
  await pause(page);
  await shot(page, 'terminal');
  await page.close();
}
async function tinyVga(browser) {
  const page = await fresh(browser);
  await openFile(page, 'examples/tiny-vga.rgate');
  await page.mouse.click(392,79);
  await page.mouse.click(604,46); // Advance one clock in reset.
  await pause(page);
  await page.mouse.click(370,266); // Release the root RESET_N switch.
  await page.mouse.click(604,46);
  await pause(page);
  await page.mouse.dblclick(963,228);
  await pause(page);
  for (let i=0; i<2; i++) {
    await page.mouse.click(550,575);
    await pause(page, 600);
  }
  await shot(page, 'tiny-vga');
  await page.close();
}
(async () => {
  fs.mkdirSync(output, {recursive:true});
  const browser = await chromium.launch({headless:true,args:['--enable-unsafe-webgpu','--use-angle=swiftshader']});
  try {
    const tasks = {editor, components, waveforms, lc3, terminal, 'tiny-vga':tinyVga, 'led-properties':ledProperties};
    const selected = process.argv[2] || 'all';
    if (selected !== 'all' && !tasks[selected]) throw new Error(`Unknown capture: ${selected}`);
    for (const [name, task] of Object.entries(tasks)) if (selected === 'all' || name === selected) {
      await task(browser);
    }
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
