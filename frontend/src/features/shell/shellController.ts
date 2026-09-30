export interface ShellUiState {
  menu: string | null; popup: 'layers'|'background'|null; menuFocus: { name: string; sequence: number } | null;
  rename: { id: string; sequence: number } | null; cutoutProperties: boolean; backgroundGenerator: boolean;
}
export function createShellController(ports: { menuChanged(open: boolean): void; focusCanvas(): void }) {
  let sequence = 0;
  let state: ShellUiState = Object.freeze({menu:null,popup:null,menuFocus:null,rename:null,cutoutProperties:false,backgroundGenerator:false});
  const listeners = new Set<()=>void>();
  const publish = (patch: Partial<ShellUiState>) => {state=Object.freeze({...state,...patch}); for(const listener of listeners)listener();};
  const setMenu = (menu:string|null) => {if(state.menu===menu)return; publish({menu,...(menu?{popup:null}:{})});ports.menuChanged(state.menu!==null||state.popup!==null);};
  return {
    getSnapshot:()=>state,subscribe(listener:()=>void){listeners.add(listener);return()=>{listeners.delete(listener);};},
    setMenu,closeMenus:()=>{publish({menu:null,popup:null});ports.menuChanged(false);},
    isMenuOpen:()=>state.menu!==null||state.popup!==null,
    setPopup(popup:'layers'|'background',open:boolean){if(open)publish({popup,menu:null});else if(state.popup===popup)publish({popup:null});ports.menuChanged(state.menu!==null||state.popup!==null);},
    openMenu(name:string){publish({menuFocus:Object.freeze({name,sequence:++sequence})});setMenu(name);},
    focusMenu(name='File'){publish({menuFocus:Object.freeze({name,sequence:++sequence})});},
    toggleMenuFocus(focused:boolean){if(state.menu||focused){setMenu(null);ports.focusCanvas();}else publish({menuFocus:Object.freeze({name:'File',sequence:++sequence})});},
    renameLayer(id:string){publish({rename:Object.freeze({id,sequence:++sequence})});},
    showCutoutProperties(value=true){publish({cutoutProperties:value});},
    showBackgroundGenerator(value=true){publish({backgroundGenerator:value});},
  };
}
export type ShellController = ReturnType<typeof createShellController>;
