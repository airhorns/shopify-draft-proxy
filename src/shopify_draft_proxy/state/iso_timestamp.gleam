//// Cross-target ISO 8601 timestamp helpers.
////
//// Gleam's stdlib doesn't include date/time formatting, so we delegate
//// to platform natives via FFI: Erlang's `calendar:system_time_to_rfc3339`
//// and JavaScript's `Date.prototype.toISOString`. Both produce the
//// canonical form `YYYY-MM-DDTHH:MM:SS.sssZ` that Shopify Admin
//// timestamps use.
////
//// Inputs and outputs are millisecond Unix epochs to keep arithmetic
//// in the synthetic identity registry trivial.

/// Format `ms` (milliseconds since the Unix epoch) as
/// `YYYY-MM-DDTHH:MM:SS.sssZ`.
@external(erlang, "iso_timestamp_ffi", "format_iso")
@external(javascript, "./iso_timestamp_ffi.js", "format_iso")
pub fn format_iso(ms: Int) -> String

/// Parse an ISO 8601 timestamp string back to milliseconds since epoch.
/// Returns `Error(Nil)` if the input is not a valid timestamp the
/// underlying platform can parse.
@external(erlang, "iso_timestamp_ffi", "parse_iso")
@external(javascript, "./iso_timestamp_ffi.js", "parse_iso")
pub fn parse_iso(iso: String) -> Result(Int, Nil)

/// Wall-clock current time, formatted as `YYYY-MM-DDTHH:MM:SS.sssZ`.
/// Used by `dump_state` for the `createdAt` field of the envelope.
/// This is non-deterministic; callers that want a fixed timestamp
/// should pass one explicitly to `dump_state`.
@external(erlang, "iso_timestamp_ffi", "now_iso")
@external(javascript, "./iso_timestamp_ffi.js", "now_iso")
pub fn now_iso() -> String
