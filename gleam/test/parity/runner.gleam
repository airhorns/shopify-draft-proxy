//// Pure-Gleam parity runner.
////
//// Replaces the legacy vitest harness in
//// `tests/unit/conformance-parity-scenarios.test.ts`. Reads a parity
//// spec, loads the capture and GraphQL document referenced by the
//// spec, drives them through `draft_proxy.process_request`, and
//// compares each target's `capturePath` slice of the capture against
//// the same `proxyPath` slice of the proxy response — applying the
//// spec's `expectedDifferences` matchers.
////
//// Per-target `proxyRequest` overrides are supported. State (store,
//// synthetic identity) is threaded forward across requests, so a
//// target can read back records the primary mutation created.
////
//// File-system paths in the spec are repo-root relative. Tests run
//// from the `gleam/` subdirectory; the runner resolves paths via `..`
//// (configurable via `RunnerConfig.repo_root`).

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import parity/diff.{type Mismatch, Mismatch}
import parity/json_value.{
  type JsonValue, JArray, JBool, JFloat, JInt, JNull, JObject, JString,
}
import parity/jsonpath
import parity/spec.{
  type Spec, type Target, NoVariables, OverrideRequest, ReusePrimary,
  VariablesFromCapture, VariablesFromFile, VariablesInline,
}
import shopify_draft_proxy/proxy/draft_proxy.{
  type DraftProxy, type Response, Request,
}
import shopify_draft_proxy/state/store as store_mod
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{
  type GiftCardConfigurationRecord, type GiftCardRecipientAttributesRecord,
  type GiftCardRecord, type GiftCardTransactionRecord, type MarketingRecord,
  type MarketingValue, type MetafieldDefinitionCapabilitiesRecord,
  type MetafieldDefinitionCapabilityRecord,
  type MetafieldDefinitionConstraintsRecord, type MetafieldDefinitionRecord,
  type MetafieldDefinitionValidationRecord, type MetaobjectCapabilitiesRecord,
  type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectDefinitionCapabilityRecord, type MetaobjectDefinitionRecord,
  type MetaobjectDefinitionTypeRecord, type MetaobjectFieldDefinitionRecord,
  type MetaobjectFieldDefinitionReferenceRecord,
  type MetaobjectFieldDefinitionValidationRecord, type MetaobjectFieldRecord,
  type MetaobjectJsonValue, type MetaobjectRecord,
  type MetaobjectStandardTemplateRecord, type Money, type PaymentSettingsRecord,
  type ProductMetafieldRecord, type ShopAddressRecord, type ShopDomainRecord,
  type ShopFeaturesRecord, type ShopPlanRecord, type ShopPolicyRecord,
  type ShopRecord, type ShopResourceLimitsRecord, type ShopifyFunctionAppRecord,
  type ShopifyFunctionRecord, GiftCardConfigurationRecord,
  GiftCardRecipientAttributesRecord, GiftCardRecord, GiftCardTransactionRecord,
  MarketingBool, MarketingFloat, MarketingInt, MarketingList, MarketingNull,
  MarketingObject, MarketingRecord, MarketingString,
  MetafieldDefinitionCapabilitiesRecord, MetafieldDefinitionCapabilityRecord,
  MetafieldDefinitionConstraintValueRecord, MetafieldDefinitionConstraintsRecord,
  MetafieldDefinitionRecord, MetafieldDefinitionTypeRecord,
  MetafieldDefinitionValidationRecord, MetaobjectBool,
  MetaobjectCapabilitiesRecord, MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionCapabilityRecord, MetaobjectDefinitionRecord,
  MetaobjectDefinitionTypeRecord, MetaobjectFieldDefinitionRecord,
  MetaobjectFieldDefinitionReferenceRecord,
  MetaobjectFieldDefinitionValidationRecord, MetaobjectFieldRecord,
  MetaobjectFloat, MetaobjectInt, MetaobjectList, MetaobjectNull,
  MetaobjectObject, MetaobjectOnlineStoreCapabilityRecord,
  MetaobjectPublishableCapabilityRecord, MetaobjectRecord,
  MetaobjectStandardTemplateRecord, MetaobjectString, Money,
  PaymentSettingsRecord, ProductMetafieldRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopPolicyRecord, ShopRecord, ShopResourceLimitsRecord,
  ShopifyFunctionAppRecord, ShopifyFunctionRecord,
}
import simplifile

pub type RunError {
  /// File could not be read off disk.
  FileError(path: String, reason: String)
  /// File contents could not be parsed as JSON.
  JsonError(path: String, reason: String)
  /// Spec was malformed.
  SpecError(reason: String)
  /// Variables JSONPath did not resolve.
  VariablesUnresolved(path: String)
  /// `fromPrimaryProxyPath` substitution path didn't resolve.
  PrimaryRefUnresolved(path: String)
  /// `fromPreviousProxyPath` substitution path didn't resolve.
  PreviousRefUnresolved(path: String)
  /// `fromProxyResponse` substitution target/path didn't resolve.
  ProxyResponseRefUnresolved(target: String, path: String)
  /// `fromCapturePath` substitution path didn't resolve.
  CaptureRefUnresolved(path: String)
  /// Capture JSONPath did not resolve for a target.
  CaptureUnresolved(target: String, path: String)
  /// Proxy response JSONPath did not resolve for a target.
  ProxyUnresolved(target: String, path: String)
  /// Proxy returned a non-200 status.
  ProxyStatus(target: String, status: Int, body: String)
}

pub type TargetReport {
  TargetReport(
    name: String,
    capture_path: String,
    proxy_path: String,
    mismatches: List(Mismatch),
  )
}

pub type Report {
  Report(scenario_id: String, targets: List(TargetReport))
}

pub type RunnerConfig {
  RunnerConfig(repo_root: String)
}

type SeedMarketingRecords {
  SeedMarketingRecords(
    activities: List(MarketingRecord),
    events: List(MarketingRecord),
  )
}

pub fn default_config() -> RunnerConfig {
  RunnerConfig(repo_root: "..")
}

pub fn run(spec_path: String) -> Result(Report, RunError) {
  run_with_config(default_config(), spec_path)
}

pub fn run_with_config(
  config: RunnerConfig,
  spec_path: String,
) -> Result(Report, RunError) {
  use spec_source <- result.try(read_file(resolve(config, spec_path)))
  use parsed <- result.try(parse_spec(spec_source))
  use capture <- result.try(load_capture(config, parsed))
  use primary_doc <- result.try(
    read_file(resolve(config, parsed.proxy_request.document_path)),
  )
  use primary_vars <- result.try(resolve_variables(
    config,
    parsed.proxy_request.variables,
    capture,
    None,
    None,
    dict.new(),
    "<primary>",
  ))
  let proxy = draft_proxy.new()
  let proxy = seed_capture_preconditions(parsed, capture, proxy)
  use #(primary_response, proxy) <- result.try(execute(
    proxy,
    primary_doc,
    primary_vars,
    "<primary>",
  ))
  use primary_value <- result.try(parse_response_body(primary_response))
  use #(_proxy, target_reports) <- result.try(run_targets(
    config,
    parsed,
    capture,
    primary_value,
    proxy,
  ))
  Ok(Report(scenario_id: parsed.scenario_id, targets: target_reports))
}

fn seed_capture_preconditions(
  parsed: Spec,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case parsed.scenario_id {
    "gift-card-search-filters" ->
      seed_gift_card_lifecycle_preconditions(capture, proxy)
    "functions-metadata-local-staging"
    | "functions-owner-metadata-local-staging"
    | "functions-live-owner-metadata-read" ->
      seed_shopify_function_preconditions(capture, proxy)
    "shop-baseline-read"
    | "shop-policy-update-parity"
    | "admin-platform-store-property-node-reads" ->
      seed_shop_preconditions(capture, proxy)
    "marketing-baseline-read" ->
      seed_marketing_baseline_preconditions(capture, proxy)
    "metafield-definitions-product-read"
    | "metafield-definition-pinning-parity" ->
      seed_metafield_definition_preconditions(capture, proxy)
    "metaobject-definitions-read"
    | "metaobjects-read"
    | "metaobject-entry-lifecycle-local-staging"
    | "metaobject-reference-lifecycle"
    | "metaobject-bulk-delete-type-lifecycle"
    | "custom-data-metaobject-field-type-matrix" ->
      seed_metaobject_preconditions(parsed.scenario_id, capture, proxy)
    _ -> proxy
  }
}

