//// Store Properties domain port.
////
//// Mirrors the shop/read and shopPolicyUpdate local-staging slice from
//// `src/proxy/store-properties.ts`. Other Store Properties roots remain
//// on the TypeScript side until their state slices are ported.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Directive, type Selection, Argument, Directive, Field,
  SelectionSet, StringValue, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type ConnectionWindow, type FragmentMap, type SourceValue,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  src_object,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type PaymentSettingsRecord, type ShopAddressRecord,
  type ShopBundlesFeatureRecord, type ShopCartTransformEligibleOperationsRecord,
  type ShopCartTransformFeatureRecord, type ShopDomainRecord,
  type ShopFeaturesRecord, type ShopPlanRecord, type ShopPolicyRecord,
  type ShopRecord, type ShopResourceLimitsRecord,
  type StorePropertyMutationPayloadRecord, type StorePropertyRecord,
  type StorePropertyValue, PaymentSettingsRecord, ShopAddressRecord,
  ShopBundlesFeatureRecord, ShopCartTransformEligibleOperationsRecord,
  ShopCartTransformFeatureRecord, ShopDomainRecord, ShopFeaturesRecord,
  ShopPlanRecord, ShopPolicyRecord, ShopRecord, ShopResourceLimitsRecord,
  StorePropertyBool, StorePropertyFloat, StorePropertyInt, StorePropertyList,
  StorePropertyMutationPayloadRecord, StorePropertyNull, StorePropertyObject,
  StorePropertyRecord, StorePropertyString,
}

const shop_policy_body_limit_chars = 524_288

const shop_policy_type_order = [
  "CONTACT_INFORMATION",
  "LEGAL_NOTICE",
  "PRIVACY_POLICY",
  "REFUND_POLICY",
  "SHIPPING_POLICY",
  "SUBSCRIPTION_POLICY",
  "TERMS_OF_SALE",
  "TERMS_OF_SERVICE",
]

const shop_baseline_hydrate_operation: String = "StorePropertiesShopBaselineHydrate"

const shop_baseline_hydrate_query: String = "query StorePropertiesShopBaselineHydrate { shop { id name myshopifyDomain url primaryDomain { id host url sslEnabled } contactEmail email currencyCode enabledPresentmentCurrencies ianaTimezone timezoneAbbreviation timezoneOffset timezoneOffsetMinutes taxesIncluded taxShipping unitSystem weightUnit shopAddress { id address1 address2 city company coordinatesValidated country countryCodeV2 formatted formattedArea latitude longitude phone province provinceCode zip } plan { partnerDevelopment publicDisplayName shopifyPlus } resourceLimits { locationLimit maxProductOptions maxProductVariants redirectLimitReached } features { avalaraAvatax branding bundles { eligibleForBundles ineligibilityReason sellsBundles } captcha cartTransform { eligibleOperations { expandOperation mergeOperation updateOperation } } dynamicRemarketing eligibleForSubscriptionMigration eligibleForSubscriptions giftCards harmonizedSystemCode legacySubscriptionGatewayEnabled liveView paypalExpressSubscriptionGatewayStatus reports sellsSubscriptions showMetrics storefront unifiedMarkets } paymentSettings { supportedDigitalWallets } shopPolicies { id title body type url createdAt updatedAt } } }"

const location_hydrate_operation: String = "StorePropertiesLocationHydrate"

const location_hydrate_query: String = "query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: \"custom\", key: \"hours\") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: [\"available\", \"committed\", \"on_hand\"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }"

pub type StorePropertiesError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

type ShopPolicyUserError {
  ShopPolicyUserError(
    field: Option(List(String)),
    message: String,
    code: Option(String),
  )
}

type PolicyValidation {
  PolicyValidation(
    type_: Option(String),
    body: Option(String),
    user_errors: List(ShopPolicyUserError),
  )
}

type StagePolicyResult {
  StagePolicyResult(
    shop_policy: Option(ShopPolicyRecord),
    user_errors: List(ShopPolicyUserError),
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
  )
}

type GenericMutationResult {
  GenericMutationResult(
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    should_log: Bool,
  )
}

type QueryFieldResult {
  QueryFieldResult(key: String, value: Json, errors: List(Json))
}

pub fn is_store_properties_query_root(name: String) -> Bool {
  case name {
    "shop"
    | "location"
    | "locations"
    | "locationByIdentifier"
    | "businessEntities"
    | "businessEntity"
    | "collection" -> True
    _ -> False
  }
}

pub fn is_store_properties_mutation_root(name: String) -> Bool {
  case name {
    "shopPolicyUpdate"
    | "locationAdd"
    | "locationEdit"
    | "locationActivate"
    | "locationDeactivate"
    | "locationDelete"
    | "publishablePublish"
    | "publishablePublishToCurrentChannel"
    | "publishableUnpublish"
    | "publishableUnpublishToCurrentChannel" -> True
    _ -> False
  }
}

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "shop" ->
      store.get_effective_shop(proxy.store) == None
    parse_operation.QueryOperation, "location" ->
      !local_has_location_id(proxy, variables)
      && list.is_empty(store.list_effective_store_property_locations(
        proxy.store,
      ))
    parse_operation.QueryOperation, "locations" ->
      list.is_empty(store.list_effective_store_property_locations(proxy.store))
    parse_operation.QueryOperation, "businessEntities" ->
      list.is_empty(store.list_effective_business_entities(proxy.store))
    parse_operation.QueryOperation, "businessEntity" ->
      !local_has_business_entity_id(proxy, variables)
      && list.is_empty(store.list_effective_business_entities(proxy.store))
    parse_operation.QueryOperation, "collection" ->
      !local_has_publishable_id(proxy, variables)
    _, _ -> False
  }
}

/// Store Properties reads are mostly Pattern 1 under cassette-backed
/// LiveHybrid: forward cold shop/business/location reads verbatim, but
/// keep reads local once a mutation has staged shop, location, or
/// publishable state. Snapshot mode continues to use the local empty
/// null/array behavior.
pub fn handle_query_request(
  proxy: DraftProxy,
  request: Request,
  parsed: parse_operation.ParsedOperation,
  primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  let want_passthrough = case proxy.config.read_mode {
    LiveHybrid ->
      should_passthrough_in_live_hybrid(
        proxy,
        parsed.type_,
        primary_root_field,
        variables,
      )
    _ -> False
  }
  case want_passthrough {
    True -> passthrough.passthrough_sync(proxy, request)
    False ->
      case process(proxy.store, document, variables) {
        Ok(envelope) -> #(
          Response(status: 200, body: envelope, headers: []),
          proxy,
        )
        Error(_) -> #(
          Response(
            status: 400,
            body: json.object([
              #(
                "errors",
                json.array(
                  [
                    json.object([
                      #(
                        "message",
                        json.string("Failed to handle store properties query"),
                      ),
                    ]),
                  ],
                  fn(x) { x },
                ),
              ),
            ]),
            headers: [],
          ),
          proxy,
        )
      }
  }
}

