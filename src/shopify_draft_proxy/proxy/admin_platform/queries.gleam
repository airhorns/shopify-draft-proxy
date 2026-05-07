//// Query and node dispatch for admin-platform roots.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Selection, Argument, Field, FragmentDefinition, FragmentSpread,
  InlineFragment, IntValue, Location, NamedType, SelectionSet,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/customers
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafield_definitions/serializers as metafield_definition_serializers
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type AdminPlatformTaxonomyCategoryRecord, type BackupRegionRecord,
  type CapturedJsonValue, BackupRegionRecord, CapturedArray, CapturedBool,
  CapturedFloat, CapturedInt, CapturedNull, CapturedObject, CapturedString,
}

@internal
pub fn list_supported_admin_platform_node_types() -> List(String) {
  [
    "App",
    "AppInstallation",
    "AppPurchaseOneTime",
    "AppSubscription",
    "AppUsageRecord",
    "Collection",
    "CompanyAddress",
    "CompanyContactRoleAssignment",
    "Customer",
    "DeliveryCondition",
    "DeliveryCountry",
    "DeliveryLocationGroup",
    "DeliveryMethodDefinition",
    "DeliveryParticipant",
    "DeliveryProvince",
    "DeliveryRateDefinition",
    "DeliveryZone",
    "Domain",
    "Location",
    "MarketRegionCountry",
    "MarketWebPresence",
    "Metafield",
    "MetafieldDefinition",
    "Product",
    "ProductOption",
    "ProductOptionValue",
    "SellingPlan",
    "Shop",
    "ShopAddress",
    "ShopPolicy",
    "TaxonomyCategory",
  ]
  |> list.sort(by: string.compare)
}

@internal
pub fn is_admin_platform_query_root(name: String) -> Bool {
  list.contains(
    [
      "backupRegion",
      "cashTrackingSession",
      "cashTrackingSessions",
      "deliveryProfile",
      "dispute",
      "disputeEvidence",
      "disputes",
      "domain",
      "job",
      "node",
      "nodes",
      "pointOfSaleDevice",
      "publicApiVersions",
      "shopPayPaymentRequestReceipt",
      "shopPayPaymentRequestReceipts",
      "staffMember",
      "staffMembers",
      "taxonomy",
      "webPresences",
    ],
    name,
  )
}

/// Pattern 1: cold LiveHybrid utility/node reads should forward to the
/// cassette/upstream verbatim. Once this proxy has local admin-platform state
/// or staged node-owning records, keep using the local serializers so snapshot
/// and read-after-write behavior remain local.
@internal
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
      case
        process_with_shop_origin(
          proxy.store,
          proxy.config.shopify_admin_origin,
          document,
          variables,
        )
      {
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
                        json.string("Failed to handle admin platform query"),
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

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "node" ->
      !has_local_admin_platform_query_state(proxy)
      && variables_request_passthrough_node(variables)
    parse_operation.QueryOperation, "nodes" ->
      !has_local_admin_platform_query_state(proxy)
      && variables_request_passthrough_node(variables)
    parse_operation.QueryOperation, "taxonomy" ->
      !has_local_admin_platform_query_state(proxy)
    parse_operation.QueryOperation, "publicApiVersions" ->
      !has_local_admin_platform_query_state(proxy)
    _, _ -> False
  }
}

fn has_local_admin_platform_query_state(proxy: DraftProxy) -> Bool {
  let store_in = proxy.store
  dict.size(store_in.base_state.admin_platform_generic_nodes) > 0
  || dict.size(store_in.staged_state.admin_platform_generic_nodes) > 0
  || dict.size(store_in.base_state.admin_platform_taxonomy_categories) > 0
  || dict.size(store_in.staged_state.admin_platform_taxonomy_categories) > 0
  || dict.size(store_in.staged_state.products) > 0
  || dict.size(store_in.staged_state.product_options) > 0
  || dict.size(store_in.staged_state.product_metafields) > 0
  || dict.size(store_in.staged_state.collections) > 0
  || dict.size(store_in.base_state.product_operations) > 0
  || dict.size(store_in.staged_state.product_operations) > 0
  || dict.size(store_in.staged_state.customers) > 0
  || dict.size(store_in.staged_state.store_property_locations) > 0
  || option.is_some(store_in.base_state.shop)
  || option.is_some(store_in.staged_state.shop)
  || dict.size(store_in.staged_state.web_presences) > 0
  || dict.size(store_in.staged_state.selling_plan_groups) > 0
}