fn seed_metafield_definition_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let candidates = [
    jsonpath.lookup(capture, "$.response.data.byIdentifier"),
    jsonpath.lookup(capture, "$.response.data.seedCatalog.nodes"),
    jsonpath.lookup(capture, "$.response.data.metafieldDefinitions.nodes"),
  ]
  let definition_sources =
    list.flat_map(candidates, fn(candidate) {
      case candidate {
        Some(JArray(items)) -> items
        Some(JObject(_)) -> [candidate |> option.unwrap(JNull)]
        _ -> []
      }
    })
  let definitions =
    list.filter_map(definition_sources, make_seed_metafield_definition)
    |> dedupe_metafield_definitions
  let metafields =
    list.flat_map(definition_sources, fn(source) {
      case make_seed_metafield_definition(source) {
        Ok(definition) ->
          seed_metafields_for_definition_source(source, definition)
        Error(_) -> []
      }
    })
  let seeded_store =
    proxy.store
    |> store_mod.upsert_base_metafield_definitions(definitions)
  let seeded_store =
    list.fold(metafields, seeded_store, fn(current, metafield) {
      let existing =
        store_mod.get_effective_metafields_by_owner_id(
          current,
          metafield.owner_id,
        )
      store_mod.replace_base_metafields_for_owner(
        current,
        metafield.owner_id,
        list.append(existing, [metafield]),
      )
    })
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn dedupe_metafield_definitions(
  definitions: List(MetafieldDefinitionRecord),
) -> List(MetafieldDefinitionRecord) {
  let #(_, kept) =
    list.fold(definitions, #(dict.new(), []), fn(acc, definition) {
      let #(seen, collected) = acc
      case dict.get(seen, definition.id) {
        Ok(_) -> #(seen, collected)
        Error(_) -> #(dict.insert(seen, definition.id, True), [
          definition,
          ..collected
        ])
      }
    })
  list.reverse(kept)
}

fn make_seed_metafield_definition(
  source: JsonValue,
) -> Result(MetafieldDefinitionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use namespace <- result.try(required_string_field(source, "namespace"))
  use key <- result.try(required_string_field(source, "key"))
  use owner_type <- result.try(required_string_field(source, "ownerType"))
  let type_source = read_object_field(source, "type")
  use type_name <- result.try(required_string_field_from_option(
    type_source,
    "name",
  ))
  Ok(MetafieldDefinitionRecord(
    id: id,
    name: name,
    namespace: namespace,
    key: key,
    owner_type: owner_type,
    type_: MetafieldDefinitionTypeRecord(
      name: type_name,
      category: read_string_field_from_option(type_source, "category"),
    ),
    description: read_string_field(source, "description"),
    validations: read_array_field(source, "validations")
      |> option.unwrap([])
      |> list.filter_map(make_seed_metafield_validation),
    access: read_object_field(source, "access")
      |> json_object_to_runtime_dict,
    capabilities: make_seed_metafield_capabilities(read_object_field(
      source,
      "capabilities",
    )),
    constraints: Some(
      make_seed_metafield_constraints(read_object_field(source, "constraints")),
    ),
    pinned_position: read_int_field(source, "pinnedPosition"),
    validation_status: read_string_field(source, "validationStatus")
      |> option.unwrap("ALL_VALID"),
  ))
}

fn required_string_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Result(String, Nil) {
  case read_string_field_from_option(value, name) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn make_seed_metafield_validation(
  source: JsonValue,
) -> Result(MetafieldDefinitionValidationRecord, Nil) {
  use name <- result.try(required_string_field(source, "name"))
  Ok(MetafieldDefinitionValidationRecord(
    name: name,
    value: read_string_field(source, "value"),
  ))
}

fn make_seed_metafield_capabilities(
  source: Option(JsonValue),
) -> MetafieldDefinitionCapabilitiesRecord {
  MetafieldDefinitionCapabilitiesRecord(
    admin_filterable: make_seed_metafield_capability(
      source |> option.then(read_object_field(_, "adminFilterable")),
    ),
    smart_collection_condition: make_seed_metafield_capability(
      source
      |> option.then(read_object_field(_, "smartCollectionCondition")),
    ),
    unique_values: make_seed_metafield_capability(
      source |> option.then(read_object_field(_, "uniqueValues")),
    ),
  )
}

fn make_seed_metafield_capability(
  source: Option(JsonValue),
) -> MetafieldDefinitionCapabilityRecord {
  MetafieldDefinitionCapabilityRecord(
    enabled: read_bool_field_from_option(source, "enabled")
      |> option.unwrap(False),
    eligible: read_bool_field_from_option(source, "eligible")
      |> option.unwrap(True),
    status: read_string_field_from_option(source, "status"),
  )
}

fn make_seed_metafield_constraints(
  source: Option(JsonValue),
) -> MetafieldDefinitionConstraintsRecord {
  MetafieldDefinitionConstraintsRecord(
    key: read_string_field_from_option(source, "key"),
    values: source
      |> option.then(read_object_field(_, "values"))
      |> option.then(read_array_field(_, "nodes"))
      |> option.unwrap([])
      |> list.filter_map(fn(value) {
        case read_string_field(value, "value") {
          Some(v) -> Ok(MetafieldDefinitionConstraintValueRecord(value: v))
          None -> Error(Nil)
        }
      }),
  )
}

fn seed_metafields_for_definition_source(
  source: JsonValue,
  definition: MetafieldDefinitionRecord,
) -> List(ProductMetafieldRecord) {
  let nodes =
    read_object_field(source, "metafields")
    |> option.then(read_array_field(_, "nodes"))
    |> option.unwrap([])
  list.filter_map(nodes, fn(node) {
    make_seed_product_metafield(node, definition)
  })
}

fn make_seed_product_metafield(
  source: JsonValue,
  definition: MetafieldDefinitionRecord,
) -> Result(ProductMetafieldRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  let owner_id =
    read_object_field(source, "owner")
    |> option.then(read_string_field(_, "id"))
    |> option.unwrap("seed-owner:" <> definition.id)
  Ok(ProductMetafieldRecord(
    id: id,
    owner_id: owner_id,
    namespace: read_string_field(source, "namespace")
      |> option.unwrap(definition.namespace),
    key: read_string_field(source, "key") |> option.unwrap(definition.key),
    type_: read_string_field(source, "type"),
    value: read_string_field(source, "value"),
    compare_digest: read_string_field(source, "compareDigest"),
    json_value: json_value.field(source, "jsonValue")
      |> option.map(runtime_json_from_json_value),
    created_at: read_string_field(source, "createdAt"),
    updated_at: read_string_field(source, "updatedAt"),
    owner_type: read_string_field(source, "ownerType")
      |> option.or(Some(definition.owner_type)),
  ))
}

fn seed_marketing_baseline_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.data") {
    Some(data) -> {
      let SeedMarketingRecords(activities: activities, events: events) =
        collect_seed_marketing_records(data, None, empty_seed_marketing())
      let seeded_store =
        proxy.store
        |> store_mod.upsert_base_marketing_activities(activities)
        |> store_mod.upsert_base_marketing_events(events)
      draft_proxy.DraftProxy(..proxy, store: seeded_store)
    }
    None -> proxy
  }
}

fn empty_seed_marketing() -> SeedMarketingRecords {
  SeedMarketingRecords(activities: [], events: [])
}