fn local_has_location_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case
          store.get_effective_store_property_location_by_id(proxy.store, id)
        {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

fn local_has_business_entity_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_business_entity_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

fn local_has_publishable_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_publishable_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, StorePropertiesError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let results =
    list.map(fields, fn(field) {
      root_query_result(store, field, fragments, variables)
    })
  let data_entries =
    list.map(results, fn(result) { #(result.key, result.value) })
  let errors = list.flat_map(results, fn(result) { result.errors })
  let entries = case errors {
    [] -> [#("data", json.object(data_entries))]
    _ -> [
      #("errors", json.array(errors, fn(error) { error })),
      #("data", json.object(data_entries)),
    ]
  }
  Ok(json.object(entries))
}

fn root_query_result(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, ..) ->
      case name.value {
        "shop" ->
          QueryFieldResult(
            key: key,
            value: serialize_shop_root(store, field, fragments),
            errors: [],
          )
        "location" ->
          QueryFieldResult(
            key: key,
            value: serialize_location_root(store, field, fragments, variables),
            errors: [],
          )
        "locations" ->
          QueryFieldResult(
            key: key,
            value: serialize_locations_root(store, field, fragments, variables),
            errors: [],
          )
        "locationByIdentifier" ->
          serialize_location_by_identifier_result(
            store,
            field,
            key,
            fragments,
            variables,
          )
        "businessEntities" ->
          QueryFieldResult(
            key: key,
            value: serialize_business_entities_root(store, field, fragments),
            errors: [],
          )
        "businessEntity" ->
          QueryFieldResult(
            key: key,
            value: serialize_business_entity_root(
              store,
              field,
              fragments,
              variables,
            ),
            errors: [],
          )
        "collection" ->
          QueryFieldResult(
            key: key,
            value: serialize_publishable_root(
              store,
              field,
              fragments,
              variables,
            ),
            errors: [],
          )
        _ -> QueryFieldResult(key: key, value: json.null(), errors: [])
      }
    _ -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, StorePropertiesError) {
  process_mutation_with_upstream(
    store,
    identity,
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, StorePropertiesError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let initial = #([], store, identity, [], [])
  let #(entries, final_store, final_identity, staged_ids, top_errors) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        data_entries,
        current_store,
        current_identity,
        current_ids,
        current_errors,
      ) = acc
      let key = get_field_response_key(field)
      case field {
        Field(name: name, ..) ->
          case name.value {
            "shopPolicyUpdate" -> {
              let result =
                stage_shop_policy_update(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let payload =
                shop_policy_update_payload_source(
                  result.shop_policy,
                  result.user_errors,
                )
              let projected =
                project_graphql_value(
                  payload,
                  selected_children(field),
                  fragments,
                )
              let #(logged_store, logged_identity) = case result.user_errors {
                [] ->
                  record_mutation_log(
                    result.store,
                    result.identity,
                    request_path,
                    document,
                    "shopPolicyUpdate",
                    result.staged_resource_ids,
                  )
                _ -> #(result.store, result.identity)
              }
              #(
                list.append(data_entries, [#(key, projected)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
                current_errors,
              )
            }
            "locationAdd"
            | "locationEdit"
            | "locationActivate"
            | "locationDeactivate"
            | "locationDelete" -> {
              let result =
                stage_location_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                  request_path,
                  document,
                  upstream,
                )
              let #(logged_store, logged_identity) = case result.should_log {
                True ->
                  record_mutation_log(
                    result.store,
                    result.identity,
                    request_path,
                    document,
                    name.value,
                    result.staged_resource_ids,
                  )
                False -> #(result.store, result.identity)
              }
              #(
                list.append(data_entries, [#(key, result.payload)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
                list.append(current_errors, result.top_level_errors),
              )
            }
            "publishablePublish"
            | "publishablePublishToCurrentChannel"
            | "publishableUnpublish"
            | "publishableUnpublishToCurrentChannel" -> {
              let result =
                stage_publishable_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(logged_store, logged_identity) =
                record_mutation_log(
                  result.store,
                  result.identity,
                  request_path,
                  document,
                  name.value,
                  result.staged_resource_ids,
                )
              #(
                list.append(data_entries, [#(key, result.payload)]),
                logged_store,
                logged_identity,
                list.append(current_ids, result.staged_resource_ids),
                current_errors,
              )
            }
            _ -> #(
              list.append(data_entries, [#(key, json.null())]),
              current_store,
              current_identity,
              current_ids,
              current_errors,
            )
          }
        _ -> #(
          list.append(data_entries, [#(key, json.null())]),
          current_store,
          current_identity,
          current_ids,
          current_errors,
        )
      }
    })
  let data = json.object(entries)
  let envelope = case top_errors {
    [] -> json.object([#("data", data)])
    _ ->
      json.object([
        #("errors", json.array(top_errors, fn(error) { error })),
        #("data", data),
      ])
  }
  Ok(MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
  ))
}

fn selected_children(field: Selection) -> List(Selection) {
  get_selected_child_fields(field, default_selected_field_options())
}

fn selected_selections(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn serialize_shop_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      project_graphql_value(
        shop_source(shop),
        selected_children(field),
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_location_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let location = case read_string(args, "id") {
    Some(id) -> store.get_effective_store_property_location_by_id(store, id)
    None ->
      case store.list_effective_store_property_locations(store) {
        [first, ..] -> Some(first)
        [] -> None
      }
  }
  case location {
    Some(record) -> project_store_property_record(record, field, fragments)
    None -> json.null()
  }
}

fn serialize_location_by_identifier_result(
  store: Store,
  field: Selection,
  key: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> QueryFieldResult {
  let args = graphql_helpers.field_args(field, variables)
  case dict.get(args, "identifier") {
    Ok(root_field.ObjectVal(identifier)) ->
      case read_string(identifier, "id") {
        Some(id) ->
          QueryFieldResult(
            key: key,
            value: case
              store.get_effective_store_property_location_by_id(store, id)
            {
              Some(record) ->
                project_store_property_record(record, field, fragments)
              None -> json.null()
            },
            errors: [],
          )
        None ->
          QueryFieldResult(key: key, value: json.null(), errors: [
            custom_location_identifier_error(key),
          ])
      }
    _ -> QueryFieldResult(key: key, value: json.null(), errors: [])
  }
}

fn serialize_locations_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let locations = store.list_effective_store_property_locations(store)
  let window =
    paginate_store_records(locations, field, variables, fn(record, _index) {
      record.cursor |> option.unwrap(record.id)
    })
  serialize_store_record_connection(field, window, fragments)
}

fn paginate_store_records(
  records: List(StorePropertyRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  get_cursor: fn(StorePropertyRecord, Int) -> String,
) -> ConnectionWindow(StorePropertyRecord) {
  paginate_connection_items(
    records,
    field,
    variables,
    get_cursor,
    default_connection_window_options(),
  )
}

fn serialize_store_record_connection(
  field: Selection,
  window: ConnectionWindow(StorePropertyRecord),
  fragments: FragmentMap,
) -> Json {
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(record, _index) {
        record.cursor |> option.unwrap(record.id)
      },
      serialize_node: fn(record, selection, _index) {
        project_graphql_value(
          store_property_data_to_source(record.data),
          selected_selections(selection),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_business_entities_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  json.array(store.list_effective_business_entities(store), fn(record) {
    project_store_property_record(record, field, fragments)
  })
}

fn serialize_business_entity_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let entity = case read_string(args, "id") {
    Some(id) -> store.get_business_entity_by_id(store, id)
    None ->
      find_primary_business_entity(store.list_effective_business_entities(store))
  }
  case entity {
    Some(record) -> project_store_property_record(record, field, fragments)
    None -> json.null()
  }
}

fn serialize_publishable_root(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case read_string(args, "id") {
    Some(id) ->
      case store.get_effective_publishable_by_id(store, id) {
        Some(record) -> project_store_property_record(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn project_store_property_record(
  record: StorePropertyRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(
    store_property_data_to_source(record.data),
    selected_selections(field),
    fragments,
  )
}

pub fn serialize_location_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_store_property_location_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        store_property_data_to_source(record.data),
        selections,
        fragments,
      )
    None -> json.null()
  }
}

fn store_property_value_to_source(value: StorePropertyValue) -> SourceValue {
  case value {
    StorePropertyNull -> SrcNull
    StorePropertyString(value) -> SrcString(value)
    StorePropertyBool(value) -> SrcBool(value)
    StorePropertyInt(value) -> SrcInt(value)
    StorePropertyFloat(value) -> SrcFloat(value)
    StorePropertyList(values) ->
      SrcList(list.map(values, store_property_value_to_source))
    StorePropertyObject(values) -> store_property_data_to_source(values)
  }
}

fn store_property_data_to_source(
  data: Dict(String, StorePropertyValue),
) -> SourceValue {
  SrcObject(
    dict.to_list(data)
    |> list.map(fn(pair) { #(pair.0, store_property_value_to_source(pair.1)) })
    |> dict.from_list,
  )
}

fn find_primary_business_entity(
  entities: List(StorePropertyRecord),
) -> Option(StorePropertyRecord) {
  case entities {
    [] -> None
    [first, ..rest] ->
      case dict.get(first.data, "primary") {
        Ok(StorePropertyBool(True)) -> Some(first)
        _ -> find_primary_business_entity(rest)
      }
  }
}

fn custom_location_identifier_error(key: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Metafield definition of type 'id' is required when using custom ids.",
      ),
    ),
    #(
      "locations",
      json.array([#(3, 5)], fn(location) {
        json.object([
          #("line", json.int(location.0)),
          #("column", json.int(location.1)),
        ])
      }),
    ),
    #("extensions", json.object([#("code", json.string("NOT_FOUND"))])),
    #("path", json.array([key], json.string)),
  ])
}

pub fn serialize_shop_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.id == id {
        True -> project_graphql_value(shop_source(shop), selections, fragments)
        False -> json.null()
      }
    None -> json.null()
  }
}

pub fn serialize_shop_address_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.shop_address.id == id {
        True ->
          project_graphql_value(
            shop_address_source(shop.shop_address),
            selections,
            fragments,
          )
        False -> json.null()
      }
    None -> json.null()
  }
}

pub fn serialize_shop_policy_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case find_policy_by_id(shop.shop_policies, id) {
        Some(policy) ->
          project_graphql_value(
            shop_policy_source(policy),
            selections,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

pub fn primary_domain_for_id(
  store: Store,
  id: String,
) -> Option(ShopDomainRecord) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.primary_domain.id == id {
        True -> Some(shop.primary_domain)
        False -> None
      }
    None -> None
  }
}

pub fn shop_source(shop: ShopRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Shop")),
    #("id", SrcString(shop.id)),
    #("name", SrcString(shop.name)),
    #("myshopifyDomain", SrcString(shop.myshopify_domain)),
    #("url", SrcString(shop.url)),
    #("primaryDomain", shop_domain_source(shop.primary_domain)),
    #("contactEmail", SrcString(shop.contact_email)),
    #("email", SrcString(shop.email)),
    #("currencyCode", SrcString(shop.currency_code)),
    #(
      "enabledPresentmentCurrencies",
      SrcList(list.map(shop.enabled_presentment_currencies, SrcString)),
    ),
    #("ianaTimezone", SrcString(shop.iana_timezone)),
    #("timezoneAbbreviation", SrcString(shop.timezone_abbreviation)),
    #("timezoneOffset", SrcString(shop.timezone_offset)),
    #("timezoneOffsetMinutes", SrcInt(shop.timezone_offset_minutes)),
    #("taxesIncluded", SrcBool(shop.taxes_included)),
    #("taxShipping", SrcBool(shop.tax_shipping)),
    #("unitSystem", SrcString(shop.unit_system)),
    #("weightUnit", SrcString(shop.weight_unit)),
    #("shopAddress", shop_address_source(shop.shop_address)),
    #("plan", shop_plan_source(shop.plan)),
    #("resourceLimits", shop_resource_limits_source(shop.resource_limits)),
    #("features", shop_features_source(shop.features)),
    #("paymentSettings", payment_settings_source(shop.payment_settings)),
    #("shopPolicies", SrcList(list.map(shop.shop_policies, shop_policy_source))),
  ])
}