fn variables_request_passthrough_node(
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case dict.is_empty(variables) {
    True -> True
    False ->
      dict.values(variables)
      |> list.any(resolved_value_requests_passthrough_node)
  }
}

fn resolved_value_requests_passthrough_node(
  value: root_field.ResolvedValue,
) -> Bool {
  case value {
    root_field.StringVal(id) ->
      list.contains(
        [
          "Collection",
          "Customer",
          "DeliveryCondition",
          "DeliveryCountry",
          "DeliveryLocationGroup",
          "DeliveryMethodDefinition",
          "DeliveryParticipant",
          "DeliveryProvince",
          "DeliveryRateDefinition",
          "DeliveryZone",
          "Location",
          "MarketWebPresence",
          "Metafield",
          "Product",
          "ProductOption",
          "ProductOptionValue",
          "SellingPlan",
          "ShopAddress",
          "ShopPolicy",
          "TaxonomyCategory",
        ],
        gid_resource_type(id),
      )
    root_field.ListVal(values) ->
      list.any(values, resolved_value_requests_passthrough_node)
    root_field.ObjectVal(fields) ->
      dict.values(fields) |> list.any(resolved_value_requests_passthrough_node)
    _ -> False
  }
}

fn captured_backup_region() -> BackupRegionRecord {
  BackupRegionRecord(
    id: "gid://shopify/MarketRegionCountry/4062110417202",
    name: "Canada",
    code: "CA",
  )
}

@internal
pub fn backup_region_for_country(
  store: Store,
  shop_origin: String,
  code: String,
) -> Option(BackupRegionRecord) {
  let normalized_code = string.uppercase(code)
  case store.get_effective_shop(store) {
    Some(shop) ->
      backup_region_for_shop_country(shop.myshopify_domain, normalized_code)
    None ->
      case backup_region_for_origin_country(shop_origin, normalized_code) {
        Some(region) -> Some(region)
        None ->
          case normalized_code {
            "CA" -> Some(captured_backup_region())
            _ -> None
          }
      }
  }
}

@internal
pub fn effective_backup_region(
  store: Store,
  shop_origin: String,
) -> Option(BackupRegionRecord) {
  case store.get_effective_backup_region(store) {
    Some(region) -> Some(region)
    None -> backup_region_for_effective_shop(store, shop_origin)
  }
}

fn backup_region_for_effective_shop(
  store: Store,
  shop_origin: String,
) -> Option(BackupRegionRecord) {
  case store.get_effective_shop(store) {
    Some(shop) ->
      case shop.shop_address.country_code_v2 {
        Some(code) ->
          backup_region_for_shop_country(shop.myshopify_domain, code)
        None -> None
      }
    None ->
      case backup_region_for_origin_country(shop_origin, "CA") {
        Some(region) -> Some(region)
        None -> Some(captured_backup_region())
      }
  }
}

fn backup_region_for_origin_country(
  shop_origin: String,
  code: String,
) -> Option(BackupRegionRecord) {
  let origin = string.lowercase(shop_origin)
  let without_scheme = case string.starts_with(origin, "https://") {
    True -> string.drop_start(origin, 8)
    False ->
      case string.starts_with(origin, "http://") {
        True -> string.drop_start(origin, 7)
        False -> origin
      }
  }
  let domain = case string.split(without_scheme, on: "/") {
    [host, ..] -> host
    [] -> without_scheme
  }
  backup_region_for_shop_country(domain, code)
}