fn collect_seed_marketing_records(
  value: JsonValue,
  cursor: Option(String),
  collected: SeedMarketingRecords,
) -> SeedMarketingRecords {
  case value {
    JArray(items) ->
      list.fold(items, collected, fn(acc, item) {
        collect_seed_marketing_records(item, cursor, acc)
      })
    JObject(fields) -> collect_seed_marketing_object(fields, cursor, collected)
    _ -> collected
  }
}

fn collect_seed_marketing_object(
  fields: List(#(String, JsonValue)),
  cursor: Option(String),
  collected: SeedMarketingRecords,
) -> SeedMarketingRecords {
  let edge_cursor = read_string_from_fields(fields, "cursor")
  let collected = case read_value_from_fields(fields, "node"), edge_cursor {
    Some(node), Some(node_cursor) ->
      collect_seed_marketing_records(node, Some(node_cursor), collected)
    _, _ -> collected
  }
  let collected = case read_string_from_fields(fields, "id") {
    Some(id) ->
      case string.starts_with(id, "gid://shopify/MarketingActivity/") {
        True ->
          SeedMarketingRecords(..collected, activities: [
            MarketingRecord(
              id: id,
              cursor: cursor,
              data: seed_marketing_data(fields),
            ),
            ..collected.activities
          ])
        False ->
          case string.starts_with(id, "gid://shopify/MarketingEvent/") {
            True ->
              SeedMarketingRecords(..collected, events: [
                MarketingRecord(
                  id: id,
                  cursor: cursor,
                  data: seed_marketing_data(fields),
                ),
                ..collected.events
              ])
            False -> collected
          }
      }
    None -> collected
  }
  list.fold(fields, collected, fn(acc, pair) {
    let #(name, child) = pair
    case name {
      "node" -> acc
      _ -> collect_seed_marketing_records(child, None, acc)
    }
  })
}

fn seed_marketing_data(
  fields: List(#(String, JsonValue)),
) -> Dict(String, MarketingValue) {
  fields
  |> list.map(fn(pair) {
    let #(key, value) = pair
    #(key, seed_marketing_value(value))
  })
  |> dict.from_list
}

fn seed_marketing_value(value: JsonValue) -> MarketingValue {
  case value {
    JNull -> MarketingNull
    JString(value) -> MarketingString(value)
    JBool(value) -> MarketingBool(value)
    JInt(value) -> MarketingInt(value)
    JFloat(value) -> MarketingFloat(value)
    JArray(items) -> MarketingList(list.map(items, seed_marketing_value))
    JObject(fields) -> MarketingObject(seed_marketing_data(fields))
  }
}

fn read_value_from_fields(
  fields: List(#(String, JsonValue)),
  name: String,
) -> Option(JsonValue) {
  fields
  |> list.find(fn(pair) { pair.0 == name })
  |> result.map(fn(pair) { pair.1 })
  |> option.from_result
}

fn read_string_from_fields(
  fields: List(#(String, JsonValue)),
  name: String,
) -> Option(String) {
  case read_value_from_fields(fields, name) {
    Some(JString(value)) -> Some(value)
    _ -> None
  }
}

fn seed_metaobject_preconditions(
  scenario_id: String,
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let definitions = collect_metaobject_definitions(capture)
  let metaobjects =
    collect_metaobjects(capture)
    |> filter_seed_metaobjects(scenario_id)
  let seeded_store =
    proxy.store
    |> store_mod.upsert_base_metaobject_definitions(definitions)
    |> store_mod.upsert_base_metaobjects(metaobjects)
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn filter_seed_metaobjects(
  metaobjects: List(MetaobjectRecord),
  scenario_id: String,
) -> List(MetaobjectRecord) {
  case scenario_id {
    "custom-data-metaobject-field-type-matrix" ->
      list.filter(metaobjects, fn(metaobject) {
        !string.starts_with(metaobject.type_, "codex_har294_type_matrix_")
      })
    _ -> metaobjects
  }
}

fn collect_metaobject_definitions(
  value: JsonValue,
) -> List(MetaobjectDefinitionRecord) {
  let current = case make_seed_metaobject_definition(value) {
    Ok(record) -> [record]
    Error(_) -> []
  }
  list.append(current, collect_metaobject_definitions_nested(value))
}

fn collect_metaobject_definitions_nested(
  value: JsonValue,
) -> List(MetaobjectDefinitionRecord) {
  case value {
    JObject(fields) ->
      list.flat_map(fields, fn(pair) { collect_metaobject_definitions(pair.1) })
    JArray(items) -> list.flat_map(items, collect_metaobject_definitions)
    _ -> []
  }
}

fn collect_metaobjects(value: JsonValue) -> List(MetaobjectRecord) {
  let current = case make_seed_metaobject(value) {
    Ok(record) -> [record]
    Error(_) -> []
  }
  list.append(current, collect_metaobjects_nested(value))
}

fn collect_metaobjects_nested(value: JsonValue) -> List(MetaobjectRecord) {
  case value {
    JObject(fields) ->
      list.flat_map(fields, fn(pair) { collect_metaobjects(pair.1) })
    JArray(items) -> list.flat_map(items, collect_metaobjects)
    _ -> []
  }
}

fn make_seed_metaobject_definition(
  source: JsonValue,
) -> Result(MetaobjectDefinitionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/MetaobjectDefinition/") {
    False -> Error(Nil)
    True -> {
      use type_ <- result.try(required_string_field(source, "type"))
      Ok(MetaobjectDefinitionRecord(
        id: id,
        type_: type_,
        name: read_string_field(source, "name"),
        description: read_string_field(source, "description"),
        display_name_key: read_string_field(source, "displayNameKey"),
        access: read_metaobject_access(read_object_field(source, "access")),
        capabilities: read_metaobject_definition_capabilities(read_object_field(
          source,
          "capabilities",
        )),
        field_definitions: read_metaobject_field_definitions(
          read_array_field(source, "fieldDefinitions") |> option.unwrap([]),
        ),
        has_thumbnail_field: read_bool_field(source, "hasThumbnailField"),
        metaobjects_count: read_int_field(source, "metaobjectsCount"),
        standard_template: read_metaobject_standard_template(read_object_field(
          source,
          "standardTemplate",
        )),
        created_at: read_string_field(source, "createdAt"),
        updated_at: read_string_field(source, "updatedAt"),
      ))
    }
  }
}

fn make_seed_metaobject(source: JsonValue) -> Result(MetaobjectRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/Metaobject/") {
    False -> Error(Nil)
    True -> {
      use handle <- result.try(required_string_field(source, "handle"))
      use type_ <- result.try(required_string_field(source, "type"))
      Ok(MetaobjectRecord(
        id: id,
        handle: handle,
        type_: type_,
        display_name: read_string_field(source, "displayName"),
        fields: read_metaobject_fields(
          read_array_field(source, "fields") |> option.unwrap([]),
        ),
        capabilities: read_metaobject_capabilities(read_object_field(
          source,
          "capabilities",
        )),
        created_at: read_string_field(source, "createdAt"),
        updated_at: read_string_field(source, "updatedAt"),
      ))
    }
  }
}

fn read_metaobject_access(
  source: Option(JsonValue),
) -> dict.Dict(String, Option(String)) {
  let base =
    dict.from_list([
      #("admin", Some("PUBLIC_READ_WRITE")),
      #("storefront", Some("NONE")),
    ])
  case source {
    Some(JObject(fields)) ->
      list.fold(fields, base, fn(acc, pair) {
        case pair.1 {
          JString(value) -> dict.insert(acc, pair.0, Some(value))
          JNull -> dict.insert(acc, pair.0, None)
          _ -> acc
        }
      })
    _ -> base
  }
}

fn read_metaobject_definition_capabilities(
  source: Option(JsonValue),
) -> MetaobjectDefinitionCapabilitiesRecord {
  MetaobjectDefinitionCapabilitiesRecord(
    publishable: read_metaobject_definition_capability(source, "publishable"),
    translatable: read_metaobject_definition_capability(source, "translatable"),
    renderable: read_metaobject_definition_capability(source, "renderable"),
    online_store: read_metaobject_definition_capability(source, "onlineStore"),
  )
}

fn read_metaobject_definition_capability(
  source: Option(JsonValue),
  key: String,
) -> Option(MetaobjectDefinitionCapabilityRecord) {
  case source {
    Some(value) ->
      case read_object_field(value, key) {
        Some(capability) ->
          Some(MetaobjectDefinitionCapabilityRecord(
            read_bool_field(capability, "enabled") |> option.unwrap(False),
          ))
        None -> None
      }
    None -> None
  }
}

fn read_metaobject_field_definitions(
  values: List(JsonValue),
) -> List(MetaobjectFieldDefinitionRecord) {
  list.filter_map(values, fn(value) {
    case make_seed_metaobject_field_definition(value) {
      Ok(record) -> Ok(record)
      Error(_) -> Error(Nil)
    }
  })
}

fn make_seed_metaobject_field_definition(
  source: JsonValue,
) -> Result(MetaobjectFieldDefinitionRecord, Nil) {
  use key <- result.try(required_string_field(source, "key"))
  use type_ <- result.try(
    read_metaobject_type(read_object_field(source, "type")),
  )
  Ok(MetaobjectFieldDefinitionRecord(
    key: key,
    name: read_string_field(source, "name"),
    description: read_string_field(source, "description"),
    required: read_bool_field(source, "required"),
    type_: type_,
    validations: read_metaobject_validations(
      read_array_field(source, "validations") |> option.unwrap([]),
    ),
  ))
}

fn read_metaobject_type(
  source: Option(JsonValue),
) -> Result(MetaobjectDefinitionTypeRecord, Nil) {
  case source {
    Some(value) -> {
      use name <- result.try(required_string_field(value, "name"))
      Ok(MetaobjectDefinitionTypeRecord(
        name: name,
        category: read_string_field(value, "category"),
      ))
    }
    None -> Error(Nil)
  }
}

fn read_metaobject_validations(
  values: List(JsonValue),
) -> List(MetaobjectFieldDefinitionValidationRecord) {
  list.filter_map(values, fn(value) {
    case read_string_field(value, "name") {
      Some(name) ->
        Ok(MetaobjectFieldDefinitionValidationRecord(
          name,
          read_string_field(value, "value"),
        ))
      None -> Error(Nil)
    }
  })
}

fn read_metaobject_standard_template(
  source: Option(JsonValue),
) -> Option(MetaobjectStandardTemplateRecord) {
  case source {
    Some(value) ->
      Some(MetaobjectStandardTemplateRecord(
        read_string_field(value, "type"),
        read_string_field(value, "name"),
      ))
    None -> None
  }
}

fn read_metaobject_fields(
  values: List(JsonValue),
) -> List(MetaobjectFieldRecord) {
  list.filter_map(values, fn(value) {
    case make_seed_metaobject_field(value) {
      Ok(record) -> Ok(record)
      Error(_) -> Error(Nil)
    }
  })
}

fn make_seed_metaobject_field(
  source: JsonValue,
) -> Result(MetaobjectFieldRecord, Nil) {
  use key <- result.try(required_string_field(source, "key"))
  Ok(MetaobjectFieldRecord(
    key: key,
    type_: read_string_field(source, "type"),
    value: read_string_field(source, "value"),
    json_value: case json_value.field(source, "jsonValue") {
      Some(value) -> json_to_metaobject_value(value)
      None -> MetaobjectNull
    },
    definition: read_metaobject_field_reference(read_object_field(
      source,
      "definition",
    )),
  ))
}

fn read_metaobject_field_reference(
  source: Option(JsonValue),
) -> Option(MetaobjectFieldDefinitionReferenceRecord) {
  case source {
    Some(value) -> {
      case
        required_string_field(value, "key"),
        read_metaobject_type(read_object_field(value, "type"))
      {
        Ok(key), Ok(type_) ->
          Some(MetaobjectFieldDefinitionReferenceRecord(
            key: key,
            name: read_string_field(value, "name"),
            required: read_bool_field(value, "required"),
            type_: type_,
          ))
        _, _ -> None
      }
    }
    None -> None
  }
}

fn read_metaobject_capabilities(
  source: Option(JsonValue),
) -> MetaobjectCapabilitiesRecord {
  let publishable = case source {
    Some(value) ->
      case read_object_field(value, "publishable") {
        Some(p) ->
          Some(
            MetaobjectPublishableCapabilityRecord(read_string_field(p, "status")),
          )
        None -> None
      }
    None -> None
  }
  let online_store = case source {
    Some(value) ->
      case read_object_field(value, "onlineStore") {
        Some(online) ->
          Some(
            MetaobjectOnlineStoreCapabilityRecord(read_string_field(
              online,
              "templateSuffix",
            )),
          )
        None -> None
      }
    None -> None
  }
  MetaobjectCapabilitiesRecord(publishable, online_store)
}

fn json_to_metaobject_value(value: JsonValue) -> MetaobjectJsonValue {
  case value {
    JNull -> MetaobjectNull
    JBool(value) -> MetaobjectBool(value)
    JInt(value) -> MetaobjectInt(value)
    JFloat(value) -> MetaobjectFloat(value)
    JString(value) -> MetaobjectString(value)
    JArray(items) -> MetaobjectList(list.map(items, json_to_metaobject_value))
    JObject(fields) ->
      MetaobjectObject(
        list.map(fields, fn(pair) {
          #(pair.0, json_to_metaobject_value(pair.1))
        })
        |> dict.from_list,
      )
  }
}

fn seed_shop_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  case jsonpath.lookup(capture, "$.readOnlyBaselines.shop.data.shop") {
    Some(shop_json) ->
      case make_seed_shop(shop_json) {
        Ok(shop) ->
          draft_proxy.DraftProxy(
            ..proxy,
            store: store_mod.upsert_base_shop(proxy.store, shop),
          )
        Error(_) -> proxy
      }
    None -> proxy
  }
}

fn make_seed_shop(source: JsonValue) -> Result(ShopRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use name <- result.try(required_string_field(source, "name"))
  use myshopify_domain <- result.try(required_string_field(
    source,
    "myshopifyDomain",
  ))
  use url <- result.try(required_string_field(source, "url"))
  use primary_domain <- result.try(
    make_seed_shop_domain(read_object_field(source, "primaryDomain")),
  )
  use shop_address <- result.try(
    make_seed_shop_address(read_object_field(source, "shopAddress")),
  )
  use plan <- result.try(make_seed_shop_plan(read_object_field(source, "plan")))
  use resource_limits <- result.try(
    make_seed_resource_limits(read_object_field(source, "resourceLimits")),
  )
  use features <- result.try(
    make_seed_shop_features(read_object_field(source, "features")),
  )
  let payment_settings =
    make_seed_payment_settings(read_object_field(source, "paymentSettings"))
  let policies =
    read_array_field(source, "shopPolicies")
    |> option.unwrap([])
    |> list.filter_map(make_seed_shop_policy)
  Ok(ShopRecord(
    id: id,
    name: name,
    myshopify_domain: myshopify_domain,
    url: url,
    primary_domain: primary_domain,
    contact_email: read_string_field(source, "contactEmail")
      |> option.unwrap(""),
    email: read_string_field(source, "email") |> option.unwrap(""),
    currency_code: read_string_field(source, "currencyCode")
      |> option.unwrap(""),
    enabled_presentment_currencies: read_string_array_field(
      source,
      "enabledPresentmentCurrencies",
    ),
    iana_timezone: read_string_field(source, "ianaTimezone")
      |> option.unwrap(""),
    timezone_abbreviation: read_string_field(source, "timezoneAbbreviation")
      |> option.unwrap(""),
    timezone_offset: read_string_field(source, "timezoneOffset")
      |> option.unwrap(""),
    timezone_offset_minutes: read_int_field(source, "timezoneOffsetMinutes")
      |> option.unwrap(0),
    taxes_included: read_bool_field(source, "taxesIncluded")
      |> option.unwrap(False),
    tax_shipping: read_bool_field(source, "taxShipping")
      |> option.unwrap(False),
    unit_system: read_string_field(source, "unitSystem") |> option.unwrap(""),
    weight_unit: read_string_field(source, "weightUnit") |> option.unwrap(""),
    shop_address: shop_address,
    plan: plan,
    resource_limits: resource_limits,
    features: features,
    payment_settings: payment_settings,
    shop_policies: policies,
  ))
}

fn make_seed_shop_domain(
  source: Option(JsonValue),
) -> Result(ShopDomainRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      use host <- result.try(required_string_field(value, "host"))
      use url <- result.try(required_string_field(value, "url"))
      Ok(ShopDomainRecord(
        id: id,
        host: host,
        url: url,
        ssl_enabled: read_bool_field(value, "sslEnabled")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_address(
  source: Option(JsonValue),
) -> Result(ShopAddressRecord, Nil) {
  case source {
    Some(value) -> {
      use id <- result.try(required_string_field(value, "id"))
      Ok(ShopAddressRecord(
        id: id,
        address1: read_string_field(value, "address1"),
        address2: read_string_field(value, "address2"),
        city: read_string_field(value, "city"),
        company: read_string_field(value, "company"),
        coordinates_validated: read_bool_field(value, "coordinatesValidated")
          |> option.unwrap(False),
        country: read_string_field(value, "country"),
        country_code_v2: read_string_field(value, "countryCodeV2"),
        formatted: read_string_array_field(value, "formatted"),
        formatted_area: read_string_field(value, "formattedArea"),
        latitude: read_float_field(value, "latitude"),
        longitude: read_float_field(value, "longitude"),
        phone: read_string_field(value, "phone"),
        province: read_string_field(value, "province"),
        province_code: read_string_field(value, "provinceCode"),
        zip: read_string_field(value, "zip"),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_shop_plan(
  source: Option(JsonValue),
) -> Result(ShopPlanRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopPlanRecord(
        partner_development: read_bool_field(value, "partnerDevelopment")
          |> option.unwrap(False),
        public_display_name: read_string_field(value, "publicDisplayName")
          |> option.unwrap(""),
        shopify_plus: read_bool_field(value, "shopifyPlus")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_resource_limits(
  source: Option(JsonValue),
) -> Result(ShopResourceLimitsRecord, Nil) {
  case source {
    Some(value) ->
      Ok(ShopResourceLimitsRecord(
        location_limit: read_int_field(value, "locationLimit")
          |> option.unwrap(0),
        max_product_options: read_int_field(value, "maxProductOptions")
          |> option.unwrap(0),
        max_product_variants: read_int_field(value, "maxProductVariants")
          |> option.unwrap(0),
        redirect_limit_reached: read_bool_field(value, "redirectLimitReached")
          |> option.unwrap(False),
      ))
    None -> Error(Nil)
  }
}

fn make_seed_shop_features(
  source: Option(JsonValue),
) -> Result(ShopFeaturesRecord, Nil) {
  case source {
    Some(value) -> {
      let bundles = case read_object_field(value, "bundles") {
        Some(b) ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: read_bool_field(b, "eligibleForBundles")
              |> option.unwrap(False),
            ineligibility_reason: read_string_field(b, "ineligibilityReason"),
            sells_bundles: read_bool_field(b, "sellsBundles")
              |> option.unwrap(False),
          )
        None ->
          ShopBundlesFeatureRecord(
            eligible_for_bundles: False,
            ineligibility_reason: None,
            sells_bundles: False,
          )
      }
      let operations = case
        read_object_field(value, "cartTransform")
        |> option.then(fn(cart) {
          read_object_field(cart, "eligibleOperations")
        })
      {
        Some(op) ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: read_bool_field(op, "expandOperation")
              |> option.unwrap(False),
            merge_operation: read_bool_field(op, "mergeOperation")
              |> option.unwrap(False),
            update_operation: read_bool_field(op, "updateOperation")
              |> option.unwrap(False),
          )
        None ->
          ShopCartTransformEligibleOperationsRecord(
            expand_operation: False,
            merge_operation: False,
            update_operation: False,
          )
      }
      Ok(ShopFeaturesRecord(
        avalara_avatax: read_bool_field(value, "avalaraAvatax")
          |> option.unwrap(False),
        branding: read_string_field(value, "branding") |> option.unwrap(""),
        bundles: bundles,
        captcha: read_bool_field(value, "captcha") |> option.unwrap(False),
        cart_transform: ShopCartTransformFeatureRecord(
          eligible_operations: operations,
        ),
        dynamic_remarketing: read_bool_field(value, "dynamicRemarketing")
          |> option.unwrap(False),
        eligible_for_subscription_migration: read_bool_field(
          value,
          "eligibleForSubscriptionMigration",
        )
          |> option.unwrap(False),
        eligible_for_subscriptions: read_bool_field(
          value,
          "eligibleForSubscriptions",
        )
          |> option.unwrap(False),
        gift_cards: read_bool_field(value, "giftCards") |> option.unwrap(False),
        harmonized_system_code: read_bool_field(value, "harmonizedSystemCode")
          |> option.unwrap(False),
        legacy_subscription_gateway_enabled: read_bool_field(
          value,
          "legacySubscriptionGatewayEnabled",
        )
          |> option.unwrap(False),
        live_view: read_bool_field(value, "liveView") |> option.unwrap(False),
        paypal_express_subscription_gateway_status: read_string_field(
          value,
          "paypalExpressSubscriptionGatewayStatus",
        )
          |> option.unwrap(""),
        reports: read_bool_field(value, "reports") |> option.unwrap(False),
        sells_subscriptions: read_bool_field(value, "sellsSubscriptions")
          |> option.unwrap(False),
        show_metrics: read_bool_field(value, "showMetrics")
          |> option.unwrap(False),
        storefront: read_bool_field(value, "storefront") |> option.unwrap(False),
        unified_markets: read_bool_field(value, "unifiedMarkets")
          |> option.unwrap(False),
      ))
    }
    None -> Error(Nil)
  }
}

fn make_seed_payment_settings(
  source: Option(JsonValue),
) -> PaymentSettingsRecord {
  PaymentSettingsRecord(supported_digital_wallets: case source {
    Some(value) -> read_string_array_field(value, "supportedDigitalWallets")
    None -> []
  })
}

fn make_seed_shop_policy(source: JsonValue) -> Result(ShopPolicyRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  use title <- result.try(required_string_field(source, "title"))
  use body <- result.try(required_string_field(source, "body"))
  use type_ <- result.try(required_string_field(source, "type"))
  use url <- result.try(required_string_field(source, "url"))
  use created_at <- result.try(required_string_field(source, "createdAt"))
  use updated_at <- result.try(required_string_field(source, "updatedAt"))
  Ok(ShopPolicyRecord(
    id: id,
    title: title,
    body: body,
    type_: type_,
    url: url,
    created_at: created_at,
    updated_at: updated_at,
  ))
}

fn seed_shopify_function_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records = case jsonpath.lookup(capture, "$.seedShopifyFunctions") {
    Some(JArray(nodes)) -> list.filter_map(nodes, make_seed_shopify_function)
    _ -> []
  }

  let seeded_store =
    list.fold(records, proxy.store, fn(current_store, record) {
      let #(_, next_store) =
        store_mod.upsert_staged_shopify_function(current_store, record)
      next_store
    })

  // The local-runtime fixture was captured after the function metadata
  // seed step had advanced the synthetic counters once.
  let #(_, identity_after_id) =
    synthetic_identity.make_synthetic_gid(
      proxy.synthetic_identity,
      "MutationLogEntry",
    )
  let #(_, identity_after_seed) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)

  draft_proxy.DraftProxy(
    ..proxy,
    store: seeded_store,
    synthetic_identity: identity_after_seed,
  )
}

fn make_seed_shopify_function(
  source: JsonValue,
) -> Result(ShopifyFunctionRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  Ok(
    ShopifyFunctionRecord(
      id: id,
      title: read_string_field(source, "title"),
      handle: read_string_field(source, "handle"),
      api_type: read_string_field(source, "apiType"),
      description: read_string_field(source, "description"),
      app_key: read_string_field(source, "appKey"),
      app: case read_object_field(source, "app") {
        Some(app) -> Some(make_seed_shopify_function_app(app))
        None -> None
      },
    ),
  )
}

fn make_seed_shopify_function_app(
  source: JsonValue,
) -> ShopifyFunctionAppRecord {
  ShopifyFunctionAppRecord(
    typename: read_string_field(source, "__typename"),
    id: read_string_field(source, "id"),
    title: read_string_field(source, "title"),
    handle: read_string_field(source, "handle"),
    api_key: read_string_field(source, "apiKey"),
  )
}

fn seed_gift_card_lifecycle_preconditions(
  capture: JsonValue,
  proxy: DraftProxy,
) -> DraftProxy {
  let records =
    [
      jsonpath.lookup(
        capture,
        "$.operations.create.response.payload.data.giftCardCreate.giftCard",
      ),
      jsonpath.lookup(
        capture,
        "$.create.response.payload.data.giftCardCreate.giftCard",
      ),
    ]
    |> list.filter_map(fn(candidate) {
      case candidate {
        Some(value) -> make_seed_gift_card(value, Some("api_client"))
        None -> Error(Nil)
      }
    })

  let empty_read_records = case
    jsonpath.lookup(
      capture,
      "$.operations.emptyRead.response.payload.data.giftCards.nodes",
    )
  {
    Some(JArray(nodes)) ->
      list.filter_map(nodes, fn(node) { make_seed_gift_card(node, None) })
    _ -> []
  }

  let records = list.append(records, empty_read_records)
  let seeded_store = case records {
    [] -> proxy.store
    _ -> store_mod.upsert_base_gift_cards(proxy.store, records)
  }
  let seeded_store = case seed_gift_card_configuration(capture) {
    Some(configuration) ->
      store_mod.upsert_base_gift_card_configuration(seeded_store, configuration)
    None -> seeded_store
  }
  draft_proxy.DraftProxy(..proxy, store: seeded_store)
}

fn make_seed_gift_card(
  source: JsonValue,
  source_override: Option(String),
) -> Result(GiftCardRecord, Nil) {
  use id <- result.try(required_string_field(source, "id"))
  case string.starts_with(id, "gid://shopify/GiftCard/") {
    False -> Error(Nil)
    True -> {
      let last_characters =
        read_string_field(source, "lastCharacters")
        |> option.unwrap(gift_card_tail(id))
      let initial_value =
        read_money_record(read_object_field(source, "initialValue"))
      let balance =
        read_money_record(
          read_object_field(source, "balance")
          |> option.or(read_object_field(source, "initialValue")),
        )
      let recipient_attributes_source =
        read_object_field(source, "recipientAttributes")
      let recipient_source =
        recipient_attributes_source
        |> option.then(read_object_field(_, "recipient"))
      let recipient_id =
        read_string_field_from_option(recipient_source, "id")
        |> option.or(read_string_field_from_option(
          read_object_field(source, "recipient"),
          "id",
        ))
      let transactions =
        read_transactions(read_object_field(source, "transactions"))
      Ok(GiftCardRecord(
        id: id,
        legacy_resource_id: read_string_field(source, "legacyResourceId")
          |> option.unwrap(gift_card_tail(id)),
        last_characters: last_characters,
        masked_code: read_string_field(source, "maskedCode")
          |> option.unwrap(masked_code(last_characters)),
        enabled: read_bool_field(source, "enabled") |> option.unwrap(True),
        deactivated_at: read_string_field(source, "deactivatedAt"),
        expires_on: read_string_field(source, "expiresOn"),
        note: read_string_field(source, "note"),
        template_suffix: read_string_field(source, "templateSuffix"),
        created_at: read_string_field(source, "createdAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        updated_at: read_string_field(source, "updatedAt")
          |> option.unwrap("2026-01-01T00:00:00Z"),
        initial_value: initial_value,
        balance: balance,
        customer_id: read_string_field_from_option(
          read_object_field(source, "customer"),
          "id",
        ),
        recipient_id: recipient_id,
        source: case source_override {
          Some(_) -> source_override
          None -> read_string_field(source, "source")
        },
        recipient_attributes: make_seed_recipient_attributes(
          recipient_attributes_source,
          recipient_id,
        ),
        transactions: transactions,
      ))
    }
  }
}

fn seed_gift_card_configuration(
  capture: JsonValue,
) -> Option(GiftCardConfigurationRecord) {
  let primary =
    jsonpath.lookup(
      capture,
      "$.operations.configurationRead.response.payload.data.giftCardConfiguration",
    )
  let fallback =
    jsonpath.lookup(
      capture,
      "$.configurationRead.response.payload.data.giftCardConfiguration",
    )
  case primary |> option.or(fallback) {
    Some(value) ->
      Some(GiftCardConfigurationRecord(
        issue_limit: read_money_record(read_object_field(value, "issueLimit")),
        purchase_limit: read_money_record(read_object_field(
          value,
          "purchaseLimit",
        )),
      ))
    None -> None
  }
}

fn make_seed_recipient_attributes(
  source: Option(JsonValue),
  recipient_id: Option(String),
) -> Option(GiftCardRecipientAttributesRecord) {
  case source {
    None -> None
    Some(value) ->
      Some(GiftCardRecipientAttributesRecord(
        id: recipient_id,
        message: read_string_field(value, "message"),
        preferred_name: read_string_field(value, "preferredName"),
        send_notification_at: read_string_field(value, "sendNotificationAt"),
      ))
  }
}

fn read_transactions(
  source: Option(JsonValue),
) -> List(GiftCardTransactionRecord) {
  case source |> option.then(read_array_field(_, "nodes")) {
    Some(nodes) ->
      list.filter_map(nodes, fn(node) {
        let amount = read_money_record(read_object_field(node, "amount"))
        Ok(GiftCardTransactionRecord(
          id: read_string_field(node, "id")
            |> option.unwrap("gid://shopify/GiftCardTransaction/0"),
          kind: case string.starts_with(amount.amount, "-") {
            True -> "DEBIT"
            False -> "CREDIT"
          },
          amount: amount,
          processed_at: read_string_field(node, "processedAt")
            |> option.unwrap("2026-01-01T00:00:00Z"),
          note: read_string_field(node, "note"),
        ))
      })
    None -> []
  }
}

fn read_money_record(source: Option(JsonValue)) -> Money {
  case source {
    Some(value) ->
      Money(
        amount: read_string_field(value, "amount") |> option.unwrap("0.0"),
        currency_code: read_string_field(value, "currencyCode")
          |> option.unwrap("CAD"),
      )
    None -> Money(amount: "0.0", currency_code: "CAD")
  }
}

fn required_string_field(
  value: JsonValue,
  name: String,
) -> Result(String, Nil) {
  case read_string_field(value, name) {
    Some(s) -> Ok(s)
    None -> Error(Nil)
  }
}

fn read_string_field(value: JsonValue, name: String) -> Option(String) {
  case json_value.field(value, name) {
    Some(JString(s)) -> Some(s)
    _ -> None
  }
}

fn read_string_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Option(String) {
  case value {
    Some(v) -> read_string_field(v, name)
    None -> None
  }
}

fn read_bool_field(value: JsonValue, name: String) -> Option(Bool) {
  case json_value.field(value, name) {
    Some(JBool(b)) -> Some(b)
    _ -> None
  }
}

fn read_bool_field_from_option(
  value: Option(JsonValue),
  name: String,
) -> Option(Bool) {
  case value {
    Some(v) -> read_bool_field(v, name)
    None -> None
  }
}

fn read_int_field(value: JsonValue, name: String) -> Option(Int) {
  case json_value.field(value, name) {
    Some(JInt(i)) -> Some(i)
    _ -> None
  }
}

fn read_float_field(value: JsonValue, name: String) -> Option(Float) {
  case json_value.field(value, name) {
    Some(JFloat(f)) -> Some(f)
    Some(JInt(i)) -> Some(int.to_float(i))
    _ -> None
  }
}

fn read_string_array_field(value: JsonValue, name: String) -> List(String) {
  case read_array_field(value, name) {
    Some(items) ->
      list.filter_map(items, fn(item) {
        case item {
          JString(s) -> Ok(s)
          _ -> Error(Nil)
        }
      })
    None -> []
  }
}

fn read_object_field(value: JsonValue, name: String) -> Option(JsonValue) {
  case json_value.field(value, name) {
    Some(JObject(_)) as object -> object
    _ -> None
  }
}

fn read_array_field(value: JsonValue, name: String) -> Option(List(JsonValue)) {
  case json_value.field(value, name) {
    Some(JArray(items)) -> Some(items)
    _ -> None
  }
}

fn json_object_to_runtime_dict(
  value: Option(JsonValue),
) -> dict.Dict(String, json.Json) {
  case value {
    Some(JObject(entries)) ->
      entries
      |> list.map(fn(pair) {
        let #(key, item) = pair
        #(key, runtime_json_from_json_value(item))
      })
      |> dict.from_list
    _ -> dict.new()
  }
}

fn runtime_json_from_json_value(value: JsonValue) -> json.Json {
  case value {
    JNull -> json.null()
    JBool(b) -> json.bool(b)
    JInt(i) -> json.int(i)
    JFloat(f) -> json.float(f)
    JString(s) -> json.string(s)
    JArray(items) -> json.array(items, runtime_json_from_json_value)
    JObject(entries) ->
      json.object(
        list.map(entries, fn(pair) {
          let #(key, item) = pair
          #(key, runtime_json_from_json_value(item))
        }),
      )
  }
}

fn gift_card_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) ->
      case string.split(tail_with_query, on: "?") {
        [tail, ..] -> tail
        [] -> id
      }
    Error(_) -> id
  }
}

fn masked_code(last_characters: String) -> String {
  "•••• •••• •••• " <> last_characters
}

fn run_targets(
  config: RunnerConfig,
  parsed: Spec,
  capture: JsonValue,
  primary_response: JsonValue,
  proxy: DraftProxy,
) -> Result(#(DraftProxy, List(TargetReport)), RunError) {
  list.try_fold(
    parsed.targets,
    #(proxy, [], None, dict.new()),
    fn(state, target) {
      let #(current_proxy, acc_reports, previous_response, named_responses) =
        state
      use #(next_proxy, report) <- result.try(run_target(
        config,
        parsed,
        target,
        capture,
        primary_response,
        previous_response,
        named_responses,
        current_proxy,
      ))
      Ok(#(
        next_proxy,
        [report.0, ..acc_reports],
        Some(report.1),
        dict.insert(named_responses, target.name, report.1),
      ))
    },
  )
  |> result.map(fn(state) {
    let #(final_proxy, reports, _, _) = state
    #(final_proxy, list.reverse(reports))
  })
}

fn run_target(
  config: RunnerConfig,
  parsed: Spec,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  proxy: DraftProxy,
) -> Result(#(DraftProxy, #(TargetReport, JsonValue)), RunError) {
  use #(actual_response, next_proxy) <- result.try(actual_response_for(
    config,
    target,
    capture,
    primary_response,
    previous_response,
    named_responses,
    proxy,
  ))
  let expected_opt = jsonpath.lookup(capture, target.capture_path)
  let actual_opt = jsonpath.lookup(actual_response, target.proxy_path)
  case expected_opt, actual_opt {
    None, None ->
      Ok(#(
        next_proxy,
        #(
          TargetReport(
            name: target.name,
            capture_path: target.capture_path,
            proxy_path: target.proxy_path,
            mismatches: [],
          ),
          actual_response,
        ),
      ))
    None, _ ->
      Error(CaptureUnresolved(target: target.name, path: target.capture_path))
    _, None ->
      Error(ProxyUnresolved(target: target.name, path: target.proxy_path))
    Some(expected), Some(actual) -> {
      let rules = spec.rules_for(parsed, target)
      let mismatches = diff_target(target, expected, actual, rules)
      Ok(#(
        next_proxy,
        #(
          TargetReport(
            name: target.name,
            capture_path: target.capture_path,
            proxy_path: target.proxy_path,
            mismatches: mismatches,
          ),
          actual_response,
        ),
      ))
    }
  }
}

