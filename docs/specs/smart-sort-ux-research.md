# Smart Sort & Export: UX research (competitor review and ideas)

Date: 2026-10-09. Input: `smart-sort-spec.md` sections 0, 5.11 and 11 (owner decisions 1–8).
Method: vendor help pages, release notes and reviews (web search and fetch). Everything here is summarised in
our own words. Where a claim comes only from a competitor's blog or marketing page, it is marked *(vendor claim)*.
Nothing was signed into and no forms were submitted. Some vendor pages could not be fetched (Adobe help returned 403);
in those cases the summary relies on the search-result extracts of the same pages.

---

## 1. Product notes

### Desktop editors and DAMs

**Lightroom Classic (People view, keywords, stacks, export)**
- People view (O key) has two areas, Named People and Unnamed People. Opening a named person shows a "Similar" strip,
  and you accept each suggestion with a checkmark. Faces can be found for the whole catalog or only "as needed" in the
  folders you open. You can tag faces in Loupe and draw a face box when detection misses one.
  [helpx face recognition](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/face-recognition.html)
- Good: accepting suggestions inside one person's view is a quick, focused queue. Indexing only "as needed" matches
  our lazy faces idea (decision 6).
- Bad: you have to open each person to see their suggestions, and there is no overall queue of photos to confirm.
- Keyword suggestions are based on keywords already on the photo and on photos taken **close in time**. Keyword sets
  hold up to 9 keywords that you apply with Alt/Option+1–9.
  [helpx keywords](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/keywords.html),
  [jkost](https://jkost.com/blog/2020/03/working-with-keywords-in-lightroom-classic.html)
- Auto-Stack by Capture Time groups a folder by a time gap you choose (0 s to 1 h). It is the usual tool for bursts
  and sessions. [helpx stacks](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/grouping-photos-stacks.html)
- Export has "Put in Subfolder" with a fixed name only, and users report that **no tokens** work in the folder name
  (tokens work only in file names). Contact sheets come from the Print module, one collection at a time, and there is
  no batch per collection. These are gaps we can fill.
  [helpx export](https://helpx.adobe.com/lightroom-classic/help/export-files-disk-or-cd.html),
  [Adobe forum](https://community.adobe.com/t5/lightroom-classic-discussions/how-to-export-into-matching-subfolders-but-under-a-different-parent/m-p/15535628),
  [contact sheets](https://expertphotography.com/contact-sheet-lightroom)

**Lightroom (cloud)**
- People clusters can be sorted by **Count** or by first or last name, and each cover photo shows its photo count.
  The feature is off by default and the analysis runs in Adobe's cloud. It is not available for uploads from Illinois
  or Texas because of biometric laws.
  [helpx people view](https://helpx.adobe.com/lightroom/desktop/organize-photos/people-view.html)
- Good: sorting by count backs up decision 8 (bubbles sorted by frequency). Bad: running in the cloud blocks the
  feature in some places. Our on-device design avoids that, and we should say so in the UI.

**Capture One (smart albums, keywords, export recipes)**
- Users report that keyword rules in smart albums are typed by hand with no picker, that nested keywords need the full
  path, and that albums sometimes come up empty.
  [C1 community](https://support.captureone.com/hc/es/community/posts/4410087937041-Smart-albums-an-easier-way-to-use-multiple-level-keywords-as-criteria)
- Export recipes have a **Sub Folder field that accepts tokens**. A slash makes nested folders, several recipes can
  write side by side under one output location, and "Cross Recipe Tokens" sets a shared job name.
  [C1 recipes](https://support.captureone.com/hc/fr/community/posts/360012368718-Output-recipe-file-and-folder-naming)
- What to copy: tokens in folder names. What to avoid: rules typed as free text (our chips and pickers already avoid this).

**Photo Mechanic**
- Code replacements load a tab-separated file of short codes and full text. Typing `=pt18=` expands to the player's
  full name, `#2` picks another column such as the surname, and `{serial}` can map a camera body to a photographer
  credit. Sports and event shooters rely on it.
  [Camera Bits](https://docs.camerabits.com/support/solutions/articles/48000223660)
- Adjust Capture Time lines up the clocks of several bodies against one reference frame. It is sold for second
  shooters. [tour](https://home.camerabits.com/tour/)
- Contact sheets per folder with page numbers in the footer, printed via the print dialog.
  [printing](https://docs.camerabits.com/support/solutions/articles/48001140998)
- What to copy: **import a roster** (name, title, team/number) instead of typing names; per-camera identity and time offset.

**Excire Foto / Excire Search (Lightroom plug-in)**
- **Search by example photo**: pick one or more reference photos to get similar images, no keywords needed, and the
  last settings can be reused in one click. There is also a text-prompt search, and a face search filtered by number
  of faces, age, smile and so on. [excire tutorial](https://excire.com/en/tutorials/excire-search/beispielsuche/),
  [Shutterbug](https://www.shutterbug.com/content/best-ai-lightroom-plugin-5-ways-excire-search-will-instantly-improve-your-lightroom-classic)
- Reviewers say its automatic keywords are strong for people and weak for specific subjects (species).
  [Hutchinson review](https://ahutchinson.substack.com/p/excire-2025-indepth-review-a-lightning)
- What to copy: "find more like these" from example photos, which is the same thing as our exemplar boost.

**ON1 Photo RAW / Photo Keyword AI**: on-device keyword tagging covering objects, people, colours and places. A 2025
review found its AI features in general "patchy". There is no named-person recognition.
[ON1](https://on1.com/products/photo-keyword-ai), [DCW review](https://www.digitalcameraworld.com/reviews/on1-photo-raw-2025-review)

**digiKam**
- Faces go through clear states: **Unknown → Unconfirmed (suggested name) → Confirmed**, plus **Ignored**. The actions
  are Confirm, Reject (back to Unknown) and Delete (not a face). You can select several similar faces, name them in one
  step and confirm. A recognition accuracy setting (default 7) trades recall against errors. Names can get shortcut keys.
  [digiKam People View](https://docs.digikam.org/en/left_sidebar/people_view.html)
- Bad: the manual admits **rejections are not remembered**, so wrong suggestions come back. A forum user could not see
  the guessed name on thumbnails without clicking each one.
  [pixls.us](https://discuss.pixls.us/t/face-recognition-in-digikam-7-0-0-beta1/15759)
- What to copy: explicit states and an Ignore action. What to avoid: forgetting rejections, and hiding suggested names.

**Immich**
- Per person you can change the feature photo, merge, hide, favourite (pin to top), and assign unrecognised faces.
  Settings: **minimum recognised faces** (people with fewer faces are hidden), maximum recognition distance, and
  minimum detection score. Faces that match nobody stay "unassigned" (strangers in the background) and are checked again
  each night. Typing a name close to an existing one offers a merge.
  [Immich docs](https://docs.immich.app/features/facial-recognition)
- What to copy: the minimum-faces threshold so crowd faces stay out of the bubble list, favourites, and merge-on-same-name.

**PhotoPrism**: "New Faces" shows only **clusters**, never every single unknown face, because a library can hold
thousands of faces of strangers (it even mentions faces on shampoo bottles). A `face:new` filter finds the rest. Users
report it is slow to load on large libraries.
[docs](https://docs.photoprism.app/user-guide/organize/people), [GH discussion](https://github.com/photoprism/photoprism/discussions/4173)

**Mylio Photos**: Batch Face Tagging lets you approve or reject whole clusters at once. Automatic tagging is off by
default, a "?" badge on a person means there are suggestions to review, and suggestions are **not written to metadata
until confirmed**. [Mylio manual](https://manual.mylio.com/topic/tag-people-facial-recognition),
[best practices](https://inspire.mylio.com/face-tagging-in-mylio-photos-best-practices-time-saving-tips/)

**DAMs (Bynder, Canto, PhotoShelter)**: the pattern is to tag a face once and have the system tag it in every existing
and new upload, with a person reviewing suggestions. PhotoShelter **PeopleID** matches faces against **reference
headshots plus name and title metadata**, RosterID combines faces with jersey numbers, and BrandID finds sponsor logos.
[Bynder](https://support.bynder.com/hc/en-us/articles/20530684581266),
[PhotoShelter PeopleID](https://go.photoshelter.com/facial-recognition-auto-tagging-peopleid),
[RosterID](https://stories.photoshelter.com/artificial-intelligence-automated-metadata-rosterid/)
- What to copy: **seed people from headshots**. A conference already publishes its speaker headshots, so the keynote
  folder can be set up before the event.

### Consumer photo apps

**Apple Photos**: you merge people by selecting them and choosing "Merge N People". **Review More Photos** goes through
suggestions one at a time with Yes / Not. "Make Key Photo" picks the face thumbnail. Favourites go to the top in big
tiles. "Feature [name] Less" / "Never Feature" pushes people down, and it can be reset.
[Apple support](https://support.apple.com/en-au/guide/photos/-phtad9d981ab/mac), [iOS](https://support.apple.com/en-gb/108795)
- Bad: users report that "Review More" finds nothing when a person is split into two groups, so you have to merge first.
  [Apple Community](https://discussions.apple.com/thread/255900660)
- Lesson: suggest merges **before** running the confirm queue.

**Google Photos**: merges are suggested with **Same / Different / Not sure**, and a merge cannot be undone. Naming two
groups with the same name offers a merge. People can be hidden from search.
**Live albums**: pick people and the album adds their photos automatically, with an option to include older photos.
[face groups](https://support.google.com/photos/answer/6128838),
[live albums](https://blog.google/products/photos/keep-your-favorite-photos-date-live-albums/)
- What to copy: the three-way answer ("Not sure" so the user can skip), and the live-album idea. Our "Also create a
  smart album per folder" option already does this, so keep it.
- What to avoid: merges that cannot be undone. Ours must go into the undo history.

**Amazon Photos**: People grouping works much like Google's. Nothing found that adds to the above.

### Pro culling tools

**Aftershoot**: there are two culling modes, fully automatic or assisted (duplicate groups, Key Faces, scores).
Duplicate grouping has a **strictness setting** (how many groups and how big). Key Faces are shown below the photo,
sorted from sharpest to blurriest, with a zoom slider and Alt/Option+arrow keys. The **People filter** shows face
thumbnails with photo counts, and the **counts follow the active filter** (for example Selected only, with total vs
filtered in a tooltip). Selecting several people can be set to **"At least one" or "Everyone"**, and "View all" shows
the full list. [People filter](https://support.aftershoot.com/en/articles/16556786-how-to-use-the-people-filter),
[get started](https://support.aftershoot.com/en/articles/5223473-get-started-with-aftershoot-culling)
- This is the closest match to our People panel. Copy the any/everyone switch and the counts that follow the filter.

**Narrative Select**: **Scenes View** groups a shoot by setup or pose so you review scene by scene. The Close-ups panel
shows crops of up to 24 faces next to the image with eyes and focus checks. Space zooms to the main face and arrow
keys step through faces. [close-ups](https://help.narrative.so/en/articles/7337371-zoom-pan-mode-and-close-ups-in-select),
[Select](https://narrative.so/select)

**FilterPixel**: Autogroup puts similar shots into stacks. Arrow keys move within a stack and **Tab jumps to the next
stack**. Auto-select keeps the best photo of each stack. There are face panels for group shots and filters such as
closed eyes. Processing runs in the cloud *(review)*.
[1.1.3 notes](https://filterpixel.com/blog/posts/whats-new-in-filterpixel-ai-culling-1-1-3),
[Shotkit](https://shotkit.com/filterpixel-review/)

**Imagen**: culling with blur, duplicate and closed-eye detection, and "Cull to exact number" for deliveries with a set
photo count *(vendor claim)*. Keyword tagging is a separate tool.
[Imagen event culling](https://imagen-ai.com/solution/ai-event-photo-culling-software/)

### Client delivery platforms (face search, codes, numbers)

**Pic-Time**: guests take a **Selfie Search** to find their own photos and can filter by Suggested People (people often
photographed with them) or by keywords. "Mandatory" mode only lets guests see photos they appear in, which suits
festivals and graduations. The selfie is not stored.
[help](https://help.pic-time.com/en/articles/9655382-how-can-gallery-guests-view-or-search-the-gallery-using-face-scan),
[blog](https://blog.pic-time.com/blog/selfie-search-for-client-galleries)

**Aftershoot Galleries**: a sidebar of detected faces, **At least one / Everyone Together**, selfie capture, and gated
access (guests see only their own photos).
[help](https://support.aftershoot.com/en/articles/14699217-face-and-selfie-search-in-galleries)

**Zenfolio Face Finder**: guests search with a selfie or an uploaded photo and can mark results "not a match". The
photographer must opt in per account or gallery and accept terms, and guests must agree too. The feature is switched
off in some places such as Illinois.
[Zenfolio help](https://success.zenfolio.com/hc/en-us/articles/44532160676371)

**Waldo**: guests text a selfie with the event hashtag to a short code, and matching photos are sent to their phone as
they are uploaded. Photographers upload during the event over Wi-Fi SD cards, and organisers can add branding and time
the delivery. Recent material mentions jersey numbers and QR codes. The sources are older (2016).
[TechCrunch](https://techcrunch.com/2016/01/21/waldo-raises-5-million-for-a-photo-finding-platform-targeting-professional-photographers-events)

**Kwikpic**: guests scan a QR code, take a selfie and get a personal folder. Branding and watermarks are included
*(vendor claim)*. [Kwikpic](https://kwikpic01.substack.com/p/how-ai-face-recognition-is-changing)

**GotPhoto (school / volume)**: a **QR card per subject** is photographed before their portraits, and uploads are sorted
automatically into a gallery for each person and grouped by class.
[GotPhoto capture methods](https://www.gotphoto.com/capture-methods)

**Bib and number recognition (RaceTagger, WaldoPro AI Bib, Flashframe, 9Pic, PhotoShelter RosterID)**: these tools read
bib, plate or jersey numbers by OCR, write them into metadata or sidecars, and sort into **one folder per competitor**
(RaceTagger *(competitor description)*). Folded or muddy bibs remain the main failure, so vendors pair OCR with face
matching and manual review. [9Pic vs RaceTagger](https://9pic.ai/compare/racetagger/),
[WaldoPro AI Bib](https://thedeadpixelssociety.com/waldopro-introduces-ai-bib-race-number-recognition-for-running-cycling-and-motocross-photography/),
[Flashframe](https://jp.flashframe.io/blog/the-cutting-edge-of-race-photo-tagging-software/)
- "PhotoFinish" turned out to be finish-line timing apps, not tagging tools.

**Pixieset, ShootProof, SmugMug, Picflow**: no face search was found in their own material. Competitor pages describe
them as gallery and proofing tools. Not confirmed from their own docs.
[Picflow blog](https://picflow.com/blog/top-pixieset-alternatives-for-photographers),
[FotoOwl](https://fotoowl.ai/blogs/best-shootproof-alternatives-for-event-photographers)

### Cross-cutting findings
1. **Every serious face tool has a confirm step** (Apple Review More, Lightroom Similar, digiKam Unconfirmed, Mylio "?",
   Bynder). Nobody trusts raw clusters for client deliveries.
2. **Strangers in the background are a known problem**: PhotoPrism shows only clusters, and Immich hides people below a
   minimum face count. At a conference with 2,000 photos, hundreds of audience faces would flood the bubble list.
3. **Time is the cheapest grouping signal**: Lightroom suggests keywords from nearby capture times and stacks by time
   gap, and Narrative groups by scene.
4. **Any vs everyone** for several people is standard in delivery tools (Aftershoot, gallery filters).
5. **Biometric law** blocks the cloud tools in some US states (Lightroom cloud, Zenfolio). Doing it all on-device is a
   real selling point and should be stated in the UI.
6. **Folder names with tokens are missing in Lightroom Classic**. Capture One and Photo Mechanic users rely on tokens and
   variables.

---

## 2. Ranked ideas for the Smart Sort dialog

Effort: S = days, M = about a week, L = several weeks. Phase: 2 = dialog, tags and export; 3 = faces and people; later.

| # | Idea | Why it helps an event photographer | Seen in | Effort | Phase |
|---|------|------------------------------------|---------|--------|-------|
| 1 | **"Review more photos" confirm queue per person.** Pick a bubble or person folder and get a full-screen queue of likely matches, sorted least sure first, with **Yes (Y) / No (N) / Not sure (S)** and Undo. Rejections are **remembered** and never suggested again. Matches stay "suggested" until confirmed or until they pass a "sure" cut-off. | A client-paid folder like "Keynote – Jane Doe" must have no strangers. Ten seconds of Y/N beats scanning a grid. | Apple Review More, Lightroom Similar ✓, digiKam Unconfirmed, Mylio "?", Google Same/Different/Not sure | M | 3 |
| 2 | **Seed people from headshots or a roster.** "Add people from photos…": point at a folder of speaker headshots (file name becomes the name), or paste or import a CSV of `name,title` and match them to bubbles. Each headshot becomes a named bubble right away. | Conferences publish speaker headshots in advance. The photographer can set up "one folder per speaker" before analysis, and names are spelled correctly. | PhotoShelter PeopleID (reference headshots + metadata), Photo Mechanic code replacements | M | 3 |
| 3 | **Hide strangers in the background.** Bubbles appear only for people seen in ≥ N photos (default 3; "Show everyone"). Add **Ignore person** (hide the bubble, keep the data), and favourite/pin bubbles to the top. | 2,000 conference photos contain hundreds of audience faces. Without this, the frequency-sorted list ends in clutter and the panel is slow. | Immich min recognised faces + favourite, PhotoPrism clusters only, digiKam Ignored, Apple Feature Less / favourites | S | 3 |
| 4 | **Sessions by capture-time gaps.** A "Split by time" option: find breaks longer than X minutes (default 20) and offer sessions ("09:02–10:15", renameable to "Morning keynote") as folders or as an AND filter on any folder ("Speakers" AND "Afternoon"). | Conference programmes run by time slot, so "Afternoon speakers" or "Panel 2" is often just a time range. No AI needed and it is always right. | Lightroom Auto-Stack by Capture Time, Lightroom keyword suggestions by time, Narrative Scenes | S–M | 2 |
| 5 | **Folder and file name tokens on export.** Folder name field accepts `{event}`, `{folder}`, `{person}`, `{session}`, `{date}`, `{camera}`; nested with `/`; file names like `{event}_{folder}_{seq}`. Event name asked once in Step 3. | Delivery folders like `2026-10-07 SNHU Summit/Speakers` without renaming by hand. Lightroom Classic users can't do this today. | Capture One subfolder tokens / Cross Recipe Tokens; missing in Lightroom Classic | S | 2 |
| 6 | **"Find more like these" folder from example photos.** Drag 1–5 photos onto "+ Folder from examples" to make a folder defined by those photos' CLIP image embeddings, optionally together with tags. Show it as image chips next to the tag chips. | Some categories are hard to describe ("sponsor booth with the blue backdrop"). Showing examples is quicker than finding words. It reuses our exemplar machinery. | Excire Search by example photo, Bynder "tag once" | M | 2 |
| 7 | **"Everyone" vs "at least one" for people in a folder.** The People toggle on a folder gets a switch: photos with **any** of the selected people, or with **all** of them (for group shots). | "Jane and John together" (CEO with award winner) is a common client request. Today only "any" is possible. | Aftershoot People filter and Galleries, Aftershoot "Everyone Together" | S | 3 |
| 8 | **Merge suggestions with Same / Different / Not sure, undoable.** When a name typed on a bubble matches an existing name, offer a merge. Before opening a confirm queue, suggest bubbles that are probably the same person. Merges go into undo. | Speakers photographed from the side or on a big screen often split into two clusters, and confirm queues then miss photos (Apple users hit this exact problem). | Google Photos, Immich, Apple (merge first) | S | 3 |
| 9 | **Counts on bubbles follow the selected folder.** When a folder is selected, each bubble shows "in this folder / total" (tooltip as Aftershoot). | You can see at a glance that Jane has 46 photos but only 12 in "Speakers", without opening anything. | Aftershoot People filter | S | 3 |
| 10 | **Face close-up strip in review.** In the person queue and folder grid, show the matched face crop large next to the thumbnail, with sharpness and eyes-open marks. Space jumps to the matched face. | You can check identity and quality without zooming, which matters most in crowd and stage shots where faces are small. | Narrative Close-ups, Aftershoot Key Faces, FilterPixel face panel | M | 3 |
| 11 | **Keyboard-driven review.** 1–9 = move to folder N (like Lightroom keyword sets Alt+1–9), Y/N/S in queues, Tab = next stack or folder, arrows move, ⌘Z undo. Show a hint bar. | Pros cull with the keyboard. A 2,000-photo review needs to go at about 2 photos per second. | Lightroom keyword sets Alt+1–9, FilterPixel Tab, Narrative arrows, digiKam shortcut keys | S | 2 |
| 12 | **Burst and duplicate stacks in review.** Collapse near-identical shots (same time ±2 s and high CLIP image similarity) into one tile with "×7". Moving the stack moves all of them. An export option keeps "best of each stack only" (sharpest; later best faces). Strictness setting. | Event shooters fire bursts at the podium. Reviewing stacks cuts the work 3–5× and stops a client folder getting 7 near-identical frames. | Aftershoot duplicates (strictness), FilterPixel Autogroup + Auto-select, Lightroom auto-stack | M (stacks) / L (best pick) | 2 stacks, later best pick |
| 13 | **Strictness per folder.** Keep the global Strict ↔ Loose slider, and add a per-folder override (small ▾ on the row) for folders that need to be strict ("Sponsors": strict, "Candids": loose). | Different categories have different costs for errors: a wrong sponsor photo is embarrassing, an extra candid is harmless. | digiKam accuracy, Immich distance, Aftershoot duplicate strictness (all per-feature thresholds) | S | 2 |
| 14 | **Split by camera / photographer, with clock sync.** Rule "Camera is …" (body serial or model, nameable "Second shooter – Ana"), a split-by-camera folder option, and a "Sync clocks…" step (pick one matching frame from each body) so time sessions (#4) are correct. | Conferences often have 2–3 shooters with clocks set differently. The client may want the credit split, or each shooter's set. | Photo Mechanic Adjust Capture Time and `{serial}` variable | M | 2 (rule) / later (sync) |
| 15 | **Contact sheet PDF per folder.** "☐ Also make a contact sheet PDF per folder" (grid, file names, folder title, page numbers). | Event clients and PR teams approve or pick from a PDF. Lightroom can only do this one collection at a time by hand. | Photo Mechanic contact sheets, Lightroom Print module (manual) | M | later |
| 16 | **Text in the photo: OCR of bibs, badges, signs, QR cards.** Read bib, jersey or badge numbers and QR cards into tags like `Bib 1234`. A "one folder per number" option. QR card per subject as in GotPhoto. | Sports and volume work sort by number, not face. Badges and session signs also help at conferences. | RaceTagger, WaldoPro AI Bib, Flashframe, PhotoShelter RosterID, GotPhoto QR | L | later |
| 17 | **Suggested tags.** Under each folder's tag field, show 5–8 suggested chips: CLIP's strongest matches from a built-in word list over this gallery, ranked by how well they separate the photos. | Helps when the photographer doesn't know what to type, and finds hidden groups ("stage lighting", "name badge"). | Lightroom keyword suggestions, Pic-Time Suggested People / Keywords | M | 2 |
| 18 | **Selfie search / delivery to attendees.** Out of scope for an offline desktop app. The local version is "Find This Person" from a photo (already decision 6). A later "export to gallery" could hand off per-person folders. Add a consent note in the UI, since cloud rivals are switched off in some US states. | Attendee delivery is the biggest trend in event platforms, but it needs hosting. Our per-person export folders are the input those platforms need. | Pic-Time, Zenfolio, Aftershoot Galleries, Waldo, Kwikpic | L | later |

Already in the spec and confirmed by the research (keep): bubbles sorted by count (Lightroom cloud Count sort, Aftershoot
counts); face analysis opt-in and off by default (Lightroom cloud, Mylio, Zenfolio); smart album per folder (Google Live
Albums); per-folder any/all tag matching (Aftershoot/Pic-Time any/everyone pattern); correcting by dragging and teaching
the model (Bynder "tag once").

---

## 3. Recommended additions to the spec (paste-ready)

9. **Confirm queue for people** (Phase 3). Each person bubble and each person folder has "Review matches…". It opens a
   full-screen queue of that person's suggested photos, least sure first, with the face crop large next to the photo,
   and buttons **Yes (Y) / No (N) / Not sure (S)** plus Undo (⌘Z). A Yes adds the face to the person's centroid and
   re-ranks the rest live. A No is stored as a rejection (`smartSort.rejectFace`) and the face is **never suggested
   again** for that person. Photos below the "sure" cut-off appear in a person folder only after a Yes. The bubble shows
   a "?" badge with the number of matches still to review. Before the queue opens, if another bubble is probably the same
   person, offer "Merge with …? Same / Different / Not sure". Merges and splits are undoable.
   Automation ids: `smartSort:review:<personId>`, `smartSort:reviewYes`, `smartSort:reviewNo`, `smartSort:reviewSkip`.
   Tests: a rejected face never comes back after re-ranking; a Yes moves a face from suggested to confirmed; a merge
   can be undone.

10. **Seed people from headshots or a name list** (Phase 3). The People panel has "Add People from Photos…": choose a
    folder of headshots, and each image with exactly one face becomes a named person (name = file name without its
    extension, editable), shown as a bubble even before any match. A "Paste names…" box accepts `name[,title]` lines,
    which become name suggestions in the bubble name fields (type-ahead). Seeded people can be put into folders straight
    away, so "one folder per speaker" can be set up before analysis finishes. Headshot embeddings stay in the library
    like all face data. Tests: a folder of 3 mock headshots gives 3 named people; their gallery matches rank first.

11. **Hide strangers in the background** (Phase 3). Bubbles appear only for people seen in at least **3** photos
    (setting: 1–10; a "Show everyone (N more)" link). Each bubble has **Ignore** (hidden, data kept, listed under
    "Ignored" where it can be restored) and **Pin to top** (pinned bubbles come before the frequency order). Counts on
    bubbles follow the selected folder: "12 / 46" = in this folder / total. Automation ids: `smartSort:minFaces`,
    `smartSort:ignore:<personId>`, `smartSort:pin:<personId>`. Tests: a mock person with 2 photos is hidden by default
    and shown with "Show everyone"; an ignored person never appears in folder pickers.

12. **Sessions by capture time** (Phase 2). Step 1 has "☐ Split into sessions when there is a gap of more than
    [20] minutes". It lists the sessions found ("Session 1 · 09:02–10:15 · 312 photos"), each renameable ("Morning
    keynote"). Sessions can be export folders by themselves or a narrowing filter on any folder or custom folder
    ("Speakers" AND session "Afternoon"), and they are available as the `{session}` token. Photos with no capture time
    go to "No time". Sessions use corrected capture times if the clocks have been synced (later: "Sync camera clocks…").
    Automation ids: `smartSort:sessions`, `smartSort:sessionGap`, `smartSort:session:<index>`. Tests: a 25-minute gap
    splits, a 10-minute gap does not (with a 20-minute setting); rename round-trips through the preset.

13. **Folder and file name tokens on export** (Phase 2). Step 3 asks once for **Event name** (default: the source
    folder name). The destination folder pattern defaults to `{event}/{folder}` and accepts `{event}`, `{folder}`,
    `{person}`, `{session}`, `{date}` (capture date) and `{camera}`, with `/` for nested folders. Export presets keep
    their own file naming, plus an optional Smart Sort file name pattern (`{event}_{folder}_{seq:4}`). Unsafe characters
    are replaced, and if two folders resolve to the same path they are numbered. The pattern is saved in the sort preset.
    Automation ids: `smartSort:eventName`, `smartSort:folderPattern`, `smartSort:filePattern`. Tests: tokens expand; a
    nested pattern makes nested folders; duplicate resolved names get " (2)".

Next in line if the owner wants more: #6 folder from example photos, #7 "everyone" mode for people folders, #11
keyboard-driven review, #12 burst stacks.