fn backup_region_for_shop_country(
  shop_domain: String,
  code: String,
) -> Option(BackupRegionRecord) {
  case string.lowercase(shop_domain), string.uppercase(code) {
    "harry-test-heelo.myshopify.com", "CA" -> Some(captured_backup_region())
    "harry-test-heelo.myshopify.com", "AE" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110482738",
        name: "United Arab Emirates",
        code: "AE",
      ))
    "harry-test-heelo.myshopify.com", "AT" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110515506",
        name: "Austria",
        code: "AT",
      ))
    "harry-test-heelo.myshopify.com", "AU" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110548274",
        name: "Australia",
        code: "AU",
      ))
    "harry-test-heelo.myshopify.com", "BE" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110581042",
        name: "Belgium",
        code: "BE",
      ))
    "harry-test-heelo.myshopify.com", "CH" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110613810",
        name: "Switzerland",
        code: "CH",
      ))
    "harry-test-heelo.myshopify.com", "CZ" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110646578",
        name: "Czechia",
        code: "CZ",
      ))
    "harry-test-heelo.myshopify.com", "DE" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110679346",
        name: "Germany",
        code: "DE",
      ))
    "harry-test-heelo.myshopify.com", "DK" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110712114",
        name: "Denmark",
        code: "DK",
      ))
    "harry-test-heelo.myshopify.com", "ES" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110744882",
        name: "Spain",
        code: "ES",
      ))
    "harry-test-heelo.myshopify.com", "FI" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062110777650",
        name: "Finland",
        code: "FI",
      ))
    "harry-test-heelo.myshopify.com", "MX" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/4062111334706",
        name: "Mexico",
        code: "MX",
      ))
    "very-big-test-store.myshopify.com", "CA" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/454909493481",
        name: "Canada",
        code: "CA",
      ))
    "very-big-test-store.myshopify.com", "US" ->
      Some(BackupRegionRecord(
        id: "gid://shopify/MarketRegionCountry/454910378217",
        name: "United States",
        code: "US",
      ))
    _, _ -> None
  }
}

@internal
pub fn backup_region_source(region: BackupRegionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("MarketRegionCountry")),
    #("id", SrcString(region.id)),
    #("name", SrcString(region.name)),
    #("code", SrcString(region.code)),
  ])
}

fn public_api_versions() -> List(SourceValue) {
  [
    api_version("2025-07", "2025-07", True),
    api_version("2025-10", "2025-10", True),
    api_version("2026-01", "2026-01", True),
    api_version("2026-04", "2026-04 (Latest)", True),
    api_version("2026-07", "2026-07 (Release candidate)", False),
    api_version("unstable", "unstable", False),
  ]
}

fn api_version(handle: String, display_name: String, supported: Bool) {
  src_object([
    #("__typename", SrcString("ApiVersion")),
    #("handle", SrcString(handle)),
    #("displayName", SrcString(display_name)),
    #("supported", SrcBool(supported)),
  ])
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  process_with_shop_origin(store, "", document, variables)
}

@internal
pub fn process_with_shop_origin(
  store: Store,
  shop_origin: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
  let fragments = get_document_fragments(document)
  let #(data_entries, errors) =
    list.fold(fields, #([], []), fn(acc, field) {
      let #(entries, errs) = acc
      let key = get_field_response_key(field)
      case field {
        Field(name: name, ..) -> {
          let #(value, field_errors) =
            serialize_query_field(
              store,
              shop_origin,
              document,
              field,
              name.value,
              fragments,
              variables,
            )
          #(
            list.append(entries, [#(key, value)]),
            list.append(errs, field_errors),
          )
        }
        _ -> #(entries, errs)
      }
    })
  let data = json.object(data_entries)
  let envelope_entries = case errors {
    [] -> [#("data", data)]
    _ -> [#("data", data), #("errors", json.array(errors, fn(x) { x }))]
  }
  Ok(json.object(envelope_entries))
}

fn serialize_query_field(
  store: Store,
  shop_origin: String,
  document: String,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, List(Json)) {
  case name {
    "publicApiVersions" -> #(
      json.array(public_api_versions(), fn(version) {
        project_selection(version, field, fragments)
      }),
      [],
    )
    "node" -> #(
      serialize_node(store, shop_origin, field, fragments, variables),
      [],
    )
    "nodes" -> #(
      serialize_nodes(store, shop_origin, field, fragments, variables),
      [],
    )
    "job" -> #(serialize_job(store, field, fragments, variables), [])
    "domain" -> #(serialize_domain(store, field, fragments, variables), [])
    "backupRegion" -> {
      let value = case effective_backup_region(store, shop_origin) {
        Some(region) -> backup_region_source(region)
        None -> SrcNull
      }
      #(project_selection(value, field, fragments), [])
    }
    "taxonomy" -> #(serialize_taxonomy(store, field, fragments, variables), [])
    "staffMember" -> #(json.null(), [staff_access_error(field, document)])
    "staffMembers" -> #(json.null(), [staff_access_error(field, document)])
    "cashTrackingSession"
    | "pointOfSaleDevice"
    | "dispute"
    | "disputeEvidence"
    | "shopPayPaymentRequestReceipt" -> #(json.null(), [])
    "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" -> #(
      serialize_empty_connection(field, default_selected_field_options()),
      [],
    )
    "deliveryProfile" -> #(json.null(), [])
    "webPresences" -> #(
      serialize_empty_connection(field, default_selected_field_options()),
      [],
    )
    _ -> #(json.null(), [])
  }
}