fn diff_target(
  target: Target,
  expected: JsonValue,
  actual: JsonValue,
  rules: List(diff.ExpectedDifference),
) -> List(Mismatch) {
  case target.selected_paths {
    [] -> diff.diff_with_expected(expected, actual, rules)
    selected_paths ->
      selected_paths
      |> list.flat_map(fn(path) {
        diff_selected_path(path, expected, actual, rules)
      })
  }
}

fn diff_selected_path(
  path: String,
  expected: JsonValue,
  actual: JsonValue,
  rules: List(diff.ExpectedDifference),
) -> List(Mismatch) {
  case jsonpath.lookup(expected, path), jsonpath.lookup(actual, path) {
    None, None -> []
    None, Some(actual_value) -> [
      Mismatch(
        path: path,
        expected: "<missing>",
        actual: json_value.to_string(actual_value),
      ),
    ]
    Some(expected_value), None -> [
      Mismatch(
        path: path,
        expected: json_value.to_string(expected_value),
        actual: "<missing>",
      ),
    ]
    Some(expected_value), Some(actual_value) ->
      diff.diff_with_expected(expected_value, actual_value, rules)
      |> list.map(fn(mismatch) {
        Mismatch(..mismatch, path: path <> string.drop_start(mismatch.path, 1))
      })
  }
}

