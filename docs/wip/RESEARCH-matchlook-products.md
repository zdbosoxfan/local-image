# Research: look/style matching in commercial products

Status: in progress. Findings appended incrementally.

## Capture One Match Look (partial)
- Official article (403 to fetcher; facts from search snippet): https://support.captureone.com/hc/en-us/articles/22188770298269-Match-Look-Tool and overview https://support.captureone.com/hc/en-us/articles/22266038214173-Match-Look, release notes 16.5.0 https://support.captureone.com/hc/en-us/articles/22162074930589
  - Multiple references -> weighted average; each target compared with each reference, SIMILARITY decides each reference's weight.
  - 4 groups toggled before apply: Normalize (exposure + WB), Light & Contrast, Color Adjust, Color Tone.
  - Impact slider 40%-125% (over-drive above 100). Apply as new Layer (filled layer; not for B&W).
  - Presets/styles store reference image + selected groups + Impact; applied to each image individually.
- Thomas Fitzgerald https://blog.thomasfitzgeraldphotography.com/blog/2024/10/capture-one-165-match-look-a-remarkable-new-feature : strongly recommends new layer so opacity can dial back; similar subjects work best; ~90-95% of the way in ~80% of cases; film match: colour ok, brightness/contrast often off; doesn't do clarity/vignette/dehaze/grain; useful references: in-camera JPEG, client reference, scanned film.
- alexonraw https://alexonraw.com/?p=8580 : any image incl. JPG as reference; result is ordinary editable tools (WB, contrast, Color Editor); batch adjusts each image individually; group of images as reference for balanced look.
- Complaints (forum): https://support.captureone.com/hc/fr/community/posts/23309278723997-Match-Look-and-Black-skin-tones-corporate-headshot-setups (green/magenta tint + too much contrast on dark skin, black hair gets blue highlights, fixing took longer than manual); https://support.captureone.com/hc/de/community/posts/22949827057949-Match-Look-question (layered image analysed from base layer only; exporting JPEG of the finished look as reference works better; snow/water poor, forests good; yellow cast on evenly lit images; event photos "way off"); https://support.captureone.com/hc/es/community/posts/22504297404701-new-feature-match-look-not-working ; Phoblographer beta review https://www.thephoblographer.com/2024/10/23/capture-one-16-5-0-beta-1-review-is-this-the-end-of-presets/ ; L-Camera forum https://www.l-camera-forum.com/topic/414748-capture-one-introduces-match-look-literally-a-game-changer ; DPReview https://www.dpreview.com/forums/thread/4791508
- Often adds excess contrast (fixable on layer).

## Imagen AI (partial)
- Official min 2,000 edited LR photos, same camera profile/style, exported JPEGs useless (sliders read 0): https://support.imagen-ai.com/hc/en-us/articles/4446646190097 ; marketing says 3,000-5,000+ https://imagen-ai.com/post/create-ai-editing-profile.md ; fine-tune by adding ~half as many edits https://support.imagen-ai.com/hc/en-us/articles/6069799037457 ; Lite profile / Talent profiles as alternatives. Output = Lightroom/Camera Raw XMP sliders.