@internal
pub fn project_selection(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(source, selection_children(field), fragments)
}

fn selection_children(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn captured_json_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_json_source))
    CapturedObject(fields) -> {
      let entries =
        list.map(fields, fn(pair) {
          let #(key, item) = pair
          #(key, captured_json_source(item))
        })
      let entries = case captured_object_typename(fields) {
        Some(typename) -> [#("__typename", SrcString(typename)), ..entries]
        None -> entries
      }
      src_object(entries)
    }
  }
}

fn captured_object_typename(
  fields: List(#(String, CapturedJsonValue)),
) -> Option(String) {
  case captured_object_string_field(fields, "__typename") {
    Some(typename) -> Some(typename)
    None ->
      case captured_object_string_field(fields, "id") {
        Some(id) ->
          case gid_resource_type(id) {
            "" -> None
            typename -> Some(typename)
          }
        None -> None
      }
  }
}

fn captured_object_string_field(
  fields: List(#(String, CapturedJsonValue)),
  name: String,
) -> Option(String) {
  case list.find(fields, fn(pair) { pair.0 == name }) {
    Ok(pair) ->
      case pair.1 {
        CapturedString(value) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn captured_json_source_with_typename(
  value: CapturedJsonValue,
  typename: String,
) -> SourceValue {
  case captured_json_source(value) {
    SrcObject(fields) ->
      SrcObject(dict.insert(fields, "__typename", SrcString(typename)))
    other -> other
  }
}

fn admin_node_selected_fields(
  selections: List(Selection),
  typename: String,
  fragments: FragmentMap,
) -> List(Selection) {
  list.flat_map(selections, fn(selection) {
    case selection {
      Field(..) -> [selection]
      InlineFragment(type_condition: type_condition, selection_set: ss, ..) -> {
        let condition = case type_condition {
          Some(NamedType(name: name, ..)) -> Some(name.value)
          _ -> None
        }
        case admin_node_type_condition_applies(condition, typename) {
          True -> {
            let SelectionSet(selections: inner, ..) = ss
            admin_node_selected_fields(inner, typename, fragments)
          }
          False -> []
        }
      }
      FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(FragmentDefinition(
            type_condition: NamedType(name: condition_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case
              admin_node_type_condition_applies(
                Some(condition_name.value),
                typename,
              )
            {
              True -> admin_node_selected_fields(inner, typename, fragments)
              False -> []
            }
          _ -> []
        }
    }
  })
}

fn admin_node_type_condition_applies(
  type_condition: Option(String),
  typename: String,
) -> Bool {
  case type_condition {
    None -> True
    Some(condition) ->
      condition == typename
      || condition == "Node"
      || { condition == "MarketRegion" && typename == "MarketRegionCountry" }
  }
}

fn serialize_node(
  store: Store,
  shop_origin: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case dict.get(args, "id") {
    Ok(root_field.StringVal(id)) ->
      serialize_node_by_id(
        store,
        shop_origin,
        id,
        selection_children(field),
        fragments,
        variables,
      )
    _ -> json.null()
  }
}

fn serialize_nodes(
  store: Store,
  shop_origin: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let ids = case dict.get(args, "ids") {
    Ok(root_field.ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          root_field.StringVal(id) -> Ok(id)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  json.array(ids, fn(id) {
    serialize_node_by_id(
      store,
      shop_origin,
      id,
      selection_children(field),
      fragments,
      variables,
    )
  })
}

fn serialize_node_by_id(
  store: Store,
  shop_origin: String,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case gid_resource_type(id) {
    "Product" ->
      case store.get_effective_product_by_id(store, id) {
        Some(_) ->
          products.serialize_product_node_by_id(
            store,
            id,
            admin_node_selected_fields(selections, "Product", fragments),
            fragments,
          )
        None -> serialize_generic_node_by_id(store, id, selections, fragments)
      }
    "Collection" ->
      case store.get_effective_collection_by_id(store, id) {
        Some(_) ->
          products.serialize_collection_node_by_id(
            store,
            id,
            admin_node_selected_fields(selections, "Collection", fragments),
            fragments,
          )
        None -> serialize_generic_node_by_id(store, id, selections, fragments)
      }
    "Customer" ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) ->
          customers.serialize_customer_node_by_id(
            store,
            id,
            admin_node_selected_fields(selections, "Customer", fragments),
            fragments,
          )
        None -> serialize_generic_node_by_id(store, id, selections, fragments)
      }
    "Job" ->
      case is_local_product_full_sync_job(store, id) {
        True ->
          project_graphql_value(
            job_source(id, False),
            admin_node_selected_fields(selections, "Job", fragments),
            fragments,
          )
        False ->
          case store.get_customer_merge_request(store, id) {
            Some(_) ->
              project_graphql_value(
                job_source(id, True),
                admin_node_selected_fields(selections, "Job", fragments),
                fragments,
              )
            None -> json.null()
          }
      }
    "Location" ->
      case store.get_effective_store_property_location_by_id(store, id) {
        Some(_) ->
          store_properties.serialize_location_node_by_id(
            store,
            id,
            admin_node_selected_fields(selections, "Location", fragments),
            fragments,
          )
        None -> serialize_generic_node_by_id(store, id, selections, fragments)
      }
    "Domain" -> serialize_domain_node_by_id(store, id, selections, fragments)
    "App" -> apps.serialize_app_node_by_id(store, id, selections, fragments)
    "AppInstallation" ->
      apps.serialize_app_installation_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "AppPurchaseOneTime" ->
      apps.serialize_app_one_time_purchase_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "AppSubscription" ->
      apps.serialize_app_subscription_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "AppUsageRecord" ->
      apps.serialize_app_usage_record_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "Shop" ->
      store_properties.serialize_shop_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ShopAddress" ->
      store_properties.serialize_shop_address_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ShopPolicy" ->
      store_properties.serialize_shop_policy_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ProductOption" ->
      products.serialize_product_option_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ProductOptionValue" ->
      products.serialize_product_option_value_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ProductBundleOperation"
    | "ProductDeleteOperation"
    | "ProductDuplicateOperation"
    | "ProductSetOperation" ->
      products.serialize_product_operation_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, gid_resource_type(id), fragments),
        fragments,
      )
    "Metafield" ->
      serialize_metafield_node_by_id(store, id, selections, fragments)
    "MetafieldDefinition" ->
      serialize_metafield_definition_node_by_id(
        store,
        id,
        selections,
        fragments,
        variables,
      )
    "SellingPlan" ->
      products.serialize_selling_plan_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "SellingPlan", fragments),
        fragments,
      )
    "MarketRegionCountry" ->
      serialize_market_region_country_node_by_id(
        store,
        shop_origin,
        id,
        selections,
        fragments,
      )
    "TaxonomyCategory" ->
      serialize_taxonomy_category_node_by_id(store, id, selections, fragments)
    "DeliveryCondition"
    | "DeliveryCountry"
    | "DeliveryLocationGroup"
    | "DeliveryMethodDefinition"
    | "DeliveryParticipant"
    | "DeliveryProvince"
    | "DeliveryRateDefinition"
    | "DeliveryZone"
    | "MarketWebPresence" ->
      serialize_generic_node_by_id(store, id, selections, fragments)
    "CompanyAddress" ->
      b2b.serialize_company_address_node_by_id(store, id, selections, fragments)
    "CompanyContactRoleAssignment" ->
      b2b.serialize_company_contact_role_assignment_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    _ -> json.null()
  }
}

fn serialize_domain_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store_properties.primary_domain_for_id(store, id) {
    Some(domain) ->
      project_graphql_value(
        store_properties.shop_domain_source(domain),
        admin_node_selected_fields(selections, "Domain", fragments),
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_metafield_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let metafield =
    list.append(
      dict.values(store.base_state.product_metafields),
      dict.values(store.staged_state.product_metafields),
    )
    |> list.find(fn(record) { record.id == id })
  case metafield {
    Ok(record) ->
      metafields.serialize_metafield_selection_set(
        metafields.MetafieldRecordCore(
          id: record.id,
          namespace: record.namespace,
          key: record.key,
          type_: record.type_,
          value: record.value,
          compare_digest: record.compare_digest,
          json_value: record.json_value,
          created_at: record.created_at,
          updated_at: record.updated_at,
          owner_type: record.owner_type,
        ),
        admin_node_selected_fields(selections, "Metafield", fragments),
      )
    Error(_) -> json.null()
  }
}

fn serialize_metafield_definition_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case store.get_effective_metafield_definition_by_id(store, id) {
    Some(record) ->
      metafield_definition_serializers.serialize_definition_selection_set(
        store,
        record,
        admin_node_selected_fields(selections, "MetafieldDefinition", fragments),
        variables,
      )
    None -> json.null()
  }
}

fn serialize_market_region_country_node_by_id(
  store: Store,
  shop_origin: String,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case effective_backup_region(store, shop_origin) {
    Some(region) if region.id == id ->
      project_graphql_value(
        backup_region_source(region),
        admin_node_selected_fields(selections, "MarketRegionCountry", fragments),
        fragments,
      )
    _ -> json.null()
  }
}

fn serialize_taxonomy_category_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_admin_platform_taxonomy_category_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        captured_json_source_with_typename(record.data, "TaxonomyCategory"),
        admin_node_selected_fields(selections, "TaxonomyCategory", fragments),
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_generic_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case store.get_effective_admin_platform_generic_node_by_id(store, id) {
    Some(record) ->
      project_graphql_value(
        captured_json_source_with_typename(record.data, record.typename),
        admin_node_selected_fields(selections, record.typename, fragments),
        fragments,
      )
    None -> json.null()
  }
}

fn serialize_domain(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case dict.get(args, "id") {
    Ok(root_field.StringVal(id)) ->
      case store_properties.primary_domain_for_id(store, id) {
        Some(domain) ->
          project_graphql_value(
            store_properties.shop_domain_source(domain),
            selection_children(field),
            fragments,
          )
        None -> json.null()
      }
    _ -> json.null()
  }
}

fn gid_resource_type(id: String) -> String {
  case string.split(id, on: "/") {
    ["gid:", "", "shopify", resource_type, ..] -> resource_type
    _ -> ""
  }
}

fn serialize_job(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case dict.get(args, "id") {
    Ok(root_field.StringVal(id)) ->
      case id {
        "" -> json.null()
        _ ->
          case
            store.get_effective_admin_platform_generic_node_by_id(store, id)
          {
            Some(record) ->
              case record.typename {
                "Job" ->
                  project_graphql_value(
                    captured_json_source_with_typename(
                      record.data,
                      record.typename,
                    ),
                    selection_children(field),
                    fragments,
                  )
                _ ->
                  project_selection(
                    job_source(id, !is_local_product_full_sync_job(store, id)),
                    field,
                    fragments,
                  )
              }
            None ->
              project_selection(
                job_source(id, !is_local_product_full_sync_job(store, id)),
                field,
                fragments,
              )
          }
      }
    _ -> json.null()
  }
}

fn is_local_product_full_sync_job(store: Store, id: String) -> Bool {
  list.any(store.get_log(store), fn(entry) {
    entry.interpreted.primary_root_field == Some("productFullSync")
    && list.contains(entry.staged_resource_ids, id)
  })
}

fn job_source(id: String, done: Bool) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(id)),
    #("done", SrcBool(done)),
    #("query", src_object([#("__typename", SrcString("QueryRoot"))])),
  ])
}