/// Resolve which JsonValue tree to use as the proxy-side response for
/// a target. Targets without a per-target override reuse the primary
/// response (no extra HTTP call). Override targets execute their own
/// request, threading proxy state forward.
fn actual_response_for(
  config: RunnerConfig,
  target: Target,
  capture: JsonValue,
  primary_response: JsonValue,
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  proxy: DraftProxy,
) -> Result(#(JsonValue, DraftProxy), RunError) {
  case target.request {
    ReusePrimary -> Ok(#(primary_response, proxy))
    OverrideRequest(request: request) -> {
      use document <- result.try(
        read_file(resolve(config, request.document_path)),
      )
      use variables <- result.try(resolve_variables(
        config,
        request.variables,
        capture,
        Some(primary_response),
        previous_response,
        named_responses,
        target.name,
      ))
      use #(response, next_proxy) <- result.try(execute(
        proxy,
        document,
        variables,
        target.name,
      ))
      use value <- result.try(parse_response_body(response))
      Ok(#(value, next_proxy))
    }
  }
}

fn parse_spec(source: String) -> Result(Spec, RunError) {
  case spec.decode(source) {
    Ok(s) -> Ok(s)
    Error(_) -> Error(SpecError(reason: "could not decode parity spec"))
  }
}

fn load_capture(
  config: RunnerConfig,
  parsed: Spec,
) -> Result(JsonValue, RunError) {
  let path = resolve(config, parsed.capture_file)
  use source <- result.try(read_file(path))
  parse_json(path, source)
}