## Lightroom (via search snippets)
- Match Total Exposures (Develop > Settings; first-selected photo is reference; global average, not subject-aware, so dark subject on bright wall mismatches): https://jkost.com/blog/2011/08/lr3-match-total-exposures.html , https://lightroomkillertips.com/automatically-matching-exposure-across-multiple-images/
- Auto Sync toggle (pushes primary edits to all selected; risk of accidental batch edits): https://jkost.com/blog/2024/09/quickly-applying-settings-to-multiple-images-in-lightroom-classic.html
- Preset Amount slider 0-200%, per-preset opt-in "Support Amount Slider"; separate Profile Amount: https://help.theluxelens.com/hc/en-us/articles/6865349975319 ; Adobe 2022 notes https://helpx.adobe.com/lightroom-classic/help/whats-new/2022-4.html
- Adaptive presets: Sky / Subject, AI masks, one click (digital-photography-school https://digital-photography-school.com/new-lightroom-updates/). Adaptive Profiles (2025) analyse the photo itself, no reference: https://petapixel.com/2025/11/03/adobes-lightroom-updates-are-what-good-ai-implementation-looks-like/
- Reference View (Shift+R, R|A icon, static reference left, active right; set via filmstrip right-click; clears on module switch): https://jkost.com/blog/?p=22872 , https://digital-photography-school.com/match-image-processing-reference-view-lightroom/
- NO Adobe reference-matching feature found as of 2026; users request "Match Look" in Adobe community: https://community.adobe.com/t5/lightroom-classic-ideas/match-look-feature/idi-p/14943471

## AI style-learning services
- Aftershoot Edits: min 2,500, recommended 5,000+, up to 5 profiles, separate colour/B&W profiles; can import LR preset: https://aftershoot.com/photography-post-processing-software/
- Evoto Personal AI Preset: min 200 per file type (RAW or JPEG), max 20,000 per Look, AI Smart Selection filters for diversity/consistency at >=50 photos, only global colour adjustments train: https://support.evoto.ai/?p=23427 , https://www.evoto.ai/features/personal-ai-preset
- Neurapix: Kickstart with 20 RAW edits or 500; plugin for LR Classic; "SmartPresets" are dynamic per-image: https://petapixel.com/2023/01/31/neurapixs-enhanced-ai-can-learn-a-photo-editing-style-in-just-2-hours , https://cdn.shutterbug.com/node/592019
- Output of all of these = slider/XMP values per image (not pixels), editable in LR; Imagen also offers per-batch options for exposure/WB tweaks: https://imagen-ai.com/post/ai-lightroom-editing.md

## Video
- Premiere Color Match (Lumetri > Comparison View, reference clip left, frame slider; Face Detection checkbox prioritises skin; result written to editable colour wheels; "small adjustments to most clips"): https://helpx.adobe.com/ee/premiere-pro/how-to/automatically-match-color.html , https://larryjordan.com/articles/abobe-premiere-pro-cc-automatically-color-match-shots/ , https://blog.adobe.com/en/publish/2018/04/03/premiere-pro-updates-spring-2018
- Resolve Color Match is chart-based (ColorChecker/SpyderCheckr/SMPTE OneShot; source/target gamma + colour space): https://support.emerson.edu/hc/en-us/articles/21709302725531-Color-Match-in-Resolve ; Shot Match = reference clip + split-screen wipe + scopes; skin protected manually via qualifier: https://noamkroll.com/how-to-match-skintones-perfectly-using-qualifiers-power-windows-in-davinci-resolve/
- MONO-Match plugin picks best frame with visible face, matches exposure/saturation/tint in Lab; fails with extreme lighting or no faces: https://www.newsshooter.com/2026/06/30/mono-match-skin-tone-matching-assistant-for-davinci-resolve/

## More Capture One detail (alexonraw landscape article https://alexonraw.com/capture-one-match-look-copy-color-grading-and-free-adaptive-styles/ ; search digest of support article)
- Impact: portraits ~40, landscapes 60-70, up to 100 for some; Svet: high Impact on a new layer then lower layer opacity gives different (better) result than low Impact.
- Normalize group (Exposure + WB) is NOT scaled by Impact.
- Turn off HDR Highlights/Shadows (flattens), sometimes Exposure/Saturation, Brightness, Master Color Balance for landscapes.
- Up to 20 references mentioned by blog; app says "up to ~10" / weighted by similarity.
- Batch: three-dot menu option locks certain adjustments (e.g. WB/exposure) to avoid fluctuation between images ("important option for more consistent batch").
- Limits: cannot replicate retouching/layers/local; match subject+theme; reference needs similar tonal contrast; all adjustments editable afterwards; People Masking (skin/eyes/lips) is a separate, complementary tool; masks can be redrawn on the Match Look layer.

## Luminar Neo Color Transfer (https://support.skylum.com/editing-tools/creative-tools/color-transfer)
- Controls: Amount, Color Intensity, Luminosity Intensity, Transition Smoothing, Color Smoothing, "Match Similar Objects' Color" (semantic sky->sky etc). Pixel-based, no B&W. Colour only.

## Evoto AI Color Match (https://support.evoto.ai/feature-introduction-ai-color-match/)
- Amount / Tone / Color sliders; fine-tune local masks (face+body skin, hair, eyes/lips/teeth, clothes, background) each with own tone+color intensity and link/unlink toggle; reference JPG/TIFF/PNG; applying resets existing colour edits (undoable); copy/paste to batch; reference management with groups; recommends similar lighting/subject.

## ON1 Match Color filter: matches colour/tone of any image with colour, luminosity, contrast, saturation sliders: https://www.on1.com/videos/how-to-use-match-color ; ON1 2026 added Starting-point presets mimicking Lightroom/C1 but no new reference matcher: https://petapixel.com/2025/09/03/on1-photo-raw-2026-has-major-ai-masking-improvements/

## CHECKLIST (what good look-matching products offer)
1. Multiple references with similarity weighting (C1) - ours: up to 10 saved looks. OK.
2. Group toggles incl. Normalize separate from style (C1). Ours has.
3. Strength: C1 40-125 (over-drive); LR 0-200; Evoto Amount/Tone/Color; Luminar Amount + colour + luminosity split. Ours: single Strength -> consider split Tone vs Colour and >100%.
4. Normalize NOT scaled by Strength (C1) - exposure/WB should stay at 100% when Strength lowered, or be a separate toggle.
5. New-layer output + layer opacity (C1; high impact + lower opacity differs from low impact).
6. Batch consistency lock (C1 three-dot menu locks adjustments), Evoto smart selection of diverse training photos, Imagen fine-tune.
7. Skin/people handling: Premiere face detection weighting; Evoto per-mask intensity (skin/hair/background); C1 separate People Masking; Resolve qualifiers. 
8. Compare view: Premiere Comparison View w/ reference left; LR Reference View (Shift+R); Resolve split-screen wipe + scopes.
9. Results = editable sliders (C1, LR, Imagen, Neurapix) rather than pixels (Luminar, ON1 filters).
10. Save look = reference(s)+group selection+strength (C1 presets).
11. Reference management: groups, batch upload (Evoto); any image incl. JPG/TIFF accepted; reference can be a rendered/edited export.
12. Warn/degrade for B&W (C1 no layer; Luminar unsupported); mismatch warning when subject unlike references.

## FAILURE MODES + MITIGATIONS
- Dark skin tones: green/magenta tint, deep shadows, hot highlights, hair blue (C1 forum). Mitigation: low Impact (~40) for portraits, skin-weighted fit (Premiere face detect), skin hue-protection, per-mask intensity (Evoto).
- Excess contrast / flattening from HDR highlight+shadow: disable those groups; offer contrast cap.
- Yellow/colour cast on evenly lit images: WB normalize over-correct -> Normalize toggle, WB lock.
- Snow / water / event / mixed light poor; reference scene must resemble target.
- Reference with edits/layers not honoured: use rendered export as reference; we should analyse rendered output.
- Brightness/contrast off when matching film.
- Fixing bad result takes longer than manual: need instant per-image undo/reset and per-image strength.
- Batch drift: WB/exposure fluctuation between frames -> lock/consistent batch.
- Evoto: applying wipes existing colour edits; users surprised -> be explicit.
- Lightroom Match Total Exposures: global mean, fooled by bright background; Auto Sync risk of overwriting.

## RECOMMENDATIONS (ranked by user value)
1. Skin/people protection: weight the fit toward detected skin/face; hue-lock skin (clamp hue shift / saturation of skin range); optional "Protect skin" toggle. Biggest documented complaint (C1 dark-skin results).
2. Keep Normalize (exposure/WB) independent of Strength, with its own toggle and a WB-only/exposure-only lock; consistent-batch should lock WB across a set.
3. Before/after + reference compare: side-by-side or wipe with a reference thumbnail (Premiere Comparison View, LR Reference View, Resolve wipe); per-image Strength override.
4. Strength range: allow >100% (C1 125%) and optional separate Tone vs Colour amounts (Evoto, Luminar).
5. Scene/mismatch warning: show similarity score per reference and warn when target is unlike any reference (snow, water, event, B&W).
6. Subject-like per-reference weights display ("62% Exteriors-3"), allow manual reference pin/disable per target.
7. Accept rendered exports (JPG/TIFF) and edited photos as references; analyse the final rendered output, not the base layer.
8. Contrast guard: cap or disable HDR-ish shadow/highlight recovery as separate sub-toggles; offer "Match colour only" preset (disable Light & Contrast + Normalize).
9. Looks saved with groups + strength + references (have); add look import/export, and reference management groups.
10. Show which sliders were changed (transparency; C1 sells this) and one-click "reset to neutral" per group.
