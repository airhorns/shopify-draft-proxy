//// Payments query dispatch.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/result
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/payments/serializers
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/state/store.{type Store}

@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  use data <- result.try(handle_payments_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

@internal
pub fn handle_query_request(
  proxy: DraftProxy,
  _request: Request,
  _parsed: parse_operation.ParsedOperation,
  _primary_root_field: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Response, DraftProxy) {
  respond_to_query(
    proxy,
    process(proxy.store, document, variables),
    "Failed to handle payments query",
  )
}

@internal
pub fn handle_payments_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, root_field.RootFieldError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(
        json.object(
          list.map(fields, fn(field) {
            #(
              get_field_response_key(field),
              serializers.query_payload(store, field, fragments, variables),
            )
          }),
        ),
      )
    }
  }
}