fn resolve_variables(
  config: RunnerConfig,
  variables: spec.ParityVariables,
  capture: JsonValue,
  primary_response: Option(JsonValue),
  previous_response: Option(JsonValue),
  named_responses: Dict(String, JsonValue),
  context: String,
) -> Result(JsonValue, RunError) {
  case variables {
    NoVariables -> Ok(JObject([]))
    VariablesFromCapture(path: path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(VariablesUnresolved(path: path))
      }
    VariablesFromFile(path: path) -> {
      let resolved = resolve(config, path)
      use source <- result.try(read_file(resolved))
      parse_json(resolved, source)
    }
    VariablesInline(template: template) -> {
      let _ = context
      substitute(
        template,
        primary_response,
        previous_response,
        named_responses,
        capture,
      )
    }
  }
}

/// Walk an inline variables template, substituting any
/// `{"fromPrimaryProxyPath": "$..."}` or `{"fromCapturePath": "$..."}`
/// markers with the corresponding value. Other nodes pass through.
fn substitute(
  template: JsonValue,
  primary: Option(JsonValue),
  previous: Option(JsonValue),
  named: Dict(String, JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_primary_ref(template) {
    Some(path) ->
      case primary {
        None -> Error(PrimaryRefUnresolved(path: path))
        Some(root) ->
          case jsonpath.lookup(root, path) {
            Some(value) -> Ok(value)
            None -> Error(PrimaryRefUnresolved(path: path))
          }
      }
    None ->
      case as_previous_ref(template) {
        Some(path) ->
          case previous {
            None -> Error(PreviousRefUnresolved(path: path))
            Some(root) ->
              case jsonpath.lookup(root, path) {
                Some(value) -> Ok(value)
                None -> Error(PreviousRefUnresolved(path: path))
              }
          }
        None ->
          case as_named_response_ref(template) {
            Some(ref) -> {
              let #(target, path) = ref
              case dict.get(named, target) {
                Ok(root) ->
                  case jsonpath.lookup(root, path) {
                    Some(value) -> Ok(value)
                    None -> Error(ProxyResponseRefUnresolved(target, path))
                  }
                Error(_) -> Error(ProxyResponseRefUnresolved(target, path))
              }
            }
            None ->
              substitute_capture_or_children(
                template,
                primary,
                previous,
                named,
                capture,
              )
          }
      }
  }
}

fn substitute_capture_or_children(
  template: JsonValue,
  primary: Option(JsonValue),
  previous: Option(JsonValue),
  named: Dict(String, JsonValue),
  capture: JsonValue,
) -> Result(JsonValue, RunError) {
  case as_capture_ref(template) {
    Some(path) ->
      case jsonpath.lookup(capture, path) {
        Some(value) -> Ok(value)
        None -> Error(CaptureRefUnresolved(path: path))
      }
    None ->
      case template {
        JObject(entries) ->
          entries
          |> list.try_map(fn(pair) {
            let #(k, v) = pair
            case substitute(v, primary, previous, named, capture) {
              Ok(v2) -> Ok(#(k, v2))
              Error(e) -> Error(e)
            }
          })
          |> result.map(JObject)
        JArray(items) ->
          items
          |> list.try_map(fn(item) {
            substitute(item, primary, previous, named, capture)
          })
          |> result.map(JArray)
        leaf -> Ok(leaf)
      }
  }
}

/// If `value` is exactly `{"fromPreviousProxyPath": "..."}` (one
/// entry with a string value), return the path. Otherwise None.
fn as_previous_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPreviousProxyPath", json_value.JString(path))]) ->
      Some(path)
    _ -> None
  }
}

