export const DEFAULT_NAMING_TEMPLATE = '{name}-local-image';

export function namingError(template: string) {
  if (!template.trim() || template.length > 160) return 'Enter a filename pattern of 1–160 characters.';
  if (/[<>:"/\\|?*\x00-\x1f]/.test(template)) return 'Use a filename without folder separators or reserved characters.';
  if (/[{}]/.test(template.replaceAll('{name}', '').replaceAll('{index}', '')))
    return 'Use only {name} and {index} in the pattern.';
  if (!template.replaceAll('{name}', 'Image').replaceAll('{index}', '001').replaceAll('.', '').trim())
    return 'Enter a filename pattern.';
  return '';
}

export function filenamePreview(template: string, source: string, index = 1) {
  const name =
    source
      .replace(/\.[^.]+$/, '')
      .replace(/[<>:"/\\|?*\x00-\x1f]/g, '_')
      .replace(/^[ .]+|[ .]+$/g, '')
      .slice(0, 160) || 'Image';
  let stem = template
    .replaceAll('{name}', name)
    .replaceAll('{index}', String(index).padStart(3, '0'))
    .replace(/^[ .]+|[ .]+$/g, '')
    .slice(0, 160)
    .replace(/[ .]+$/g, '');
  const encoded = new TextEncoder().encode(stem);
  let end = Math.min(230, encoded.length);
  while (end < encoded.length && (encoded[end] & 0xc0) === 0x80) end--;
  stem = new TextDecoder().decode(encoded.slice(0, end)).replace(/[ .]+$/g, '');
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(stem)) stem = '_' + stem;
  return (stem || 'Image') + '.png';
}
