//// Segments query handling.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
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
import shopify_draft_proxy/proxy/segments/serializers
import shopify_draft_proxy/proxy/segments/types.{type SegmentsError, ParseFailed}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{is_proxy_synthetic_gid}

@internal
pub fn is_segment_query_root(name: String) -> Bool {
  case name {
    "segment" -> True
    "segments" -> True
    "segmentsCount" -> True
    "segmentFilters" -> True
    "segmentFilterSuggestions" -> True
    "segmentValueSuggestions" -> True
    "segmentMigrations" -> True
    "customerSegmentMembers" -> True
    "customerSegmentMembersQuery" -> True
    "customerSegmentMembership" -> True
    _ -> False
  }
}

/// True iff any string variable names a segment that local state must
/// answer itself. This keeps LiveHybrid passthrough from forwarding
/// staged, deleted, or proxy-synthetic segment IDs upstream.
@internal
pub fn local_has_segment_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id) || local_segment_id_known(proxy.store, id)
      _ -> False
    }
  })
}

/// True iff segment lifecycle state has been staged locally, or any
/// variable carries a local segment ID. Connection/count/catalog roots
/// use this to stay local after segment writes while cold reads can
/// still pass through verbatim in LiveHybrid mode.
@internal
pub fn local_has_staged_segments(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  serializers.store_has_staged_segments(proxy.store)
  || local_has_segment_id(proxy, variables)
}

fn local_segment_id_known(store: Store, id: String) -> Bool {
  case store.get_effective_segment_by_id(store, id) {
    Some(_) -> True
    None ->
      case dict.get(store.staged_state.deleted_segment_ids, id) {
        Ok(True) -> True
        _ -> False
      }
  }
}

@internal
pub fn handle_segments_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SegmentsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
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
    parse_operation.QueryOperation, "segment" ->
      !local_has_segment_id(proxy, variables)
    parse_operation.QueryOperation, "segments"
    | parse_operation.QueryOperation, "segmentsCount"
    | parse_operation.QueryOperation, "segmentFilters"
    | parse_operation.QueryOperation, "segmentFilterSuggestions"
    | parse_operation.QueryOperation, "segmentValueSuggestions"
    | parse_operation.QueryOperation, "segmentMigrations"
    -> !local_has_staged_segments(proxy, variables)
    _, _ -> False
  }
}

/// Segments cold catalog reads are Pattern 1 in cassette-backed
/// LiveHybrid: forward Shopify's baseline payload verbatim until local
/// segment lifecycle state exists. Snapshot and post-mutation reads
/// continue through the local serializer so read-after-write stays
/// local-only.
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
                        json.string("Failed to handle segments query"),
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

/// Convenience: parse + handle + wrap, for the dispatcher.
@internal
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, SegmentsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let results =
        list.map(fields, fn(field) {
          root_query_result(store, field, fragments, variables, document)
        })
      let data_entries =
        list.map(results, fn(result) { #(result.key, result.value) })
      let errors = list.flat_map(results, fn(result) { result.errors })
      let null_data = list.any(results, fn(result) { result.null_data })
      let data = case null_data {
        True -> json.null()
        False -> json.object(data_entries)
      }
      let entries = case errors {
        [] -> [#("data", data)]
        _ -> [
          #("data", data),
          #("errors", json.array(errors, fn(error) { error })),
        ]
      }
      Ok(json.object(entries))
    }
  }
}

type QueryFieldResult {
  QueryFieldResult(
    key: String,
    value: Json,
    errors: List(Json),
    null_data: Bool,
  )
}

// ---------------------------------------------------------------------------
// Query dispatch
// ---------------------------------------------------------------------------

@internal
pub fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value =
        serializers.root_payload_for_field(store, field, fragments, variables)
      #(key, value)
    })
  json.object(entries)
}

fn root_query_result(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
) -> QueryFieldResult {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, ..) -> {
      let value =
        serializers.root_payload_for_field(store, field, fragments, variables)
      case name.value {
        "segment" -> {
          let args = graphql_helpers.field_args(field, variables)
          case
            graphql_helpers.read_arg_string_nonempty(args, "id"),
            serializers.json_is_null(value)
          {
            Some(_), True ->
              QueryFieldResult(
                key: key,
                value: value,
                errors: [
                  serializers.segment_not_found_error(field, document, key),
                ],
                null_data: False,
              )
            _, _ ->
              QueryFieldResult(
                key: key,
                value: value,
                errors: [],
                null_data: False,
              )
          }
        }
        "customerSegmentMembersQuery" -> {
          let args = graphql_helpers.field_args(field, variables)
          case
            graphql_helpers.read_arg_string_nonempty(args, "id"),
            serializers.json_is_null(value)
          {
            Some(_), True ->
              QueryFieldResult(
                key: key,
                value: value,
                errors: [
                  serializers.customer_segment_members_query_not_found_error(
                    field,
                    document,
                    key,
                  ),
                ],
                null_data: False,
              )
            _, _ ->
              QueryFieldResult(
                key: key,
                value: value,
                errors: [],
                null_data: False,
              )
          }
        }
        "customerSegmentMembers" -> {
          let args = graphql_helpers.field_args(field, variables)
          case
            serializers.customer_segment_members_error(
              store,
              args,
              field,
              document,
              key,
            )
          {
            Some(error) ->
              QueryFieldResult(
                key: key,
                value: json.null(),
                errors: [error],
                null_data: True,
              )
            None ->
              QueryFieldResult(
                key: key,
                value: value,
                errors: [],
                null_data: False,
              )
          }
        }
        _ ->
          QueryFieldResult(key: key, value: value, errors: [], null_data: False)
      }
    }
    _ ->
      QueryFieldResult(
        key: key,
        value: json.null(),
        errors: [],
        null_data: False,
      )
  }
}
