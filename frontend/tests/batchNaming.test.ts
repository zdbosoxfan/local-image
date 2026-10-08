import { test } from 'node:test';
import assert from 'node:assert/strict';
import { filenamePreview, namingError } from '../src/features/batch/naming.ts';

test('filename examples handle sequence, source extensions and portable filenames', () => {
  assert.equal(filenamePreview('product-{index}-{name}', 'Photo.final.jpeg', 7), 'product-007-Photo.final.png');
  assert.equal(filenamePreview('{name}', 'CON.jpg'), '_CON.png');
  assert.equal(filenamePreview('{name}', 'A:B.png'), 'A_B.png');
  for (const pattern of ['../{name}', '{unknown}', '.. . ', '', 'a'.repeat(161)])
    assert.ok(namingError(pattern), pattern);
  assert.equal(namingError('{name}-{index}'), '');
  const unicode = filenamePreview('{name}', '水'.repeat(160) + '.png');
  assert.ok(new TextEncoder().encode(unicode).length <= 244);
  assert.ok(!unicode.includes('\uFFFD'));
  assert.equal(filenamePreview('{name}', 'a'.repeat(130) + '.png'), 'a'.repeat(130) + '.png');
});
