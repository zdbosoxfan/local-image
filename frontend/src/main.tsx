import { createRoot } from 'react-dom/client';
import { FluentProvider, webDarkTheme } from '@fluentui/react-components';
import { createDOMRenderer, RendererProvider } from '@griffel/react';
import { App } from './App.tsx';
import { createEditorController } from './editorController.ts';
import './styles.css';

const bootstrap = window.__LOCAL_IMAGE_BOOTSTRAP__;
const legacy = window.LocalImageLegacyEditor;
const shell = document.getElementById('react-shell-root');
const layers = document.getElementById('react-layers-root');
if (!bootstrap?.nonce || !bootstrap.token || !legacy || !shell || !layers) {
  throw new Error('React editor startup contract is unavailable. Restart with LOCAL_IMAGE_FRONTEND=legacy.');
}
const renderer = createDOMRenderer(document, { styleElementAttributes: { nonce: bootstrap.nonce } });
const controller = createEditorController(legacy, bootstrap.token);
const theme = {
  ...webDarkTheme,
  fontFamilyBase: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
  fontSizeBase100: 'var(--ui-note)',
  fontSizeBase200: 'var(--ui-note)',
  fontSizeBase300: 'var(--ui-font)',
  fontSizeBase400: 'var(--ui-font)',
  lineHeightBase100: 'calc(var(--ui-note) * 1.4)',
  lineHeightBase200: 'calc(var(--ui-note) * 1.4)',
  lineHeightBase300: 'calc(var(--ui-font) * 1.4)',
  lineHeightBase400: 'calc(var(--ui-font) * 1.4)',
  borderRadiusMedium: '3px',
  colorNeutralBackground1: '#22252b',
  colorNeutralBackground2: '#1c1f25',
  colorNeutralBackground3: '#2b2f36',
};
shell.dataset.reactOwned = 'true';
layers.dataset.reactOwned = 'true';
// A custom Portal mount is outside the provider's DOM ancestry. Share this one
// provider's generated theme class (and nonced theme rule) with its owned mount.
createRoot(shell).render(<RendererProvider renderer={renderer}><FluentProvider theme={theme} className="li-provider" ref={element => { if (element) layers.className = element.className; }}><App controller={controller} layersMount={layers} /></FluentProvider></RendererProvider>);
document.body.dataset.reactReady = 'true';
