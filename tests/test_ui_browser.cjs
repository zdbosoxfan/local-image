// Real-browser acceptance against an isolated running development backend.
// Requires Playwright; run: node tests/test_ui_browser.cjs [artifact-directory]
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const output = path.resolve(process.argv[2] || 'qa-artifacts');
const testUrl = new URL('/remove',process.env.LOCAL_REMOVE_TEST_URL || 'http://127.0.0.1:51247').href;
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

async function verifyDesktopMenus(page) {
  const names=['file','edit','layer','select','view'];
  assert.equal(await page.locator('.menu:visible').count(),0,'Commands stay hidden until a menu is opened');
  assert.equal(await page.locator('.menu-foldout[open]').count(),0,'Menu details start collapsed');
  assert.equal(await page.locator('#output-format').isVisible(),false,'Copy format is not exposed on the workspace');
  assert.equal(await page.locator('#recent').isVisible(),false,'Recent sessions are not exposed on the workspace');
  const activeItems=menu=>page.locator('#'+menu+'-menu').evaluate(element=>{
    const items=[...element.querySelectorAll('button,summary,select')].filter(item=>{
      const closed=item.closest('details:not([open])');
      return !item.matches(':disabled')&&item.getClientRects().length&&getComputedStyle(item).visibility!=='hidden'
        &&(!closed||closed.querySelector(':scope > summary')===item);
    });
    return {count:items.length,index:items.indexOf(document.activeElement)};
  });
  for(const [index,name] of names.entries()) {
    const trigger=page.locator('#'+name+'-menu-trigger');
    await trigger.focus();await page.keyboard.press('ArrowDown');
    assert.deepEqual(await page.locator('.menu:visible').evaluateAll(elements=>elements.map(element=>element.id)),[name+'-menu'],'Exactly the requested menu opens');
    const initial=await activeItems(name);
    if(initial.count) {
      assert.equal(initial.index,0,'ArrowDown enters the first visible enabled command in '+name);
      await page.keyboard.press('ArrowDown');assert.equal((await activeItems(name)).index,1%initial.count,'Arrow navigation skips unavailable commands');
      await page.keyboard.press('ArrowUp');assert.equal((await activeItems(name)).index,0);
      await page.keyboard.press('End');assert.equal((await activeItems(name)).index,initial.count-1,'End reaches the last visible command');
      await page.keyboard.press('Home');assert.equal((await activeItems(name)).index,0);
    } else assert.equal(await trigger.evaluate(element=>element===document.activeElement),true,'An empty menu retains trigger focus');
    const next=names[(index+1)%names.length];
    await page.keyboard.press('ArrowRight');
    assert.deepEqual(await page.locator('.menu:visible').evaluateAll(elements=>elements.map(element=>element.id)),[next+'-menu'],'Right arrow switches to the adjacent menu');
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.menu:visible').count(),0,'Escape closes the menu');
    assert.equal(await page.locator('#'+next+'-menu-trigger').evaluate(element=>element===document.activeElement),true,'Escape returns focus to the visible menu trigger');
  }
  await page.locator('#file-menu-trigger').click();
  const format=page.locator('#file-menu .menu-foldout').filter({has:page.locator('[data-output-format="original"]')});
  const recent=page.locator('#file-menu .menu-foldout').filter({has:page.locator('#recent')});
  const formatSubmenu=format.locator('.submenu');
  assert.equal(await formatSubmenu.isVisible(),false,'Opening File does not expose the format submenu');
  assert.equal(await page.locator('#recent').isVisible(),false,'Opening File does not expand Recent');
  await format.locator('summary').focus();await page.keyboard.press('ArrowRight');
  assert.equal(await formatSubmenu.isVisible(),true,'Right arrow opens the format submenu');
  assert.equal(await page.locator('[data-output-format="original"]').evaluate(element=>element===document.activeElement),true,'Submenu receives focus on its first command');
  const bounds=await formatSubmenu.boundingBox(),parent=await page.locator('#file-menu').boundingBox();
  assert.ok(bounds.x>=parent.x+parent.width-3,'Format opens beside File instead of expanding its rows');
  const screen=page.viewportSize();assert.ok(bounds.x+bounds.width<=screen.width&&bounds.y+bounds.height<=screen.height,'Submenu remains within the desktop');
  await page.keyboard.press('ArrowLeft');
  assert.equal(await formatSubmenu.isVisible(),false,'Left arrow closes only the submenu');
  assert.equal(await page.locator('#file-menu').isVisible(),true);
  assert.equal(await format.locator('summary').evaluate(element=>element===document.activeElement),true,'Submenu returns focus to its owner');
  await page.keyboard.press('Enter');
  assert.equal(await formatSubmenu.isVisible(),true,'Enter also opens the submenu');
  await page.keyboard.press('ArrowDown');
  assert.equal(await page.locator('[data-output-format="png"]').evaluate(element=>element===document.activeElement),true,'Arrow navigation stays within the submenu');
  await page.keyboard.press('Enter');
  assert.equal(await page.locator('.menu:visible').count(),0,'Choosing a format closes the command menus');
  assert.equal(await page.locator('#output-format').inputValue(),'png','Format command updates the export choice');
  await page.reload();await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('Quick Heal ready'));
  await page.locator('#file-menu-trigger').click();await format.locator('summary').focus();await page.keyboard.press('ArrowRight');
  assert.equal(await page.locator('[data-output-format="png"]').getAttribute('aria-checked'),'true','The chosen output format survives a reload');
  assert.equal(await page.locator('#output-format').isVisible(),false,'Implementation select stays hidden');
  await recent.locator('summary').hover();
  await page.waitForFunction(()=>document.querySelector('#recent').closest('details').open&&document.querySelectorAll('#file-menu details[open]').length===1);
  assert.equal(await formatSubmenu.isVisible(),false,'Hovering a sibling closes the previous submenu');
  assert.equal(await recent.locator('.submenu').isVisible(),true,'Hover opens Recent beside File');
  await recent.locator('summary').focus();await page.keyboard.press('ArrowRight');await page.keyboard.press('Escape');
  assert.equal(await page.locator('#file-menu').isVisible(),true,'Escape closes the child before its parent menu');
  assert.equal(await recent.evaluate(element=>element.open),false);
  await format.locator('summary').focus();await page.keyboard.press('ArrowRight');await page.locator('[data-output-format="original"]').click();
  await page.locator('#file-menu-trigger').click();
  assert.equal(await page.locator('.menu-foldout[open]').count(),0,'Reopening a menu resets secondary details');
  await page.locator('#viewport').click({position:{x:8,y:8}});
  assert.equal(await page.locator('.menu:visible').count(),0,'An outside click dismisses the menu');
  await page.keyboard.press('Alt+e');
  assert.equal(await page.locator('#edit-menu').isVisible(),true,'Alt+E opens Edit');
  await page.keyboard.press('Tab');
  assert.equal(await page.locator('.menu:visible').count(),0,'Tab closes an open menu');
  assert.equal(await page.evaluate(()=>!!document.activeElement.closest('.menu')),false,'Tab leaves the hidden menu commands');
  await page.locator('#layers-options').click();
  assert.deepEqual(await page.locator('.menu:visible').evaluateAll(elements=>elements.map(element=>element.id)),['layer-menu'],'The Layers panel uses the same command menu');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#layers-options').evaluate(element=>element===document.activeElement),true,'Panel menu returns focus to its own trigger');
  await page.locator('#viewport').focus();await page.keyboard.press('F10');
  assert.equal(await page.locator('#file-menu-trigger').evaluate(element=>element===document.activeElement),true,'F10 enters the menu bar');
  await page.keyboard.press('F10');assert.equal(await page.locator('#viewport').evaluate(element=>element===document.activeElement),true,'F10 restores document focus');
  console.log('PASS: desktop menus, hidden foldouts, enabled-command keyboard navigation and focus return');
}

