//// Mirrors `src/state/synthetic-identity.ts`.
////
//// The TypeScript class mutates two counters in place: `nextSyntheticId`
//// and `nextSyntheticTime`. Gleam values are immutable, so each
//// generator function returns the new value paired with an updated
//// registry; callers thread the registry through their own state.
////
//// The TypeScript proxy keeps a single registry alive for the lifetime
//// of the `DraftProxy` instance. When we wire the registry into the
//// Gleam port's request pipeline (Phase 2 task #15), the dispatcher
//// will own the registry and update it once per mutation.

import gleam/int
import gleam/result
import gleam/string
import shopify_draft_proxy/state/iso_timestamp

/// 2024-01-01T00:00:00.000Z in Unix milliseconds. Mirrors the
/// `Date.parse('2024-01-01T00:00:00.000Z')` constant the TS class uses
/// as its starting timestamp. The proxy fixes this so synthetic
/// timestamps stay deterministic across runs.
const epoch_2024_01_01: Int = 1_704_067_200_000

/// Two monotonically increasing counters: an integer id used to mint
/// fresh `gid://shopify/...` resource identifiers, and a millisecond
/// epoch used to mint fresh `createdAt`/`updatedAt`-style timestamps.
pub type SyntheticIdentityRegistry {
  SyntheticIdentityRegistry(next_synthetic_id: Int, next_synthetic_time: Int)
}

/// Versioned dump for state restoration. The TypeScript counterpart
/// stores the timestamp as an ISO string; we mirror that shape so the
/// JSON form is byte-identical between implementations.
pub type SyntheticIdentityStateDumpV1 {
  SyntheticIdentityStateDumpV1(
    next_synthetic_id: Int,
    next_synthetic_timestamp: String,
  )
}

/// Reasons `restore_state` can refuse a dump.
pub type RestoreError {
  /// `next_synthetic_id` was less than 1 — the TS version requires a
  /// positive integer.
  InvalidSyntheticId(Int)
  /// The ISO timestamp string failed to parse on the host platform.
  InvalidSyntheticTimestamp(String)
}

/// Fresh registry, equivalent to `new SyntheticIdentityRegistry()` in TS.
pub fn new() -> SyntheticIdentityRegistry {
  SyntheticIdentityRegistry(
    next_synthetic_id: 1,
    next_synthetic_time: epoch_2024_01_01,
  )
}

/// Reset both counters to their starting values. Mirrors `reset()`.
pub fn reset(
  _registry: SyntheticIdentityRegistry,
) -> SyntheticIdentityRegistry {
  new()
}

/// Mint a fresh `gid://shopify/<resourceType>/<id>`. Returns the new gid
/// and the registry with `next_synthetic_id` incremented.
pub fn make_synthetic_gid(
  registry: SyntheticIdentityRegistry,
  resource_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  let id = registry.next_synthetic_id
  let gid = "gid://shopify/" <> resource_type <> "/" <> int.to_string(id)
  let next = SyntheticIdentityRegistry(..registry, next_synthetic_id: id + 1)
  #(gid, next)
}

/// Mint a fresh gid tagged with the proxy's synthetic marker. Mirrors
/// `makeProxySyntheticGid`. Used for entities that the proxy fabricates
/// in response to mutations so it can later spot them in `nodes(ids:…)`
/// queries.
pub fn make_proxy_synthetic_gid(
  registry: SyntheticIdentityRegistry,
  resource_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  let id = registry.next_synthetic_id
  let gid =
    "gid://shopify/"
    <> resource_type
    <> "/"
    <> int.to_string(id)
    <> "?shopify-draft-proxy=synthetic"
  let next = SyntheticIdentityRegistry(..registry, next_synthetic_id: id + 1)
  #(gid, next)
}

/// Mint a fresh ISO 8601 timestamp. Returns the timestamp and the
/// registry with `next_synthetic_time` advanced by one second, matching
/// `makeSyntheticTimestamp` in TS.
pub fn make_synthetic_timestamp(
  registry: SyntheticIdentityRegistry,
) -> #(String, SyntheticIdentityRegistry) {
  let current = iso_timestamp.format_iso(registry.next_synthetic_time)
  let next =
    SyntheticIdentityRegistry(
      ..registry,
      next_synthetic_time: registry.next_synthetic_time + 1000,
    )
  #(current, next)
}

/// Snapshot the registry into a versioned dump record. Mirrors
/// `dumpState`.
pub fn dump_state(
  registry: SyntheticIdentityRegistry,
) -> SyntheticIdentityStateDumpV1 {
  SyntheticIdentityStateDumpV1(
    next_synthetic_id: registry.next_synthetic_id,
    next_synthetic_timestamp: iso_timestamp.format_iso(
      registry.next_synthetic_time,
    ),
  )
}

/// Build a registry from a dump, validating the inputs. Mirrors
/// `restoreState` but returns a `Result` instead of throwing, since
/// Gleam doesn't have exceptions.
pub fn restore_state(
  dump: SyntheticIdentityStateDumpV1,
) -> Result(SyntheticIdentityRegistry, RestoreError) {
  use _ <- result.try(case dump.next_synthetic_id < 1 {
    True -> Error(InvalidSyntheticId(dump.next_synthetic_id))
    False -> Ok(Nil)
  })
  use ms <- result.try(
    iso_timestamp.parse_iso(dump.next_synthetic_timestamp)
    |> result.replace_error(InvalidSyntheticTimestamp(
      dump.next_synthetic_timestamp,
    )),
  )
  Ok(SyntheticIdentityRegistry(
    next_synthetic_id: dump.next_synthetic_id,
    next_synthetic_time: ms,
  ))
}

/// Detect a gid produced by `make_proxy_synthetic_gid`. Mirrors
/// `isProxySyntheticGid`.
pub fn is_proxy_synthetic_gid(value: String) -> Bool {
  string.starts_with(value, "gid://shopify/")
  && string.contains(value, "?shopify-draft-proxy=synthetic")
}
