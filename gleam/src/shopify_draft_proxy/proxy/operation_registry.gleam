//// Mirrors `src/proxy/operation-registry.ts`.
////
//// The TS module imports `config/operation-registry.json` directly via
//// `with { type: 'json' }` and validates it against
//// `operationRegistrySchema`. We can't replicate the static-import dance
//// portably across Gleam's two targets, so this module exposes a
//// `parse(json_string)` that callers feed with the raw JSON. A thin
//// FFI/loader shim (separate concern, future pass) reads the file at
//// startup and hands the string in.
////
//// The registry itself is just `List(RegistryEntry)` — no map/dict
//// caching here. Lookup helpers (`find_entry`, `list_implemented`)
//// stream the list, which is fine for ~100 entries; if dispatch on the
//// hot path benefits from a map, callers can build their own.

import gleam/dynamic/decode
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}

/// Mirrors the TS `OperationType`. Subscriptions are out of scope.
pub type OperationType {
  Query
  Mutation
}

/// Mirrors the TS `CapabilityDomain` union. The list is closed and the
/// JSON schema enforces it, so unknown values during decode are an
/// error (we surface them as `UnknownDomain` decode errors rather than
/// folding them into `Unknown`, which is reserved for the
/// "no entry matched" capability fallback).
pub type CapabilityDomain {
  Products
  AdminPlatform
  B2b
  Apps
  Media
  BulkOperations
  Customers
  Orders
  StoreProperties
  Discounts
  Events
  Functions
  Payments
  Marketing
  OnlineStore
  SavedSearches
  Privacy
  Segments
  ShippingFulfillments
  GiftCards
  Webhooks
  Localization
  Markets
  Metafields
  Metaobjects
  Unknown
}

/// Mirrors the TS `CapabilityExecution`. `Passthrough` only appears in
/// the capability fallback today — every entry in the JSON registry is
/// `overlay-read` or `stage-locally` — but the type carries it because
/// `capabilities.ts` returns it for the unknown branch.
pub type CapabilityExecution {
  OverlayRead
  StageLocally
  Passthrough
}

/// Mirrors `OperationRegistryEntry`. `support_notes` is optional —
/// defaults to `None` when the JSON omits it.
pub type RegistryEntry {
  RegistryEntry(
    name: String,
    type_: OperationType,
    domain: CapabilityDomain,
    execution: CapabilityExecution,
    implemented: Bool,
    match_names: List(String),
    runtime_tests: List(String),
    support_notes: Option(String),
  )
}

/// Parse a JSON string into a list of registry entries. Mirrors the
/// `operationRegistrySchema.parse(...)` pass at module load time in
/// `operation-registry.ts`.
pub fn parse(input: String) -> Result(List(RegistryEntry), json.DecodeError) {
  json.parse(input, decode.list(of: registry_entry_decoder()))
}

/// Vendored operation registry. Mirrors the TS module-load
/// `operationRegistrySchema.parse(operationRegistryJson)` at
/// `src/proxy/operation-registry.ts:32`. The actual list lives in
/// `operation_registry_data.default_registry/0` as Gleam source — no
/// JSON, no FFI, no runtime IO. The data module imports types from
/// here (one-way edge), so callers reach the registry via
/// `operation_registry_data.default_registry()` directly. Regenerated
/// from `config/operation-registry.json` via
/// `gleam/scripts/sync-operation-registry.sh` when the TS
/// implementation's registry changes.
fn registry_entry_decoder() -> decode.Decoder(RegistryEntry) {
  use name <- decode.field("name", decode.string)
  use type_ <- decode.field("type", operation_type_decoder())
  use domain <- decode.field("domain", domain_decoder())
  use execution <- decode.field("execution", execution_decoder())
  use implemented <- decode.field("implemented", decode.bool)
  use match_names <- decode.field("matchNames", decode.list(of: decode.string))
  use runtime_tests <- decode.field(
    "runtimeTests",
    decode.list(of: decode.string),
  )
  use support_notes <- decode.optional_field(
    "supportNotes",
    None,
    decode.optional(decode.string),
  )
  decode.success(RegistryEntry(
    name: name,
    type_: type_,
    domain: domain,
    execution: execution,
    implemented: implemented,
    match_names: match_names,
    runtime_tests: runtime_tests,
    support_notes: support_notes,
  ))
}