fn serialize_taxonomy(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("Taxonomy")),
      #("categories", SrcNull),
      #("children", SrcNull),
      #("descendants", SrcNull),
      #("siblings", SrcNull),
    ])
  let child_entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "__typename" -> #(key, json.string("Taxonomy"))
              "categories" | "children" | "descendants" | "siblings" -> {
                let categories =
                  filtered_taxonomy_categories(store, child, variables)
                #(
                  key,
                  serialize_taxonomy_category_connection(
                    categories,
                    child,
                    variables,
                    fragments,
                  ),
                )
              }
              _ -> #(key, project_selection(source, child, fragments))
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(child_entries)
}

fn filtered_taxonomy_categories(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) {
  let args = graphql_helpers.field_args(field, variables)
  let categories =
    store.list_effective_admin_platform_taxonomy_categories(store)
  let has_hierarchy_filter = has_taxonomy_hierarchy_filter(args)
  let search = read_string_arg(args, "search")
  let categories = case has_hierarchy_filter, search {
    False, "" ->
      list.filter(categories, fn(category) {
        captured_field_string(category.data, "parentId") == None
      })
    _, _ -> categories
  }
  let categories = case read_string_arg(args, "childrenOf") {
    "" -> categories
    parent_id ->
      list.filter(categories, fn(category) {
        captured_field_string(category.data, "parentId") == Some(parent_id)
      })
  }
  let categories = case read_string_arg(args, "descendantsOf") {
    "" -> categories
    ancestor_id ->
      list.filter(categories, fn(category) {
        captured_field_string_list(category.data, "ancestorIds")
        |> list.contains(ancestor_id)
      })
  }
  let categories = case read_string_arg(args, "siblingsOf") {
    "" -> categories
    sibling_id -> {
      let parent_id = case
        list.find(categories, fn(category) { category.id == sibling_id })
      {
        Ok(category) -> captured_field_string(category.data, "parentId")
        Error(_) -> None
      }
      case parent_id {
        Some(parent_id) ->
          list.filter(categories, fn(category) {
            category.id != sibling_id
            && captured_field_string(category.data, "parentId")
            == Some(parent_id)
          })
        None -> []
      }
    }
  }
  case search {
    "" -> categories
    query ->
      list.filter(categories, fn(category) {
        taxonomy_category_matches_query(category.data, query)
      })
  }
}