async function main() {
  const browser=await chromium.launch({headless:true,channel:process.env.BROWSER_CHANNEL||'msedge'});
  const page=await browser.newPage({viewport:{width:1440,height:900},deviceScaleFactor:1});
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  page.on('console',entry=>{if(entry.type()==='error')errors.push(entry.text());});
  const check=async(fn,label)=>{await page.waitForFunction(fn,null,{timeout:20000});console.log('PASS: '+label);};
  try {
    await page.goto(testUrl);
    await check(()=>document.querySelector('#status').textContent.includes('Quick Heal ready'),'Quick Heal starts without an AI connection');
    assert.equal(await page.locator('#mode-heal').getAttribute('aria-pressed'),'true');
    const theme=await page.evaluate(()=>({canvas:getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),font:getComputedStyle(document.documentElement).fontFamily}));
    assert.equal(theme.canvas,'#202020','Compact neutral desktop theme reaches the rendered page');
    assert.match(theme.font,/Segoe/,'Local interface typography is applied');
    await page.screenshot({animations:'disabled',path:path.join(output,'01-empty-desktop.png')});
    const controls=await page.locator('[id]').evaluateAll(nodes=>nodes.map(n=>n.id));
    assert.equal(new Set(controls).size,controls.length,'All element IDs are unique');
    await verifyDesktopMenus(page);
    await page.locator('#file').setInputFiles({name:'Synthetic texture.png',mimeType:'image/png',buffer:pngFixture()});
    await check(()=>!document.querySelector('#stage').hidden&&document.querySelector('#viewport').getAttribute('aria-busy')!=='true','Opening a photo succeeds');
    assert.equal(await page.locator('#folder-panel').isVisible(),false,'A single photo has no redundant 1/1 folder strip');
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
    await page.locator('#hand').click();
    assert.equal(await page.locator('.context-actions').isVisible(),false,'Pan hides repair actions');
    assert.equal(await page.locator('#brush-options').isVisible(),false,'Pan hides brush settings');
    await page.locator('#hand').click();
    assert.equal(await page.locator('.context-actions').isVisible(),true,'Leaving Pan restores repair actions');
    assert.equal(await page.locator('#remove').isEnabled(),true,'Pan preserves the pending selection');
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
    const layerStyles=await page.locator('#layers .layer').evaluateAll(elements=>elements.map(element=>{const style=getComputedStyle(element);return {radius:parseFloat(style.borderTopLeftRadius),margin:parseFloat(style.marginBottom)};}));
    assert.ok(layerStyles.length>0&&layerStyles.every(style=>style.radius<=2&&style.margin<=1),'Layers form a contiguous list rather than separate cards');
    const downloaded=page.waitForEvent('download');await page.locator('#file-menu-trigger').click();await page.locator('#save').click();
    const copy=await downloaded;await copy.saveAs(path.join(output,'synthetic-repaired.png'));
    assert.ok(fs.statSync(path.join(output,'synthetic-repaired.png')).size>1000,'Export produces an image');
    const projectDownload=page.waitForEvent('download');await page.locator('#file-menu-trigger').click();await page.locator('#save-project').click();
    const project=await projectDownload;await project.saveAs(path.join(output,'synthetic-edit.lremove'));
    assert.ok(fs.statSync(path.join(output,'synthetic-edit.lremove')).size>1000,'Editable project downloads');
    await page.screenshot({animations:'disabled',path:path.join(output,'03-repaired-desktop.png')});
    await page.locator('#edit-menu-trigger').click();await page.locator('#settings').click();assert.equal(await page.locator('#settings-dialog').isVisible(),true);
    assert.equal(await page.locator('.menu:visible').count(),0,'Opening Settings dismisses its menu');
    await page.keyboard.press('Escape');assert.equal(await page.locator('#settings-dialog').isVisible(),false);
    // Keyboard pen path -> switch tools must refresh guidance immediately.
    await page.locator('#viewport').focus();await page.keyboard.press('p');
    await page.mouse.click(cx,cy);await page.mouse.click(cx+40,cy+30);
    await page.locator('[data-tool="rectangle"]').click();
    assert.equal(await page.locator('#finish').isVisible(),false,'Switching from pen hides the path-only action');
    assert.equal(await page.locator('[data-tool="rectangle"]').getAttribute('aria-pressed'),'true');
    assert.equal(await page.locator('#brush-options').isVisible(),false,'Shape tools do not expose brush-only options');
    for(const [width,height] of [[1440,900],[1280,720],[1024,768],[800,560]]) {
      await page.setViewportSize({width,height});
      await page.screenshot({animations:'disabled',path:path.join(output,`viewport-${width}.png`)});
      const layout=await page.evaluate(()=>({body:document.body.scrollWidth,screen:innerWidth,canvas:document.querySelector('#viewport').getBoundingClientRect().toJSON(),apply:document.querySelector('#remove').getBoundingClientRect().toJSON()}));
      assert.ok(layout.body<=layout.screen,`No horizontal overflow at ${width}px`);
      assert.ok(layout.canvas.width>=width*.6&&layout.canvas.height>=height*.65,`Photo canvas retains a useful share of the ${width}×${height} desktop`);
      assert.ok(layout.apply.width>0&&layout.apply.height>0,`Repair action is rendered at ${width}px`);
      assert.ok(layout.apply.right<=width&&layout.apply.left>=0,`Repair action remains inside the window at ${width}px`);
      assert.ok(layout.apply.bottom<=height&&layout.apply.top>=0,`Repair action is vertically visible at ${width}px`);
      await page.locator('#file-menu-trigger').click();
      const projectSave=page.locator('#save-project');await projectSave.scrollIntoViewIfNeeded();
      const saveBounds=await projectSave.boundingBox();
      assert.ok(saveBounds.y>=0&&saveBounds.y+saveBounds.height<=height,`Project save is reachable at ${width}px`);
      await page.keyboard.press('Escape');
    }
    await page.setViewportSize({width:1440,height:900});
    await page.reload();await check(()=>document.querySelector('#mode-heal').getAttribute('aria-pressed')==='true','Chosen method survives reload');
    assert.deepEqual(errors,[],'No browser runtime or console errors');
    console.log('PASS: browser acceptance, real repair/export/project, comparison, flat layers, contextual controls and four desktop layouts');
  } catch(error) { await page.screenshot({animations:'disabled',path:path.join(output,'failure.png')});throw error;
  } finally { await browser.close(); }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
