# FS008 / FS009 original-source scope check

Read-only audit, 2026-10-12. This corrects requirement provenance; it does **not** close either finding, waive ordinary-group support, or change the full original app-completion goal. No code edits, tests, network or CI runs. Existing implementation/tests were not revalidated.

## Controlling original sources

**S**: `/home/zdavidson/.local/share/local-image-dev/specs/bugfix/settings-vecfiles.md`, line 13 (SHA-256 `e0491042131065ee62825f566a83c2043fb143dee9692467a5e963d70ab73c8f`):

> FS-009 (ordinary PDFs must open — default transparency groups, indirect /Length, common filters ASCII85/Hex/LZW/CCITT, inline images, permissions-only /Encrypt with an empty user password, leading bytes before %PDF) WITHOUT reopening the resource-exhaustion holes the files job closed (keep budgets, cancellation, worker-thread decoding).

Same line, explicit FS008 instruction:

> FS-008: export native blend modes/clips in SVG and PDF instead of flattening the whole document.

S lines 15–16 require checking current code and, for a real finding, a minimal fix with a regression that fails before and passes after. This is an outcome requirement, not authority to equate a passing subset with complete compatibility.

**R**: `/home/zdavidson/.local/share/local-image-dev/reviews/fable-settings-vecfiles-review.md`, FS009 lines 81–93 (SHA-256 `e74dbfaa54e950b2e96f561746be07ba674f00098e34866062efe446f63fae43`):

- Line 84 calls out the bad user outcome: **“each is a hard error, with no fallback to a raster import”**.
- Line 85 identifies omitted `/I` or explicit `/I false` in **page-level and form-XObject** transparency groups as ordinary affected input.
- Line 93, expressly labelled **“Suggested fix”**, says: **“accept transparency groups with `/I false` (non-isolation only changes backdrop blending, which the importer already approximates) and only refuse `/K true`”**.
- The same line says: **“for the rest, offer a "Place as image" rasterised fallback instead of a hard error.”**
- Lines 87/91 enumerate RunLength, JPX, JBIG2 and FunctionType 4 as additional refused cases; line 91 describes existing `/Predictor`, `/DeviceN > 32` restrictions as reasonable. This is the historical review's diagnosis, not a current decoder-safety proof or prohibition on safely supporting more cases.

R's FS008 line 79 suggests native target-supported blends/isolation, then **“reserve the whole-scene flatten for `clipped`, blend-if, excluded channels, adjustments and Lab”**. **S's explicit native-clips instruction is stronger and controls:** R's suggested clipped-layer exception cannot be used to mark native clip retention unnecessary.

## Requirement classification

| Area | Original required outcome / allowed route | Tracking consequence |
|---|---|---|
| Ordinary default/nonisolated groups | S requires ordinary PDFs to open; R specifically identifies page/Form groups with omitted/false I. Neither limits this to all-Normal paint, no opacity/masks, or only a narrow flat fixture. | Keep ordinary-group compatibility open until representative nested/backdrop/blend/opacity/mask/clip interactions actually work. Do not redefine the requirement around today's supported subset. |
| Knockout `/K true` | R explicitly permits refusing this group case. Neither S nor R demands a native knockout compositor. “Only refuse” is in the transparency-group clause; it does not authorize unsafe input admission or forbid other legitimate malformed/resource refusals. | Remove **mandatory native knockout implementation** from original-FS009 completion blockers. Track a clear supported refusal for K=true (and safe optional fallback if provided), with relevant negative coverage. |
| Named common import features | S explicitly requires indirect lengths, ASCII85/Hex/LZW/CCITT, inline images, permission-only empty-password encryption and leading bytes. R supplies details such as first-1024-byte header handling and bounded decoding. | Preserve each as a separate required acceptance row. Do not silently move this explicit core list wholesale to “unsupported”. All current bounds/cancellation/worker constraints remain. |
| Remaining codecs/functions/complexity | R proposes a user-available **Place as image** fallback for the rest, rather than a hard error. Its own wording is a suggested remedy, not a command that every feature must become editable native data. | Native implementation is one route, not a universal mandate. A safe, visually correct, explicit usable fallback can satisfy the unsupported-case user outcome; retain a per-case disposition and evidence. A warning alone or an error mentioning a nonexistent fallback cannot. |
| General group colour spaces, Type3/wrappers/patterns | Universal native preservation/conversion is not independently listed as mandatory in S/R. These are real semantic hazards when the implementation admits them, and may occur in ordinary inputs. | Separate **correctness of admitted/ordinary cases** from **universal native feature coverage**. Keep ordinary cases supported; unsupported remainder needs an honest bounded fallback/disposition. Do not declare incorrect imported colours “complete” because a warning exists. |
| Native group schema / CPU/GPU policy | Independent isolation fields, coverage bookkeeping, interpreter marker plumbing and CPU fallback are later engineering proposals. S/R do not prescribe a schema or require a native GPU path for every PDF construct. | If chosen and shipped, validate their correctness, persistence and actual execution paths; these are implementation obligations, not evidence that S originally demanded every proposed feature. |
| Encryption extensions | Empty-password permissions-only opening is explicit. A nonempty-password entry UI is not. S/R do not enumerate revisions/xref formats. | Nonempty-password UI is an expansion, not an FS009 blocker. Do not infer that “R2–R4 passes” closes all ordinary empty-password input; map remaining ordinary containers/security variants to demonstrated support or a truthful viable fallback. A fallback still needs safe authentication/decode. |
| FS008 export | S expressly requires native blends **and clips** in both SVG/PDF instead of whole-document flatten. R's 50-shapes/Multiply, clipped-layer, adjustment and pass-through-opacity examples explain the loss. | Keep representable blend/clip/vector/text retention and independent appearance checks required. Unsupported target interactions may need correct bounded partial/scene fallback, but fourteen blend mappings plus warned clipping flatten is not full FS008 completion. Never flatten a backdrop-dependent subtree against transparent and call it equivalent. |