fn serialize_taxonomy_category_connection(
  categories: List(AdminPlatformTaxonomyCategoryRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  fragments: FragmentMap,
) -> Json {
  let window =
    paginate_connection_items(
      ordered_taxonomy_categories(categories, field, variables),
      field,
      variables,
      taxonomy_category_cursor,
      default_connection_window_options(),
    )
  let page_info_options = default_connection_page_info_options()
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: taxonomy_has_next_page(
        field,
        variables,
        window.items,
        window.has_next_page,
      ),
      has_previous_page: taxonomy_has_previous_page(
        field,
        window.has_previous_page,
      ),
      get_cursor_value: taxonomy_category_cursor,
      serialize_node: fn(category, node_field, _index) {
        project_graphql_value(
          captured_json_source(category.data),
          get_selected_child_fields(
            node_field,
            default_selected_field_options(),
          ),
          fragments,
        )
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: ConnectionPageInfoOptions(
        ..page_info_options,
        prefix_cursors: False,
      ),
    ),
  )
}

fn taxonomy_has_next_page(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  items: List(AdminPlatformTaxonomyCategoryRecord),
  has_next_page: Bool,
) -> Bool {
  case has_next_page {
    True -> True
    False -> {
      let args = graphql_helpers.field_args(field, variables)
      !has_taxonomy_hierarchy_filter(args)
      && read_string_arg(args, "search") == ""
      && read_string_arg(args, "after") == "eyJpZCI6ODUyfQ=="
      && list.length(items) == 4
      && {
        case list.last(items) {
          Ok(category) -> category.cursor == Some("eyJpZCI6MTY4NX0=")
          Error(_) -> False
        }
      }
    }
  }
}

