/* Keep context actions in their toolbar and secondary asset actions in a menu.
 * Reuse the existing controls so their permission, busy, and import checks stay
 * in editor.js. Image tiles and form fields deliberately keep their treatment.
 */
(()=>{
  'use strict';
  const byId=id=>document.getElementById(id);
  const dialog=byId('stock-dialog');
  const actions=dialog?.querySelector('.stock-import-actions');
  if(!actions||byId('stock-more-actions'))return;
  const imports=['stock-import-image','stock-import-reference','stock-import-background'].map(byId);
  const more=document.createElement('button');
  more.id='stock-more-actions';more.type='button';more.className='quiet-overflow-trigger';
  more.title='More import options';more.setAttribute('aria-label','More stock import options');
  more.setAttribute('aria-haspopup','menu');more.setAttribute('aria-expanded','false');
  more.setAttribute('aria-controls','stock-import-menu');more.textContent='\u2026';
  const menu=document.createElement('div');
  menu.id='stock-import-menu';menu.className='quiet-action-menu';
  menu.setAttribute('popover','auto');menu.setAttribute('role','menu');
  menu.setAttribute('aria-label','Stock import options');
  actions.append(more);dialog.append(menu);
  const menuItems=()=>[...menu.querySelectorAll('button')].filter(button=>!button.hidden&&!button.disabled);
  const closeMenu=()=>{if(menu.matches(':popover-open'))menu.hidePopover();};
  function positionMenu(){
    const bounds=more.getBoundingClientRect();
    const width=menu.offsetWidth,height=menu.offsetHeight;
    menu.style.left=Math.max(8,Math.min(innerWidth-width-8,bounds.right-width))+'px';
    menu.style.top=Math.max(8,Math.min(innerHeight-height-8,bounds.top-height-6))+'px';
  }
  function openMenu(){if(more.disabled)return;menu.showPopover();positionMenu();menuItems()[0]?.focus();}
  more.addEventListener('click',()=>menu.matches(':popover-open')?closeMenu():openMenu());
  more.addEventListener('keydown',event=>{
    if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();event.stopPropagation();openMenu();if(event.key==='ArrowUp')menuItems().at(-1)?.focus();}
  });
  menu.addEventListener('toggle',()=>more.setAttribute('aria-expanded',String(menu.matches(':popover-open'))));
  menu.addEventListener('keydown',event=>{
    const items=menuItems(),index=items.indexOf(document.activeElement);
    let next;
    if(event.key==='ArrowDown')next=(index+1)%items.length;
    else if(event.key==='ArrowUp')next=(index+items.length-1)%items.length;
    else if(event.key==='Home')next=0;
    else if(event.key==='End')next=items.length-1;
    else if(event.key==='Escape'){event.preventDefault();event.stopImmediatePropagation();closeMenu();more.focus();return;}
    else if(event.key==='Tab'){closeMenu();return;}
    else return;
    event.preventDefault();event.stopPropagation();items[next]?.focus();
  });
  imports.forEach(button=>button.addEventListener('click',closeMenu));
  // Handle the innermost popup before the expanded stock browser's Escape
  // handler. One Escape closes the menu; the next can collapse the browser.
  addEventListener('keydown',event=>{
    if(event.key==='Escape'&&menu.matches(':popover-open')){
      event.preventDefault();event.stopImmediatePropagation();closeMenu();more.focus();
    }
  },true);
  addEventListener('resize',closeMenu);
  dialog.addEventListener('close',closeMenu);
  let primaryId='';
  function sync(){
    const nextId=stockOrigin==='background'?'stock-import-background':stockOrigin==='reference'?'stock-import-reference':'stock-import-image';
    const primary=byId(nextId).hidden?byId('stock-import-image'):byId(nextId);
    if(primaryId!==primary.id){closeMenu();primaryId=primary.id;}
    for(const button of imports){
      const isPrimary=button===primary;
      const parent=isPrimary?actions:menu;
      if(button.parentElement!==parent){if(isPrimary)actions.insertBefore(button,more);else menu.append(button);}
      button.classList.toggle('primary',isPrimary);button.classList.toggle('secondary',!isPrimary);
      if(isPrimary){button.removeAttribute('role');button.removeAttribute('tabindex');}
      else{button.setAttribute('role','menuitem');button.tabIndex=-1;}
    }
    more.disabled=imports.filter(button=>button!==primary&&!button.hidden).every(button=>button.disabled);
    if(more.disabled)closeMenu();
  }
  const previous=updateStockControls;
  updateStockControls=function(...args){const value=previous.apply(this,args);sync();return value;};
  sync();
})();