pub fn shop_domain_source(domain: ShopDomainRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Domain")),
    #("id", SrcString(domain.id)),
    #("host", SrcString(domain.host)),
    #("url", SrcString(domain.url)),
    #("sslEnabled", SrcBool(domain.ssl_enabled)),
  ])
}

fn shop_address_source(address: ShopAddressRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopAddress")),
    #("id", SrcString(address.id)),
    #("address1", graphql_helpers.option_string_source(address.address1)),
    #("address2", graphql_helpers.option_string_source(address.address2)),
    #("city", graphql_helpers.option_string_source(address.city)),
    #("company", graphql_helpers.option_string_source(address.company)),
    #("coordinatesValidated", SrcBool(address.coordinates_validated)),
    #("country", graphql_helpers.option_string_source(address.country)),
    #(
      "countryCodeV2",
      graphql_helpers.option_string_source(address.country_code_v2),
    ),
    #("formatted", SrcList(list.map(address.formatted, SrcString))),
    #(
      "formattedArea",
      graphql_helpers.option_string_source(address.formatted_area),
    ),
    #("latitude", optional_float_source(address.latitude)),
    #("longitude", optional_float_source(address.longitude)),
    #("phone", graphql_helpers.option_string_source(address.phone)),
    #("province", graphql_helpers.option_string_source(address.province)),
    #(
      "provinceCode",
      graphql_helpers.option_string_source(address.province_code),
    ),
    #("zip", graphql_helpers.option_string_source(address.zip)),
  ])
}

fn shop_plan_source(plan: ShopPlanRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPlan")),
    #("partnerDevelopment", SrcBool(plan.partner_development)),
    #("publicDisplayName", SrcString(plan.public_display_name)),
    #("shopifyPlus", SrcBool(plan.shopify_plus)),
  ])
}

fn shop_resource_limits_source(
  limits: ShopResourceLimitsRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopResourceLimits")),
    #("locationLimit", SrcInt(limits.location_limit)),
    #("maxProductOptions", SrcInt(limits.max_product_options)),
    #("maxProductVariants", SrcInt(limits.max_product_variants)),
    #("redirectLimitReached", SrcBool(limits.redirect_limit_reached)),
  ])
}

fn shop_features_source(features: ShopFeaturesRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopFeatures")),
    #("avalaraAvatax", SrcBool(features.avalara_avatax)),
    #("branding", SrcString(features.branding)),
    #("bundles", shop_bundles_feature_source(features.bundles)),
    #("captcha", SrcBool(features.captcha)),
    #(
      "cartTransform",
      shop_cart_transform_feature_source(features.cart_transform),
    ),
    #("dynamicRemarketing", SrcBool(features.dynamic_remarketing)),
    #(
      "eligibleForSubscriptionMigration",
      SrcBool(features.eligible_for_subscription_migration),
    ),
    #("eligibleForSubscriptions", SrcBool(features.eligible_for_subscriptions)),
    #("giftCards", SrcBool(features.gift_cards)),
    #("harmonizedSystemCode", SrcBool(features.harmonized_system_code)),
    #(
      "legacySubscriptionGatewayEnabled",
      SrcBool(features.legacy_subscription_gateway_enabled),
    ),
    #("liveView", SrcBool(features.live_view)),
    #(
      "paypalExpressSubscriptionGatewayStatus",
      SrcString(features.paypal_express_subscription_gateway_status),
    ),
    #("reports", SrcBool(features.reports)),
    #("sellsSubscriptions", SrcBool(features.sells_subscriptions)),
    #("showMetrics", SrcBool(features.show_metrics)),
    #("storefront", SrcBool(features.storefront)),
    #("unifiedMarkets", SrcBool(features.unified_markets)),
  ])
}

fn shop_bundles_feature_source(
  feature: ShopBundlesFeatureRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopBundlesFeature")),
    #("eligibleForBundles", SrcBool(feature.eligible_for_bundles)),
    #(
      "ineligibilityReason",
      graphql_helpers.option_string_source(feature.ineligibility_reason),
    ),
    #("sellsBundles", SrcBool(feature.sells_bundles)),
  ])
}

fn shop_cart_transform_feature_source(
  feature: ShopCartTransformFeatureRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopCartTransformFeature")),
    #(
      "eligibleOperations",
      shop_cart_transform_eligible_operations_source(
        feature.eligible_operations,
      ),
    ),
  ])
}

fn shop_cart_transform_eligible_operations_source(
  operations: ShopCartTransformEligibleOperationsRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopCartTransformEligibleOperations")),
    #("expandOperation", SrcBool(operations.expand_operation)),
    #("mergeOperation", SrcBool(operations.merge_operation)),
    #("updateOperation", SrcBool(operations.update_operation)),
  ])
}

fn payment_settings_source(settings: PaymentSettingsRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentSettings")),
    #(
      "supportedDigitalWallets",
      SrcList(list.map(settings.supported_digital_wallets, SrcString)),
    ),
  ])
}

fn shop_policy_source(policy: ShopPolicyRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPolicy")),
    #("id", SrcString(policy.id)),
    #("title", SrcString(policy.title)),
    #("body", SrcString(policy.body)),
    #("type", SrcString(policy.type_)),
    #("url", SrcString(policy.url)),
    #("createdAt", SrcString(policy.created_at)),
    #("updatedAt", SrcString(policy.updated_at)),
    #("translations", SrcList([])),
  ])
}

fn shop_policy_update_payload_source(
  policy: Option(ShopPolicyRecord),
  errors: List(ShopPolicyUserError),
) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPolicyUpdatePayload")),
    #("shopPolicy", case policy {
      Some(p) -> shop_policy_source(p)
      None -> SrcNull
    }),
    #("userErrors", SrcList(list.map(errors, shop_policy_user_error_source))),
  ])
}

fn shop_policy_user_error_source(error: ShopPolicyUserError) -> SourceValue {
  src_object([
    #("__typename", SrcString("ShopPolicyUserError")),
    #("field", case error.field {
      Some(parts) -> SrcList(list.map(parts, SrcString))
      None -> SrcNull
    }),
    #("message", SrcString(error.message)),
    #("code", graphql_helpers.option_string_source(error.code)),
  ])
}

fn stage_shop_policy_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  _fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> StagePolicyResult {
  let args = case root_field.get_field_arguments(field, variables) {
    Ok(d) -> d
    Error(_) -> dict.new()
  }
  let input = read_shop_policy_input(args)
  let validation = validate_shop_policy_input(input)
  case validation.user_errors, validation.type_, validation.body {
    [], Some(type_), Some(body) ->
      stage_valid_shop_policy_update(
        hydrate_shop_baseline_if_needed(store, upstream),
        identity,
        type_,
        body,
      )
    _, _, _ ->
      StagePolicyResult(
        shop_policy: None,
        user_errors: validation.user_errors,
        store: store,
        identity: identity,
        staged_resource_ids: [],
      )
  }
}

