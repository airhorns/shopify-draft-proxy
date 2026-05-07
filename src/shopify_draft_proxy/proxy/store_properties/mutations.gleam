//// Mutation handling for Store Properties roots.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Directive, type Selection, Argument, Directive, Field,
  StringValue, VariableValue,
}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_api_versions
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcNull, SrcString,
  get_document_fragments, get_field_response_key, project_graphql_field_value,
  project_graphql_value, read_arg_bool, read_arg_object, read_arg_string,
  read_arg_string_list, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/store_properties/serializers.{
  json_path, product_metafield_to_core, selected_children,
  serialize_location_record, shop_from_json, shop_policy_update_payload_source,
  store_property_data_to_source, store_property_record_from_json,
  store_property_value_from_json,
}
import shopify_draft_proxy/proxy/store_properties/types as store_properties_types
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type InventoryTransferLocationSnapshotRecord,
  type ProductMetafieldRecord, type ShopPolicyRecord, type ShopRecord,
  type StorePropertyMutationPayloadRecord, type StorePropertyRecord,
  type StorePropertyValue, CapturedArray, CapturedObject, CapturedString,
  ProductMetafieldRecord, ShopPolicyRecord, ShopRecord, StorePropertyBool,
  StorePropertyInt, StorePropertyList, StorePropertyMutationPayloadRecord,
  StorePropertyNull, StorePropertyObject, StorePropertyRecord,
  StorePropertyString,
}

const shop_policy_body_limit_chars = 524_287

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

const iso_3166_alpha2_country_codes = [
  "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AS", "AT", "AU",
  "AW", "AX", "AZ", "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL",
  "BM", "BN", "BO", "BQ", "BR", "BS", "BT", "BV", "BW", "BY", "BZ", "CA", "CC",
  "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN", "CO", "CR", "CU", "CV",
  "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE", "EG",
  "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "GA", "GB", "GD",
  "GE", "GF", "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT",
  "GU", "GW", "GY", "HK", "HM", "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM",
  "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM", "JO", "JP", "KE", "KG", "KH",
  "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK",
  "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH",
  "MK", "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW",
  "MX", "MY", "MZ", "NA", "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP", "NR",
  "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM", "PN", "PR",
  "PS", "PT", "PW", "PY", "QA", "RE", "RO", "RS", "RU", "RW", "SA", "SB", "SC",
  "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS",
  "ST", "SV", "SX", "SY", "SZ", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL",
  "TM", "TN", "TO", "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY",
  "UZ", "VA", "VC", "VE", "VG", "VI", "VN", "VU", "WF", "WS", "YE", "YT", "ZA",
  "ZM", "ZW",
]

const shop_baseline_hydrate_operation: String = "StorePropertiesShopBaselineHydrate"

const shop_baseline_hydrate_query: String = "query StorePropertiesShopBaselineHydrate { shop { id name myshopifyDomain url primaryDomain { id host url sslEnabled } contactEmail email currencyCode enabledPresentmentCurrencies ianaTimezone timezoneAbbreviation timezoneOffset timezoneOffsetMinutes taxesIncluded taxShipping unitSystem weightUnit shopAddress { id address1 address2 city company coordinatesValidated country countryCodeV2 formatted formattedArea latitude longitude phone province provinceCode zip } plan { partnerDevelopment publicDisplayName shopifyPlus } resourceLimits { locationLimit maxProductOptions maxProductVariants redirectLimitReached } features { avalaraAvatax branding bundles { eligibleForBundles ineligibilityReason sellsBundles } captcha cartTransform { eligibleOperations { expandOperation mergeOperation updateOperation } } dynamicRemarketing eligibleForSubscriptionMigration eligibleForSubscriptions giftCards harmonizedSystemCode legacySubscriptionGatewayEnabled liveView paypalExpressSubscriptionGatewayStatus reports sellsSubscriptions showMetrics storefront unifiedMarkets } paymentSettings { supportedDigitalWallets } shopPolicies { id title body type url createdAt updatedAt } } }"

const location_hydrate_operation: String = "StorePropertiesLocationHydrate"

const location_hydrate_query: String = "query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: \"custom\", key: \"hours\") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: [\"available\", \"committed\", \"on_hand\"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }"

@internal
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

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
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
                  let #(logged_store, logged_identity) = case
                    result.user_errors
                  {
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
                  let #(logged_store, logged_identity) = case
                    result.should_log
                  {
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
      MutationOutcome(
        data: envelope,
        store: final_store,
        identity: final_identity,
        staged_resource_ids: staged_ids,
        log_drafts: [],
      )
    }
  }
}

