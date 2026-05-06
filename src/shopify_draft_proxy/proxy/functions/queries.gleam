import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/option
import gleam/result
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/functions/serializers
import shopify_draft_proxy/proxy/graphql_helpers
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/state/store.{type Store}

@internal
pub fn is_function_query_root(name: String) -> Bool {
  case name {
    "validation" -> True
    "validations" -> True
    "cartTransforms" -> True
    "shopifyFunction" -> True
    "shopifyFunctions" -> True
    _ -> False
  }
}

@internal
pub fn handle_function_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(err)
    Ok(fields) -> {
      let fragments = graphql_helpers.get_document_fragments(document)
      Ok(serializers.serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_function_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

@internal
pub fn local_has_function_metadata(proxy: DraftProxy) -> Bool {
  store_has_function_metadata(proxy.store)
}

fn store_has_function_metadata(store_in: Store) -> Bool {
  dict.size(store_in.base_state.shopify_functions) > 0
  || dict.size(store_in.staged_state.shopify_functions) > 0
  || dict.size(store_in.base_state.validations) > 0
  || dict.size(store_in.staged_state.validations) > 0
  || dict.size(store_in.staged_state.deleted_validation_ids) > 0
  || dict.size(store_in.base_state.cart_transforms) > 0
  || dict.size(store_in.staged_state.cart_transforms) > 0
  || dict.size(store_in.staged_state.deleted_cart_transform_ids) > 0
  || option.is_some(store_in.base_state.tax_app_configuration)
  || option.is_some(store_in.staged_state.tax_app_configuration)
}

fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "validation" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "validations" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "cartTransforms" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "shopifyFunction" ->
      !local_has_function_metadata(proxy)
    parse_operation.QueryOperation, "shopifyFunctions" ->
      !local_has_function_metadata(proxy)
    _, _ -> False
  }
}

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
      should_passthrough_in_live_hybrid(proxy, parsed.type_, primary_root_field)
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
                        json.string("Failed to handle functions query"),
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