fn stage_location_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  request_path: String,
  document: String,
  upstream: UpstreamContext,
) -> GenericMutationResult {
  case root_name {
    "locationAdd" ->
      stage_location_add(store, identity, field, fragments, variables)
    "locationEdit" ->
      stage_location_edit(store, identity, field, fragments, variables)
    "locationActivate" | "locationDeactivate" ->
      case
        admin_api_version_at_least(request_path, "2026-04")
        && !has_idempotency_key(field, document, variables)
      {
        True -> missing_idempotency_location_result(store, identity, root_name)
        False ->
          stage_location_lifecycle_mutation(
            store,
            identity,
            root_name,
            field,
            fragments,
            variables,
            upstream,
          )
      }
    "locationDelete" ->
      stage_location_delete(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      )
    _ ->
      GenericMutationResult(
        payload: json.null(),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
        should_log: False,
      )
  }
}

fn stage_location_lifecycle_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let store = case read_string(args, "locationId") {
    Some(id) -> hydrate_location_if_missing(store, upstream, id)
    None -> store
  }
  case root_name {
    "locationActivate" ->
      stage_location_activate(store, identity, field, fragments, variables)
    "locationDeactivate" ->
      stage_location_deactivate(store, identity, field, fragments, variables)
    _ ->
      GenericMutationResult(
        payload: json.null(),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
        should_log: False,
      )
  }
}

fn stage_location_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let input = read_object(args, "input")
  let name = input |> option.then(fn(values) { read_string(values, "name") })
  case name {
    Some(value) ->
      case string.trim(value) {
        "" -> location_add_blank_name_result(store, identity, field, fragments)
        _ -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(identity, "Location")
          let record =
            StorePropertyRecord(
              id: id,
              cursor: None,
              data: dict.from_list([
                #("__typename", StorePropertyString("Location")),
                #("id", StorePropertyString(id)),
                #("name", StorePropertyString(value)),
                #("isActive", StorePropertyBool(True)),
                #("activatable", StorePropertyBool(False)),
                #("deactivatable", StorePropertyBool(True)),
                #("deletable", StorePropertyBool(False)),
              ]),
            )
          let #(_, next_store) =
            store.upsert_staged_store_property_location(store, record)
          let payload_source =
            src_object([
              #("location", store_property_data_to_source(record.data)),
              #("userErrors", SrcList([])),
            ])
          GenericMutationResult(
            payload: project_graphql_value(
              payload_source,
              selected_children(field),
              fragments,
            ),
            store: next_store,
            identity: next_identity,
            staged_resource_ids: [id],
            top_level_errors: [],
            should_log: True,
          )
        }
      }
    _ -> {
      location_add_blank_name_result(store, identity, field, fragments)
    }
  }
}

fn location_add_blank_name_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> GenericMutationResult {
  let payload_source =
    src_object([
      #("location", SrcNull),
      #(
        "userErrors",
        SrcList([
          src_object([
            #("field", SrcList([SrcString("input"), SrcString("name")])),
            #("message", SrcString("Add a location name")),
          ]),
        ]),
      ),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    should_log: False,
  )
}

fn stage_location_edit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let id = read_string(args, "id")
  case id {
    Some(location_id) ->
      case
        store.get_effective_store_property_location_by_id(store, location_id)
      {
        Some(record) -> {
          let input = read_object(args, "input")
          let next_data = case
            input |> option.then(fn(values) { read_string(values, "name") })
          {
            Some(name) ->
              dict.insert(record.data, "name", StorePropertyString(name))
            None -> record.data
          }
          let next_record = StorePropertyRecord(..record, data: next_data)
          let #(_, next_store) =
            store.upsert_staged_store_property_location(store, next_record)
          let payload_source =
            src_object([
              #("location", store_property_data_to_source(next_record.data)),
              #("userErrors", SrcList([])),
            ])
          GenericMutationResult(
            payload: project_graphql_value(
              payload_source,
              selected_children(field),
              fragments,
            ),
            store: next_store,
            identity: identity,
            staged_resource_ids: [location_id],
            top_level_errors: [],
            should_log: True,
          )
        }
        None ->
          location_user_error_result(
            store,
            identity,
            field,
            fragments,
            "location",
            "userErrors",
            "id",
            "Location not found.",
            None,
          )
      }
    None ->
      location_user_error_result(
        store,
        identity,
        field,
        fragments,
        "location",
        "userErrors",
        "id",
        "Location not found.",
        None,
      )
  }
}

fn stage_location_activate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  case read_string(args, "locationId") {
    Some(location_id) ->
      case
        store.get_effective_store_property_location_by_id(store, location_id)
      {
        Some(record) ->
          case location_bool_field(record, "isActive", True) {
            True ->
              location_lifecycle_result(
                store,
                identity,
                field,
                fragments,
                "locationActivateUserErrors",
                Some(record),
                [],
                [location_id],
                True,
              )
            False ->
              case location_bool_field(record, "activatable", True) {
                False ->
                  location_lifecycle_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "locationActivateUserErrors",
                    Some(record),
                    [
                      location_error_source(
                        "locationId",
                        "Location cannot be activated.",
                        "GENERIC_ERROR",
                      ),
                    ],
                    [],
                    False,
                  )
                True ->
                  case has_duplicate_active_location_name(store, record) {
                    True ->
                      location_lifecycle_result(
                        store,
                        identity,
                        field,
                        fragments,
                        "locationActivateUserErrors",
                        Some(record),
                        [
                          location_error_source(
                            "locationId",
                            "A location with this name already exists.",
                            "HAS_NON_UNIQUE_NAME",
                          ),
                        ],
                        [],
                        False,
                      )
                    False -> {
                      let #(now, next_identity) =
                        synthetic_identity.make_synthetic_timestamp(identity)
                      let next_record =
                        StorePropertyRecord(
                          ..record,
                          data: record.data
                            |> dict.insert("isActive", StorePropertyBool(True))
                            |> dict.insert(
                              "activatable",
                              StorePropertyBool(True),
                            )
                            |> dict.insert(
                              "deactivatable",
                              StorePropertyBool(True),
                            )
                            |> dict.insert("deactivatedAt", StorePropertyNull)
                            |> dict.insert(
                              "deletable",
                              StorePropertyBool(False),
                            )
                            |> dict.insert(
                              "fulfillsOnlineOrders",
                              StorePropertyBool(location_bool_field(
                                record,
                                "fulfillsOnlineOrders",
                                True,
                              )),
                            )
                            |> dict.insert(
                              "shipsInventory",
                              StorePropertyBool(location_bool_field(
                                record,
                                "shipsInventory",
                                False,
                              )),
                            )
                            |> dict.insert(
                              "updatedAt",
                              StorePropertyString(now),
                            ),
                        )
                      let #(_, next_store) =
                        store.upsert_staged_store_property_location(
                          store,
                          next_record,
                        )
                      location_lifecycle_result(
                        next_store,
                        next_identity,
                        field,
                        fragments,
                        "locationActivateUserErrors",
                        Some(next_record),
                        [],
                        [location_id],
                        True,
                      )
                    }
                  }
              }
          }
        None ->
          location_lifecycle_not_found_result(
            store,
            identity,
            field,
            fragments,
            "locationActivateUserErrors",
            "locationId",
          )
      }
    None ->
      location_lifecycle_not_found_result(
        store,
        identity,
        field,
        fragments,
        "locationActivateUserErrors",
        "locationId",
      )
  }
}

fn stage_location_deactivate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  case read_string(args, "locationId") {
    Some(location_id) ->
      case
        store.get_effective_store_property_location_by_id(store, location_id)
      {
        Some(record) ->
          case location_bool_field(record, "isActive", True) {
            False ->
              location_lifecycle_result(
                store,
                identity,
                field,
                fragments,
                "locationDeactivateUserErrors",
                Some(record),
                [],
                [location_id],
                True,
              )
            True ->
              case location_bool_field(record, "hasUnfulfilledOrders", False) {
                True ->
                  location_lifecycle_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "locationDeactivateUserErrors",
                    Some(record),
                    [
                      location_error_source(
                        "locationId",
                        "Location could not be deactivated because it has pending orders.",
                        "HAS_FULFILLMENT_ORDERS_ERROR",
                      ),
                    ],
                    [],
                    False,
                  )
                False ->
                  case
                    is_only_active_online_fulfilling_location(store, record)
                  {
                    True ->
                      location_lifecycle_result(
                        store,
                        identity,
                        field,
                        fragments,
                        "locationDeactivateUserErrors",
                        Some(record),
                        [
                          location_error_source(
                            "locationId",
                            "Location could not be deactivated because it is the only location that fulfills online orders.",
                            "CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT",
                          ),
                        ],
                        [],
                        False,
                      )
                    False -> {
                      let #(now, next_identity) =
                        synthetic_identity.make_synthetic_timestamp(identity)
                      let next_record =
                        StorePropertyRecord(
                          ..record,
                          data: record.data
                            |> dict.insert("isActive", StorePropertyBool(False))
                            |> dict.insert(
                              "activatable",
                              StorePropertyBool(True),
                            )
                            |> dict.insert(
                              "deactivatable",
                              StorePropertyBool(True),
                            )
                            |> dict.insert(
                              "deactivatedAt",
                              StorePropertyString(now),
                            )
                            |> dict.insert("deletable", StorePropertyBool(True))
                            |> dict.insert(
                              "fulfillsOnlineOrders",
                              StorePropertyBool(False),
                            )
                            |> dict.insert(
                              "hasActiveInventory",
                              StorePropertyBool(False),
                            )
                            |> dict.insert(
                              "shipsInventory",
                              StorePropertyBool(False),
                            )
                            |> dict.insert(
                              "updatedAt",
                              StorePropertyString(now),
                            ),
                        )
                      let #(_, next_store) =
                        store.upsert_staged_store_property_location(
                          store,
                          next_record,
                        )
                      location_lifecycle_result(
                        next_store,
                        next_identity,
                        field,
                        fragments,
                        "locationDeactivateUserErrors",
                        Some(next_record),
                        [],
                        [location_id],
                        True,
                      )
                    }
                  }
              }
          }
        None ->
          location_lifecycle_not_found_result(
            store,
            identity,
            field,
            fragments,
            "locationDeactivateUserErrors",
            "locationId",
          )
      }
    None ->
      location_lifecycle_not_found_result(
        store,
        identity,
        field,
        fragments,
        "locationDeactivateUserErrors",
        "locationId",
      )
  }
}