fn stage_shop_policy_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  _fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> store_properties_types.StagePolicyResult {
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
      store_properties_types.StagePolicyResult(
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
) -> store_properties_types.GenericMutationResult {
  case root_name {
    "locationAdd" ->
      stage_location_add(store, identity, field, fragments, variables)
    "locationEdit" ->
      stage_location_edit(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      )
    "locationActivate" | "locationDeactivate" ->
      case
        admin_api_versions.at_least(request_path, "2026-04")
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
      store_properties_types.GenericMutationResult(
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
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let store = case read_arg_string(args, "locationId") {
    Some(id) -> hydrate_location_if_missing(store, upstream, id)
    None -> store
  }
  let store = case read_arg_string(args, "destinationLocationId") {
    Some(id) -> hydrate_location_if_missing(store, upstream, id)
    None -> store
  }
  case root_name {
    "locationActivate" ->
      stage_location_activate(store, identity, field, fragments, variables)
    "locationDeactivate" ->
      stage_location_deactivate(store, identity, field, fragments, variables)
    _ ->
      store_properties_types.GenericMutationResult(
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
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let input = read_arg_object(args, "input")
  let name =
    input |> option.then(fn(values) { read_arg_string(values, "name") })
  let address =
    input |> option.then(fn(values) { read_arg_object(values, "address") })
  case name, input, address {
    Some(value), Some(input_values), Some(address_values) ->
      case
        string.trim(value),
        validate_location_add_country_code(address_values)
      {
        "", _ ->
          location_add_blank_name_result(store, identity, field, fragments)
        _, Error(_) ->
          location_add_invalid_country_code_result(
            store,
            identity,
            field,
            fragments,
          )
        _, Ok(_) -> {
          let #(id, next_identity) =
            synthetic_identity.make_synthetic_gid(identity, "Location")
          let base_fields = [
            #("__typename", StorePropertyString("Location")),
            #("id", StorePropertyString(id)),
            #("name", StorePropertyString(value)),
            #("isActive", StorePropertyBool(True)),
            #("activatable", StorePropertyBool(False)),
            #("deactivatable", StorePropertyBool(True)),
            #("deletable", StorePropertyBool(False)),
            #(
              "fulfillsOnlineOrders",
              StorePropertyBool(
                read_arg_bool(input_values, "fulfillsOnlineOrders")
                |> option.unwrap(True),
              ),
            ),
            #(
              "address",
              StorePropertyObject(location_add_address_data(address_values)),
            ),
          ]
          let fields = case location_add_capabilities(input_values) {
            Some(capabilities) ->
              list.append(base_fields, [#("capabilities", capabilities)])
            None -> base_fields
          }
          let record =
            StorePropertyRecord(
              id: id,
              cursor: None,
              data: dict.from_list(fields),
            )
          let #(_, next_store) =
            store.upsert_staged_store_property_location(store, record)
          let payload_source =
            src_object([
              #("location", store_property_data_to_source(record.data)),
              #("userErrors", SrcList([])),
            ])
          store_properties_types.GenericMutationResult(
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
    _, _, _ -> {
      location_add_blank_name_result(store, identity, field, fragments)
    }
  }
}

fn location_add_blank_name_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> store_properties_types.GenericMutationResult {
  let payload_source =
    src_object([
      #("location", SrcNull),
      #(
        "userErrors",
        SrcList([
          src_object([
            #("field", SrcList([SrcString("input"), SrcString("name")])),
            #("message", SrcString("Add a location name")),
            #("code", SrcString("BLANK")),
          ]),
        ]),
      ),
    ])
  store_properties_types.GenericMutationResult(
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

fn location_add_invalid_country_code_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> store_properties_types.GenericMutationResult {
  let payload_source =
    src_object([
      #("location", SrcNull),
      #(
        "userErrors",
        SrcList([
          src_object([
            #(
              "field",
              SrcList([
                SrcString("input"),
                SrcString("address"),
                SrcString("countryCode"),
              ]),
            ),
            #("message", SrcString("Country code is invalid")),
            #("code", SrcString("INVALID")),
          ]),
        ]),
      ),
    ])
  store_properties_types.GenericMutationResult(
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

fn location_add_address_data(
  address: Dict(String, root_field.ResolvedValue),
) -> Dict(String, StorePropertyValue) {
  [
    "address1",
    "address2",
    "city",
    "country",
    "countryCode",
    "province",
    "provinceCode",
    "zip",
    "phone",
  ]
  |> list.filter_map(fn(field_name) {
    case read_arg_string(address, field_name) {
      Some(value) ->
        Ok(#(
          field_name,
          StorePropertyString(case field_name {
            "countryCode" -> string.uppercase(value)
            _ -> value
          }),
        ))
      None -> Error(Nil)
    }
  })
  |> dict.from_list
}

fn validate_location_add_country_code(
  address: Dict(String, root_field.ResolvedValue),
) -> Result(String, Nil) {
  case read_arg_string(address, "countryCode") {
    Some(raw) -> {
      let code = string.uppercase(string.trim(raw))
      case list.contains(iso_3166_alpha2_country_codes, code) {
        True -> Ok(code)
        False -> Error(Nil)
      }
    }
    None -> Error(Nil)
  }
}

fn location_add_capabilities(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(StorePropertyValue) {
  let add = read_arg_string_list(input, "capabilitiesToAdd")
  let remove = read_arg_string_list(input, "capabilitiesToRemove")
  case add, remove {
    None, None -> None
    _, _ -> {
      let additions = add |> option.unwrap([])
      let removals = remove |> option.unwrap([])
      Some(StorePropertyList(
        additions
        |> list.filter(fn(capability) { !list.contains(removals, capability) })
        |> list.map(StorePropertyString),
      ))
    }
  }
}

fn stage_location_edit(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let id = read_arg_string(args, "id")
  case id {
    Some(location_id) -> {
      // Pattern 2: edit needs the prior Location row so partial address/name
      // inputs merge locally without forwarding the supported mutation.
      let store = hydrate_location_if_missing(store, upstream, location_id)
      case
        store.get_effective_store_property_location_by_id(store, location_id)
      {
        Some(record) -> {
          let input = read_arg_object(args, "input")
          let input_values = option.unwrap(input, dict.new())
          let errors = validate_location_edit_input(store, record, input_values)
          case errors {
            [_, ..] ->
              location_edit_result(
                store,
                identity,
                field,
                fragments,
                variables,
                None,
                errors,
                [],
                False,
              )
            [] -> {
              let #(next_data, changed) =
                apply_location_edit_data(record.data, input_values)
              let changed =
                changed
                || !list.is_empty(read_object_list(input_values, "metafields"))
              let #(next_data, next_identity) = case changed {
                True -> {
                  let #(now, identity_after_timestamp) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  #(
                    dict.insert(
                      next_data,
                      "updatedAt",
                      StorePropertyString(now),
                    ),
                    identity_after_timestamp,
                  )
                }
                False -> #(next_data, identity)
              }
              let next_record = StorePropertyRecord(..record, data: next_data)
              let #(_, location_store) =
                store.upsert_staged_store_property_location(store, next_record)
              let #(metafield_store, final_identity, metafield_ids) =
                apply_location_edit_metafields(
                  location_store,
                  next_identity,
                  location_id,
                  input_values,
                )
              location_edit_result(
                metafield_store,
                final_identity,
                field,
                fragments,
                variables,
                Some(next_record),
                [],
                [location_id, ..metafield_ids],
                True,
              )
            }
          }
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
            Some("NOT_FOUND"),
          )
      }
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
        Some("NOT_FOUND"),
      )
  }
}

fn location_edit_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  location: Option(StorePropertyRecord),
  errors: List(store_properties_types.LocationEditUserError),
  staged_resource_ids: List(String),
  should_log: Bool,
) -> store_properties_types.GenericMutationResult {
  let payload =
    json.object(
      list.map(selected_children(field), fn(selection) {
        let key = get_field_response_key(selection)
        case selection {
          Field(name: name, ..) ->
            case name.value {
              "location" -> #(key, case location {
                Some(record) ->
                  serialize_location_record(
                    store,
                    record,
                    selection,
                    fragments,
                    variables,
                  )
                None -> json.null()
              })
              "userErrors" -> #(
                key,
                project_graphql_field_value(
                  src_object([
                    #(
                      "userErrors",
                      SrcList(list.map(errors, location_edit_error_source)),
                    ),
                  ]),
                  selection,
                  fragments,
                ),
              )
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      }),
    )
  store_properties_types.GenericMutationResult(
    payload: payload,
    store: store,
    identity: identity,
    staged_resource_ids: staged_resource_ids,
    top_level_errors: [],
    should_log: should_log,
  )
}

fn location_edit_error_source(
  error: store_properties_types.LocationEditUserError,
) -> SourceValue {
  src_object([
    #("field", SrcList(list.map(error.field, SrcString))),
    #("message", SrcString(error.message)),
    #("code", graphql_helpers.option_string_source(error.code)),
  ])
}

fn validate_location_edit_input(
  store: Store,
  record: StorePropertyRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(store_properties_types.LocationEditUserError) {
  []
  |> list.append(validate_location_edit_name(input))
  |> list.append(validate_location_edit_address(record, input))
  |> list.append(validate_location_edit_fulfills_online_orders(
    store,
    record,
    input,
  ))
  |> list.append(validate_location_edit_metafields(store, record, input))
}

fn validate_location_edit_name(
  input: Dict(String, root_field.ResolvedValue),
) -> List(store_properties_types.LocationEditUserError) {
  case read_arg_string(input, "name") {
    Some(name) ->
      case string.trim(name) {
        "" -> [
          store_properties_types.LocationEditUserError(
            field: ["input", "name"],
            message: "Add a location name",
            code: Some("BLANK"),
          ),
        ]
        _ -> []
      }
    None -> []
  }
}

fn validate_location_edit_address(
  record: StorePropertyRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(store_properties_types.LocationEditUserError) {
  let address = merged_location_address(record, input)
  case address {
    None -> []
    Some(fields) ->
      [
        required_location_address_error(fields, "countryCode", "Add a country"),
        required_location_address_error(
          fields,
          "address1",
          "Add a street for this address",
        ),
        required_location_address_error(fields, "city", "Add a city"),
        required_location_address_error(
          fields,
          "zip",
          "Add a postal / ZIP code",
        ),
      ]
      |> list.filter_map(fn(error) {
        case error {
          Some(value) -> Ok(value)
          None -> Error(Nil)
        }
      })
  }
}

fn required_location_address_error(
  fields: Dict(String, StorePropertyValue),
  key: String,
  message: String,
) -> Option(store_properties_types.LocationEditUserError) {
  case dict.get(fields, key) {
    Ok(StorePropertyString(_)) -> None
    _ ->
      Some(store_properties_types.LocationEditUserError(
        field: ["input", "address", key],
        message: message,
        code: Some("BLANK"),
      ))
  }
}

fn validate_location_edit_fulfills_online_orders(
  store: Store,
  record: StorePropertyRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(store_properties_types.LocationEditUserError) {
  case read_arg_bool(input, "fulfillsOnlineOrders") {
    Some(False) ->
      case location_bool_field(record, "fulfillsOnlineOrders", True) {
        False -> []
        True ->
          case location_bool_field(record, "isFulfillmentService", False) {
            True -> [
              store_properties_types.LocationEditUserError(
                field: ["input", "fulfillsOnlineOrders"],
                message: "Cannot modify the online order fulfillment preference for fulfillment service locations.",
                code: Some(
                  "CANNOT_MODIFY_ONLINE_ORDER_FULFILLMENT_FOR_FS_LOCATION",
                ),
              ),
            ]
            False ->
              case location_bool_field(record, "hasUnfulfilledOrders", False) {
                True -> [
                  store_properties_types.LocationEditUserError(
                    field: ["input", "fulfillsOnlineOrders"],
                    message: "Cannot disable online order fulfillment while the location has pending fulfillment orders.",
                    code: Some("CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT"),
                  ),
                ]
                False ->
                  case
                    is_only_active_online_fulfilling_location(store, record)
                    || location_bound_to_delivery_profile(store, record.id)
                  {
                    True -> [
                      store_properties_types.LocationEditUserError(
                        field: ["input"],
                        message: "Online order fulfillment could not be disabled for this location as it is the only location that fulfills online orders.",
                        code: Some("CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT"),
                      ),
                    ]
                    False -> []
                  }
              }
          }
      }
    _ -> []
  }
}

fn validate_location_edit_metafields(
  store: Store,
  record: StorePropertyRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(store_properties_types.LocationEditUserError) {
  let existing_count =
    store.get_effective_metafields_by_owner_id(store, record.id)
    |> list.length
  read_object_list(input, "metafields")
  |> list.index_map(fn(metafield_input, index) {
    validate_location_edit_metafield(metafield_input, existing_count + index)
  })
  |> list.flatten
}

fn validate_location_edit_metafield(
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(store_properties_types.LocationEditUserError) {
  []
  |> list.append(validate_location_edit_metafield_required(
    input,
    index,
    "key",
    "Key is required.",
  ))
  |> list.append(validate_location_edit_metafield_required(
    input,
    index,
    "value",
    "Value is required.",
  ))
  |> list.append(validate_location_edit_metafield_type(input, index))
}

fn validate_location_edit_metafield_required(
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
  key: String,
  message: String,
) -> List(store_properties_types.LocationEditUserError) {
  case read_arg_string(input, key) {
    Some(value) ->
      case string.trim(value) {
        "" -> [blank_location_metafield_error(index, key, message)]
        _ -> []
      }
    None -> [blank_location_metafield_error(index, key, message)]
  }
}

fn blank_location_metafield_error(
  index: Int,
  key: String,
  message: String,
) -> store_properties_types.LocationEditUserError {
  store_properties_types.LocationEditUserError(
    field: ["input", "metafields", int.to_string(index), key],
    message: message,
    code: Some("BLANK"),
  )
}

fn validate_location_edit_metafield_type(
  input: Dict(String, root_field.ResolvedValue),
  index: Int,
) -> List(store_properties_types.LocationEditUserError) {
  case read_arg_string(input, "type") {
    Some(type_) ->
      case list.contains(metafields.valid_type_names(), type_) {
        True -> []
        False -> [
          store_properties_types.LocationEditUserError(
            field: ["input", "metafields", int.to_string(index), "type"],
            message: "Type must be one of the following: "
              <> metafields.valid_type_names_message()
              <> ".",
            code: Some("INVALID_TYPE"),
          ),
        ]
      }
    None -> [
      store_properties_types.LocationEditUserError(
        field: ["input", "metafields", int.to_string(index), "type"],
        message: "Type can't be blank",
        code: Some("BLANK"),
      ),
    ]
  }
}

fn apply_location_edit_data(
  data: Dict(String, StorePropertyValue),
  input: Dict(String, root_field.ResolvedValue),
) -> #(Dict(String, StorePropertyValue), Bool) {
  let #(data, changed) = case read_arg_string(input, "name") {
    Some(name) -> #(dict.insert(data, "name", StorePropertyString(name)), True)
    None -> #(data, False)
  }
  let #(data, changed) = case read_arg_bool(input, "fulfillsOnlineOrders") {
    Some(value) -> #(
      dict.insert(data, "fulfillsOnlineOrders", StorePropertyBool(value)),
      True,
    )
    None -> #(data, changed)
  }
  let #(data, changed) = case merged_location_address_from_data(data, input) {
    Some(address) -> #(
      dict.insert(data, "address", StorePropertyObject(address)),
      True,
    )
    None -> #(data, changed)
  }
  #(data, changed)
}

fn apply_location_edit_metafields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  owner_id: String,
  input: Dict(String, root_field.ResolvedValue),
) -> #(Store, SyntheticIdentityRegistry, List(String)) {
  let inputs = read_object_list(input, "metafields")
  case inputs {
    [] -> #(store, identity, [])
    _ -> {
      let existing = store.get_effective_metafields_by_owner_id(store, owner_id)
      let #(metafields_for_owner, changed, next_identity) =
        upsert_location_metafields(owner_id, inputs, existing, identity)
      let next_store =
        store.replace_staged_metafields_for_owner(
          store,
          owner_id,
          metafields_for_owner,
        )
      #(next_store, next_identity, list.map(changed, fn(record) { record.id }))
    }
  }
}

fn upsert_location_metafields(
  owner_id: String,
  inputs: List(Dict(String, root_field.ResolvedValue)),
  existing: List(ProductMetafieldRecord),
  identity: SyntheticIdentityRegistry,
) -> #(
  List(ProductMetafieldRecord),
  List(ProductMetafieldRecord),
  SyntheticIdentityRegistry,
) {
  list.fold(inputs, #(existing, [], identity), fn(acc, input) {
    let #(current, changed, current_identity) = acc
    let namespace =
      read_arg_string(input, "namespace") |> option.unwrap("custom")
    let key = read_arg_string(input, "key") |> option.unwrap("")
    let type_ = read_arg_string(input, "type")
    let raw_value = read_arg_string(input, "value")
    let found =
      current
      |> list.find(fn(metafield) {
        metafield.namespace == namespace && metafield.key == key
      })
      |> option.from_result
    let type_ =
      type_
      |> option.or(case found {
        Some(record) -> record.type_
        None -> None
      })
    let value = metafields.normalize_metafield_value(type_, raw_value)
    let #(id, identity_after_id) = case found {
      Some(record) -> #(record.id, current_identity)
      None ->
        synthetic_identity.make_synthetic_gid(current_identity, "Metafield")
    }
    let #(created_at, identity_after_created) = case found {
      Some(record) -> #(
        option.unwrap(record.created_at, "2024-01-01T00:00:00Z"),
        identity_after_id,
      )
      None -> synthetic_identity.make_synthetic_timestamp(identity_after_id)
    }
    let #(updated_at, identity_after_updated) = case found {
      Some(record) ->
        case value == record.value && type_ == record.type_ {
          True -> #(
            option.unwrap(record.updated_at, created_at),
            identity_after_created,
          )
          False ->
            synthetic_identity.make_synthetic_timestamp(identity_after_created)
        }
      None -> #(created_at, identity_after_created)
    }
    let core =
      ProductMetafieldRecord(
        id: id,
        owner_id: owner_id,
        namespace: namespace,
        key: key,
        type_: type_,
        value: value,
        compare_digest: None,
        json_value: metafields.parse_metafield_json_value(type_, value),
        created_at: Some(created_at),
        updated_at: Some(updated_at),
        owner_type: Some("LOCATION"),
        market_localizable_content: option.map(found, fn(record) {
          record.market_localizable_content
        })
          |> option.unwrap([]),
      )
    let record = case found {
      Some(existing_record)
        if value == existing_record.value && type_ == existing_record.type_
      ->
        ProductMetafieldRecord(
          ..core,
          compare_digest: existing_record.compare_digest,
        )
      _ ->
        ProductMetafieldRecord(
          ..core,
          compare_digest: Some(
            metafields.make_metafield_compare_digest(product_metafield_to_core(
              core,
            )),
          ),
        )
    }
    let next = replace_metafield_by_identity(current, namespace, key, record)
    #(next, list.append(changed, [record]), identity_after_updated)
  })
}

