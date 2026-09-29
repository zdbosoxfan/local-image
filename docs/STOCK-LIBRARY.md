# Stock image library

Open the stock library, search Openverse, and select an image to inspect its creator, source page and license. **Open image** creates an editable document. **Use as background** applies the selection to the current cutout while retaining its transform and undo history. **Add reference** imports an image for the Image Gen reference list.

The built-in provider is **Openverse**, using its public API and Flickr source. It works without an API key. Search includes CC BY, CC BY-SA, CC0 and public-domain records; it excludes no-derivatives and noncommercial-only licenses. Each image still has its own terms. Attribution and share-alike requirements can apply. The dialog links the original source and license, and the image/background attribution is retained in saved `.lremove` projects. Flattened image exports do not replace the need to provide credit when publishing. [Openverse API client documentation](https://docs.openverse.org/packages/js/api_client/index.html), [Openverse media metadata](https://github.com/WordPress/openverse/blob/main/documentation/meta/media_properties/api.md)

Search and image retrieval need an internet connection. Results use a short, one-minute query cache; download identifiers expire after an hour or an app restart. Search again if a result expires. Openverse or Flickr may remove an image or temporarily limit requests; the app reports these failures without altering the current image. Imported documents remain available locally. Wikimedia Commons is not included as a provider. Pexels and Unsplash are not connected APIs.

Only explicit selections download full images. Previews are proxied through the local backend. The browser cannot submit an arbitrary URL: imports refer to a server-held result ID, and all requests and redirects stay on approved HTTPS image hosts. Downloads are limited to 40 MiB and decoded images to 40 megapixels. JPEG, PNG, WebP and TIFF inputs are decoded, oriented, converted from an embedded color profile to sRGB, and saved as clean PNG data. Provider HTML, scripts and embedded image text are not executed or retained.

Developer API:

- `GET /api/local-remove/stock/providers` returns Openverse and the default provider.
- `GET /api/local-remove/stock/search?provider=openverse&query=forest&page=0` returns result IDs, local thumbnail URLs, source/creator/license information and the next page.
- `GET /api/local-remove/stock/thumbnail/{id}` returns a bounded PNG preview.
- `POST /api/local-remove/stock/import` accepts `{id, target: "image"}` or `{id, target: "background", session_id, revision}`. It requires the editor's same-origin CSRF token and returns `{session, target, attribution}`. No native setup permission is needed for importing into the app's managed cache.

Validation on 2026-09-29: 13 provider/security tests passed. A live Openverse search returned 12 Flickr forest images; preview and full-image download/normalization succeeded. The selected test image was **Fall-Forest** by **Chris Sorge**, licensed **CC BY-SA 2.0**. Evidence is in `qa-artifacts/stock/live-provider.json`, with locally normalized preview and source PNGs. App-level import, project and UI checks are documented separately in the release validation report.
