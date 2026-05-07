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
- Live 2026-04 capture for additional staged upload resources shows
  `BULK_MUTATION_VARIABLES`, `URL_REDIRECT_IMPORT`, `RETURN_LABEL`, and
  `DISPUTE_FILE_UPLOAD` use the same Google form field names for `POST`.
  `COLLECTION_IMAGE` and deprecated `PRODUCT_IMAGE` also use that IMAGE-family
  form shape. Captured `PUT` targets for those Google form resources use only
  `content_type` and `acl`. The proxy matches those captured parameter names
  and order while keeping values and URLs inert.
- `SHOP_IMAGE` is exposed by the 2026-04 resource enum, but the current
  conformance app receives top-level `ACCESS_DENIED` for that resource. The
  local handler treats `SHOP_IMAGE` as an IMAGE-family upload target so it does
  not emit proxy-specific parameter names; a scoped live target capture remains
  blocked until the conformance credential can access that resource.
- `CUSTOMER_IMPORT`, `INVENTORY_IMPORT`, `ARTICLE_IMAGE`, `THEME_ARCHIVE`, and
  `TRANSLATIONS_IMPORT` are not public 2026-04 enum values on the captured
  conformance store. Variable requests for those resources are rejected by
  Shopify as `INVALID_VARIABLE`, and the proxy keeps them out of its accepted
  resource enum rather than guessing target shapes.
- The staged upload fields that intentionally remain placeholders are `url`,
  `resourceUrl`, resource path keys, storage account/signature fields, and
  policy values. Static non-secret form values match the capture where Shopify
  returns them: MIME `Content-Type`, `success_action_status: 201`,
  `acl: private`, and `x-goog-algorithm: GOOG4-RSA-SHA256` for IMAGE and FILE.
- HAR-704 captured staged upload validation behavior for Admin GraphQL 2026-04.
  `VIDEO` and `MODEL_3D` inputs require `fileSize`; missing values return a
  field-scoped `userErrors` entry and a null placeholder target. Invalid enum
  resource values are rejected as top-level GraphQL `INVALID_VARIABLE` errors
  before resolver handling. IMAGE-family resources reject unsupported MIME
  values with a `mimeType` userError. Current Shopify accepts FILE staged
  uploads with arbitrary MIME strings such as `application/x-msdownload`, so the
  proxy does not impose an IMAGE-style MIME allowlist on FILE resources.
- The live `stagedUploadsCreate.userErrors` type exposes `field` and `message`;
  it does not expose a selectable `code` field in the 2026-04 schema.
- Shopify's media guide documents staged uploads as a two-step upload flow:
  obtain signed target metadata, upload bytes directly to Shopify storage, then
  pass the returned `resourceUrl` to `fileCreate` or product media inputs. The
  proxy's local upload endpoint is limited to storing bytes in memory for
  bulk-mutation variable JSONL handoff; it does not model Shopify storage or
  media processing side effects.
- File mutations stage local `FileRecord` state and do not proxy supported roots upstream at runtime.
- `fileCreate` validates original source URL presence/length, non-http(s)
  schemes, filename/originalSource extension mismatches, duplicate-resolution
  mode compatibility, and the private-core `referencesToAdd` cardinality
  guardrail before staging. It derives a filename from the source when no
  filename is supplied, creates stable synthetic Shopify GIDs by content type,
  and returns uploaded file status. IMAGE files sourced from a usable URL keep
  that URL available through `MediaImage.image` and `preview.image`
  immediately; the proxy does not suppress the image payload solely because the
  staged file is still `UPLOADED`. The proxy does not apply the older fabricated
  512-character `alt` ceiling on `fileCreate`.
