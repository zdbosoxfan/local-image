/** Source ownership map for the staged extraction. References use function
 * names rather than drifting line numbers. The parity suite reads the actual
 * checkout, not README screenshots or a historical UI mock. Integration must
 * remove the corresponding legacy listeners/functions before mounting this
 * controller; keeping both active is unsupported. */
export const canvasExtraction = Object.freeze({
  sourceBaseline: 'e42aab6a49f222796dbbc9dc1ebf4211f46e9876',
  camera: {
    source: 'backend/frontend/editor.js',
    functions: [
      'pixelRatio',
      'photoZoom',
      'setSizes',
      'clampCamera',
      'fitImage',
      'localPoint',
      'setPhotoZoom',
      'applyCamera',
    ],
    target: 'canvasMath.ts + canvasController.ts',
  },
  selection: {
    source: 'backend/frontend/editor.js',
    functions: [
      'paintMask',
      'refreshMask',
      'snapshot',
      'coord',
      'mode',
      'stroke',
      'shape',
      'shapeDraft',
      'penDraft',
      'finishPen',
      'endGesture',
      'startPan',
      'undoEdit selection branch',
      'redoEdit selection branch',
      'selectionPayload',
    ],
    target: 'canvasController.ts',
  },
  browserView: {
    source: 'backend/frontend/editor.js',
    functions: [
      'rememberCurrentView',
      'restoreView',
      'pointer/wheel/resize listeners',
      'Space mid-stroke transition',
      'resetTransientInput',
    ],
    target: 'canvasController.ts',
  },
  stackTransforms: {
    source: 'backend/frontend/layers-studio.js',
    functions: [
      'pointOnLayer',
      'geometry',
      'drawHandles',
      'loadMoveAssets',
      'paintMoving',
      'pointerdown/move/up/cancel/lostpointercapture',
      'finishMove',
    ],
    target: 'canvasMath.ts + canvasController.ts',
  },
  legacyCutout: {
    source: 'backend/frontend/editor.js',
    functions: ['prepareTransformAssets', 'paintTransformPreview', 'legacy transform pointer branch'],
    target: 'canvasController.ts optional commitLegacyCutoutTransform port',
  },
  intentionalBoundaryChanges: [
    'No toolbar, menu, dialog or input element lookups; browser interaction values and commands are explicit.',
    'One set of owned viewport listeners replaces base handlers plus stack capture overrides; their decision priority is retained.',
    'No global shortcut listener; the existing application command dispatcher calls setSpaceHeld, finishPen, cancelPen and selection history methods.',
    'Snapshot subscriptions publish interaction boundaries, selection changes and explicit zoom commands; pointer-move painting stays inside this controller.',
    'Accepted document identity/revision and navigation epochs guard asynchronous mask restoration, layer preview loading and transform commits.',
    'Selection history remains browser-local with the existing twelve-mask bound; callers preserve selection-before-backend-stack undo priority.',
    'No project serialization, backend stack history, native requests or authoritative pixel exports are moved into this controller.',
  ],
});