A fallback is a compatibility route only when it is implemented, reachable from the user's failed Open/Place workflow, bounded/cancellable and faithful to the source. R's “Place as image” allowance does not authorize bypassing preflight into an unbounded decoder, silently rasterizing editable import, discarding grouping semantics, or shipping Poppler merely because it is the test oracle. Nor does it reduce the direct ordinary-group support work already required and underway.

## Specific accumulated scope drift

These records mix real technical risks with unsupported claims that every risk is an original mandatory native feature:

- `recovery/PDF-GROUP-CORE-REVIEW.md:52` lists **“knockout, general blending color spaces, wrappers/patterns”** as full-FS009 requirements.
- `recovery/pdf-group-core-v1/REPORT.md:41` says full FS009 **“still requires”** knockout/general Gray and other blending-space semantics alongside native export work.
- `recovery/pdf-normal-outer-v4/SCHEMA-DESIGN-AND-VALIDATION.md:25,48` calls knockout, general colour conversion and wrapper/pattern semantics **“subsequent required FS009 work”**. This is stronger than S/R and directly conflicts with R's permitted K=true refusal if interpreted as requiring knockout support.
- `recovery/PDF-GROUP-IMPLEMENTATION-PROPOSAL.md:11–17` is a valuable design/risk proposal, not independent authority for universal native support. `PDF-REMAINING-SEMANTICS-SURVEY.md:15` correctly retains knockout rejection until supported; its “general support” design and later reports should not erase that distinction.
- `recovery/PDF-ENCRYPTION-IMPLEMENTATION.md:96–102` includes **nonempty-password UI** among remaining FS009 work although S/R require empty-password opening only.
- Conversely, `recovery/BLEND-EXPORT-IMPLEMENTATION.md` explicitly retains whole-scope fallback for clipped stacks, and `coordinator-findings.json` records fourteen native blend mappings. Those are partial implementation/evidence descriptions, not grounds to drop S's native clip requirement.

## Recommended correction to active requirement tracking

Preserve historical reports as historical evidence; append a provenance correction or link this scope check rather than pretending those proposals never existed. In active `coordinator-findings.json`, worker prompts and release tracking, give each FS009/FS008 row four fields: **original source+line**, **required user outcome**, **chosen support/fallback/refusal route**, **remaining implementation and acceptance evidence**.

Use distinct statuses: required ordinary compatibility pending; supported and independently verified; unsupported with implemented/verified Place-as-image route; expressly permitted refusal (K=true); optional native enhancement; implementation regression. Replace blanket “knockout/general-colour/native everything still mandatory” with those explicit rows. Keep universal native knockout/general colour-space coverage and nonempty-password UI as separately justified proposals unless another original requirement or direct user instruction actually mandates them.

Keep FS009 and FS008 overall **open**: this audit proves scope provenance only. Closure still needs complete mapping of the original required inputs, safe usable handling of remaining cases, original-source appearance/structural oracles where appropriate, actual application workflows, and applicable persistence/cancel/CPU-GPU/export acceptance. Existing test counts, reviewed patches, and warnings do not establish that closure.
