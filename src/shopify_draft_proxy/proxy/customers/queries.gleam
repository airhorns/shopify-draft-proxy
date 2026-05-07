//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/customers/customer_types.{
  type CustomersError, ParseFailed,
}
import shopify_draft_proxy/proxy/customers/serializers.{
  customer_count_search_extensions, serialize_root_fields, wrap_query_payload,
}
import shopify_draft_proxy/proxy/graphql_helpers.{get_document_fragments}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

@internal
pub fn is_customer_query_root(name: String) -> Bool {
  case name {
    "customer"
    | "customers"
    | "customersCount"
    | "customerByIdentifier"
    | "customerAccountPage"
    | "customerAccountPages"
    | "customerMergePreview"
    | "customerMergeJobStatus"
    | "storeCreditAccount"
    | "customerPaymentMethod" -> True
    _ -> False
  }
}

/// True iff the requested `customer(id:)` argument resolves to a
/// customer that's already in local state (base or staged). Used by
/// the dispatcher to skip `LiveHybrid` passthrough when a prior
/// staged mutation has already produced the record we'd otherwise
/// fetch — e.g. a `customerCreate` followed by `customer(id: <newly
/// staged synthetic gid>)` in the same scenario.
@internal
pub fn local_has_customer_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id) || local_customer_id_known(proxy.store, id)
      _ -> False
    }
  })
}

@internal
pub fn local_customer_id_known(store: Store, id: String) -> Bool {
  case store.get_effective_customer_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_customer_ids, id) {
        Ok(True) -> True
        _ ->
          case dict.get(store.staged_state.merged_customer_ids, id) {
            Ok(_) -> True
            Error(_) -> False
          }
      }
  }
}

/// In `LiveHybrid` mode, decide whether this customer-domain
/// operation should be answered by reaching upstream verbatim instead
/// of from local state. Internal helper for `handle_query_request` —
/// the dispatcher does not consult this directly anymore.
///
/// The customer-domain operations on this list are aggregates and
/// catalog reads that the local handler can't compute the right
/// answer for without reaching upstream. `customer(id:)` only
/// passes through when the requested id isn't in local state — a
/// staged-create-then-read flow stays local end-to-end.
///
/// In `Snapshot` mode the same operations stay local (typically
/// with a degenerate empty answer that matches empty-snapshot
/// expectations).
@internal
pub fn should_passthrough_in_live_hybrid(
  proxy: DraftProxy,
  type_: parse_operation.GraphQLOperationType,
  primary_root_field: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case type_, primary_root_field {
    parse_operation.QueryOperation, "customersCount" -> True
    parse_operation.QueryOperation, "customerByIdentifier" ->
      list.is_empty(store.list_effective_customers(proxy.store))
    parse_operation.QueryOperation, "customer" ->
      !local_has_customer_id(proxy, variables)
    parse_operation.QueryOperation, "customers" ->
      list.is_empty(store.list_effective_customers(proxy.store))
    _, _ -> False
  }
}

@internal
pub fn request_upstream_context(
  proxy: DraftProxy,
  request: Request,
) -> UpstreamContext {
  upstream_query.UpstreamContext(
    transport: proxy.upstream_transport,
    origin: proxy.config.shopify_admin_origin,
    headers: request.headers,
    allow_upstream_reads: proxy.config.read_mode == LiveHybrid,
  )
}

/// Domain entrypoint for the customer query path. The dispatcher
/// always lands here for customer-domain reads regardless of
/// `read_mode`; the handler itself decides whether to compute the
/// answer from local state or to forward to upstream verbatim via
/// `passthrough.passthrough_sync` (when in `LiveHybrid` mode and the
/// operation is one we know we can't satisfy locally — see
/// `should_passthrough_in_live_hybrid`).
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
        process_with_upstream(
          proxy,
          document,
          variables,
          request_upstream_context(proxy, request),
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
                        json.string("Failed to handle customers query"),
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

@internal
pub fn handle_customer_query(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(
        proxy,
        fields,
        fragments,
        variables,
        empty_upstream_context(),
      ))
    }
  }
}

@internal
pub fn process(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, CustomersError) {
  process_with_upstream(proxy, document, variables, empty_upstream_context())
}

@internal
pub fn process_with_upstream(
  proxy: DraftProxy,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(Json, CustomersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let data =
        serialize_root_fields(proxy, fields, fragments, variables, upstream)
      let search_extensions =
        customer_count_search_extensions(fields, variables)
      Ok(wrap_query_payload(data, search_extensions))
    }
  }
}
