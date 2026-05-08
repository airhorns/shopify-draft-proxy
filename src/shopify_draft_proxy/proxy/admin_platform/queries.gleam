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
  InlineFragment, IntValue, Location, Name, NamedType, SelectionSet,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/b2b/serializers as b2b_serializers
import shopify_draft_proxy/proxy/bulk_operations/serializers as bulk_operation_serializers
import shopify_draft_proxy/proxy/customers
import shopify_draft_proxy/proxy/customers/serializers as customer_serializers
import shopify_draft_proxy/proxy/discounts
import shopify_draft_proxy/proxy/functions/serializers as function_serializers
import shopify_draft_proxy/proxy/gift_cards/queries as gift_card_queries
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt, SrcList, SrcNull,
  SrcObject, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, get_selected_child_fields,
  paginate_connection_items, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/marketing/queries as marketing_queries
import shopify_draft_proxy/proxy/markets
import shopify_draft_proxy/proxy/media
import shopify_draft_proxy/proxy/metafield_definitions
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/metaobject_definitions/serializers as metaobject_serializers
import shopify_draft_proxy/proxy/online_store
import shopify_draft_proxy/proxy/orders
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/payments/serializers as payment_serializers
import shopify_draft_proxy/proxy/products
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/saved_searches/queries as saved_search_queries
import shopify_draft_proxy/proxy/segments/serializers as segment_serializers
import shopify_draft_proxy/proxy/shipping_fulfillments
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/webhooks/serializers as webhook_serializers
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type AdminPlatformTaxonomyCategoryRecord, type CapturedJsonValue,
  CapturedArray, CapturedBool, CapturedFloat, CapturedInt, CapturedNull,
  CapturedObject, CapturedString,
}