fn stage_location_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let location_id = read_string(args, "locationId")
  case location_id {
    Some(id) -> {
      // Pattern 2: delete validation needs the prior Location row so
      // active/inventory guardrails are local, not proxied mutations.
      let store = hydrate_location_if_missing(store, upstream, id)
      case store.get_effective_store_property_location_by_id(store, id) {
        Some(record) ->
          case
            store_property_bool_field(record, "isActive") |> option.unwrap(True)
          {
            True ->
              active_location_delete_result(store, identity, field, fragments)
            False -> {
              let next_store =
                store.delete_staged_store_property_location(store, id)
              let payload_source =
                src_object([
                  #("deletedLocationId", SrcString(id)),
                  #("locationDeleteUserErrors", SrcList([])),
                ])
              GenericMutationResult(
                payload: project_graphql_value(
                  payload_source,
                  selected_children(field),
                  fragments,
                ),
                store: next_store,
                identity: identity,
                staged_resource_ids: [id],
                top_level_errors: [],
                should_log: True,
              )
            }
          }
        None ->
          location_user_error_result(
            store,
            identity,
            field,
            fragments,
            "deletedLocationId",
            "locationDeleteUserErrors",
            "locationId",
            "Location not found.",
            Some("LOCATION_NOT_FOUND"),
          )
      }
    }
    None ->
      location_user_error_result(
        store,
        identity,
        field,
        fragments,
        "deletedLocationId",
        "locationDeleteUserErrors",
        "locationId",
        "Location not found.",
        Some("LOCATION_NOT_FOUND"),
      )
  }
}

fn missing_idempotency_location_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
) -> GenericMutationResult {
  GenericMutationResult(
    payload: json.null(),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [missing_idempotency_error(root_name)],
    should_log: False,
  )
}

fn missing_idempotency_error(root_name: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "The @idempotent directive is required for this mutation but was not provided.",
      ),
    ),
    #(
      "locations",
      json.array([#(3, 3)], fn(location) {
        json.object([
          #("line", json.int(location.0)),
          #("column", json.int(location.1)),
        ])
      }),
    ),
    #("extensions", json.object([#("code", json.string("BAD_REQUEST"))])),
    #("path", json.array([root_name], json.string)),
  ])
}

fn has_idempotency_key(
  field: Selection,
  _document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case read_field_idempotency_key(field, variables) {
    Some(_) -> True
    None -> False
  }
}

fn read_field_idempotency_key(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case field {
    Field(directives: directives, ..) ->
      read_idempotency_key_from_directives(directives, variables)
    _ -> None
  }
}

fn read_idempotency_key_from_directives(
  directives: List(Directive),
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  let directive_arguments =
    directives
    |> list.filter_map(fn(directive) {
      case directive {
        Directive(name: name, arguments: arguments, ..)
          if name.value == "idempotent"
        -> Ok(arguments)
        _ -> Error(Nil)
      }
    })
    |> list.first
    |> option.from_result
  case directive_arguments {
    None -> None
    Some(arguments) -> {
      let argument = case find_argument(arguments, "key") {
        Some(argument) -> Some(argument)
        None -> find_argument(arguments, "idempotencyKey")
      }
      case argument {
        Some(Argument(value: StringValue(value: value, ..), ..)) ->
          non_empty_string(value)
        Some(Argument(value: VariableValue(variable: variable), ..)) ->
          case dict.get(variables, variable.name.value) {
            Ok(root_field.StringVal(value)) -> non_empty_string(value)
            _ -> None
          }
        _ -> None
      }
    }
  }
}

fn find_argument(arguments: List(Argument), name: String) -> Option(Argument) {
  arguments
  |> list.find_map(fn(argument) {
    case argument {
      Argument(name: argument_name, ..) if argument_name.value == name ->
        Ok(argument)
      _ -> Error(Nil)
    }
  })
  |> option.from_result
}

fn non_empty_string(value: String) -> Option(String) {
  let trimmed = string.trim(value)
  case string.length(trimmed) > 0 {
    True -> Some(trimmed)
    False -> None
  }
}

fn admin_api_version_at_least(
  request_path: String,
  minimum_version: String,
) -> Bool {
  case
    admin_api_version_from_path(request_path),
    parse_admin_api_version(minimum_version)
  {
    Some(version), Some(minimum) -> compare_admin_api_versions(version, minimum)
    _, _ -> False
  }
}

fn admin_api_version_from_path(path: String) -> Option(#(Int, Int)) {
  case string.split(path, "/") {
    ["", "admin", "api", version, "graphql.json"] ->
      parse_admin_api_version(version)
    _ -> None
  }
}

fn parse_admin_api_version(version: String) -> Option(#(Int, Int)) {
  case string.split(version, "-") {
    [year, month] ->
      case int.parse(year), int.parse(month) {
        Ok(parsed_year), Ok(parsed_month) -> Some(#(parsed_year, parsed_month))
        _, _ -> None
      }
    _ -> None
  }
}

fn compare_admin_api_versions(version: #(Int, Int), minimum: #(Int, Int)) {
  let #(year, month) = version
  let #(minimum_year, minimum_month) = minimum
  year > minimum_year || { year == minimum_year && month >= minimum_month }
}

fn active_location_delete_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> GenericMutationResult {
  let payload_source =
    src_object([
      #("deletedLocationId", SrcNull),
      #(
        "locationDeleteUserErrors",
        SrcList([
          src_object([
            #("field", SrcList([SrcString("locationId")])),
            #(
              "message",
              SrcString("The location cannot be deleted while it is active."),
            ),
            #("code", SrcString("LOCATION_IS_ACTIVE")),
          ]),
          src_object([
            #("field", SrcList([SrcString("locationId")])),
            #(
              "message",
              SrcString(
                "The location cannot be deleted while it has inventory.",
              ),
            ),
            #("code", SrcString("LOCATION_HAS_INVENTORY")),
          ]),
        ]),
      ),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    should_log: False,
  )
}

fn location_user_error_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  nullable_field: String,
  errors_field: String,
  error_field: String,
  message: String,
  code: Option(String),
) -> GenericMutationResult {
  let error_entries = case code {
    Some(value) -> [
      #("field", SrcList([SrcString(error_field)])),
      #("message", SrcString(message)),
      #("code", SrcString(value)),
    ]
    None -> [
      #("field", SrcList([SrcString(error_field)])),
      #("message", SrcString(message)),
    ]
  }
  let payload_source =
    src_object([
      #(nullable_field, SrcNull),
      #(errors_field, SrcList([src_object(error_entries)])),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: [],
    top_level_errors: [],
    should_log: False,
  )
}

fn location_lifecycle_not_found_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors_field: String,
  error_field: String,
) -> GenericMutationResult {
  location_lifecycle_result(
    store,
    identity,
    field,
    fragments,
    errors_field,
    None,
    [
      location_error_source(
        error_field,
        "Location not found.",
        "LOCATION_NOT_FOUND",
      ),
    ],
    [],
    False,
  )
}

fn location_lifecycle_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors_field: String,
  location: Option(StorePropertyRecord),
  errors: List(SourceValue),
  staged_resource_ids: List(String),
  should_log: Bool,
) -> GenericMutationResult {
  let payload_source =
    src_object([
      #("location", case location {
        Some(record) -> store_property_data_to_source(record.data)
        None -> SrcNull
      }),
      #(errors_field, SrcList(errors)),
    ])
  GenericMutationResult(
    payload: project_graphql_value(
      payload_source,
      selected_children(field),
      fragments,
    ),
    store: store,
    identity: identity,
    staged_resource_ids: staged_resource_ids,
    top_level_errors: [],
    should_log: should_log,
  )
}