- `fileUpdate` validates file ids, URL fields, alt text length, product references, Shopify's mutually exclusive `originalSource` / `previewImageSource` update rule, READY state, type-specific `originalSource` / `filename` support, filename extension preservation, source plus `revertToVersionId` conflict, missing `revertToVersionId` media versions, and typed-GID mismatches before updating staged records. Captured public Admin GraphQL 2026-04 behavior keeps the 512-character `alt` ceiling, reports non-URL source values as `INVALID_IMAGE_SOURCE_URL` on `previewImageSource`, rejects over-length `originalSource` as a top-level `INVALID_FIELD_ARGUMENTS` error, and accepts over-length `previewImageSource`. `referencesToAdd` can attach a READY file to product media, and `referencesToRemove` can remove the file from product media while keeping the file visible through Files API reads. Successful updates preserve the existing file status rather than promoting files to `READY`.
- Files API validation userErrors follow captured aggregate behavior. `fileDelete`
  missing IDs aggregate into one `FILE_DOES_NOT_EXIST` entry on `["fileIds"]`
  with comma-joined GIDs. `fileUpdate` missing IDs also aggregate into one
  `FILE_DOES_NOT_EXIST` entry on `["files"]`, but Shopify 2026-04 interpolates
  the id list as a quoted array string such as `["gid://...", "gid://..."]`.
  Non-ready `fileUpdate` inputs collapse to one `NON_READY_STATE` entry with
  Shopify's generic `Non-ready files cannot be updated.` message.
- In LiveHybrid mode, `fileUpdate` may issue narrow reads before validation: product reads hydrate referenced products, media-version reads validate `revertToVersionId` ownership, and file reads hydrate existing READY Shopify file records needed for local validation/staging. Supported mutation handling still stages locally and does not write upstream at runtime.
- In Snapshot mode, `fileUpdate.revertToVersionId` existence validation is skipped when the version is not already known through LiveHybrid hydration. The public Admin GraphQL schemas currently checked through 2026-04 and unstable do not expose `FileUpdateInput.revertToVersionId`, so checked-in live conformance can prove the reference-target branch but the version-id branch remains covered by deterministic LiveHybrid cassette tests and internal Shopify behavior notes.
- `fileDelete` marks files deleted in local state so downstream reads and product media references can observe the deletion. In LiveHybrid mode, deletes of product-owned media ids may first hydrate the owning product/media relationship from upstream so the local delete can remove that media node from downstream `product.media` reads. The payload's `deletedFileIds` are rebuilt from the local record's actual Files API type (`MediaImage`, `Video`, `GenericFile`, etc.) rather than echoing a caller-supplied alias GID unchanged.
- Shopify's backend can reject `fileDelete` with `FILE_LOCKED` while another media-processing mutation owns a per-file lock. The proxy has no concurrent Shopify media-processing jobs or cross-request lock manager, so it does not fabricate `FILE_LOCKED`; this remains an explicit fidelity divergence unless a future local processing model introduces lockable file state.
- `fileAcknowledgeUpdateFailed(fileIds:)` is currently a local
  payload-shape stub for existing `READY` Files API records and does not proxy
  supported requests upstream at runtime. The mutation returns selected `files`
  and `userErrors` from the local file model but does not mutate file state or
  stamp acknowledgement metadata, because Shopify exposes no
  `updateFailureAcknowledgedAt` field.
- Acknowledgement validation follows captured Shopify behavior for the supported
  local subset: unknown or deleted IDs return `files: null` with
  `FILE_DOES_NOT_EXIST`, and non-ready file states such as failed file creation
  return `NON_READY_STATE`. When any requested acknowledgement id is missing,
  Shopify returns the aggregated missing-id error and does not also report
  state errors for other supplied ids. When all ids exist but multiple files
  are non-ready, the non-ready ids aggregate into one `NON_READY_STATE` entry
  with the `Files with ids X, Y are not in the READY state.` message.
- The proxy does not model Shopify's internal `MediaError` rows,
  `mediaWarnings`, `mediaable.status`, or `preview_image.status`. Local Files
  API and product-media reads therefore expose empty `mediaErrors` /
  `mediaWarnings` lists for no-data shape, but `fileAcknowledgeUpdateFailed`
  does not claim to clear real failed inner media state.
- The proxy still does not independently perform Shopify's asynchronous
  external media processing or generate real storage-transfer failures. Those
  failed-inner-state branches remain outside the supported runtime boundary
  until the media model stores the relevant Shopify failure rows/statuses.
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
- HAR-706 narrows that support claim: until the normalized media model stores
  Shopify `MediaError` / `mediaWarnings` rows and separate inner mediaable or
  preview-image failure statuses, acknowledgement is intentionally a no-op for
  READY files. It preserves the mutation payload shape, parent READY
  validation, downstream empty error/warning list shape, and raw mutation log
  behavior without stamping synthetic acknowledgement metadata.
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
