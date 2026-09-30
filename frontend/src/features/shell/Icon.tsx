export function Icon({ name }: { name: string }) {
  const historyPath = name === 'undo' ? 'M9 5 3 10l6 5M4 10h10a6 6 0 0 1 0 12' : name === 'redo' ? 'm15 5 6 5-6 5M20 10H10a6 6 0 0 0 0 12' : null;
  return <svg className="li-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">{historyPath ? <path d={historyPath} /> : <use href={`#i-${name}`} />}</svg>;
}