fn location_error_source(
  field: String,
  message: String,
  code: String,
) -> SourceValue {
  src_object([
    #("field", SrcList([SrcString(field)])),
    #("message", SrcString(message)),
    #("code", SrcString(code)),
  ])
}

fn location_bool_field(
  record: StorePropertyRecord,
  key: String,
  default: Bool,
) -> Bool {
  store_property_bool_field(record, key) |> option.unwrap(default)
}

fn location_string_field(
  record: StorePropertyRecord,
  key: String,
) -> Option(String) {
  case dict.get(record.data, key) {
    Ok(StorePropertyString(value)) -> Some(value)
    _ -> None
  }
}

fn has_duplicate_active_location_name(
  store: Store,
  record: StorePropertyRecord,
) -> Bool {
  case location_string_field(record, "name") {
    None -> False
    Some(name) ->
      store.list_effective_store_property_locations(store)
      |> list.any(fn(other) {
        other.id != record.id
        && location_bool_field(other, "isActive", True)
        && location_string_field(other, "name") == Some(name)
      })
  }
}

fn is_only_active_online_fulfilling_location(
  store: Store,
  record: StorePropertyRecord,
) -> Bool {
  case
    location_bool_field(record, "isActive", True)
    && location_bool_field(record, "fulfillsOnlineOrders", True)
  {
    False -> False
    True -> {
      let active_online_count =
        store.list_effective_store_property_locations(store)
        |> list.filter(fn(location) {
          location_bool_field(location, "isActive", True)
          && location_bool_field(location, "fulfillsOnlineOrders", True)
        })
        |> list.length
      active_online_count <= 1
    }
  }
}

fn stage_publishable_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root_name: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let id = read_string(args, "id") |> option.unwrap("")
  let key = root_name <> ":" <> id
  // Pattern 2: generic publishable roots stage locally, but cold
  // LiveHybrid parity hydrates the captured post-publication
  // projection with a read-shaped cassette before projecting the
  // requested payload and downstream read state.
  let payload_record =
    store.get_store_property_mutation_payload(store, key)
    |> option.or(fetch_publishable_payload(upstream, root_name, id))
  let payload_data = case payload_record {
    Some(record) -> record.data
    None ->
      dict.from_list([
        #("publishable", StorePropertyNull),
        #(
          "userErrors",
          StorePropertyList([
            StorePropertyObject(
              dict.from_list([
                #("field", StorePropertyList([StorePropertyString("id")])),
                #("message", StorePropertyString("Publishable not found.")),
              ]),
            ),
          ]),
        ),
      ])
  }
  let next_store = case dict.get(payload_data, "publishable") {
    Ok(StorePropertyObject(data)) ->
      case dict.get(data, "id") {
        Ok(StorePropertyString(publishable_id)) -> {
          let #(_, staged) =
            store.upsert_staged_publishable(
              store,
              StorePropertyRecord(id: publishable_id, cursor: None, data: data),
            )
          staged
        }
        _ -> store
      }
    _ -> store
  }
  GenericMutationResult(
    payload: project_graphql_value(
      store_property_data_to_source(payload_data),
      selected_children(field),
      fragments,
    ),
    store: next_store,
    identity: identity,
    staged_resource_ids: case id {
      "" -> []
      _ -> [id]
    },
    top_level_errors: [],
    should_log: True,
  )
}

fn hydrate_shop_baseline_if_needed(
  store: Store,
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_shop(store) {
    Some(_) -> store
    None ->
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          shop_baseline_hydrate_operation,
          shop_baseline_hydrate_query,
          json.object([]),
        )
      {
        Ok(value) ->
          case
            json_path(value, ["data", "shop"]) |> option.then(shop_from_json)
          {
            Some(shop) -> store.upsert_base_shop(store, shop)
            None -> store
          }
        Error(_) -> store
      }
  }
}

