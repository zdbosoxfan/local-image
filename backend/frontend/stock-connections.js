/* Source selection remains a normal dropdown. Connection details are opt-in. */
(() => {
  const $=id=>document.getElementById(id), select=$('stock-provider'), form=$('stock-search-form');
  if(!form)return;
  const row=document.createElement('div');row.className='stock-source-row';
  const label=document.createElement('label');label.htmlFor=select.id;label.textContent='Source';
  const settings=document.createElement('button');settings.type='button';settings.className='stock-connection-toggle';settings.title='Stock source connection';settings.setAttribute('aria-label','Stock source connection');
  settings.innerHTML='<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 5h14M3 10h14M3 15h14M7 3v4m6 1v4m-5 1v4"/></svg>';
  row.append(label,select,settings);form.after(row);
  const connection=document.createElement('div');connection.id='stock-connection';connection.hidden=true;
  connection.innerHTML='<label for="stock-api-key">API key</label><input id="stock-api-key" type="password" autocomplete="off" spellcheck="false"><div class="stock-connection-actions"><a id="stock-get-key" target="_blank" rel="noopener noreferrer">Get a key</a><button id="stock-disconnect" type="button">Disconnect</button><button id="stock-connect" type="button" class="primary">Connect</button></div><p id="stock-connection-note" role="status"></p>';
  row.after(connection);
  const credit=document.createElement('a');credit.className='stock-provider-credit';credit.target='_blank';credit.rel='noopener noreferrer';connection.after(credit);
  let saving=false,previousProvider='';
  function sync(){
    const provider=stockProviders.find(item=>item.id===select.value),needsKey=!!provider?.needs_key;
    settings.hidden=!needsKey;settings.disabled=saving||stockLoading||stockImporting;
    const changed=previousProvider!==select.value;previousProvider=select.value;
    if(changed){connection.hidden=!needsKey||provider.available!==false;$('stock-api-key').value='';$('stock-connection-note').textContent='';}
    if(!needsKey)connection.hidden=true;
    settings.setAttribute('aria-expanded',String(!connection.hidden));
    $('stock-disconnect').hidden=!needsKey||provider.available===false;
    $('stock-api-key').disabled=saving;$('stock-connect').disabled=saving;$('stock-disconnect').disabled=saving;
    $('stock-api-key').setAttribute('aria-label',(provider?.label||'Stock')+' API key');
    $('stock-get-key').href=provider?.connect_url||'https://www.pexels.com/api/';
    credit.hidden=!needsKey;credit.textContent=needsKey?'Photos from '+provider.label:'';
    credit.href=select.value==='unsplash'?'https://unsplash.com/?utm_source=local_image&utm_medium=referral':'https://www.pexels.com/';
    if(needsKey&&provider.available===false&&!saving){$('stock-empty').textContent='Connect '+provider.label+' to search.';$('stock-status').textContent='';}
  }
  settings.onclick=()=>{connection.hidden=!connection.hidden;settings.setAttribute('aria-expanded',String(!connection.hidden));if(!connection.hidden)$('stock-api-key').focus();};
  async function save(disconnect){
    if(saving)return;const provider=select.value,key=disconnect?'':$('stock-api-key').value.trim();
    if(!disconnect&&!key){$('stock-api-key').focus();return;}
    saving=true;sync();
    try{
      const saved=await(await api('/api/local-remove/stock/connection/'+encodeURIComponent(provider),{method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify({key})})).json();
      $('stock-api-key').value='';const response=await(await api('/api/local-remove/stock/providers')).json();stockProviders=response.providers;
      $('stock-connection-note').textContent=disconnect?(saved.connected?'The environment still supplies a key. Remove it there to disconnect.':'Disconnected.'):'Saved on this PC, encrypted for your Windows account.';
      connection.hidden=!disconnect;if(!disconnect&&$('stock-query').value.trim())await searchStock(0);
    }catch(error){$('stock-connection-note').textContent=error.message;connection.hidden=false;}
    finally{saving=false;updateStockControls();}
  }
  $('stock-connect').onclick=()=>save(false);$('stock-disconnect').onclick=()=>save(true);
  $('stock-api-key').onkeydown=event=>{if(event.key==='Enter'){event.preventDefault();save(false);}};
  select.addEventListener('change',sync);
  const previous=updateStockControls;updateStockControls=function(...args){const result=previous.apply(this,args);sync();return result;};
  // Always include a visible photographer link for Pexels/Unsplash thumbnails.
  const previousRender=renderStockResults;renderStockResults=function(...args){const result=previousRender.apply(this,args);
    $('stock-results').querySelectorAll('.stock-result').forEach((button,index)=>{const item=stockResults[index];if(!item||item.provider==='openverse')return;
      const author=document.createElement('a');author.className='stock-photo-credit';author.href=item.creator_url||item.source_url;author.target='_blank';author.rel='noopener noreferrer';author.textContent=item.creator;author.title='Photo by '+item.creator+' on '+(item.provider==='pexels'?'Pexels':'Unsplash');
      const cell=document.createElement('div');cell.className='stock-result-cell';button.before(cell);cell.append(button,author);
    });return result;};
  sync();
})();
