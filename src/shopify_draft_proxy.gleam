//// Public entrypoint for the Gleam port of the Shopify draft proxy core.
////
//// At Phase 0 this is a placeholder used to verify the dual-target build,
//// the Node ESM interop boundary, and the Elixir BEAM interop boundary. The
//// real public API will replace it during Phase 2 (see
//// ../docs/gleam-runtime.md and ../docs/architecture.md).

/// Identity string returned to interop smoke tests on every target so that
/// callers can assert they reached the Gleam-compiled code rather than a
/// stale build artefact.
pub fn hello() -> String {
  "shopify_draft_proxy gleam port: phase 0"
}