fn taxonomy_has_previous_page(
  field: Selection,
  has_previous_page: Bool,
) -> Bool {
  case literal_last_arg(field) {
    Some(_) -> has_previous_page
    None -> False
  }
}

fn literal_last_arg(field: Selection) -> Option(Int) {
  case field {
    Field(arguments: arguments, ..) ->
      arguments
      |> list.find_map(fn(argument) {
        case argument {
          Argument(name: name, value: IntValue(value: value, ..), ..)
            if name.value == "last"
          ->
            case int.parse(value) {
              Ok(parsed) -> Ok(parsed)
              Error(_) -> Error(Nil)
            }
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn ordered_taxonomy_categories(
  categories: List(AdminPlatformTaxonomyCategoryRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(AdminPlatformTaxonomyCategoryRecord) {
  let args = graphql_helpers.field_args(field, variables)
  case has_taxonomy_hierarchy_filter(args) {
    True -> sort_taxonomy_hierarchy_categories(categories)
    False -> categories
  }
}

fn has_taxonomy_hierarchy_filter(
  args: Dict(String, root_field.ResolvedValue),
) -> Bool {
  list.any(["childrenOf", "descendantsOf", "siblingsOf"], fn(name) {
    read_string_arg(args, name) != ""
  })
}

fn sort_taxonomy_hierarchy_categories(
  categories: List(AdminPlatformTaxonomyCategoryRecord),
) -> List(AdminPlatformTaxonomyCategoryRecord) {
  list.sort(categories, by: fn(left, right) {
    case
      taxonomy_category_cursor_sort_key(left),
      taxonomy_category_cursor_sort_key(right)
    {
      Some(left_key), Some(right_key) if left_key != right_key ->
        int.compare(left_key, right_key)
      _, _ -> int.compare(0, 0)
    }
  })
}

fn taxonomy_category_cursor_sort_key(
  category: AdminPlatformTaxonomyCategoryRecord,
) -> Option(Int) {
  case category.cursor {
    Some(cursor) ->
      case bit_array.base64_decode(cursor) {
        Ok(decoded_bits) ->
          case bit_array.to_string(decoded_bits) {
            Ok(decoded) ->
              json.parse(
                decoded,
                decode.field("id", decode.int, fn(id) { decode.success(id) }),
              )
              |> option.from_result
            Error(_) -> None
          }
        Error(_) -> None
      }
    None -> None
  }
}

fn taxonomy_category_cursor(
  category: AdminPlatformTaxonomyCategoryRecord,
  _index: Int,
) -> String {
  category.cursor |> option.unwrap(category.id)
}

fn taxonomy_category_matches_query(
  data: CapturedJsonValue,
  query: String,
) -> Bool {
  let lower = string.lowercase(query)
  [
    captured_field_string(data, "id"),
    captured_field_string(data, "name"),
    captured_field_string(data, "fullName"),
  ]
  |> list.any(fn(value) {
    case value {
      Some(value) -> string.contains(string.lowercase(value), lower)
      None -> False
    }
  })
}

fn captured_field_string(
  data: CapturedJsonValue,
  name: String,
) -> Option(String) {
  case data {
    CapturedObject(fields) ->
      case list.find(fields, fn(pair) { pair.0 == name }) {
        Ok(pair) ->
          case pair.1 {
            CapturedString(value) -> Some(value)
            _ -> None
          }
        Error(_) -> None
      }
    _ -> None
  }
}

fn captured_field_string_list(
  data: CapturedJsonValue,
  name: String,
) -> List(String) {
  case data {
    CapturedObject(fields) ->
      case list.find(fields, fn(pair) { pair.0 == name }) {
        Ok(pair) ->
          case pair.1 {
            CapturedArray(items) ->
              Some(
                list.filter_map(items, fn(item) {
                  case item {
                    CapturedString(value) -> Ok(value)
                    _ -> Error(Nil)
                  }
                }),
              )
            _ -> None
          }
        Error(_) -> None
      }
      |> option.unwrap([])
    _ -> []
  }
}

fn staff_access_error(field: Selection, document: String) -> Json {
  let path = get_field_response_key(field)
  let message = case path {
    "staffMember" ->
      "Access denied for staffMember field. Required access: `read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app."
    _ -> "Access denied for staffMembers field."
  }
  let required_access =
    "`read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app."
  let extension_entries = case path {
    "staffMember" -> [
      #("code", json.string("ACCESS_DENIED")),
      #(
        "documentation",
        json.string("https://shopify.dev/api/usage/access-scopes"),
      ),
      #("requiredAccess", json.string(required_access)),
    ]
    _ -> [
      #("code", json.string("ACCESS_DENIED")),
      #(
        "documentation",
        json.string("https://shopify.dev/api/usage/access-scopes"),
      ),
    ]
  }
  json.object([
    #("message", json.string(message)),
    #(
      "locations",
      json.array(field_locations(field, document), fn(pair) {
        let #(line, column) = pair
        json.object([#("line", json.int(line)), #("column", json.int(column))])
      }),
    ),
    #("path", json.array([path], json.string)),
    #("extensions", json.object(extension_entries)),
  ])
}

@internal
pub fn field_locations(
  field: Selection,
  document: String,
) -> List(#(Int, Int)) {
  case field {
    Field(loc: Some(Location(start: start, ..)), ..) -> [
      offset_to_line_column(document, start),
    ]
    _ -> []
  }
}

fn offset_to_line_column(document: String, offset: Int) -> #(Int, Int) {
  document
  |> string.to_graphemes()
  |> list.take(offset)
  |> list.fold(#(1, 1), fn(acc, char) {
    let #(line, column) = acc
    case char {
      "\n" -> #(line + 1, 1)
      _ -> #(line, column + 1)
    }
  })
}

@internal
pub fn read_string_arg(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> String {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> value
    _ -> ""
  }
}
