// Real GPUI pixel captures; no DOM assertions are used to validate the canvas.
const {chromium} = require('../web/node_modules/playwright');
const fs = require('fs');
(async () => {
 const browser = await chromium.launch({headless:true,args:['--enable-unsafe-webgpu','--use-angle=swiftshader']});
 const out=process.argv[2]; fs.mkdirSync(out,{recursive:true});
 for(const [width,height] of [[1240,820],[1020,620]]) for(const appearance of ['Light','Dark']) {
 const page = await browser.newPage({viewport:{width,height},deviceScaleFactor:1});
 page.on('pageerror', e=>console.error(e));
 await page.addInitScript(({appearance})=>localStorage.setItem('rgate.v1.settings.json',JSON.stringify({version:1,appearance,theme:'Classic',sidebar_width:210,components_width:205,modules_height:196,bottom_height:176})),{appearance});
 await page.goto('http://localhost:8080'); await page.waitForTimeout(5000);
 await page.screenshot({path:`${out}/${width}-${appearance.toLowerCase()}-edit.png`});
 // Simulate tab starts paused. Probe A, Cout and Sum using the Nets controls.
 await page.mouse.click(394,79); await page.waitForTimeout(300);
 for(const y of [311,431,455]) await page.mouse.click(184,y);
 await page.waitForTimeout(300);
 await page.screenshot({path:`${out}/${width}-${appearance.toLowerCase()}-waveforms.png`});
 await page.close();
 }
 await browser.close();
})();
