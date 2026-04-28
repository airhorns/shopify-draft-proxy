# Media Endpoint Group

The media group covers the Admin Files API roots that can be modeled without
performing external upload/storage side effects. Product-specific media roots
remain documented with products because they stage product-owned media records.

## Current support and limitations

### Supported roots

Snapshot/local reads:

- `files`
- `fileSavedSearches`

Local staged mutations:

- `stagedUploadsCreate`
- `fileCreate`
- `fileUpdate`
- `fileDelete`

Explicitly unsupported:

- `fileAcknowledgeUpdateFailed`

### Behavior notes

- `files` reads serialize normalized `FileRecord` state in snapshot mode and
  after local file mutations. The local connection uses shared cursor/pageInfo
  helpers and omits files marked deleted by staged `fileDelete`.
- `fileSavedSearches` currently returns an empty Shopify-like connection in
  snapshot mode. Saved-search records are not modeled yet.
- `stagedUploadsCreate` returns inert draft-proxy target metadata so clients can
  observe the mutation payload shape without the proxy creating cloud storage
  objects or accepting uploaded bytes. Returned URLs use
  `shopify-draft-proxy.local` placeholders and are not a supported upload
  endpoint.
- Shopify's media guide documents staged uploads as a two-step upload flow:
  obtain signed target metadata, upload bytes directly to Shopify storage, then
  pass the returned `resourceUrl` to `fileCreate` or product media inputs. The
  proxy intentionally models only the metadata placeholder today; it does not
  accept the upload bytes or prove upload success/failure.
- File mutations stage local `FileRecord` state and do not proxy supported roots upstream at runtime.
- `fileCreate` validates original source URLs and alt text length, derives a filename from the source when no filename is supplied, creates stable synthetic Shopify GIDs by content type, and returns uploaded file status.
- `fileUpdate` validates file ids, URL fields, alt text length, product references, and Shopify's mutually exclusive `originalSource` / `previewImageSource` update rule before updating staged records. Captured parity covers successful updates after ready-state polling; richer non-ready/locked failure-state behavior remains future work.
- `fileDelete` marks files deleted in local state so downstream reads and product media references can observe the deletion.
- `fileAcknowledgeUpdateFailed` remains registered but unimplemented. It is a
  Files API side-effect root tied to upload/failure state that is not modeled
  locally yet, so runtime requests still use the unsupported mutation
  passthrough escape hatch and are logged as proxied with registry metadata.
- Product-owned media mutations (`productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia`) are part of the products group because their read-after-write behavior is tied to product state.

## Historical and developer notes

### Conformance notes

- Existing checked-in parity evidence covers `fileCreate`, `fileUpdate`, and
  `fileDelete` payloads plus product-media reference cleanup.
- HAR-313 adds local executable coverage for `files`, `fileSavedSearches`,
  `stagedUploadsCreate`, and the explicit `fileAcknowledgeUpdateFailed`
  unsupported boundary. Live staged-upload target payload capture is still
  needed before the inert metadata is tightened to exact Shopify upload
  parameter parity.

### Validation anchors

- Runtime flow: `tests/integration/media-draft-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/file*.json` and matching files under `config/parity-requests/`
