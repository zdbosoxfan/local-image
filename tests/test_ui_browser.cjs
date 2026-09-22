// Real-browser acceptance against an isolated running development backend.
// Requires Playwright; run: node tests/test_ui_browser.cjs [artifact-directory]
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const output = path.resolve(process.argv[2] || 'qa-artifacts');
fs.mkdirSync(output, { recursive: true });

// Deterministic synthetic photo: texture, color variation, and a removable mark.
// Generated from scratch; no private photo or network fixture is required.
function pngFixture() {
  const width=960, height=640, pixels=Buffer.alloc((width*3+1)*height);
  for(let y=0;y<height;y++)for(let x=0;x<width;x++) {
    const offset=y*(width*3+1)+1+x*3;
    const texture=Math.sin(x*.09)*3+Math.cos(y*.08)*3+((x*17+y*23)%9);
    const mark=Math.hypot(x-480,y-320)<24;
    const rgb=mark?[157,63,46]:[104+texture+y*.045,130+texture+y*.035,112+texture+y*.025];
    rgb.forEach((value,index)=>pixels[offset+index]=value);
  }
  const crc32=buffer=>{let crc=0xffffffff;for(const value of buffer){crc^=value;for(let i=0;i<8;i++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}return(crc^0xffffffff)>>>0;};
  const chunk=(name,data)=>{const type=Buffer.from(name),head=Buffer.alloc(4),tail=Buffer.alloc(4);head.writeUInt32BE(data.length);tail.writeUInt32BE(crc32(Buffer.concat([type,data])));return Buffer.concat([head,type,data,tail]);};
  const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);header[8]=8;header[9]=2;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',zlib.deflateSync(pixels)),chunk('IEND',Buffer.alloc(0))]);
}