@internal
pub fn list_supported_admin_platform_node_types() -> List(String) {
  [
    "AbandonedCheckout",
    "Abandonment",
    "App",
    "Article",
    "AppInstallation",
    "AppPurchaseOneTime",
    "AppSubscription",
    "AppUsageRecord",
    "Blog",
    "BulkOperation",
    "CalculatedOrder",
    "CartTransform",
    "Channel",
    "Collection",
    "Comment",
    "Company",
    "CompanyAddress",
    "CompanyContact",
    "CompanyContactRole",
    "CompanyContactRoleAssignment",
    "CompanyLocation",
    "Customer",
    "CustomerAccountNativePage",
    "CustomerPaymentMethod",
    "CustomerSegmentMembersQuery",
    "DeliveryCarrierService",
    "DeliveryCondition",
    "DeliveryCountry",
    "DeliveryLocationGroup",
    "DeliveryMethodDefinition",
    "DeliveryParticipant",
    "DeliveryProfile",
    "DeliveryProvince",
    "DeliveryRateDefinition",
    "DeliveryZone",
    "DiscountAutomaticNode",
    "DiscountCodeNode",
    "DiscountNode",
    "Domain",
    "DraftOrder",
    "ExternalVideo",
    "Fulfillment",
    "FulfillmentOrder",
    "GenericFile",
    "GiftCard",
    "InventoryItem",
    "InventoryLevel",
    "InventoryShipment",
    "InventoryTransfer",
    "Location",
    "Market",
    "MarketCatalog",
    "MarketRegionCountry",
    "MarketWebPresence",
    "MediaImage",
    "Metafield",
    "MetafieldDefinition",
    "Metaobject",
    "MetaobjectDefinition",
    "MarketingActivity",
    "MarketingEvent",
    "Model3d",
    "OnlineStoreTheme",
    "Order",
    "Page",
    "PaymentCustomization",
    "PaymentSchedule",
    "PaymentTerms",
    "PriceList",
    "Product",
    "ProductBundleOperation",
    "ProductDeleteOperation",
    "ProductDuplicateOperation",
    "ProductFeed",
    "ProductOption",
    "ProductOptionValue",
    "ProductSetOperation",
    "ProductVariant",
    "Publication",
    "ReverseDelivery",
    "ReverseFulfillmentOrder",
    "SavedSearch",
    "ScriptTag",
    "Segment",
    "SellingPlan",
    "SellingPlanGroup",
    "ServerPixel",
    "Shop",
    "ShopAddress",
    "ShopPolicy",
    "StorefrontAccessToken",
    "StoreCreditAccount",
    "TaxonomyCategory",
    "UrlRedirect",
    "Validation",
    "Video",
    "WebPixel",
    "WebhookSubscription",
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
  let base = store_in.base_state
  let staged = store_in.staged_state
  option.is_some(base.shop)
  || option.is_some(staged.shop)
  || local_node_state_counts([
    dict.size(base.admin_platform_generic_nodes),
    dict.size(staged.admin_platform_generic_nodes),
    dict.size(base.admin_platform_taxonomy_categories),
    dict.size(staged.admin_platform_taxonomy_categories),
    dict.size(base.abandoned_checkouts),
    dict.size(staged.abandoned_checkouts),
    dict.size(base.abandonments),
    dict.size(staged.abandonments),
    dict.size(base.products),
    dict.size(staged.products),
    dict.size(base.product_variants),
    dict.size(staged.product_variants),
    dict.size(base.product_options),
    dict.size(staged.product_options),
    dict.size(base.product_operations),
    dict.size(staged.product_operations),
    dict.size(base.product_metafields),
    dict.size(staged.product_metafields),
    dict.size(base.product_media),
    dict.size(staged.product_media),
    dict.size(base.files),
    dict.size(staged.files),
    dict.size(base.collections),
    dict.size(staged.collections),
    dict.size(base.product_feeds),
    dict.size(staged.product_feeds),
    dict.size(base.publications),
    dict.size(staged.publications),
    dict.size(base.channels),
    dict.size(base.selling_plan_groups),
    dict.size(staged.selling_plan_groups),
    dict.size(base.customers),
    dict.size(staged.customers),
    dict.size(base.draft_orders),
    dict.size(staged.draft_orders),
    dict.size(base.orders),
    dict.size(staged.orders),
    dict.size(base.inventory_transfers),
    dict.size(staged.inventory_transfers),
    dict.size(base.inventory_shipments),
    dict.size(staged.inventory_shipments),
    dict.size(base.carrier_services),
    dict.size(staged.carrier_services),
    dict.size(base.fulfillments),
    dict.size(staged.fulfillments),
    dict.size(base.fulfillment_orders),
    dict.size(staged.fulfillment_orders),
    dict.size(base.shipping_orders),
    dict.size(staged.shipping_orders),
    dict.size(base.reverse_deliveries),
    dict.size(staged.reverse_deliveries),
    dict.size(base.reverse_fulfillment_orders),
    dict.size(staged.reverse_fulfillment_orders),
    dict.size(base.calculated_orders),
    dict.size(staged.calculated_orders),
    dict.size(base.delivery_profiles),
    dict.size(staged.delivery_profiles),
    dict.size(base.discounts),
    dict.size(staged.discounts),
    dict.size(base.bulk_operations),
    dict.size(staged.bulk_operations),
    dict.size(base.gift_cards),
    dict.size(staged.gift_cards),
    dict.size(base.store_property_locations),
    dict.size(staged.store_property_locations),
    dict.size(base.web_presences),
    dict.size(staged.web_presences),
    dict.size(base.online_store_content),
    dict.size(staged.online_store_content),
    dict.size(base.online_store_integrations),
    dict.size(staged.online_store_integrations),
    dict.size(base.url_redirects),
    dict.size(staged.url_redirects),
    dict.size(base.webhook_subscriptions),
    dict.size(staged.webhook_subscriptions),
    dict.size(base.apps),
    dict.size(staged.apps),
    dict.size(base.app_installations),
    dict.size(staged.app_installations),
    dict.size(base.app_one_time_purchases),
    dict.size(staged.app_one_time_purchases),
    dict.size(base.app_subscriptions),
    dict.size(staged.app_subscriptions),
    dict.size(base.app_usage_records),
    dict.size(staged.app_usage_records),
    dict.size(base.cart_transforms),
    dict.size(staged.cart_transforms),
    dict.size(base.customer_segment_members_queries),
    dict.size(staged.customer_segment_members_queries),
  ])
}

fn local_node_state_counts(counts: List(Int)) -> Bool {
  list.any(counts, fn(count) { count > 0 })
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
          "AbandonedCheckout",
          "Abandonment",
          "CartTransform",
          "Collection",
          "Customer",
          "CustomerSegmentMembersQuery",
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
      let value = case markets.effective_backup_region(store, shop_origin) {
        Some(region) -> markets.backup_region_source(region)
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
    "AbandonedCheckout" ->
      orders.serialize_abandoned_checkout_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "AbandonedCheckout", fragments),
        fragments,
      )
    "Abandonment" ->
      orders.serialize_abandonment_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Abandonment", fragments),
        fragments,
      )
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
    "Order" ->
      orders.serialize_order_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Order", fragments),
        fragments,
        variables,
      )
    "DraftOrder" ->
      orders.serialize_draft_order_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "DraftOrder", fragments),
        fragments,
      )
    "ProductVariant" ->
      products.serialize_product_variant_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "ProductVariant", fragments),
        fragments,
      )
    "InventoryItem" ->
      products.serialize_inventory_item_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "InventoryItem", fragments),
        fragments,
      )
    "InventoryLevel" ->
      products.serialize_inventory_level_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "InventoryLevel", fragments),
        fragments,
      )
    "InventoryTransfer" ->
      products.serialize_inventory_transfer_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "InventoryTransfer", fragments),
        fragments,
      )
    "InventoryShipment" ->
      products.serialize_inventory_shipment_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "InventoryShipment", fragments),
        fragments,
      )
    "ProductFeed" ->
      products.serialize_product_feed_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "ProductFeed", fragments),
        fragments,
      )
    "Publication" ->
      products.serialize_publication_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Publication", fragments),
        fragments,
      )
    "Channel" ->
      products.serialize_channel_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Channel", fragments),
        fragments,
      )
    "SellingPlanGroup" ->
      products.serialize_selling_plan_group_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "SellingPlanGroup", fragments),
        variables,
        fragments,
      )
    "GenericFile" | "MediaImage" | "Video" | "ExternalVideo" | "Model3d" ->
      serialize_media_node_by_id(
        store,
        id,
        gid_resource_type(id),
        selections,
        fragments,
      )
    "BulkOperation" ->
      case store.get_effective_bulk_operation_by_id(store, id) {
        Some(record) ->
          bulk_operation_serializers.project_bulk_operation(
            record,
            synthetic_node_field(
              "BulkOperation",
              admin_node_selected_fields(selections, "BulkOperation", fragments),
            ),
            fragments,
          )
        None -> json.null()
      }
    "GiftCard" ->
      case store.get_effective_gift_card_by_id(store, id) {
        Some(record) ->
          gift_card_queries.project_gift_card(
            record,
            synthetic_node_field(
              "GiftCard",
              admin_node_selected_fields(selections, "GiftCard", fragments),
            ),
            fragments,
            variables,
          )
        None -> json.null()
      }
    "DiscountNode" ->
      discounts.serialize_discount_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "DiscountNode", fragments),
        fragments,
      )
    "DiscountCodeNode" ->
      discounts.serialize_discount_owner_node_by_id(
        store,
        id,
        "code",
        "DiscountCodeNode",
        admin_node_selected_fields(selections, "DiscountCodeNode", fragments),
        fragments,
      )
    "DiscountAutomaticNode" ->
      discounts.serialize_discount_owner_node_by_id(
        store,
        id,
        "automatic",
        "DiscountAutomaticNode",
        admin_node_selected_fields(
          selections,
          "DiscountAutomaticNode",
          fragments,
        ),
        fragments,
      )
    "MetaobjectDefinition" ->
      case store.get_effective_metaobject_definition_by_id(store, id) {
        Some(record) ->
          metaobject_serializers.serialize_definition_selection(
            store,
            record,
            synthetic_node_field(
              "MetaobjectDefinition",
              admin_node_selected_fields(
                selections,
                "MetaobjectDefinition",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "Metaobject" ->
      case store.get_effective_metaobject_by_id(store, id) {
        Some(record) ->
          metaobject_serializers.serialize_metaobject_selection(
            store,
            record,
            synthetic_node_field(
              "Metaobject",
              admin_node_selected_fields(selections, "Metaobject", fragments),
            ),
            fragments,
          )
        None -> json.null()
      }
    "Market" ->
      markets.serialize_market_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Market", fragments),
        fragments,
      )
    "MarketCatalog" ->
      markets.serialize_market_catalog_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "MarketCatalog", fragments),
        fragments,
      )
    "PriceList" ->
      markets.serialize_price_list_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "PriceList", fragments),
        fragments,
      )
    "WebhookSubscription" ->
      case store.get_effective_webhook_subscription_by_id(store, id) {
        Some(record) ->
          webhook_serializers.project_webhook_subscription(
            record,
            synthetic_node_field(
              "WebhookSubscription",
              admin_node_selected_fields(
                selections,
                "WebhookSubscription",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "SavedSearch" ->
      case saved_search_queries.get_effective_saved_search_by_id(store, id) {
        Some(record) ->
          saved_search_queries.project_saved_search(
            record,
            synthetic_node_field(
              "SavedSearch",
              admin_node_selected_fields(selections, "SavedSearch", fragments),
            ),
            fragments,
          )
        None -> json.null()
      }
    "Segment" ->
      case store.get_effective_segment_by_id(store, id) {
        Some(record) ->
          segment_serializers.project_segment(
            record,
            synthetic_node_field(
              "Segment",
              admin_node_selected_fields(selections, "Segment", fragments),
            ),
            fragments,
          )
        None -> json.null()
      }
    "PaymentCustomization" ->
      case store.get_effective_payment_customization_by_id(store, id) {
        Some(record) ->
          payment_serializers.project_payment_customization(
            record,
            synthetic_node_field(
              "PaymentCustomization",
              admin_node_selected_fields(
                selections,
                "PaymentCustomization",
                fragments,
              ),
            ),
            fragments,
            variables,
          )
        None -> json.null()
      }
    "PaymentTerms" ->
      case store.get_effective_payment_terms_by_id(store, id) {
        Some(record) ->
          project_graphql_value(
            payment_serializers.payment_terms_source(record),
            admin_node_selected_fields(selections, "PaymentTerms", fragments),
            fragments,
          )
        None -> json.null()
      }
    "PaymentSchedule" ->
      case store.get_effective_payment_schedule_by_id(store, id) {
        Some(#(_, record)) ->
          project_graphql_value(
            payment_serializers.payment_schedule_source(record),
            admin_node_selected_fields(selections, "PaymentSchedule", fragments),
            fragments,
          )
        None -> json.null()
      }
    "CustomerPaymentMethod" ->
      case store.get_effective_customer_payment_method_by_id(store, id, True) {
        Some(record) ->
          project_graphql_value(
            payment_serializers.payment_method_source(store, record),
            admin_node_selected_fields(
              selections,
              "CustomerPaymentMethod",
              fragments,
            ),
            fragments,
          )
        None -> json.null()
      }
    "StoreCreditAccount" ->
      case store.get_effective_store_credit_account_by_id(store, id) {
        Some(record) ->
          project_graphql_value(
            customer_serializers.store_credit_account_source(store, record),
            admin_node_selected_fields(
              selections,
              "StoreCreditAccount",
              fragments,
            ),
            fragments,
          )
        None -> json.null()
      }
    "CustomerAccountNativePage" ->
      case store.get_effective_customer_account_page_by_id(store, id) {
        Some(record) ->
          customer_serializers.project_account_page(
            record,
            synthetic_node_field(
              "CustomerAccountNativePage",
              admin_node_selected_fields(
                selections,
                "CustomerAccountNativePage",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "Company" ->
      case store.get_effective_b2b_company_by_id(store, id) {
        Some(record) ->
          b2b_serializers.serialize_company(
            store,
            record,
            synthetic_node_field(
              "Company",
              admin_node_selected_fields(selections, "Company", fragments),
            ),
            fragments,
            variables,
          )
        None -> json.null()
      }
    "CompanyContact" ->
      case store.get_effective_b2b_company_contact_by_id(store, id) {
        Some(record) ->
          b2b_serializers.serialize_contact(
            store,
            record,
            synthetic_node_field(
              "CompanyContact",
              admin_node_selected_fields(
                selections,
                "CompanyContact",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "CompanyContactRole" ->
      case store.get_effective_b2b_company_contact_role_by_id(store, id) {
        Some(record) ->
          b2b_serializers.project_source(
            b2b_serializers.role_source(record),
            synthetic_node_field(
              "CompanyContactRole",
              admin_node_selected_fields(
                selections,
                "CompanyContactRole",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "CompanyLocation" ->
      case store.get_effective_b2b_company_location_by_id(store, id) {
        Some(record) ->
          b2b_serializers.serialize_location(
            store,
            record,
            synthetic_node_field(
              "CompanyLocation",
              admin_node_selected_fields(
                selections,
                "CompanyLocation",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "Validation" ->
      case store.get_effective_validation_by_id(store, id) {
        Some(record) ->
          function_serializers.project_validation(
            store,
            record,
            synthetic_node_field(
              "Validation",
              admin_node_selected_fields(selections, "Validation", fragments),
            ),
            fragments,
          )
        None -> json.null()
      }
    "CartTransform" ->
      function_serializers.serialize_cart_transform_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "CartTransform", fragments),
        fragments,
      )
    "CustomerSegmentMembersQuery" ->
      segment_serializers.serialize_customer_segment_members_query_node_by_id(
        store,
        id,
        admin_node_selected_fields(
          selections,
          "CustomerSegmentMembersQuery",
          fragments,
        ),
      )
    "MarketingActivity" ->
      case store.get_effective_marketing_activity_record_by_id(store, id) {
        Some(record) ->
          marketing_queries.project_marketing_record(
            record,
            synthetic_node_field(
              "MarketingActivity",
              admin_node_selected_fields(
                selections,
                "MarketingActivity",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "MarketingEvent" ->
      case store.get_effective_marketing_event_record_by_id(store, id) {
        Some(record) ->
          marketing_queries.project_marketing_record(
            record,
            synthetic_node_field(
              "MarketingEvent",
              admin_node_selected_fields(
                selections,
                "MarketingEvent",
                fragments,
              ),
            ),
            fragments,
          )
        None -> json.null()
      }
    "DeliveryCarrierService" ->
      shipping_fulfillments.serialize_delivery_carrier_service_node_by_id(
        store,
        id,
        admin_node_selected_fields(
          selections,
          "DeliveryCarrierService",
          fragments,
        ),
        fragments,
      )
    "DeliveryProfile" ->
      shipping_fulfillments.serialize_delivery_profile_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "DeliveryProfile", fragments),
        fragments,
      )
    "DeliveryCondition"
    | "DeliveryCountry"
    | "DeliveryLocationGroup"
    | "DeliveryMethodDefinition"
    | "DeliveryParticipant"
    | "DeliveryProvince"
    | "DeliveryRateDefinition"
    | "DeliveryZone" ->
      shipping_fulfillments.serialize_delivery_profile_nested_node_by_id(
        store,
        id,
        gid_resource_type(id),
        admin_node_selected_fields(selections, gid_resource_type(id), fragments),
        fragments,
        fn() { serialize_generic_node_by_id(store, id, selections, fragments) },
      )
    "Fulfillment" ->
      shipping_fulfillments.serialize_fulfillment_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Fulfillment", fragments),
        fragments,
      )
    "FulfillmentOrder" ->
      shipping_fulfillments.serialize_fulfillment_order_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "FulfillmentOrder", fragments),
        fragments,
      )
    "ReverseDelivery" ->
      shipping_fulfillments.serialize_reverse_delivery_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "ReverseDelivery", fragments),
        fragments,
      )
    "ReverseFulfillmentOrder" ->
      shipping_fulfillments.serialize_reverse_fulfillment_order_node_by_id(
        store,
        id,
        admin_node_selected_fields(
          selections,
          "ReverseFulfillmentOrder",
          fragments,
        ),
        fragments,
      )
    "CalculatedOrder" ->
      shipping_fulfillments.serialize_calculated_order_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "CalculatedOrder", fragments),
        fragments,
      )
    "Article" ->
      online_store.serialize_content_node_by_id(
        store,
        id,
        "article",
        "Article",
        admin_node_selected_fields(selections, "Article", fragments),
        fragments,
        variables,
      )
    "Blog" ->
      online_store.serialize_content_node_by_id(
        store,
        id,
        "blog",
        "Blog",
        admin_node_selected_fields(selections, "Blog", fragments),
        fragments,
        variables,
      )
    "Comment" ->
      online_store.serialize_content_node_by_id(
        store,
        id,
        "comment",
        "Comment",
        admin_node_selected_fields(selections, "Comment", fragments),
        fragments,
        variables,
      )
    "Page" ->
      online_store.serialize_content_node_by_id(
        store,
        id,
        "page",
        "Page",
        admin_node_selected_fields(selections, "Page", fragments),
        fragments,
        variables,
      )
    "OnlineStoreTheme" ->
      online_store.serialize_integration_node_by_id(
        store,
        id,
        "theme",
        "OnlineStoreTheme",
        admin_node_selected_fields(selections, "OnlineStoreTheme", fragments),
        fragments,
      )
    "ScriptTag" ->
      online_store.serialize_integration_node_by_id(
        store,
        id,
        "scriptTag",
        "ScriptTag",
        admin_node_selected_fields(selections, "ScriptTag", fragments),
        fragments,
      )
    "WebPixel" ->
      online_store.serialize_integration_node_by_id(
        store,
        id,
        "webPixel",
        "WebPixel",
        admin_node_selected_fields(selections, "WebPixel", fragments),
        fragments,
      )
    "ServerPixel" ->
      online_store.serialize_integration_node_by_id(
        store,
        id,
        "serverPixel",
        "ServerPixel",
        admin_node_selected_fields(selections, "ServerPixel", fragments),
        fragments,
      )
    "StorefrontAccessToken" ->
      online_store.serialize_integration_node_by_id(
        store,
        id,
        "storefrontAccessToken",
        "StorefrontAccessToken",
        admin_node_selected_fields(
          selections,
          "StorefrontAccessToken",
          fragments,
        ),
        fragments,
      )
    "UrlRedirect" ->
      online_store.serialize_url_redirect_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "UrlRedirect", fragments),
        fragments,
      )
    "Job" ->
      case store.get_effective_admin_platform_generic_node_by_id(store, id) {
        Some(record) ->
          case record.typename {
            "Job" ->
              project_graphql_value(
                captured_json_source_with_typename(record.data, "Job"),
                admin_node_selected_fields(selections, "Job", fragments),
                fragments,
              )
            _ -> json.null()
          }
        None ->
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
    "Domain" ->
      store_properties.serialize_domain_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Domain", fragments),
        fragments,
      )
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
      metafields.serialize_metafield_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "Metafield", fragments),
      )
    "MetafieldDefinition" ->
      metafield_definitions.serialize_metafield_definition_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "MetafieldDefinition", fragments),
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
      markets.serialize_market_region_country_node_by_id(
        store,
        shop_origin,
        id,
        admin_node_selected_fields(selections, "MarketRegionCountry", fragments),
        fragments,
      )
    "TaxonomyCategory" ->
      serialize_taxonomy_category_node_by_id(store, id, selections, fragments)
    "MarketWebPresence" ->
      markets.serialize_web_presence_node_by_id(
        store,
        id,
        admin_node_selected_fields(selections, "MarketWebPresence", fragments),
        fragments,
        fn() { serialize_generic_node_by_id(store, id, selections, fragments) },
      )
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

fn serialize_media_node_by_id(
  store: Store,
  id: String,
  typename: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let selected = admin_node_selected_fields(selections, typename, fragments)
  case
    media.serialize_file_node_by_id(store, id, typename, selected, fragments)
  {
    Some(file_json) -> file_json
    None ->
      products.serialize_product_media_node_by_id(
        store,
        id,
        typename,
        selected,
        fragments,
      )
  }
}

fn synthetic_node_field(
  name: String,
  selections: List(Selection),
) -> Selection {
  Field(
    alias: None,
    name: Name(value: name, loc: None),
    arguments: [],
    directives: [],
    selection_set: Some(SelectionSet(selections: selections, loc: None)),
    loc: None,
  )
}

@internal
pub fn serialize_taxonomy_category_node_by_id(
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

@internal
pub fn serialize_generic_node_by_id(
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
