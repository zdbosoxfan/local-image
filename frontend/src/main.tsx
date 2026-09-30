import {createRoot} from 'react-dom/client';
import {FluentProvider} from '@fluentui/react-components';
import {createDOMRenderer,RendererProvider} from '@griffel/react';
import {createApplication} from './application.ts';
import {FullApp} from './FullApp.tsx';
import {localImageTheme} from './theme.ts';
import './styles.css';
import './editor.css';

const bootstrap=window.__LOCAL_IMAGE_BOOTSTRAP__;
const root=document.getElementById('react-app-root');
if(!bootstrap?.nonce||!bootstrap.token||!root)throw Error('The editor startup contract is unavailable.');
const renderer=createDOMRenderer(document,{styleElementAttributes:{nonce:bootstrap.nonce}});
const application=createApplication(bootstrap.token);
// The single store/controller boundary is also available to the desktop's
// diagnostics. Native transport and credentials stay private in their owners.
declare global {interface Window {
 LocalImageEditor?: Pick<typeof application.props.controller,'getSnapshot'|'subscribe'|'commands'> & {canvas:Pick<typeof application.canvas,'getSnapshot'>};
 LocalImageReactFeatures?: Pick<typeof application.props,'settings'|'models'|'assets'|'generation'|'batch'>;
}}
window.LocalImageEditor=Object.freeze({getSnapshot:application.props.controller.getSnapshot,subscribe:application.props.controller.subscribe,commands:application.props.controller.commands,canvas:Object.freeze({getSnapshot:application.canvas.getSnapshot})});
window.LocalImageReactFeatures=Object.freeze({settings:application.props.settings,models:application.props.models,assets:application.props.assets,generation:application.props.generation,batch:application.props.batch});
createRoot(root).render(<RendererProvider renderer={renderer}><FluentProvider theme={localImageTheme} className="li-provider" ref={element=>{
 if(element){for(const mount of Object.values(application.props.mounts))mount.className=element.className;document.getElementById('editor-layout')!.className='li-editor-layout '+element.className;}
}}><FullApp {...application.props}/></FluentProvider></RendererProvider>);
void application.initialize().catch(error=>application.props.controller.report(error instanceof Error?error.message:'The editor could not finish starting.',true));
window.addEventListener('pagehide',event=>{if(!event.persisted)application.dispose();else application.canvas.resetTransientInput();});