fn replace_metafield_by_identity(
  records: List(ProductMetafieldRecord),
  namespace: String,
  key: String,
  record: ProductMetafieldRecord,
) -> List(ProductMetafieldRecord) {
  let without =
    list.filter(records, fn(candidate) {
      !{ candidate.namespace == namespace && candidate.key == key }
    })
  list.append(without, [record])
}

fn stage_location_activate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  case read_arg_string(args, "locationId") {
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
                  case location_activation_limit_reached(record) {
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
                            "Your shop has reached its location limit.",
                            "LOCATION_LIMIT",
                          ),
                        ],
                        [],
                        False,
                      )
                    False ->
                      case location_has_incomplete_mass_relocation(record) {
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
                                "Location has an ongoing relocation.",
                                "HAS_ONGOING_RELOCATION",
                              ),
                            ],
                            [],
                            False,
                          )
                        False ->
                          case
                            has_duplicate_active_location_name(store, record)
                          {
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
                                synthetic_identity.make_synthetic_timestamp(
                                  identity,
                                )
                              let next_record =
                                StorePropertyRecord(
                                  ..record,
                                  data: record.data
                                    |> dict.insert(
                                      "isActive",
                                      StorePropertyBool(True),
                                    )
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
                                      StorePropertyNull,
                                    )
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
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let destination_location_id = read_arg_string(args, "destinationLocationId")
  case read_arg_string(args, "locationId") {
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
              case
                location_deactivation_guard_errors(
                  store,
                  record,
                  location_id,
                  destination_location_id,
                )
              {
                [] -> {
                  let #(now, next_identity) =
                    synthetic_identity.make_synthetic_timestamp(identity)
                  let next_record =
                    StorePropertyRecord(
                      ..record,
                      data: record.data
                        |> dict.insert("isActive", StorePropertyBool(False))
                        |> dict.insert("activatable", StorePropertyBool(True))
                        |> dict.insert("deactivatable", StorePropertyBool(True))
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
                        |> dict.insert("updatedAt", StorePropertyString(now)),
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
                errors ->
                  location_lifecycle_result(
                    store,
                    identity,
                    field,
                    fragments,
                    "locationDeactivateUserErrors",
                    Some(record),
                    errors,
                    [],
                    False,
                  )
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

fn location_deactivation_guard_errors(
  store: Store,
  record: StorePropertyRecord,
  location_id: String,
  destination_location_id: Option(String),
) -> List(SourceValue) {
  let common_errors =
    []
    |> append_location_deactivation_error(
      is_only_active_online_fulfilling_location(store, record),
      "locationId",
      "At least one location must fulfill online orders.",
      "CANNOT_DISABLE_ONLINE_ORDER_FULFILLMENT",
    )
    |> append_location_deactivation_error(
      location_is_permanently_blocked_from_deactivation(record),
      "locationId",
      "Location could not be deactivated because it either has a fulfillment service or is the only location with a shipping address.",
      "PERMANENTLY_BLOCKED_FROM_DEACTIVATION_ERROR",
    )
    |> append_location_deactivation_error(
      location_is_temporarily_blocked_from_deactivation(record),
      "locationId",
      "%{location_name} has incoming inventory. You'll be able to deactivate it once the inventory has been received.",
      "TEMPORARILY_BLOCKED_FROM_DEACTIVATION_ERROR",
    )
    |> append_location_deactivation_error(
      location_has_active_retail_subscription(record),
      "locationId",
      "Location could not be deactivated because it has active retail subscriptions.",
      "HAS_ACTIVE_RETAIL_SUBSCRIPTIONS",
    )
    |> append_location_deactivation_error(
      location_has_incoming_from_external_document_sources(record),
      "locationId",
      "Location could not be deactivated because it has incoming inventory from external document sources.",
      "HAS_INCOMING_FROM_EXTERNAL_DOCUMENT_SOURCES",
    )

  case common_errors {
    [_, ..] -> common_errors
    [] ->
      case destination_location_id {
        Some(destination_id) ->
          location_destination_guard_errors(store, location_id, destination_id)
        None -> location_deactivation_no_destination_errors(store, record)
      }
  }
}

fn location_destination_guard_errors(
  store: Store,
  location_id: String,
  destination_id: String,
) -> List(SourceValue) {
  case destination_id == location_id {
    True -> [
      location_deactivation_error(
        "destinationLocationId",
        "Location could not be deactivated because the destination location cannot be set to the location to be deactivated.",
        "DESTINATION_LOCATION_IS_THE_SAME_LOCATION",
      ),
    ]
    False ->
      case
        store.get_effective_store_property_location_by_id(store, destination_id)
      {
        None -> [
          destination_location_not_found_or_inactive_error(),
        ]
        Some(destination) ->
          case location_is_merchant_managed(destination) {
            False -> [
              location_deactivation_error(
                "destinationLocationId",
                "destination location not shopify managed",
                "DESTINATION_LOCATION_NOT_SHOPIFY_MANAGED",
              ),
            ]
            True ->
              case location_bool_field(destination, "isActive", True) {
                False -> [
                  destination_location_not_found_or_inactive_error(),
                ]
                True -> []
              }
          }
      }
  }
}

fn destination_location_not_found_or_inactive_error() -> SourceValue {
  location_deactivation_error(
    "destinationLocationId",
    "Location could not be deactivated because the destination location could be not found or is inactive.",
    "DESTINATION_LOCATION_NOT_FOUND_OR_INACTIVE",
  )
}

fn location_deactivation_no_destination_errors(
  store: Store,
  record: StorePropertyRecord,
) -> List(SourceValue) {
  []
  |> append_location_deactivation_error(
    location_has_stocked_inventory(record),
    "locationId",
    "Location could not be deactivated without specifying where to relocate inventory stocked at the location.",
    "HAS_ACTIVE_INVENTORY_ERROR",
  )
  |> append_location_deactivation_error(
    location_has_pending_orders(record),
    "locationId",
    "Location could not be deactivated because it has pending orders.",
    "HAS_FULFILLMENT_ORDERS_ERROR",
  )
  |> append_location_deactivation_error(
    location_has_open_purchase_orders(record),
    "locationId",
    "Location could not be deactivated because it has open purchase orders.",
    "HAS_OPEN_PURCHASE_ORDERS_ERROR",
  )
  |> append_location_deactivation_error(
    location_has_active_transfers(store, record),
    "locationId",
    "Location could not be deactivated because it has active transfers.",
    "HAS_ACTIVE_TRANSFERS_ERROR",
  )
}

fn append_location_deactivation_error(
  errors: List(SourceValue),
  condition: Bool,
  field: String,
  message: String,
  code: String,
) -> List(SourceValue) {
  case condition {
    True ->
      list.append(errors, [
        location_deactivation_error(field, message, code),
      ])
    False -> errors
  }
}

fn location_deactivation_error(
  field: String,
  message: String,
  code: String,
) -> SourceValue {
  location_error_source(field, message, code)
}

fn stage_location_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let location_id = read_arg_string(args, "locationId")
  case location_id {
    Some(id) -> {
      // Pattern 2: delete validation needs the prior Location row so
      // active/inventory guardrails are local, not proxied mutations.
      let store = hydrate_location_if_missing(store, upstream, id)
      case store.get_effective_store_property_location_by_id(store, id) {
        Some(record) ->
          case location_delete_guard_errors(record) {
            [] -> {
              let next_store =
                store.delete_staged_store_property_location(store, id)
              let payload_source =
                src_object([
                  #("deletedLocationId", SrcString(id)),
                  #("locationDeleteUserErrors", SrcList([])),
                ])
              store_properties_types.GenericMutationResult(
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
            errors ->
              location_delete_error_result(
                store,
                identity,
                field,
                fragments,
                errors,
              )
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
) -> store_properties_types.GenericMutationResult {
  store_properties_types.GenericMutationResult(
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

fn location_delete_error_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  errors: List(SourceValue),
) -> store_properties_types.GenericMutationResult {
  let payload_source =
    src_object([
      #("deletedLocationId", SrcNull),
      #("locationDeleteUserErrors", SrcList(errors)),
    ])
  store_properties_types.GenericMutationResult(
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

fn location_delete_guard_errors(
  record: StorePropertyRecord,
) -> List(SourceValue) {
  case location_is_merchant_managed(record) {
    False -> [
      location_delete_error("Location not found.", "LOCATION_NOT_FOUND"),
    ]
    True -> {
      let errors =
        []
        |> append_location_delete_error(
          location_bool_field(record, "isActive", False),
          "The location cannot be deleted while it is active.",
          "LOCATION_IS_ACTIVE",
        )
        |> append_location_delete_error(
          location_has_stocked_inventory(record),
          "The location cannot be deleted while it has inventory.",
          "LOCATION_HAS_INVENTORY",
        )
        |> append_location_delete_error(
          location_has_pending_orders(record),
          "The location cannot be deleted while it has pending orders.",
          "LOCATION_HAS_PENDING_ORDERS",
        )
        |> append_location_delete_error(
          location_has_active_retail_subscription(record),
          "The location cannot be deleted while it has an active retail subscription.",
          "LOCATION_HAS_ACTIVE_RETAIL_SUBSCRIPTION",
        )
        |> append_primary_location_delete_error(record)
      case
        errors,
        store_property_bool_field(record, "deletable") |> option.unwrap(True)
      {
        [], False -> [
          location_delete_error(
            "Location cannot be deleted.",
            "LOCATION_NOT_DELETABLE",
          ),
        ]
        _, _ -> errors
      }
    }
  }
}

fn append_primary_location_delete_error(
  errors: List(SourceValue),
  record: StorePropertyRecord,
) -> List(SourceValue) {
  case errors, location_is_primary(record) {
    [], True ->
      list.append(errors, [
        location_delete_error(
          "The primary location cannot be deleted.",
          "LOCATION_IS_PRIMARY",
        ),
      ])
    _, _ -> errors
  }
}

fn append_location_delete_error(
  errors: List(SourceValue),
  condition: Bool,
  message: String,
  code: String,
) -> List(SourceValue) {
  case condition {
    True -> list.append(errors, [location_delete_error(message, code)])
    False -> errors
  }
}

fn location_delete_error(message: String, code: String) -> SourceValue {
  location_error_source("locationId", message, code)
}

fn location_is_merchant_managed(record: StorePropertyRecord) -> Bool {
  case
    first_location_bool_field(record, ["merchantManaged", "merchant_managed"])
  {
    Some(value) -> value
    None ->
      !location_bool_field(record, "isFulfillmentService", False)
      && case dict.get(record.data, "fulfillmentService") {
        Ok(StorePropertyObject(_)) -> False
        _ -> True
      }
  }
}

fn location_is_primary(record: StorePropertyRecord) -> Bool {
  first_location_bool_field(record, [
    "isPrimary",
    "primary",
    "isPrimaryLocation",
  ])
  |> option.unwrap(False)
}

fn location_has_pending_orders(record: StorePropertyRecord) -> Bool {
  first_location_bool_field(record, [
    "hasPendingOrders",
    "hasUnfulfilledOrders",
    "hasPendingFulfillmentOrders",
  ])
  |> option.unwrap(False)
  || location_inventory_levels_have_quantity(record, "committed")
}

fn location_has_active_retail_subscription(
  record: StorePropertyRecord,
) -> Bool {
  first_location_bool_field(record, [
    "hasActiveRetailSubscription",
    "hasActiveRetailSubscriptions",
    "activeRetailSubscription",
  ])
  |> option.unwrap(False)
}

fn location_is_permanently_blocked_from_deactivation(
  record: StorePropertyRecord,
) -> Bool {
  first_location_bool_field(record, [
    "permanentlyBlockedFromDeactivation",
    "permanently_blocked_from_deactivation",
    "permanentlyBlockedFromDeactivationError",
  ])
  |> option.unwrap(False)
  || location_is_primary(record)
}

fn location_is_temporarily_blocked_from_deactivation(
  record: StorePropertyRecord,
) -> Bool {
  first_location_bool_field(record, [
    "temporarilyBlockedFromDeactivation",
    "temporarily_blocked_from_deactivation",
    "temporarilyBlockedFromDeactivationError",
  ])
  |> option.unwrap(False)
}

fn location_has_incoming_from_external_document_sources(
  record: StorePropertyRecord,
) -> Bool {
  first_location_bool_field(record, [
    "hasIncomingFromExternalDocumentSources",
    "hasIncomingFromExternalDocumentSource",
    "incomingFromExternalDocumentSources",
    "incomingFromExternalDocumentSource",
  ])
  |> option.unwrap(False)
}

fn location_has_open_purchase_orders(record: StorePropertyRecord) -> Bool {
  first_location_bool_field(record, [
    "hasOpenPurchaseOrders",
    "hasOpenPurchaseOrder",
    "openPurchaseOrders",
  ])
  |> option.unwrap(False)
}

fn location_has_active_transfers(
  store: Store,
  record: StorePropertyRecord,
) -> Bool {
  first_location_bool_field(record, [
    "hasActiveTransfers",
    "hasActiveTransfer",
    "activeTransfers",
  ])
  |> option.unwrap(False)
  || location_has_effective_active_transfer(store, record.id)
}

fn location_has_effective_active_transfer(
  store: Store,
  location_id: String,
) -> Bool {
  store.list_effective_inventory_transfers(store)
  |> list.any(fn(transfer) {
    inventory_transfer_is_active_for_deactivation(transfer.status)
    && {
      transfer_location_matches(transfer.origin, location_id)
      || transfer_location_matches(transfer.destination, location_id)
    }
  })
}

fn inventory_transfer_is_active_for_deactivation(status: String) -> Bool {
  case string.uppercase(status) {
    "CANCELLED" | "CANCELED" | "CLOSED" | "COMPLETED" | "COMPLETE" -> False
    _ -> True
  }
}

fn transfer_location_matches(
  location: Option(InventoryTransferLocationSnapshotRecord),
  location_id: String,
) -> Bool {
  case location {
    Some(snapshot) -> snapshot.id == Some(location_id)
    None -> False
  }
}

fn location_has_stocked_inventory(record: StorePropertyRecord) -> Bool {
  first_location_bool_field(record, ["hasActiveInventory", "hasInventory"])
  |> option.unwrap(False)
  || location_inventory_levels_have_stock(record)
}

fn first_location_bool_field(
  record: StorePropertyRecord,
  keys: List(String),
) -> Option(Bool) {
  case keys {
    [] -> None
    [key, ..rest] ->
      case store_property_bool_field(record, key) {
        Some(value) -> Some(value)
        None -> first_location_bool_field(record, rest)
      }
  }
}

fn location_activation_limit_reached(record: StorePropertyRecord) -> Bool {
  case
    first_location_bool_field(record, [
      "reachedLocationLimit",
      "locationLimitReached",
    ])
  {
    Some(value) -> value
    None ->
      nested_store_property_bool_field(record.data, [
        "shop",
        "resourceLimits",
        "locationLimitReached",
      ])
      |> option.unwrap(False)
  }
}

fn location_has_incomplete_mass_relocation(
  record: StorePropertyRecord,
) -> Bool {
  first_location_bool_field(record, [
    "hasIncompleteMassRelocation",
    "has_incomplete_mass_relocation",
    "hasOngoingRelocation",
  ])
  |> option.unwrap(False)
}

fn nested_store_property_bool_field(
  data: Dict(String, StorePropertyValue),
  path: List(String),
) -> Option(Bool) {
  case path {
    [] -> None
    [key] ->
      case dict.get(data, key) {
        Ok(StorePropertyBool(value)) -> Some(value)
        _ -> None
      }
    [key, ..rest] ->
      case dict.get(data, key) {
        Ok(StorePropertyObject(child)) ->
          nested_store_property_bool_field(child, rest)
        _ -> None
      }
  }
}

fn location_inventory_levels_have_stock(record: StorePropertyRecord) -> Bool {
  location_inventory_quantity_values(record)
  |> list.any(fn(quantity) {
    quantity.name != "committed" && quantity.quantity > 0
  })
}

fn location_inventory_levels_have_quantity(
  record: StorePropertyRecord,
  quantity_name: String,
) -> Bool {
  location_inventory_quantity_values(record)
  |> list.any(fn(quantity) {
    quantity.name == quantity_name && quantity.quantity > 0
  })
}

fn location_inventory_quantity_values(
  record: StorePropertyRecord,
) -> List(store_properties_types.LocationInventoryQuantity) {
  case dict.get(record.data, "inventoryLevels") {
    Ok(StorePropertyObject(connection)) ->
      store_property_object_list_field(connection, "nodes")
      |> list.flat_map(inventory_level_quantity_values)
    _ -> []
  }
}

fn inventory_level_quantity_values(
  level: Dict(String, StorePropertyValue),
) -> List(store_properties_types.LocationInventoryQuantity) {
  store_property_object_list_field(level, "quantities")
  |> list.filter_map(fn(quantity) {
    case
      store_property_string_field(quantity, "name"),
      store_property_int_field(quantity, "quantity")
    {
      Some(name), Some(amount) ->
        Ok(store_properties_types.LocationInventoryQuantity(
          name: name,
          quantity: amount,
        ))
      _, _ -> Error(Nil)
    }
  })
}

fn store_property_object_list_field(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> List(Dict(String, StorePropertyValue)) {
  case dict.get(data, key) {
    Ok(StorePropertyList(values)) ->
      values
      |> list.filter_map(fn(value) {
        case value {
          StorePropertyObject(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn store_property_string_field(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> Option(String) {
  case dict.get(data, key) {
    Ok(StorePropertyString(value)) -> Some(value)
    _ -> None
  }
}

fn store_property_int_field(
  data: Dict(String, StorePropertyValue),
  key: String,
) -> Option(Int) {
  case dict.get(data, key) {
    Ok(StorePropertyInt(value)) -> Some(value)
    _ -> None
  }
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
) -> store_properties_types.GenericMutationResult {
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
  store_properties_types.GenericMutationResult(
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
) -> store_properties_types.GenericMutationResult {
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
) -> store_properties_types.GenericMutationResult {
  let payload_source =
    src_object([
      #("location", case location {
        Some(record) -> store_property_data_to_source(record.data)
        None -> SrcNull
      }),
      #(errors_field, SrcList(errors)),
    ])
  store_properties_types.GenericMutationResult(
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
) -> store_properties_types.GenericMutationResult {
  let args = graphql_helpers.field_args(field, variables)
  let id = read_arg_string(args, "id") |> option.unwrap("")
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
  store_properties_types.GenericMutationResult(
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

pub fn hydrate_shop_baseline_if_needed(
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

fn stage_valid_shop_policy_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  type_: String,
  body: String,
) -> store_properties_types.StagePolicyResult {
  case store.get_effective_shop(store) {
    None ->
      store_properties_types.StagePolicyResult(
        shop_policy: None,
        user_errors: [
          store_properties_types.ShopPolicyUserError(
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
          title: shop_policy_title(type_),
          body: body,
          type_: type_,
          url: build_shop_policy_url(shop, id, type_),
          created_at: case existing {
            Some(policy) -> policy.created_at
            None -> now
          },
          updated_at: now,
          migrated_to_html: True,
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
      store_properties_types.StagePolicyResult(
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
) -> store_properties_types.PolicyValidation {
  let type_ = case input {
    Some(values) -> read_arg_string(values, "type")
    None -> None
  }
  let body = case input {
    Some(values) -> read_arg_string(values, "body")
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
      case
        type_ == Some("SUBSCRIPTION_POLICY") && string.trim(value) == "",
        string.byte_size(value) > shop_policy_body_limit_chars
      {
        True, _ -> [
          store_properties_types.ShopPolicyUserError(
            field: Some(["shopPolicy", "body"]),
            message: "Purchase options cancellation policy required",
            code: None,
          ),
        ]
        _, True -> [
          store_properties_types.ShopPolicyUserError(
            field: Some(["shopPolicy", "body"]),
            message: "Body is too big (maximum is 512 KB)",
            code: Some("TOO_BIG"),
          ),
        ]
        _, False -> []
      }
    None -> [
      store_properties_types.ShopPolicyUserError(
        field: Some(["shopPolicy", "body"]),
        message: "Body is required",
        code: None,
      ),
    ]
  }
  store_properties_types.PolicyValidation(
    type_: type_,
    body: body,
    user_errors: list.append(type_errors, body_errors),
  )
}

fn invalid_type_error() -> store_properties_types.ShopPolicyUserError {
  store_properties_types.ShopPolicyUserError(
    field: Some(["shopPolicy", "type"]),
    message: "Type is invalid",
    code: None,
  )
}

fn read_object_list(
  values: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(Dict(String, root_field.ResolvedValue)) {
  case dict.get(values, key) {
    Ok(root_field.ListVal(items)) ->
      items
      |> list.filter_map(fn(item) {
        case item {
          root_field.ObjectVal(fields) -> Ok(fields)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn merged_location_address(
  record: StorePropertyRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, StorePropertyValue)) {
  merged_location_address_from_data(record.data, input)
}

fn merged_location_address_from_data(
  data: Dict(String, StorePropertyValue),
  input: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, StorePropertyValue)) {
  let existing = case dict.get(data, "address") {
    Ok(StorePropertyObject(fields)) -> Some(fields)
    _ -> None
  }
  let address_input = read_arg_object(input, "address")
  case existing, address_input {
    None, None -> None
    _, _ -> {
      let base = option.unwrap(existing, dict.new())
      let values = option.unwrap(address_input, dict.new())
      Some(
        base
        |> insert_optional_address_string(values, "address1")
        |> insert_optional_address_string(values, "address2")
        |> insert_optional_address_string(values, "city")
        |> insert_optional_address_string(values, "phone")
        |> insert_optional_address_string(values, "zip")
        |> insert_optional_address_string(values, "countryCode")
        |> insert_optional_address_string(values, "provinceCode")
        |> maybe_insert_country_name(values),
      )
    }
  }
}

fn insert_optional_address_string(
  fields: Dict(String, StorePropertyValue),
  values: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, StorePropertyValue) {
  case read_arg_string(values, key) {
    Some(value) -> dict.insert(fields, key, StorePropertyString(value))
    None -> fields
  }
}

fn maybe_insert_country_name(
  fields: Dict(String, StorePropertyValue),
  values: Dict(String, root_field.ResolvedValue),
) -> Dict(String, StorePropertyValue) {
  case read_arg_string(values, "countryCode") {
    Some("CA") -> dict.insert(fields, "country", StorePropertyString("Canada"))
    Some("US") ->
      dict.insert(fields, "country", StorePropertyString("United States"))
    _ -> fields
  }
}

fn location_bound_to_delivery_profile(
  store: Store,
  location_id: String,
) -> Bool {
  store.list_effective_delivery_profiles(store)
  |> list.any(fn(profile) {
    profile.merchant_owned
    && captured_json_contains_string(profile.data, location_id)
  })
}

fn captured_json_contains_string(
  value: CapturedJsonValue,
  needle: String,
) -> Bool {
  case value {
    CapturedString(value) -> value == needle
    CapturedArray(items) ->
      list.any(items, fn(item) { captured_json_contains_string(item, needle) })
    CapturedObject(fields) ->
      list.any(fields, fn(pair) {
        let #(_, child) = pair
        captured_json_contains_string(child, needle)
      })
    _ -> False
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
    store_types.MutationLogEntry(
      id: log_id,
      received_at: received_at,
      operation_name: None,
      path: request_path,
      query: document,
      variables: dict.new(),
      staged_resource_ids: staged_ids,
      status: store_types.Staged,
      interpreted: store_types.InterpretedMetadata(
        operation_type: store_types.Mutation,
        operation_name: None,
        root_fields: [root_name],
        primary_root_field: Some(root_name),
        capability: store_types.Capability(
          operation_name: Some(root_name),
          domain: "store-properties",
          execution: "stage-locally",
        ),
      ),
      notes: Some("Locally staged " <> root_name <> " in shopify-draft-proxy."),
    )
  #(store.record_mutation_log_entry(store, entry), identity_final)
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
    "CONTACT_INFORMATION" -> "Contact Information"
    "LEGAL_NOTICE" -> "Legal Notice"
    "PRIVACY_POLICY" -> "Privacy Policy"
    "REFUND_POLICY" -> "Refund Policy"
    "SHIPPING_POLICY" -> "Shipping Policy"
    "SUBSCRIPTION_POLICY" -> "Subscription Policy"
    "TERMS_OF_SALE" -> "Terms of Sale"
    "TERMS_OF_SERVICE" -> "Terms of Service"
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
      shop_policy_url_base(shop)
      <> "/"
      <> shop_tail
      <> "/policies/"
      <> policy_tail
      <> ".html?locale=en"
    _, _ ->
      shop_policy_url_base(shop)
      <> "/policies/"
      <> string.replace(string.lowercase(type_), "_", "-")
  }
}

fn shop_policy_url_base(shop: ShopRecord) -> String {
  case string.trim(shop.primary_domain.url) {
    "" -> {
      case string.trim(shop.primary_domain.host) {
        "" -> fallback_shop_url(shop)
        host -> "https://" <> host
      }
    }
    url -> trim_trailing_slash(url)
  }
}

fn fallback_shop_url(shop: ShopRecord) -> String {
  case string.trim(shop.url) {
    "" -> "https://" <> shop.myshopify_domain
    url -> trim_trailing_slash(url)
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