fn operation_type_decoder() -> decode.Decoder(OperationType) {
  use raw <- decode.then(decode.string)
  case raw {
    "query" -> decode.success(Query)
    "mutation" -> decode.success(Mutation)
    other -> decode.failure(Query, "OperationType:" <> other)
  }
}

fn domain_decoder() -> decode.Decoder(CapabilityDomain) {
  use raw <- decode.then(decode.string)
  case parse_domain(raw) {
    Some(d) -> decode.success(d)
    None -> decode.failure(Unknown, "CapabilityDomain:" <> raw)
  }
}

fn execution_decoder() -> decode.Decoder(CapabilityExecution) {
  use raw <- decode.then(decode.string)
  case raw {
    "overlay-read" -> decode.success(OverlayRead)
    "stage-locally" -> decode.success(StageLocally)
    "passthrough" -> decode.success(Passthrough)
    other -> decode.failure(OverlayRead, "CapabilityExecution:" <> other)
  }
}

fn parse_domain(raw: String) -> Option(CapabilityDomain) {
  case raw {
    "products" -> Some(Products)
    "admin-platform" -> Some(AdminPlatform)
    "b2b" -> Some(B2b)
    "apps" -> Some(Apps)
    "media" -> Some(Media)
    "bulk-operations" -> Some(BulkOperations)
    "customers" -> Some(Customers)
    "orders" -> Some(Orders)
    "store-properties" -> Some(StoreProperties)
    "discounts" -> Some(Discounts)
    "events" -> Some(Events)
    "functions" -> Some(Functions)
    "payments" -> Some(Payments)
    "marketing" -> Some(Marketing)
    "online-store" -> Some(OnlineStore)
    "saved-searches" -> Some(SavedSearches)
    "privacy" -> Some(Privacy)
    "segments" -> Some(Segments)
    "shipping-fulfillments" -> Some(ShippingFulfillments)
    "gift-cards" -> Some(GiftCards)
    "webhooks" -> Some(Webhooks)
    "localization" -> Some(Localization)
    "markets" -> Some(Markets)
    "metafields" -> Some(Metafields)
    "metaobjects" -> Some(Metaobjects)
    "unknown" -> Some(Unknown)
    _ -> None
  }
}

/// All entries, untouched. In the TS this is a defensive copy; in
/// Gleam the list is immutable so we just return it.
pub fn list_entries(registry: List(RegistryEntry)) -> List(RegistryEntry) {
  registry
}

/// Mirrors `listImplementedOperationRegistryEntries`.
pub fn list_implemented(registry: List(RegistryEntry)) -> List(RegistryEntry) {
  list.filter(registry, fn(entry) { entry.implemented })
}

/// Mirrors `findOperationRegistryEntry`. `names` is searched in order,
/// returning the first entry whose `type_` matches and whose
/// `match_names` contains the candidate. Empty / `None` candidates are
/// skipped to mirror the TS filter.
pub fn find_entry(
  registry: List(RegistryEntry),
  type_: OperationType,
  names: List(Option(String)),
) -> Option(RegistryEntry) {
  let candidates =
    list.filter_map(names, fn(name) {
      case name {
        Some(value) ->
          case value {
            "" -> Error(Nil)
            _ -> Ok(value)
          }
        None -> Error(Nil)
      }
    })
  find_first_match(registry, type_, candidates)
}

fn find_first_match(
  registry: List(RegistryEntry),
  type_: OperationType,
  candidates: List(String),
) -> Option(RegistryEntry) {
  case candidates {
    [] -> None
    [candidate, ..rest] ->
      case match_for(registry, type_, candidate) {
        Some(entry) -> Some(entry)
        None -> find_first_match(registry, type_, rest)
      }
  }
}

fn match_for(
  registry: List(RegistryEntry),
  type_: OperationType,
  candidate: String,
) -> Option(RegistryEntry) {
  list.find(registry, fn(entry) {
    entry.type_ == type_ && list.contains(entry.match_names, candidate)
  })
  |> result_to_option
}

fn result_to_option(r: Result(a, b)) -> Option(a) {
  case r {
    Ok(value) -> Some(value)
    Error(_) -> None
  }
}