/// If `value` is exactly `{"fromProxyResponse": "...", "path": "..."}`
/// or the same entries in the opposite order, return target/path.
fn as_named_response_ref(value: JsonValue) -> Option(#(String, String)) {
  case value {
    JObject([
      #("fromProxyResponse", json_value.JString(target)),
      #("path", json_value.JString(path)),
    ]) -> Some(#(target, path))
    JObject([
      #("path", json_value.JString(path)),
      #("fromProxyResponse", json_value.JString(target)),
    ]) -> Some(#(target, path))
    _ -> None
  }
}

/// If `value` is exactly `{"fromPrimaryProxyPath": "..."}` (one entry
/// with a string value), return the path. Otherwise None.
fn as_primary_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromPrimaryProxyPath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

/// If `value` is exactly `{"fromCapturePath": "..."}` (one entry with
/// a string value), return the path. Otherwise None.
fn as_capture_ref(value: JsonValue) -> Option(String) {
  case value {
    JObject([#("fromCapturePath", json_value.JString(path))]) -> Some(path)
    _ -> None
  }
}

fn execute(
  proxy: DraftProxy,
  document: String,
  variables: JsonValue,
  context: String,
) -> Result(#(Response, DraftProxy), RunError) {
  let body = build_graphql_body(document, variables)
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2025-01/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let #(response, next_proxy) = draft_proxy.process_request(proxy, request)
  case response.status {
    200 -> Ok(#(response, next_proxy))
    status ->
      Error(ProxyStatus(
        target: context,
        status: status,
        body: json.to_string(response.body),
      ))
  }
}

fn build_graphql_body(document: String, variables: JsonValue) -> String {
  let query = json.to_string(json.string(document))
  let vars = json_value.to_string(variables)
  "{\"query\":" <> query <> ",\"variables\":" <> vars <> "}"
}

fn parse_response_body(response: Response) -> Result(JsonValue, RunError) {
  let serialized = json.to_string(response.body)
  parse_json("<proxy-response>", serialized)
}

fn read_file(path: String) -> Result(String, RunError) {
  case simplifile.read(path) {
    Ok(s) -> Ok(s)
    Error(reason) ->
      Error(FileError(path: path, reason: simplifile.describe_error(reason)))
  }
}

fn parse_json(path: String, source: String) -> Result(JsonValue, RunError) {
  case json_value.parse(source) {
    Ok(v) -> Ok(v)
    Error(e) -> Error(JsonError(path: path, reason: e.message))
  }
}

fn resolve(config: RunnerConfig, path: String) -> String {
  case string.starts_with(path, "/") {
    True -> path
    False -> config.repo_root <> "/" <> path
  }
}

pub fn has_mismatches(report: Report) -> Bool {
  list.any(report.targets, fn(t) { t.mismatches != [] })
}

pub fn render(report: Report) -> String {
  case has_mismatches(report) {
    False -> "OK: " <> report.scenario_id
    True ->
      report.scenario_id
      <> "\n"
      <> string.join(list.map(report.targets, render_target), "\n")
  }
}

fn render_target(target: TargetReport) -> String {
  case target.mismatches {
    [] -> "  [" <> target.name <> "] OK"
    mismatches ->
      "  ["
      <> target.name
      <> "] "
      <> int.to_string(list.length(mismatches))
      <> " mismatch(es):\n"
      <> diff.render_mismatches(mismatches)
  }
}

pub fn into_assert(report: Report) -> Result(Nil, String) {
  case has_mismatches(report) {
    False -> Ok(Nil)
    True -> Error(render(report))
  }
}

pub fn render_error(error: RunError) -> String {
  case error {
    FileError(path, reason) -> "file error at " <> path <> ": " <> reason
    JsonError(path, reason) -> "json error at " <> path <> ": " <> reason
    SpecError(reason) -> "spec error: " <> reason
    VariablesUnresolved(path) -> "variables jsonpath did not resolve: " <> path
    PrimaryRefUnresolved(path) ->
      "fromPrimaryProxyPath did not resolve in primary response: " <> path
    PreviousRefUnresolved(path) ->
      "fromPreviousProxyPath did not resolve in previous proxy response: "
      <> path
    ProxyResponseRefUnresolved(target, path) ->
      "fromProxyResponse did not resolve for target '"
      <> target
      <> "' at "
      <> path
    CaptureRefUnresolved(path) ->
      "fromCapturePath did not resolve in capture: " <> path
    CaptureUnresolved(target, path) ->
      "capture jsonpath did not resolve for target '" <> target <> "': " <> path
    ProxyUnresolved(target, path) ->
      "proxy response jsonpath did not resolve for target '"
      <> target
      <> "': "
      <> path
    ProxyStatus(target, status, body) ->
      "proxy returned status "
      <> int.to_string(status)
      <> " for target '"
      <> target
      <> "': "
      <> body
  }
}
