# Media Endpoint Group

The media group is fully implemented in the operation registry for the Admin file mutation roots. Product-specific media roots remain documented with products because they stage product-owned media records.

## Supported roots

Local staged mutations:

- `fileCreate`
- `fileUpdate`
- `fileDelete`

## Behavior notes

- File mutations stage local `FileRecord` state and do not proxy supported roots upstream at runtime.
- `fileCreate` validates original source URLs and alt text length, derives a filename from the source when no filename is supplied, creates stable synthetic Shopify GIDs by content type, and returns uploaded file status.
- `fileUpdate` validates file ids, URL fields, alt text length, and product references before updating staged records.
- `fileDelete` marks files deleted in local state so downstream reads and product media references can observe the deletion.
- Product-owned media mutations (`productCreateMedia`, `productUpdateMedia`, and `productDeleteMedia`) are part of the products group because their read-after-write behavior is tied to product state.

## Validation anchors

- Runtime flow: `tests/integration/media-draft-flow.test.ts`
- Conformance fixtures and requests: `config/parity-specs/file*.json` and matching files under `config/parity-requests/`
