// Coordinate-driven smoke exercise. Review captured pixels; this is not an assertion suite.
const {chromium}=require('../web/node_modules/playwright');
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--enable-unsafe-webgpu','--use-angle=swiftshader']});
 const page=await browser.newPage({viewport:{width:1240,height:820}});
 const dir='docs/design/ui-refresh/interactions'; require('fs').mkdirSync(dir,{recursive:true});
 const shot=async name=>{await page.waitForTimeout(250);await page.screenshot({path:`${dir}/${name}.png`});};
 await page.goto('http://localhost:8080');await page.waitForTimeout(5000);
 await page.mouse.click(1124,117);await page.keyboard.type('xor');await shot('01-search');
 await page.mouse.click(1108,207);await page.mouse.click(500,450);await page.keyboard.press('Escape');await shot('02-placement');
 await page.mouse.click(156,46);await shot('03-undo');await page.mouse.click(180,46);await shot('04-redo');
 await page.keyboard.press('v');await page.mouse.click(500,450);await shot('05-selection');
 // Wire tool: a free orthogonal route, then cancel rather than altering the fixture.
 await page.keyboard.press('w');await page.mouse.click(450,470);await page.mouse.click(550,470);await page.keyboard.press('Escape');
 // Resize left pane; inspect serialized persistence after reload.
 await page.mouse.move(210,350);await page.mouse.down();await page.mouse.move(240,350,{steps:10});await page.mouse.up();
 await page.waitForTimeout(1500);
 const before=await page.evaluate(()=>localStorage.getItem('rgate.v1.settings.json'));
 await page.reload();await page.waitForTimeout(5000);await shot('06-reload-panel');
 const after=await page.evaluate(()=>localStorage.getItem('rgate.v1.settings.json'));
 require('fs').writeFileSync(`${dir}/persistence.json`,JSON.stringify({before:JSON.parse(before),after:JSON.parse(after)},null,2));
 // Reload correctly offers recovery for the edited fixture; discard it before simulation.
 await page.mouse.click(663,207);await page.waitForTimeout(300);
 await page.mouse.click(394,79);await page.waitForTimeout(300);
 await page.mouse.click(509,46);await page.waitForTimeout(1500);await page.mouse.move(700,600);await shot('07-running');
 await page.mouse.click(531,46);await page.mouse.click(214,311);await shot('08-paused-probe');
 await browser.close();
})();