async function main() {
  const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
  const page=await browser.newPage({viewport:{width:1440,height:960},deviceScaleFactor:1});
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  page.on('console',entry=>{if(entry.type()==='error')errors.push(entry.text());});
  const check=async(fn,label)=>{await page.waitForFunction(fn,null,{timeout:20000});console.log('PASS: '+label);};
  try {
    await page.goto('http://127.0.0.1:51247/remove');
    await check(()=>document.querySelector('#status').textContent.includes('Quick Heal ready'),'Quick Heal starts without an AI connection');
    assert.equal(await page.locator('#mode-heal').getAttribute('aria-pressed'),'true');
    const theme=await page.evaluate(()=>({canvas:getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),font:getComputedStyle(document.documentElement).fontFamily}));
    assert.equal(theme.canvas,'#171819','Design tokens reach the rendered page');
    assert.match(theme.font,/Segoe/,'Local interface typography is applied');
    await page.screenshot({animations:'disabled',path:path.join(output,'01-empty-desktop.png')});
    const controls=await page.locator('[id]').evaluateAll(nodes=>nodes.map(n=>n.id));
    assert.equal(new Set(controls).size,controls.length,'All element IDs are unique');
    await page.locator('#file').setInputFiles({name:'Synthetic texture.png',mimeType:'image/png',buffer:pngFixture()});
    await check(()=>!document.querySelector('#stage').hidden&&!document.querySelector('#viewport').getAttribute('aria-busy').includes('true'),'Opening a photo succeeds');
    await page.locator('#viewport').focus();
    await page.keyboard.press('r');
    const photo=await page.locator('#photo').boundingBox();
    const cx=photo.x+photo.width/2, cy=photo.y+photo.height/2;
    await page.mouse.move(cx-36,cy-36);await page.mouse.down();await page.mouse.move(cx+36,cy+36,{steps:8});await page.mouse.up();
    assert.equal(await page.locator('#remove').isEnabled(),true,'A painted selection enables healing');
    await page.locator('#mode-ai').focus();await page.keyboard.press('Space');
    assert.equal(await page.locator('#mode-ai').getAttribute('aria-pressed'),'true','Space activates a focused button rather than panning the canvas');
    assert.equal(await page.locator('#remove').isEnabled(),false,'Unavailable AI cannot submit a repair');
    assert.equal(await page.locator('#workflow-settings').isVisible(),true,'Unavailable AI offers settings');
    await page.locator('#mode-heal').click();
    assert.equal(await page.locator('#remove').isEnabled(),true,'Switching methods preserves selection');
    await page.screenshot({animations:'disabled',path:path.join(output,'02-selection-desktop.png')});
    await page.locator('#remove').click();
    await check(()=>document.querySelectorAll('#layers [data-layer-id]').length===1&&!document.querySelector('#remove').textContent.includes('Working'),'Real texture repair creates an editable layer');
    assert.equal(await page.locator('#progress-strip').isVisible(),false,'Progress clears after completion');
    await page.locator('#before').click();
    assert.equal(await page.locator('#before').textContent(),'Back to edits');
    await page.locator('#viewport').focus();await page.keyboard.press('Backslash');
    assert.equal(await page.locator('#before').getAttribute('aria-pressed'),'false');
    const visibility=page.locator('#layers [data-layer-id] .visibility').first();
    await visibility.click();await check(()=>document.querySelector('#layers [data-layer-id]').classList.contains('is-hidden'),'Layer can be hidden');
    await visibility.click();await check(()=>!document.querySelector('#layers [data-layer-id]').classList.contains('is-hidden'),'Layer can be restored');
    const downloaded=page.waitForEvent('download');await page.locator('#document-export').click();
    const copy=await downloaded;await copy.saveAs(path.join(output,'synthetic-repaired.png'));
    assert.ok(fs.statSync(path.join(output,'synthetic-repaired.png')).size>1000,'Export produces an image');
    const projectDownload=page.waitForEvent('download');await page.locator('#file-menu-trigger').click();await page.locator('#save-project').click();
    const project=await projectDownload;await project.saveAs(path.join(output,'synthetic-edit.lremove'));
    assert.ok(fs.statSync(path.join(output,'synthetic-edit.lremove')).size>1000,'Editable project downloads');
    await page.screenshot({animations:'disabled',path:path.join(output,'03-repaired-desktop.png')});
    await page.locator('#settings').click();assert.equal(await page.locator('#settings-dialog').isVisible(),true);
    await page.keyboard.press('Escape');assert.equal(await page.locator('#settings-dialog').isVisible(),false);
    // Keyboard pen path -> switch tools must refresh guidance immediately.
    await page.locator('#viewport').focus();await page.keyboard.press('p');
    await page.mouse.click(cx,cy);await page.mouse.click(cx+40,cy+30);
    await page.locator('[data-tool="rectangle"]').click();
    assert.notEqual(await page.locator('#workflow-title').textContent(),'Finish your selection');
    for(const [width,height] of [[1100,760],[800,560],[560,760],[430,800]]) {
      await page.setViewportSize({width,height});
      await page.screenshot({animations:'disabled',path:path.join(output,`viewport-${width}.png`)});
      const layout=await page.evaluate(()=>({body:document.body.scrollWidth,screen:innerWidth,canvas:document.querySelector('#viewport').getBoundingClientRect().toJSON(),apply:document.querySelector('#remove').getBoundingClientRect().toJSON()}));
      assert.ok(layout.body<=layout.screen,`No horizontal overflow at ${width}px`);
      assert.ok(layout.canvas.width>200&&layout.canvas.height>140,`Usable canvas at ${width}px`);
      assert.ok(layout.apply.right<=width&&layout.apply.left>=0,`Repair action remains inside the window at ${width}px`);
      assert.ok(layout.apply.bottom<=height&&layout.apply.top>=0,`Repair action is vertically visible at ${width}px`);
      const projectSave=page.locator('.project-save');await projectSave.scrollIntoViewIfNeeded();
      const saveBounds=await projectSave.boundingBox();
      assert.ok(saveBounds.y>=0&&saveBounds.y+saveBounds.height<=height,`Project save is reachable at ${width}px`);
      await page.locator('#remove').scrollIntoViewIfNeeded();
    }
    await page.setViewportSize({width:1440,height:960});
    await page.reload();await check(()=>document.querySelector('#mode-heal').getAttribute('aria-pressed')==='true','Chosen method survives reload');
    assert.deepEqual(errors,[],'No browser runtime or console errors');
    console.log('PASS: browser acceptance, real repair/export/project, comparison, layers, dialogs, guidance and four narrow layouts');
  } catch(error) { await page.screenshot({animations:'disabled',path:path.join(output,'failure.png')});throw error;
  } finally { await browser.close(); }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
