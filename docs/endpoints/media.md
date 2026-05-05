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
- `fileAcknowledgeUpdateFailed`

### Behavior notes

- `files` reads serialize normalized `FileRecord` state in snapshot mode and
  after local file mutations. The local connection uses shared cursor/pageInfo
  helpers and omits files marked deleted by staged `fileDelete`.
- `fileSavedSearches` currently returns an empty Shopify-like connection in
  snapshot mode. Saved-search records are not modeled yet.
- `stagedUploadsCreate` returns inert draft-proxy target metadata so clients can
  observe the mutation payload shape without the proxy creating cloud storage
  objects. Returned URLs use `shopify-draft-proxy.local` placeholders. The JS
  HTTP adapter accepts bytes posted back to those placeholder routes only as an
  in-memory staged-upload handoff for local `bulkOperationRunMutation` imports;
  it does not prove external media upload success.
- HAR-405 captured live Shopify Admin GraphQL 2026-04 staged upload targets for
  representative IMAGE, FILE, VIDEO, and MODEL_3D inputs. The proxy now matches
  the captured target count, selected `userErrors` shape, parameter order, and
  parameter names for those resources. IMAGE and FILE use Shopify's captured
  form field names: `Content-Type`, `success_action_status`, `acl`, `key`,
  `x-goog-date`, `x-goog-credential`, `x-goog-algorithm`, `x-goog-signature`,
  and `policy`. VIDEO and MODEL_3D use the captured signed upload field names:
  `GoogleAccessId`, `key`, `policy`, and `signature`.
- The staged upload fields that intentionally remain placeholders are `url`,
  `resourceUrl`, resource path keys, storage account/signature fields, and
  policy values. Static non-secret form values match the capture where Shopify
  returns them: MIME `Content-Type`, `success_action_status: 201`,
  `acl: private`, and `x-goog-algorithm: GOOG4-RSA-SHA256` for IMAGE and FILE.
- Shopify's media guide documents staged uploads as a two-step upload flow:
  obtain signed target metadata, upload bytes directly to Shopify storage, then
  pass the returned `resourceUrl` to `fileCreate` or product media inputs. The
  proxy's local upload endpoint is limited to storing bytes in memory for
  bulk-mutation variable JSONL handoff; it does not model Shopify storage or
  media processing side effects.
- File mutations stage local `FileRecord` state and do not proxy supported roots upstream at runtime.
- `fileCreate` validates original source URLs and alt text length, derives a filename from the source when no filename is supplied, creates stable synthetic Shopify GIDs by content type, and returns uploaded file status. IMAGE files sourced from a usable URL keep that URL available through `MediaImage.image` and `preview.image` immediately; the proxy does not suppress the image payload solely because the staged file is still `UPLOADED`.
- `fileUpdate` validates file ids, URL fields, alt text length, product references, and Shopify's mutually exclusive `originalSource` / `previewImageSource` update rule before updating staged records. `referencesToAdd` can attach a locally staged file to product media, and `referencesToRemove` can remove the file from product media while keeping the file visible through Files API reads. Captured parity covers successful updates after ready-state polling; richer non-ready/locked failure-state behavior remains future work.
- In LiveHybrid mode, `fileUpdate.referencesToAdd` may issue a narrow product read before validation when the referenced product is not already local. The read hydrates only the product identity/metadata needed for local product-media attachment; the supported mutation still stages locally and does not write upstream at runtime.
- `fileDelete` marks files deleted in local state so downstream reads and product media references can observe the deletion. In LiveHybrid mode, deletes of product-owned media ids may first hydrate the owning product/media relationship from upstream so the local delete can remove that media node from downstream `product.media` reads. The payload's `deletedFileIds` are rebuilt from the local record's actual Files API type (`MediaImage`, `Video`, `GenericFile`, etc.) rather than echoing a caller-supplied alias GID unchanged.
- Shopify's backend can reject `fileDelete` with `FILE_LOCKED` while another media-processing mutation owns a per-file lock. The proxy has no concurrent Shopify media-processing jobs or cross-request lock manager, so it does not fabricate `FILE_LOCKED`; this remains an explicit fidelity divergence unless a future local processing model introduces lockable file state.
- `fileAcknowledgeUpdateFailed(fileIds:)` stages a local acknowledgement for
  existing `READY` Files API records and does not proxy supported requests
  upstream at runtime. The mutation returns selected `files` and `userErrors`
  from the local file model, records `updateFailureAcknowledgedAt` in meta state
  for inspection/commit context, and preserves downstream `files` read payloads.
- Acknowledgement validation follows captured Shopify behavior for the supported
  local subset: unknown or deleted IDs return `files: null` with
  `FILE_DOES_NOT_EXIST`, and non-ready file states such as failed file creation
  return `NON_READY_STATE`.
- The proxy still does not independently perform Shopify's asynchronous
  external media processing. It can acknowledge a local failure/update state
  once represented in normalized file state, but generating real
  storage-transfer failures remains outside the supported runtime boundary.
- Product-owned media mutations (`productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia`) are part of the products group because their read-after-write behavior is tied to product state.

## Historical and developer notes

### Conformance notes

- Existing checked-in parity evidence covers `fileCreate`, `fileUpdate`, and
  `fileDelete` payloads plus product-media reference cleanup.
- Existing live captures confirm Shopify serializes `FileStatus` enum values as
  `UPLOADED`, `PROCESSING`, `READY`, and `FAILED` in the covered Files API
  flows. Fresh public-URL `MediaImage` creates in the checked-in 2025-01 and
  2026-04 media captures returned `UPLOADED`, and the HAR-708 immediate
  reverse-ordered `files` read observed Shopify advancing that new file to
  `PROCESSING`; failed source processing returned `FAILED`.
- HAR-313 adds local executable coverage for `files`, `fileSavedSearches`,
  `stagedUploadsCreate`, and the former explicit
  `fileAcknowledgeUpdateFailed` unsupported boundary. Live staged-upload target
  payload capture was added in HAR-405 for IMAGE, FILE, VIDEO, and MODEL_3D
  target metadata while preserving the no-upload/no-storage runtime boundary.
- HAR-375 adds local executable coverage for `fileAcknowledgeUpdateFailed`
  acknowledgement payloads and downstream `files` reads. The Shopify 2026-04
  live capture records that the mutation takes `fileIds`, returns a `files`
  list, accepts READY files, reports `FILE_DOES_NOT_EXIST` for unknown/deleted
  IDs, and reports `NON_READY_STATE` for FAILED files produced by a bad-source
  create. A safely staged bad-source update stayed READY in the capture and was
  accepted by acknowledgement, so richer external update-failure generation is
  documented as an upload boundary rather than fabricated locally.
- HAR-429 adds executable local-runtime parity for the Files API product-reference
  lifecycle: a local `fileCreate` MediaImage is updated through
  `fileUpdate.referencesToAdd`, becomes visible in downstream `product.media`,
  and remains visible through top-level `files`. This is intentionally local
  evidence because external upload byte transfer is still outside the proxy
  boundary; existing live Files API fixtures anchor the generic create/update
  payload family.
- HAR-534 migrates the remaining media parity scenarios to cassette-backed
  LiveHybrid execution. `fileUpdate.referencesToAdd` uses a product hydrate
  cassette entry before local staging, and `fileDelete` of a product-owned
  MediaImage uses a media-reference hydrate entry before staging the local
  delete and downstream product-media removal.

### Validation anchors

- Conformance fixtures and requests: `config/parity-specs/media/file*.json` and matching files under `config/parity-requests/media/`
