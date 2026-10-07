export function Icon({ name }: { name: string }) {
  const paths: Record<string, string> = {
    undo: 'M9 5 3 10l6 5M4 10h10a6 6 0 0 1 0 12',
    redo: 'm15 5 6 5-6 5M20 10H10a6 6 0 0 0 0 12',
    add: 'M12 5v14M5 12h14',
    more: 'M5 12h.01M12 12h.01M19 12h.01',
    close: 'm6 6 12 12M18 6 6 18',
    'chevron-up': 'm6 15 6-6 6 6',
    'chevron-down': 'm6 9 6 6 6-6',
    search: 'M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Zm4.5-2 5 5',
    refresh: 'M20 8a8 8 0 1 0 0 8M20 3v5h-5',
    info: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0-11v6m0-9h.01',
    'arrow-up': 'M12 19V5m-6 6 6-6 6 6',
    subtract: 'M5 12h14',
    eject: 'm6 14 6-8 6 8H6Zm0 4h12',
    stop: 'M6 6h12v12H6Z',
  };
  const path = paths[name];
  return (
    <svg className="li-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {path ? <path d={path} /> : <use href={`#i-${name}`} />}
    </svg>
  );
}
