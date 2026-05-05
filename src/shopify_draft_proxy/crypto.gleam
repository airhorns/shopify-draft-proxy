//// Cross-target crypto helpers for the proxy.
////
//// Pass 17 introduces this for the apps mutation path: the
//// `delegateAccessTokenCreate` handler stores a sha256 of the raw token
//// rather than the token itself, and `delegateAccessTokenDestroy`
//// looks the token up by its hash. The Gleam stdlib does not include
//// hashing, so we delegate via FFI to Erlang's `crypto:hash/2`
//// (with `binary:encode_hex/2` for the lowercase hex form) and Node's
//// `crypto.createHash('sha256').update(s).digest('hex')`.
////
//// Both adapters return the lowercase hex digest of the UTF-8 encoded
//// input, matching the TS version exactly so the two implementations
//// produce byte-identical token hashes.

/// Compute the lowercase hex sha256 of a string.
@external(erlang, "crypto_ffi", "sha256_hex")
@external(javascript, "./crypto_ffi.js", "sha256_hex")
pub fn sha256_hex(input: String) -> String

/// Compute the lowercase hex md5 of a string.
@external(erlang, "crypto_ffi", "md5_hex")
@external(javascript, "./crypto_ffi.js", "md5_hex")
pub fn md5_hex(input: String) -> String
