import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash } from 'node:crypto';

const root = new URL('../../tests/fixtures/legacy-frontend-e42/', import.meta.url);
const hash = (value: string | Uint8Array) => createHash('sha256').update(value).digest('hex');
const provenance = JSON.parse(fs.readFileSync(new URL('provenance.json', root), 'utf8'));

test('historical math oracle bytes and exact excerpts retain their pinned provenance', () => {
  assert.equal(provenance.source_commit, 'e42aab6a49f222796dbbc9dc1ebf4211f46e9876');
  const camera = fs.readFileSync(new URL(provenance.camera_regions.file, root));
  assert.equal(hash(camera), provenance.camera_regions.sha256);
  const regions = JSON.parse(camera.toString('utf8'));
  assert.deepEqual(Object.keys(regions), [
    'clampCamera',
    'fitImage',
    'setPhotoZoom',
    'coord',
    'layerGeometry',
    'stackGesture',
  ]);
  for (const entry of provenance.camera_regions.regions) {
    assert.match(entry.source_path, /^backend\/frontend\/(editor|layers-studio)\.js$/);
    assert.match(entry.source_blob_sha256, /^[a-f0-9]{64}$/);
    assert.ok(entry.end_line_exclusive > entry.start_line);
    assert.equal(hash(regions[entry.key]), entry.excerpt_sha256);
    assert.ok(regions[entry.key].startsWith(entry.start_marker));
  }
  const size = fs.readFileSync(new URL(provenance.generation_size.file, root));
  assert.equal(provenance.generation_size.source_path, 'backend/frontend/generation-size.js');
  assert.equal(hash(size), 'fed11b4a957d46f8db22d489f54a05e906d880f1960bf207eeb8f544f1ba96e8');
  assert.equal(hash(size), provenance.generation_size.sha256);
  assert.equal(hash(size), provenance.generation_size.source_blob_sha256);
});
