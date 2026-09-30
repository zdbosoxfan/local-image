# Pinned legacy frontend arithmetic oracles

These are test-only references from commit
`e42aab6a49f222796dbbc9dc1ebf4211f46e9876`. They are not application assets,
current UI implementations, or evidence that the retired interface passes the
current acceptance suite. Do not update them to make a changed algorithm pass.

`camera-regions.json` stores six exact source excerpts from the pinned
`backend/frontend/editor.js` and `backend/frontend/layers-studio.js`. The
existing parity tests supply their controlled VM context and run the original
arithmetic against the extracted TypeScript implementation. No formulas were
rewritten in these excerpts.

`generation-size.cjs` contains the exact pinned bytes of
`backend/frontend/generation-size.js`. Only the filename extension differs, so
Node selects the file's existing CommonJS pure-helper export. Its historical
DOM adapter is not executed or packaged by this test use.

`provenance.json` records the commit, original paths, complete source-blob
SHA-256 hashes, exclusive extraction markers and line bounds, excerpt hashes,
and fixture hashes. Each camera excerpt and the full generation-size file was
verified equal to the working legacy source before creating the fixtures.
`frontend/tests/legacyOracles.test.ts` checks fixture integrity; the original
camera and dimension parity assertions continue to run against these oracles.

To independently verify provenance, read the original bytes using
`git show <source_commit>:<source_path>` and compare their SHA-256 with the
manifest. For camera excerpts, decode UTF-8, find `start_marker`, then slice to
the first `end_marker_exclusive` after it; hash that exact UTF-8 substring.
The source and fixture hashes make line-ending or excerpt-boundary changes
explicit. These files remain outside the production packaging directories.

The old API/controller tests and production sources remain in place while
retirement is reviewed. Their seven meaningful cases now map as follows:

| Original `editorApi.test.ts` guarantee | Current owner and test |
| --- | --- |
| Same-image queue, execution revision, zero opacity, page token | `frontend/tests/documentApi.test.ts`: serializes at execution revision |
| Different images do not block one another | `documentApi.test.ts`: independent image queues |
| Asset work waits for layer queue and captures accepted state at execution | `frontend/src/editor/documentController.test.mjs`: ported asset operation |
| Conflict invalidates queued writes, no retry, deliberate recovery | `documentApi.test.ts`: conflicts invalidate queued writes |
| Stale/wrong-document response rejection | `documentApi.test.ts`: rejects stale and wrong-document responses |
| Newer accepted metadata overtakes an in-flight result | `documentApi.test.ts`: newer observed revision |
| Network/malformed JSON failure without retry | `documentApi.test.ts`: network and malformed JSON failures |

One classification is intentionally different in the current API: a valid image
identity with an old revision rejects with conflict `409`; a wrong identity is
an invalid backend protocol response and `readDocument` rejects with `502`.
The port asserts each exact status, invalidation of queued writes, no retries,
and no adoption of rejected metadata. No production behavior was changed to
reproduce the historical status wording.
