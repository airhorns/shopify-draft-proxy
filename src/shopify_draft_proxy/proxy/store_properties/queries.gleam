//// Query handling for Store Properties roots.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/store_properties/serializers.{
  serialize_business_entities_root, serialize_business_entity_root,
  serialize_location_by_identifier_result, serialize_location_root,
  serialize_locations_root, serialize_publishable_root, serialize_shop_root,
}
import shopify_draft_proxy/proxy/store_properties/types as store_properties_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

@internal
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

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use fields <- result.try(root_field.get_root_fields(document))
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
) -> store_properties_types.QueryFieldResult {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, ..) ->
      case name.value {
        "shop" ->
          store_properties_types.QueryFieldResult(
            key: key,
            value: serialize_shop_root(store, field, fragments),
            errors: [],
          )
        "location" ->
          store_properties_types.QueryFieldResult(
            key: key,
            value: serialize_location_root(store, field, fragments, variables),
            errors: [],
          )
        "locations" ->
          store_properties_types.QueryFieldResult(
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
          store_properties_types.QueryFieldResult(
            key: key,
            value: serialize_business_entities_root(store, field, fragments),
            errors: [],
          )
        "businessEntity" ->
          store_properties_types.QueryFieldResult(
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
          store_properties_types.QueryFieldResult(
            key: key,
            value: serialize_publishable_root(
              store,
              field,
              fragments,
              variables,
            ),
            errors: [],
          )
        _ ->
          store_properties_types.QueryFieldResult(
            key: key,
            value: json.null(),
            errors: [],
          )
      }
    _ ->
      store_properties_types.QueryFieldResult(
        key: key,
        value: json.null(),
        errors: [],
      )
  }
}