fn hydrate_location_if_missing(
  store: Store,
  upstream: UpstreamContext,
  id: String,
) -> Store {
  case store.get_effective_store_property_location_by_id(store, id) {
    Some(_) -> store
    None ->
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          location_hydrate_operation,
          location_hydrate_query,
          json.object([#("id", json.string(id))]),
        )
      {
        Ok(value) ->
          case
            json_path(value, ["data", "location"])
            |> option.then(fn(raw) {
              store_property_record_from_json(raw, id, "Location")
            })
          {
            Some(location) ->
              store.upsert_base_store_property_location(store, location)
            None -> store
          }
        Error(_) -> store
      }
  }
}

fn fetch_publishable_payload(
  upstream: UpstreamContext,
  root_name: String,
  id: String,
) -> Option(StorePropertyMutationPayloadRecord) {
  case id {
    "" -> None
    _ -> {
      let operation_name = publishable_hydrate_operation(root_name)
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          operation_name,
          publishable_hydrate_query(operation_name),
          json.object([#("id", json.string(id))]),
        )
      {
        Ok(value) ->
          case
            json_path(value, ["data", "publishable"])
            |> option.then(publishable_payload_data_from_json(value))
          {
            Some(data) ->
              Some(StorePropertyMutationPayloadRecord(
                key: root_name <> ":" <> id,
                data: data,
              ))
            None -> None
          }
        Error(_) -> None
      }
    }
  }
}

fn publishable_payload_data_from_json(
  response: commit.JsonValue,
) -> fn(commit.JsonValue) -> Option(Dict(String, StorePropertyValue)) {
  fn(publishable) {
    let base_fields = [
      #("publishable", store_property_value_from_json(publishable)),
      #("userErrors", StorePropertyList([])),
    ]
    let fields = case json_path(response, ["data", "shop"]) {
      Some(shop) -> [
        #("shop", store_property_value_from_json(shop)),
        ..base_fields
      ]
      None -> base_fields
    }
    Some(dict.from_list(fields))
  }
}

fn publishable_hydrate_operation(root_name: String) -> String {
  case root_name {
    "publishablePublish" -> "StorePropertiesPublishablePublishHydrate"
    "publishablePublishToCurrentChannel" ->
      "StorePropertiesPublishablePublishToCurrentChannelHydrate"
    "publishableUnpublish" -> "StorePropertiesPublishableUnpublishHydrate"
    "publishableUnpublishToCurrentChannel" ->
      "StorePropertiesPublishableUnpublishToCurrentChannelHydrate"
    _ -> "StorePropertiesPublishableHydrate"
  }
}

fn publishable_hydrate_query(operation_name: String) -> String {
  "query "
  <> operation_name
  <> "($id: ID!) { "
  <> "publishable: node(id: $id) { "
  <> "... on Product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } "
  <> "... on Collection { id title handle publishedOnCurrentPublication publishedOnPublication(publicationId: \"gid://shopify/Publication/0\") availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } "
  <> "} shop { publicationCount } }"
}

fn json_path(
  value: commit.JsonValue,
  path: List(String),
) -> Option(commit.JsonValue) {
  case path {
    [] -> Some(value)
    [key, ..rest] ->
      case json_get(value, key) {
        Some(child) -> json_path(child, rest)
        None -> None
      }
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      case list.find(fields, fn(pair) { pair.0 == key }) {
        Ok(pair) -> Some(pair.1)
        Error(_) -> None
      }
    _ -> None
  }
}

fn store_property_record_from_json(
  value: commit.JsonValue,
  fallback_id: String,
  fallback_typename: String,
) -> Option(StorePropertyRecord) {
  use data <- option.then(store_property_object_from_json(value))
  let id = store_property_string(data, "id") |> option.unwrap(fallback_id)
  let data = case dict.has_key(data, "__typename") {
    True -> data
    False ->
      dict.insert(
        data,
        "__typename",
        StorePropertyString(infer_typename(id, fallback_typename)),
      )
  }
  Some(StorePropertyRecord(id: id, cursor: None, data: data))
}

fn store_property_object_from_json(
  value: commit.JsonValue,
) -> Option(Dict(String, StorePropertyValue)) {
  case value {
    commit.JsonObject(fields) ->
      Some(dict.from_list(
        fields
        |> list.map(fn(pair) {
          #(pair.0, store_property_value_from_json(pair.1))
        }),
      ))
    _ -> None
  }
}

fn store_property_value_from_json(
  value: commit.JsonValue,
) -> StorePropertyValue {
  case value {
    commit.JsonNull -> StorePropertyNull
    commit.JsonBool(value) -> StorePropertyBool(value)
    commit.JsonInt(value) -> StorePropertyInt(value)
    commit.JsonFloat(value) -> StorePropertyFloat(value)
    commit.JsonString(value) -> StorePropertyString(value)
    commit.JsonArray(values) ->
      StorePropertyList(list.map(values, store_property_value_from_json))
    commit.JsonObject(fields) ->
      StorePropertyObject(dict.from_list(
        fields
        |> list.map(fn(pair) {
          #(pair.0, store_property_value_from_json(pair.1))
        }),
      ))
  }
}

fn store_property_string(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> Option(String) {
  case dict.get(data, key) {
    Ok(StorePropertyString(value)) -> Some(value)
    _ -> None
  }
}

fn infer_typename(id: String, fallback: String) -> String {
  case string.split(id, on: "/") |> list.reverse {
    [tail, resource, ..] ->
      case string.contains(tail, "?") {
        True -> resource
        False -> resource
      }
    _ -> fallback
  }
}

fn shop_from_json(value: commit.JsonValue) -> Option(ShopRecord) {
  case value {
    commit.JsonObject(_) ->
      Some(ShopRecord(
        id: json_string(value, "id", ""),
        name: json_string(value, "name", ""),
        myshopify_domain: json_string(value, "myshopifyDomain", ""),
        url: json_string(value, "url", ""),
        primary_domain: shop_domain_from_json(json_object(
          value,
          "primaryDomain",
        )),
        contact_email: json_string(value, "contactEmail", ""),
        email: json_string(value, "email", ""),
        currency_code: json_string(value, "currencyCode", ""),
        enabled_presentment_currencies: json_string_list(
          value,
          "enabledPresentmentCurrencies",
        ),
        iana_timezone: json_string(value, "ianaTimezone", ""),
        timezone_abbreviation: json_string(value, "timezoneAbbreviation", ""),
        timezone_offset: json_string(value, "timezoneOffset", ""),
        timezone_offset_minutes: json_int(value, "timezoneOffsetMinutes", 0),
        taxes_included: json_bool(value, "taxesIncluded", False),
        tax_shipping: json_bool(value, "taxShipping", False),
        unit_system: json_string(value, "unitSystem", ""),
        weight_unit: json_string(value, "weightUnit", ""),
        shop_address: shop_address_from_json(json_object(value, "shopAddress")),
        plan: shop_plan_from_json(json_object(value, "plan")),
        resource_limits: shop_resource_limits_from_json(json_object(
          value,
          "resourceLimits",
        )),
        features: shop_features_from_json(json_object(value, "features")),
        payment_settings: payment_settings_from_json(json_object(
          value,
          "paymentSettings",
        )),
        shop_policies: json_array(value, "shopPolicies")
          |> list.filter_map(shop_policy_from_json),
      ))
    _ -> None
  }
}

fn shop_domain_from_json(value: commit.JsonValue) -> ShopDomainRecord {
  ShopDomainRecord(
    id: json_string(value, "id", ""),
    host: json_string(value, "host", ""),
    url: json_string(value, "url", ""),
    ssl_enabled: json_bool(value, "sslEnabled", False),
  )
}

fn shop_address_from_json(value: commit.JsonValue) -> ShopAddressRecord {
  ShopAddressRecord(
    id: json_string(value, "id", ""),
    address1: json_string_option(value, "address1"),
    address2: json_string_option(value, "address2"),
    city: json_string_option(value, "city"),
    company: json_string_option(value, "company"),
    coordinates_validated: json_bool(value, "coordinatesValidated", False),
    country: json_string_option(value, "country"),
    country_code_v2: json_string_option(value, "countryCodeV2"),
    formatted: json_string_list(value, "formatted"),
    formatted_area: json_string_option(value, "formattedArea"),
    latitude: json_float_option(value, "latitude"),
    longitude: json_float_option(value, "longitude"),
    phone: json_string_option(value, "phone"),
    province: json_string_option(value, "province"),
    province_code: json_string_option(value, "provinceCode"),
    zip: json_string_option(value, "zip"),
  )
}

fn shop_plan_from_json(value: commit.JsonValue) -> ShopPlanRecord {
  ShopPlanRecord(
    partner_development: json_bool(value, "partnerDevelopment", False),
    public_display_name: json_string(value, "publicDisplayName", ""),
    shopify_plus: json_bool(value, "shopifyPlus", False),
  )
}

fn shop_resource_limits_from_json(
  value: commit.JsonValue,
) -> ShopResourceLimitsRecord {
  ShopResourceLimitsRecord(
    location_limit: json_int(value, "locationLimit", 0),
    max_product_options: json_int(value, "maxProductOptions", 0),
    max_product_variants: json_int(value, "maxProductVariants", 0),
    redirect_limit_reached: json_bool(value, "redirectLimitReached", False),
  )
}

fn shop_features_from_json(value: commit.JsonValue) -> ShopFeaturesRecord {
  ShopFeaturesRecord(
    avalara_avatax: json_bool(value, "avalaraAvatax", False),
    branding: json_string(value, "branding", ""),
    bundles: shop_bundles_feature_from_json(json_object(value, "bundles")),
    captcha: json_bool(value, "captcha", False),
    cart_transform: shop_cart_transform_feature_from_json(json_object(
      value,
      "cartTransform",
    )),
    dynamic_remarketing: json_bool(value, "dynamicRemarketing", False),
    eligible_for_subscription_migration: json_bool(
      value,
      "eligibleForSubscriptionMigration",
      False,
    ),
    eligible_for_subscriptions: json_bool(
      value,
      "eligibleForSubscriptions",
      False,
    ),
    gift_cards: json_bool(value, "giftCards", False),
    harmonized_system_code: json_bool(value, "harmonizedSystemCode", False),
    legacy_subscription_gateway_enabled: json_bool(
      value,
      "legacySubscriptionGatewayEnabled",
      False,
    ),
    live_view: json_bool(value, "liveView", False),
    paypal_express_subscription_gateway_status: json_string(
      value,
      "paypalExpressSubscriptionGatewayStatus",
      "",
    ),
    reports: json_bool(value, "reports", False),
    sells_subscriptions: json_bool(value, "sellsSubscriptions", False),
    show_metrics: json_bool(value, "showMetrics", False),
    storefront: json_bool(value, "storefront", False),
    unified_markets: json_bool(value, "unifiedMarkets", False),
  )
}

fn shop_bundles_feature_from_json(
  value: commit.JsonValue,
) -> ShopBundlesFeatureRecord {
  ShopBundlesFeatureRecord(
    eligible_for_bundles: json_bool(value, "eligibleForBundles", False),
    ineligibility_reason: json_string_option(value, "ineligibilityReason"),
    sells_bundles: json_bool(value, "sellsBundles", False),
  )
}

fn shop_cart_transform_feature_from_json(
  value: commit.JsonValue,
) -> ShopCartTransformFeatureRecord {
  ShopCartTransformFeatureRecord(
    eligible_operations: shop_cart_transform_eligible_operations_from_json(
      json_object(value, "eligibleOperations"),
    ),
  )
}

fn shop_cart_transform_eligible_operations_from_json(
  value: commit.JsonValue,
) -> ShopCartTransformEligibleOperationsRecord {
  ShopCartTransformEligibleOperationsRecord(
    expand_operation: json_bool(value, "expandOperation", False),
    merge_operation: json_bool(value, "mergeOperation", False),
    update_operation: json_bool(value, "updateOperation", False),
  )
}

fn payment_settings_from_json(
  value: commit.JsonValue,
) -> PaymentSettingsRecord {
  PaymentSettingsRecord(supported_digital_wallets: json_string_list(
    value,
    "supportedDigitalWallets",
  ))
}

fn shop_policy_from_json(
  value: commit.JsonValue,
) -> Result(ShopPolicyRecord, Nil) {
  case value {
    commit.JsonObject(_) ->
      Ok(ShopPolicyRecord(
        id: json_string(value, "id", ""),
        title: json_string(value, "title", ""),
        body: json_string(value, "body", ""),
        type_: json_string(value, "type", ""),
        url: json_string(value, "url", ""),
        created_at: json_string(value, "createdAt", ""),
        updated_at: json_string(value, "updatedAt", ""),
      ))
    _ -> Error(Nil)
  }
}

fn json_object(value: commit.JsonValue, key: String) -> commit.JsonValue {
  case json_get(value, key) {
    Some(child) -> child
    None -> commit.JsonObject([])
  }
}

fn json_array(value: commit.JsonValue, key: String) -> List(commit.JsonValue) {
  case json_get(value, key) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_string(
  value: commit.JsonValue,
  key: String,
  default: String,
) -> String {
  json_string_option(value, key) |> option.unwrap(default)
}

fn json_string_option(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

fn json_string_list(value: commit.JsonValue, key: String) -> List(String) {
  json_array(value, key)
  |> list.filter_map(fn(item) {
    case item {
      commit.JsonString(value) -> Ok(value)
      _ -> Error(Nil)
    }
  })
}

fn json_bool(value: commit.JsonValue, key: String, default: Bool) -> Bool {
  case json_get(value, key) {
    Some(commit.JsonBool(value)) -> value
    _ -> default
  }
}

fn json_int(value: commit.JsonValue, key: String, default: Int) -> Int {
  case json_get(value, key) {
    Some(commit.JsonInt(value)) -> value
    _ -> default
  }
}

fn json_float_option(value: commit.JsonValue, key: String) -> Option(Float) {
  case json_get(value, key) {
    Some(commit.JsonFloat(value)) -> Some(value)
    Some(commit.JsonInt(value)) -> Some(int.to_float(value))
    _ -> None
  }
}

fn stage_valid_shop_policy_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  type_: String,
  body: String,
) -> StagePolicyResult {
  case store.get_effective_shop(store) {
    None ->
      StagePolicyResult(
        shop_policy: None,
        user_errors: [
          ShopPolicyUserError(
            field: Some(["shopPolicy"]),
            message: "Shop baseline is required to stage a shop policy update",
            code: None,
          ),
        ],
        store: store,
        identity: identity,
        staged_resource_ids: [],
      )
    Some(shop) -> {
      let existing = find_policy_by_type(shop.shop_policies, type_)
      let #(now, identity_after_time) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let #(id, identity_after_id) = case existing {
        Some(policy) -> #(policy.id, identity_after_time)
        None ->
          synthetic_identity.make_synthetic_gid(
            identity_after_time,
            "ShopPolicy",
          )
      }
      let policy =
        ShopPolicyRecord(
          id: id,
          title: case existing {
            Some(policy) -> policy.title
            None -> shop_policy_title(type_)
          },
          body: body,
          type_: type_,
          url: case existing {
            Some(policy) -> policy.url
            None -> build_shop_policy_url(shop, id, type_)
          },
          created_at: case existing {
            Some(policy) -> policy.created_at
            None -> now
          },
          updated_at: now,
        )
      let other_policies =
        list.filter(shop.shop_policies, fn(candidate) {
          candidate.type_ != policy.type_
        })
      let updated_shop =
        ShopRecord(
          ..shop,
          shop_policies: sort_shop_policies([policy, ..other_policies]),
        )
      let #(_, next_store) = store.stage_shop(store, updated_shop)
      StagePolicyResult(
        shop_policy: Some(policy),
        user_errors: [],
        store: next_store,
        identity: identity_after_id,
        staged_resource_ids: [policy.id],
      )
    }
  }
}

fn validate_shop_policy_input(
  input: Option(Dict(String, root_field.ResolvedValue)),
) -> PolicyValidation {
  let type_ = case input {
    Some(values) -> read_string(values, "type")
    None -> None
  }
  let body = case input {
    Some(values) -> read_string(values, "body")
    None -> None
  }
  let type_errors = case type_ {
    Some(value) ->
      case list.contains(shop_policy_type_order, value) {
        True -> []
        False -> [invalid_type_error()]
      }
    None -> [invalid_type_error()]
  }
  let body_errors = case body {
    Some(value) ->
      case string.byte_size(value) > shop_policy_body_limit_chars {
        True -> [
          ShopPolicyUserError(
            field: Some(["shopPolicy", "body"]),
            message: "Body is too big (maximum is 512 KB)",
            code: Some("TOO_BIG"),
          ),
        ]
        False -> []
      }
    None -> [
      ShopPolicyUserError(
        field: Some(["shopPolicy", "body"]),
        message: "Body is required",
        code: None,
      ),
    ]
  }
  PolicyValidation(
    type_: type_,
    body: body,
    user_errors: list.append(type_errors, body_errors),
  )
}

fn invalid_type_error() -> ShopPolicyUserError {
  ShopPolicyUserError(
    field: Some(["shopPolicy", "type"]),
    message: "Type is invalid",
    code: None,
  )
}

fn read_string(
  values: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(values, key) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_object(
  values: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(values, key) {
    Ok(root_field.ObjectVal(value)) -> Some(value)
    _ -> None
  }
}

fn store_property_bool_field(
  record: StorePropertyRecord,
  key: String,
) -> Option(Bool) {
  case dict.get(record.data, key) {
    Ok(StorePropertyBool(value)) -> Some(value)
    _ -> None
  }
}

fn read_shop_policy_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "shopPolicy") {
    Ok(root_field.ObjectVal(values)) -> Some(values)
    _ -> None
  }
}

fn record_mutation_log(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  root_name: String,
  staged_ids: List(String),
) -> #(Store, SyntheticIdentityRegistry) {
  let #(log_id, identity_after_log_id) =
    synthetic_identity.make_synthetic_gid(identity, "MutationLogEntry")
  let #(received_at, identity_final) =
    synthetic_identity.make_synthetic_timestamp(identity_after_log_id)
  let entry =
    store.MutationLogEntry(
      id: log_id,
      received_at: received_at,
      operation_name: None,
      path: request_path,
      query: document,
      variables: dict.new(),
      staged_resource_ids: staged_ids,
      status: store.Staged,
      interpreted: store.InterpretedMetadata(
        operation_type: store.Mutation,
        operation_name: None,
        root_fields: [root_name],
        primary_root_field: Some(root_name),
        capability: store.Capability(
          operation_name: Some(root_name),
          domain: "store-properties",
          execution: "stage-locally",
        ),
      ),
      notes: Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
    )
  #(store.record_mutation_log_entry(store, entry), identity_final)
}

fn find_policy_by_id(
  policies: List(ShopPolicyRecord),
  id: String,
) -> Option(ShopPolicyRecord) {
  case policies {
    [] -> None
    [policy, ..rest] ->
      case policy.id == id {
        True -> Some(policy)
        False -> find_policy_by_id(rest, id)
      }
  }
}

fn find_policy_by_type(
  policies: List(ShopPolicyRecord),
  type_: String,
) -> Option(ShopPolicyRecord) {
  case policies {
    [] -> None
    [policy, ..rest] ->
      case policy.type_ == type_ {
        True -> Some(policy)
        False -> find_policy_by_type(rest, type_)
      }
  }
}

fn sort_shop_policies(
  policies: List(ShopPolicyRecord),
) -> List(ShopPolicyRecord) {
  list.sort(policies, fn(left, right) {
    let by_index =
      int.compare(policy_type_index(left.type_), policy_type_index(right.type_))
    case by_index {
      order.Eq -> string.compare(left.type_, right.type_)
      _ -> by_index
    }
  })
}

fn policy_type_index(type_: String) -> Int {
  policy_type_index_loop(shop_policy_type_order, type_, 0)
}

fn policy_type_index_loop(
  types: List(String),
  type_: String,
  index: Int,
) -> Int {
  case types {
    [] -> 999
    [first, ..rest] ->
      case first == type_ {
        True -> index
        False -> policy_type_index_loop(rest, type_, index + 1)
      }
  }
}

fn shop_policy_title(type_: String) -> String {
  case type_ {
    "CONTACT_INFORMATION" -> "Contact"
    "LEGAL_NOTICE" -> "Legal notice"
    "PRIVACY_POLICY" -> "Privacy policy"
    "REFUND_POLICY" -> "Refund policy"
    "SHIPPING_POLICY" -> "Shipping policy"
    "SUBSCRIPTION_POLICY" -> "Cancellation policy"
    "TERMS_OF_SALE" -> "Terms of sale"
    "TERMS_OF_SERVICE" -> "Terms of service"
    _ -> type_
  }
}

fn build_shop_policy_url(
  shop: ShopRecord,
  policy_id: String,
  type_: String,
) -> String {
  case read_numeric_gid_tail(shop.id), read_numeric_gid_tail(policy_id) {
    Some(shop_tail), Some(policy_tail) ->
      "https://checkout.shopify.com/"
      <> shop_tail
      <> "/policies/"
      <> policy_tail
      <> ".html?locale=en"
    _, _ ->
      trim_trailing_slash(shop.url)
      <> "/policies/"
      <> string.replace(string.lowercase(type_), "_", "-")
  }
}

fn trim_trailing_slash(value: String) -> String {
  case string.ends_with(value, "/") {
    True -> string.drop_end(value, 1)
    False -> value
  }
}

fn read_numeric_gid_tail(id: String) -> Option(String) {
  case string.split(id, on: "/") |> list.last {
    Ok(tail_with_query) -> {
      let tail = case string.split(tail_with_query, on: "?") {
        [head, ..] -> head
        [] -> tail_with_query
      }
      case int.parse(tail) {
        Ok(_) -> Some(tail)
        Error(_) -> None
      }
    }
    Error(_) -> None
  }
}

fn optional_float_source(value: Option(Float)) -> SourceValue {
  case value {
    Some(f) -> SrcFloat(f)
    None -> SrcNull
  }
}
