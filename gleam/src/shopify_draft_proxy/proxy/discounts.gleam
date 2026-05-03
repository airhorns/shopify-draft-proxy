//// Discount-domain read and mutation staging.
////
//// This module ports the discount owner-node surface with a deliberately
//// flexible normalized record: the store tracks id/type/status/code fields for
//// local lifecycle behavior and keeps a captured/projectable owner-node payload
//// for Shopify-like field projection.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SelectedFieldOptions, SrcBool, SrcFloat,
  SrcInt, SrcList, SrcNull, SrcObject, SrcString, get_document_fragments,
  get_field_response_key, get_selected_child_fields, project_graphql_value,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type RequiredArgument, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/proxy_state.{type DraftProxy}
import shopify_draft_proxy/proxy/upstream_query
import shopify_draft_proxy/shopify/upstream_client.{type SyncTransport}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, type DiscountRecord, type ShopifyFunctionRecord,
  CapturedArray, CapturedBool, CapturedFloat, CapturedInt, CapturedNull,
  CapturedObject, CapturedString, DiscountRecord,
}

/// Upstream context threaded into mutation handlers so they can issue
/// `upstream_query.fetch_sync` calls when the local store doesn't have
/// enough information to compute the response. In production, callers
/// pass `transport: None` and reads fall through to the live HTTP shim
/// (Erlang) or are skipped (JS). Parity tests install a recorded
/// cassette as the transport.
pub type UpstreamContext {
  UpstreamContext(
    transport: Option(SyncTransport),
    origin: String,
    headers: Dict(String, String),
  )
}

/// Build an `UpstreamContext` whose `fetch_sync` calls will fail with
/// `NoTransportInstalled` on JS and fall through to the live HTTP shim
/// on Erlang. Used by callers that don't have access to inbound headers
/// or the proxy config (e.g. legacy tests).
pub fn empty_upstream_context() -> UpstreamContext {
  UpstreamContext(transport: None, origin: "", headers: dict.new())
}

pub type DiscountsError {
  ParseFailed(root_field.RootFieldError)
}

pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

type MutationResult {
  MutationResult(
    key: String,
    payload: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
  )
}

pub fn is_discount_query_root(name: String) -> Bool {
  case name {
    "discountNodes"
    | "discountNodesCount"
    | "discountNode"
    | "codeDiscountNodes"
    | "codeDiscountNode"
    | "codeDiscountNodeByCode"
    | "automaticDiscountNodes"
    | "automaticDiscountNode" -> True
    _ -> False
  }
}

pub fn is_discount_mutation_root(name: String) -> Bool {
  case name {
    "discountCodeBasicCreate"
    | "discountCodeBasicUpdate"
    | "discountCodeBxgyCreate"
    | "discountCodeBxgyUpdate"
    | "discountCodeFreeShippingCreate"
    | "discountCodeFreeShippingUpdate"
    | "discountCodeAppCreate"
    | "discountCodeAppUpdate"
    | "discountCodeActivate"
    | "discountCodeDeactivate"
    | "discountCodeDelete"
    | "discountCodeBulkActivate"
    | "discountCodeBulkDeactivate"
    | "discountCodeBulkDelete"
    | "discountRedeemCodeBulkAdd"
    | "discountCodeRedeemCodeBulkDelete"
    | "discountRedeemCodeBulkDelete"
    | "discountAutomaticBasicCreate"
    | "discountAutomaticBasicUpdate"
    | "discountAutomaticBxgyCreate"
    | "discountAutomaticBxgyUpdate"
    | "discountAutomaticFreeShippingCreate"
    | "discountAutomaticFreeShippingUpdate"
    | "discountAutomaticAppCreate"
    | "discountAutomaticAppUpdate"
    | "discountAutomaticActivate"
    | "discountAutomaticDeactivate"
    | "discountAutomaticDelete"
    | "discountAutomaticBulkDelete" -> True
    _ -> False
  }
}

/// True iff any string-typed variable value in the request resolves to
/// a discount that's already in local state, or is a proxy-synthetic
/// gid. The dispatcher uses this to skip `LiveHybrid` passthrough so
/// that read-after-create reads of a synthetic id stay local (and so
/// that read-after-delete reads of a synthetic id correctly return
/// null instead of forwarding a synthetic gid upstream where it would
/// 404).
///
/// We scan every string variable value rather than keying on `"id"`
/// because GraphQL operations frequently rebind the argument under a
/// different variable name (e.g. `discountNode(id: $codeId)`).
pub fn local_has_discount_id(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  dict.values(variables)
  |> list.any(fn(value) {
    case value {
      root_field.StringVal(id) ->
        is_proxy_synthetic_gid(id)
        || case store.get_effective_discount_by_id(proxy.store, id) {
          Some(_) -> True
          None -> False
        }
      _ -> False
    }
  })
}

/// True iff the local store has any staged discount records, or any
/// variable carries a proxy-synthetic gid. The dispatcher uses this to
/// keep aggregate / connection / by-code reads on the local handler
/// once a lifecycle scenario has staged or deleted discounts —
/// passthrough would otherwise forward synthetic gids upstream (404)
/// or skip the empty/null answer the lifecycle test expects after a
/// delete.
pub fn local_has_staged_discounts(
  proxy: DraftProxy,
  variables: Dict(String, root_field.ResolvedValue),
) -> Bool {
  let has_synthetic =
    dict.values(variables)
    |> list.any(fn(value) {
      case value {
        root_field.StringVal(s) -> is_proxy_synthetic_gid(s)
        _ -> False
      }
    })
  has_synthetic || !list.is_empty(store.list_effective_discounts(proxy.store))
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, DiscountsError) {
  use data <- result.try(handle_discount_query(store, document, variables))
  Ok(json.object([#("data", data)]))
}

pub fn handle_discount_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, DiscountsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(
        json.object(
          list.map(fields, fn(field) {
            #(
              get_field_response_key(field),
              root_query_payload(store, field, fragments, variables),
            )
          }),
        ),
      )
    }
  }
}

fn root_query_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "discountNode" -> {
          let id = read_string_arg(field, variables, "id")
          id
          |> option.then(fn(id) {
            store.get_effective_discount_by_id(store, id)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_node_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "codeDiscountNode" | "automaticDiscountNode" -> {
          let wanted_kind = case name.value {
            "codeDiscountNode" -> "code"
            _ -> "automatic"
          }
          let id = read_string_arg(field, variables, "id")
          id
          |> option.then(fn(id) {
            store.get_effective_discount_by_id(store, id)
          })
          |> option.then(fn(record) {
            case record.owner_kind == wanted_kind {
              True -> Some(record)
              False -> None
            }
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_owner_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "codeDiscountNodeByCode" -> {
          let code = read_string_arg(field, variables, "code")
          code
          |> option.then(fn(code) {
            find_effective_discount_by_code(store, code)
          })
          |> option.map(fn(record) {
            project_graphql_value(
              discount_owner_source(record),
              child_fields(field),
              fragments,
            )
          })
          |> option.unwrap(json.null())
        }
        "discountNodes" ->
          serialize_discount_connection(
            filter_discounts(
              store.list_effective_discounts(store),
              field,
              variables,
            ),
            field,
            fragments,
            DiscountNodeConnection,
          )
        "codeDiscountNodes" ->
          serialize_discount_connection(
            filter_discounts(
              store.list_effective_discounts(store),
              field,
              variables,
            )
              |> list.filter(fn(record) { record.owner_kind == "code" }),
            field,
            fragments,
            OwnerNodeConnection,
          )
        "automaticDiscountNodes" ->
          serialize_discount_connection(
            filter_discounts(
              store.list_effective_discounts(store),
              field,
              variables,
            )
              |> list.filter(fn(record) { record.owner_kind == "automatic" }),
            field,
            fragments,
            OwnerNodeConnection,
          )
        "discountNodesCount" ->
          serialize_count(
            list.length(filter_discounts(
              store.list_effective_discounts(store),
              field,
              variables,
            )),
            field,
          )
        _ -> json.null()
      }
    _ -> json.null()
  }
}

type ConnectionMode {
  DiscountNodeConnection
  OwnerNodeConnection
}

fn serialize_discount_connection(
  records: List(DiscountRecord),
  field: Selection,
  fragments: FragmentMap,
  mode: ConnectionMode,
) -> Json {
  let first = read_int_arg(field, dict.new(), "first") |> option.unwrap(50)
  let records = list.take(records, int.max(0, first))
  let node_for = fn(record: DiscountRecord) {
    case mode {
      DiscountNodeConnection -> discount_node_source(record)
      OwnerNodeConnection -> discount_owner_source(record)
    }
  }
  let children = child_fields(field)
  json.object(
    list.map(children, fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: child_name, ..) ->
          case child_name.value {
            "nodes" -> #(
              key,
              json.array(records, fn(record) {
                project_graphql_value(
                  node_for(record),
                  child_fields(child),
                  fragments,
                )
              }),
            )
            "edges" -> #(
              key,
              json.array(records, fn(record) {
                let cursor =
                  record.cursor |> option.unwrap("cursor:" <> record.id)
                project_graphql_value(
                  SrcObject(
                    dict.from_list([
                      #("cursor", SrcString(cursor)),
                      #("node", node_for(record)),
                    ]),
                  ),
                  child_fields(child),
                  fragments,
                )
              }),
            )
            "pageInfo" -> #(key, serialize_page_info(records, child))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_page_info(
  records: List(DiscountRecord),
  field: Selection,
) -> Json {
  let start = case records {
    [first, ..] ->
      first.cursor |> option.unwrap("cursor:" <> first.id) |> json.string
    [] -> json.null()
  }
  let end = case list.reverse(records) {
    [last, ..] ->
      last.cursor |> option.unwrap("cursor:" <> last.id) |> json.string
    [] -> json.null()
  }
  json.object(
    list.map(child_fields(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "hasNextPage" -> #(key, json.bool(False))
            "hasPreviousPage" -> #(key, json.bool(False))
            "startCursor" -> #(key, start)
            "endCursor" -> #(key, end)
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_count(count: Int, field: Selection) -> Json {
  json.object(
    list.map(child_fields(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "count" -> #(key, json.int(count))
            "precision" -> #(key, json.string("EXACT"))
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn filter_discounts(
  records: List(DiscountRecord),
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(DiscountRecord) {
  let query = read_string_arg(field, variables, "query")
  case query {
    None -> records
    Some(query) -> {
      let q = string.lowercase(query)
      case string.contains(q, "code:") {
        True -> []
        False ->
          list.filter(records, fn(record) {
            let status_ok = case string.contains(q, "status:active") {
              True -> record.status == "ACTIVE"
              False ->
                case string.contains(q, "status:expired") {
                  True -> record.status == "EXPIRED"
                  False -> True
                }
            }
            let type_ok = case string.contains(q, "type:app") {
              True -> record.discount_type == "app"
              False ->
                case string.contains(q, "type:free_shipping") {
                  True -> record.discount_type == "free_shipping"
                  False ->
                    case string.contains(q, "type:code") {
                      True -> record.owner_kind == "code"
                      False -> True
                    }
                }
            }
            status_ok && type_ok
          })
      }
    }
  }
}

fn discount_owner_source(record: DiscountRecord) -> SourceValue {
  captured_to_source(record.payload)
}

fn discount_node_source(record: DiscountRecord) -> SourceValue {
  case captured_to_source(record.payload) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      SrcObject(
        fields
        |> dict.delete("automaticDiscount")
        |> dict.delete("codeDiscount")
        |> dict.insert("id", SrcString(record.id))
        |> dict.insert("discount", discount),
      )
    }
    _ ->
      SrcObject(
        dict.from_list([
          #("id", SrcString(record.id)),
          #("discount", SrcNull),
        ]),
      )
  }
}

fn child_fields(field: Selection) -> List(Selection) {
  get_selected_child_fields(
    field,
    SelectedFieldOptions(include_inline_fragments: True),
  )
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, DiscountsError) {
  process_mutation_with_upstream(
    store,
    identity,
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

/// Variant of `process_mutation` that threads an `UpstreamContext` into
/// the per-handler logic. Used by the dispatcher when the proxy has an
/// `upstream_transport` installed (parity cassette in tests, live HTTP
/// in production), so that handlers like `discountCodeBasicCreate` can
/// consult upstream for cross-discount uniqueness checks before staging.
pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, DiscountsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        document,
        operation_path,
        upstream,
      ))
    }
  }
}

fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
  operation_path: String,
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(entries, all_errors, final_store, final_identity, staged_ids, drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, errors, current_store, current_identity, staged, drafts) =
        acc
      case field {
        Field(name: name, ..) -> {
          let top_level_errors =
            validate_required_field_arguments(
              field,
              variables,
              name.value,
              required_arguments_for_root(name.value),
              operation_path,
              document,
            )
          case top_level_errors {
            [_, ..] -> #(
              entries,
              list.append(errors, top_level_errors),
              current_store,
              current_identity,
              staged,
              drafts,
            )
            [] -> {
              let result =
                handle_discount_mutation_field(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> entries
              }
              let next_staged = case result.top_level_errors {
                [] -> list.append(staged, result.staged_resource_ids)
                _ -> staged
              }
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  case result.staged_resource_ids {
                    [] -> store.Failed
                    _ -> store.Staged
                  },
                  "discounts",
                  "stage-locally",
                  Some("discount mutation staged locally in Gleam port"),
                )
              #(
                next_entries,
                next_errors,
                result.store,
                result.identity,
                next_staged,
                list.append(drafts, [draft]),
              )
            }
          }
        }
        _ -> acc
      }
    })
  let envelope = case all_errors {
    [] -> json.object([#("data", json.object(entries))])
    _ -> json.object([#("errors", json.preprocessed_array(all_errors))])
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: case all_errors {
      [] -> staged_ids
      _ -> []
    },
    log_drafts: drafts,
  )
}

fn required_arguments_for_root(root: String) -> List(RequiredArgument) {
  case root {
    "discountCodeBasicCreate" -> [
      RequiredArgument("basicCodeDiscount", "DiscountCodeBasicInput!"),
    ]
    "discountCodeBasicUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("basicCodeDiscount", "DiscountCodeBasicInput!"),
    ]
    "discountCodeBxgyCreate" -> [
      RequiredArgument("bxgyCodeDiscount", "DiscountCodeBxgyInput!"),
    ]
    "discountCodeBxgyUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("bxgyCodeDiscount", "DiscountCodeBxgyInput!"),
    ]
    "discountAutomaticBasicCreate" -> [
      RequiredArgument("automaticBasicDiscount", "DiscountAutomaticBasicInput!"),
    ]
    "discountAutomaticBasicUpdate" -> [
      RequiredArgument("id", "ID!"),
      RequiredArgument("automaticBasicDiscount", "DiscountAutomaticBasicInput!"),
    ]
    _ -> []
  }
}

fn handle_discount_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  case root {
    "discountCodeBasicCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "basic",
        "basicCodeDiscount",
        upstream,
      )
    "discountCodeBasicUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "basicCodeDiscount",
      )
    "discountCodeBxgyCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "bxgy",
        "bxgyCodeDiscount",
        upstream,
      )
    "discountCodeBxgyUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "bxgyCodeDiscount",
      )
    "discountCodeFreeShippingCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "free_shipping",
        "freeShippingCodeDiscount",
        upstream,
      )
    "discountCodeFreeShippingUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "freeShippingCodeDiscount",
      )
    "discountCodeAppCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "app",
        "codeAppDiscount",
        upstream,
      )
    "discountCodeAppUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "code",
        "codeAppDiscount",
      )
    "discountAutomaticBasicCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "basic",
        "automaticBasicDiscount",
        upstream,
      )
    "discountAutomaticBasicUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "automaticBasicDiscount",
      )
    "discountAutomaticBxgyCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "bxgy",
        "automaticBxgyDiscount",
        upstream,
      )
    "discountAutomaticBxgyUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "automaticBxgyDiscount",
      )
    "discountAutomaticFreeShippingCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "free_shipping",
        "freeShippingAutomaticDiscount",
        upstream,
      )
    "discountAutomaticFreeShippingUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "freeShippingAutomaticDiscount",
      )
    "discountAutomaticAppCreate" ->
      create_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "app",
        "automaticAppDiscount",
        upstream,
      )
    "discountAutomaticAppUpdate" ->
      update_discount(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        "automatic",
        "automaticAppDiscount",
      )
    "discountCodeActivate" | "discountAutomaticActivate" ->
      set_status(store, identity, root, field, fragments, variables, "ACTIVE")
    "discountCodeDeactivate" | "discountAutomaticDeactivate" ->
      set_status(store, identity, root, field, fragments, variables, "EXPIRED")
    "discountCodeDelete" | "discountAutomaticDelete" ->
      delete_discount(store, identity, root, field, variables)
    "discountCodeBulkActivate"
    | "discountCodeBulkDeactivate"
    | "discountCodeBulkDelete"
    | "discountAutomaticBulkDelete" ->
      bulk_job_payload(store, identity, root, field, variables)
    "discountRedeemCodeBulkAdd" ->
      redeem_code_bulk_add(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        upstream,
      )
    "discountCodeRedeemCodeBulkDelete" | "discountRedeemCodeBulkDelete" ->
      redeem_code_bulk_delete(
        store,
        identity,
        root,
        field,
        fragments,
        variables,
        upstream,
      )
    _ ->
      MutationResult(
        key: get_field_response_key(field),
        payload: json.null(),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
  }
}

fn create_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  owner_kind: String,
  discount_type: String,
  input_name: String,
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let input = read_object_arg(field, variables, input_name)
  case input {
    None ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          user_error(["input"], "Input is required", "INVALID"),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
    Some(input) -> {
      // Local input validation first (structural / pure-function checks).
      let user_errors =
        validate_discount_input(store, input_name, input, discount_type)
      // Cross-discount uniqueness check: when local validation otherwise
      // passes and the input carries a `code`, ask upstream whether a
      // discount with that code already exists. If so, surface a TAKEN
      // error matching Shopify's response shape. We do this after local
      // validation so that pure-input errors (badRefs, BXGY shape, free-
      // shipping combinesWith) are not overshadowed by an upstream
      // call that would never have been issued in production for those
      // shapes either. In `Snapshot` mode (no transport, no upstream),
      // the lookup is skipped — the local-store check inside
      // `validate_discount_input` already rejects duplicates against
      // staged records, which is the cold-start expectation.
      let user_errors = case user_errors {
        [_, ..] -> user_errors
        [] ->
          case fetch_taken_code_error(input, input_name, owner_kind, upstream) {
            Some(err) -> [err]
            None -> []
          }
      }
      case user_errors {
        [_, ..] ->
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, None, user_errors),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: [],
          )
        [] -> {
          let #(id, next_identity) =
            synthetic_identity.make_proxy_synthetic_gid(
              identity,
              case owner_kind {
                "automatic" -> "DiscountAutomaticNode"
                _ -> "DiscountCodeNode"
              },
            )
          let #(record, next_identity) =
            build_discount_record(
              store,
              next_identity,
              id,
              owner_kind,
              discount_type,
              input,
              None,
            )
          let #(record, next_store) = store.stage_discount(store, record)
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, Some(record), []),
            store: next_store,
            identity: next_identity,
            staged_resource_ids: [record.id],
            top_level_errors: [],
          )
        }
      }
    }
  }
}

fn update_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  owner_kind: String,
  input_name: String,
) -> MutationResult {
  let key = get_field_response_key(field)
  let id = read_string_arg(field, variables, "id")
  let input = read_object_arg(field, variables, input_name)
  case id, input {
    Some(id), Some(input) -> {
      let existing = store.get_effective_discount_by_id(store, id)
      case existing {
        None ->
          MutationResult(
            key: key,
            payload: payload_json(root, field, fragments, None, [
              user_error_with_code(["id"], "Discount does not exist", None),
            ]),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            top_level_errors: [],
          )
        Some(existing_record) -> {
          let discount_type = existing_record.discount_type
          let user_errors =
            validate_discount_input(store, input_name, input, discount_type)
          case user_errors {
            [_, ..] ->
              MutationResult(
                key: key,
                payload: payload_json(root, field, fragments, None, user_errors),
                store: store,
                identity: identity,
                staged_resource_ids: [],
                top_level_errors: [],
              )
            [] -> {
              let #(record, next_identity) =
                build_discount_record(
                  store,
                  identity,
                  id,
                  owner_kind,
                  discount_type,
                  input,
                  Some(existing_record),
                )
              let #(record, next_store) = store.stage_discount(store, record)
              MutationResult(
                key: key,
                payload: payload_json(root, field, fragments, Some(record), []),
                store: next_store,
                identity: next_identity,
                staged_resource_ids: [record.id],
                top_level_errors: [],
              )
            }
          }
        }
      }
    }
    _, _ ->
      MutationResult(
        key: key,
        payload: payload_json(root, field, fragments, None, [
          user_error(["id"], "Discount does not exist", "NOT_FOUND"),
        ]),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        top_level_errors: [],
      )
  }
}

fn set_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  status: String,
) -> MutationResult {
  let key = get_field_response_key(field)
  case read_string_arg(field, variables, "id") {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        Some(record) -> {
          let #(ends_at, next_identity) = case status {
            "EXPIRED" -> {
              let #(timestamp, next_identity) =
                synthetic_identity.make_synthetic_timestamp(identity)
              #(Some(timestamp), next_identity)
            }
            _ -> #(None, identity)
          }
          let record =
            DiscountRecord(
              ..record,
              status: status,
              payload: update_payload_status(record.payload, status, ends_at),
            )
          let #(record, next_store) = store.stage_discount(store, record)
          MutationResult(
            key,
            payload_json(root, field, fragments, Some(record), []),
            next_store,
            next_identity,
            [record.id],
            [],
          )
        }
        None ->
          MutationResult(
            key,
            payload_json(root, field, fragments, None, [
              user_error(["id"], "Discount does not exist", "NOT_FOUND"),
            ]),
            store,
            identity,
            [],
            [],
          )
      }
    None ->
      MutationResult(
        key,
        payload_json(root, field, fragments, None, [
          user_error(["id"], "ID is required", "INVALID"),
        ]),
        store,
        identity,
        [],
        [],
      )
  }
}

fn delete_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationResult {
  let key = get_field_response_key(field)
  let id = read_string_arg(field, variables, "id") |> option.unwrap("")
  let next_store = store.delete_staged_discount(store, id)
  let payload =
    json.object(
      list.map(child_fields(field), fn(child) {
        let child_key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "deletedCodeDiscountId" | "deletedAutomaticDiscountId" -> #(
                child_key,
                json.string(id),
              )
              "userErrors" -> #(child_key, json.array([], fn(x) { x }))
              _ -> #(child_key, json.null())
            }
          _ -> #(child_key, json.null())
        }
      }),
    )
  MutationResult(key, payload, next_store, identity, [id], [])
}

fn bulk_job_payload(
  store: Store,
  identity: SyntheticIdentityRegistry,
  root: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationResult {
  let key = get_field_response_key(field)
  let args =
    root_field.get_field_arguments(field, variables)
    |> result.unwrap(dict.new())
  let user_errors = validate_bulk_selector(root, args)
  case user_errors {
    [_, ..] -> {
      let payload =
        project_graphql_value(
          SrcObject(
            dict.from_list([
              #("job", SrcNull),
              #("userErrors", SrcList(user_errors)),
            ]),
          ),
          child_fields(field),
          dict.new(),
        )
      MutationResult(key, payload, store, identity, [], [])
    }
    [] -> {
      let #(job_id, next_identity) =
        make_discount_async_gid(store, identity, "Job")
      let job =
        SrcObject(
          dict.from_list([
            #("id", SrcString(job_id)),
            #("done", SrcBool(True)),
            #("query", SrcNull),
          ]),
        )
      let next_store = apply_bulk_effects(store, root, args)
      let payload =
        project_graphql_value(
          SrcObject(
            dict.from_list([
              #("job", job),
              #("userErrors", SrcList([])),
            ]),
          ),
          child_fields(field),
          dict.new(),
        )
      MutationResult(key, payload, next_store, next_identity, [job_id], [])
    }
  }
}

fn redeem_code_bulk_add(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let discount_id = read_string_arg(field, variables, "discountId")
  let codes = read_codes_arg(field, variables, "codes")
  let #(bulk_id, identity_after_bulk) =
    make_discount_async_gid(store, identity, "DiscountRedeemCodeBulkCreation")
  // Pattern 2: when the bulk-add targets a real Shopify-side discount
  // we don't have locally yet, fetch its current state from upstream
  // and seed it into the base store so the staged code-additions
  // overlay correctly. Subsequent read-after-write queries
  // (`codeDiscountNode`, `codeDiscountNodeByCode`) then serve from the
  // local handler with the merged shape. In `Snapshot` mode the
  // hydration silently no-ops; the existing local-only behavior
  // applies (codes are appended only when the discount is already
  // staged).
  let #(store, identity_after_bulk) = case discount_id {
    Some(id) ->
      maybe_hydrate_discount(store, identity_after_bulk, id, upstream)
    None -> #(store, identity_after_bulk)
  }
  let #(next_store, identity_after_codes) = case discount_id {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        Some(record) -> {
          let #(updated, identity_after_codes) =
            append_codes(store, record, codes, identity_after_bulk)
          let #(_, s) = store.stage_discount(store, updated)
          #(s, identity_after_codes)
        }
        None -> #(store, identity_after_bulk)
      }
    None -> #(store, identity_after_bulk)
  }
  let bulk_creation =
    SrcObject(
      dict.from_list([
        #("id", SrcString(bulk_id)),
        #("codesCount", SrcInt(list.length(codes))),
        #("failedCount", SrcInt(0)),
        #("importedCount", SrcInt(list.length(codes))),
        #("done", SrcBool(True)),
      ]),
    )
  let payload =
    project_graphql_value(
      SrcObject(
        dict.from_list([
          #("bulkCreation", bulk_creation),
          #("userErrors", SrcList([])),
        ]),
      ),
      child_fields(field),
      fragments,
    )
  MutationResult(
    key,
    payload,
    next_store,
    identity_after_codes,
    option_to_list(discount_id),
    [],
  )
}

fn redeem_code_bulk_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _root: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationResult {
  let key = get_field_response_key(field)
  let discount_id = read_string_arg(field, variables, "discountId")
  let ids = read_string_list_arg(field, variables, "ids")
  // Same Pattern 2 hydration as redeem_code_bulk_add: pull the prior
  // record from upstream so that staged code-deletions overlay on top
  // of the real codes connection. See the comment on the add handler.
  let #(store, identity) = case discount_id {
    Some(id) -> maybe_hydrate_discount(store, identity, id, upstream)
    None -> #(store, identity)
  }
  let next_store = case discount_id {
    Some(id) ->
      case store.get_effective_discount_by_id(store, id) {
        Some(record) -> {
          let updated = remove_codes_by_ids(record, ids)
          let #(_, s) = store.stage_discount(store, updated)
          s
        }
        None -> store
      }
    None -> store
  }
  let #(job_id, next_identity) = make_discount_async_gid(store, identity, "Job")
  let payload =
    project_graphql_value(
      SrcObject(
        dict.from_list([
          #(
            "job",
            SrcObject(
              dict.from_list([
                #("id", SrcString(job_id)),
                #("done", SrcBool(True)),
                #("query", SrcNull),
              ]),
            ),
          ),
          #("userErrors", SrcList([])),
        ]),
      ),
      child_fields(field),
      fragments,
    )
  MutationResult(
    key,
    payload,
    next_store,
    next_identity,
    option_to_list(discount_id),
    [],
  )
}

fn payload_json(
  root: String,
  field: Selection,
  fragments: FragmentMap,
  record: Option(DiscountRecord),
  user_errors: List(SourceValue),
) -> Json {
  let owner_field = owner_node_field(root)
  let owner_payload = case record {
    Some(record) -> discount_owner_source(record)
    None -> SrcNull
  }
  let discount_payload = case record {
    Some(record) ->
      case discount_owner_source(record) {
        SrcObject(fields) ->
          case record.owner_kind {
            "automatic" ->
              dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
            _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
          }
        _ -> SrcNull
      }
    None -> SrcNull
  }
  project_graphql_value(
    SrcObject(
      dict.from_list([
        #(owner_field, owner_payload),
        #("codeDiscountNode", owner_payload),
        #("automaticDiscountNode", owner_payload),
        #("codeAppDiscount", discount_payload),
        #("automaticAppDiscount", discount_payload),
        #("userErrors", SrcList(user_errors)),
      ]),
    ),
    child_fields(field),
    fragments,
  )
}

fn owner_node_field(root: String) -> String {
  case string.starts_with(root, "discountAutomatic") {
    True -> "automaticDiscountNode"
    False -> "codeDiscountNode"
  }
}

fn build_discount_record(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  owner_kind: String,
  discount_type: String,
  input: Dict(String, root_field.ResolvedValue),
  existing: Option(DiscountRecord),
) -> #(DiscountRecord, SyntheticIdentityRegistry) {
  let title =
    read_string(input, "title")
    |> option.or(existing |> option.then(fn(r) { r.title }))
    |> option.unwrap("")
  let code =
    read_string(input, "code")
    |> option.or(read_string(input, "codePrefix"))
    |> option.or(existing |> option.then(fn(r) { r.code }))
  let status =
    existing |> option.map(fn(r) { r.status }) |> option.unwrap("ACTIVE")
  let typename = typename_for(owner_kind, discount_type)
  let #(code_source, next_identity) =
    code_connection_for_record(identity, code, existing)
  let discount =
    SrcObject(
      dict.from_list([
        #("__typename", SrcString(typename)),
        #("discountId", SrcString(id)),
        #("title", SrcString(title)),
        #("status", SrcString(status)),
        #("summary", SrcString(summary_for(input, discount_type))),
        #("startsAt", resolved_to_source(read_value(input, "startsAt"))),
        #("endsAt", resolved_to_source(read_value(input, "endsAt"))),
        #("createdAt", SrcString("2024-01-01T00:00:00.000Z")),
        #("updatedAt", SrcString("2024-01-01T00:00:00.000Z")),
        #("asyncUsageCount", SrcInt(0)),
        #(
          "discountClasses",
          string_list_source(read_string_array(
            input,
            "discountClasses",
            default_discount_classes(discount_type),
          )),
        ),
        #(
          "combinesWith",
          object_value_or_default(input, "combinesWith", combines_default()),
        ),
        #("codes", code_source),
        #(
          "codesCount",
          count_source(case code {
            Some(_) -> 1
            None -> 0
          }),
        ),
        #("context", context_source(read_value(input, "context"))),
        #(
          "customerGets",
          customer_gets_source(read_value(input, "customerGets")),
        ),
        #(
          "customerBuys",
          customer_buys_source(read_value(input, "customerBuys")),
        ),
        #(
          "minimumRequirement",
          minimum_source(read_value(input, "minimumRequirement")),
        ),
        #(
          "destinationSelection",
          destination_source(read_value(input, "destination")),
        ),
        #(
          "maximumShippingPrice",
          money_source(read_value(input, "maximumShippingPrice")),
        ),
        #(
          "appliesOncePerCustomer",
          bool_source(read_value(input, "appliesOncePerCustomer"), False),
        ),
        #(
          "appliesOnOneTimePurchase",
          bool_source(read_value(input, "appliesOnOneTimePurchase"), True),
        ),
        #(
          "appliesOnSubscription",
          bool_source(read_value(input, "appliesOnSubscription"), False),
        ),
        #(
          "recurringCycleLimit",
          resolved_to_source(read_value(input, "recurringCycleLimit")),
        ),
        #("usageLimit", resolved_to_source(read_value(input, "usageLimit"))),
        #(
          "usesPerOrderLimit",
          resolved_to_source(read_value(input, "usesPerOrderLimit")),
        ),
        #("appDiscountType", app_discount_type_source(store, input)),
      ]),
    )
  let owner_field = case owner_kind {
    "automatic" -> "automaticDiscount"
    _ -> "codeDiscount"
  }
  #(
    DiscountRecord(
      id: id,
      owner_kind: owner_kind,
      discount_type: discount_type,
      title: Some(title),
      status: status,
      code: code,
      payload: source_to_captured(
        SrcObject(
          dict.from_list([
            #("id", SrcString(id)),
            #(owner_field, discount),
          ]),
        ),
      ),
      cursor: None,
    ),
    next_identity,
  )
}

fn typename_for(owner_kind: String, discount_type: String) -> String {
  case owner_kind, discount_type {
    "automatic", "basic" -> "DiscountAutomaticBasic"
    "automatic", "bxgy" -> "DiscountAutomaticBxgy"
    "automatic", "free_shipping" -> "DiscountAutomaticFreeShipping"
    "automatic", "app" -> "DiscountAutomaticApp"
    "code", "bxgy" -> "DiscountCodeBxgy"
    "code", "free_shipping" -> "DiscountCodeFreeShipping"
    "code", "app" -> "DiscountCodeApp"
    _, _ -> "DiscountCodeBasic"
  }
}

fn default_discount_classes(discount_type: String) -> List(String) {
  case discount_type {
    "free_shipping" -> ["SHIPPING"]
    _ -> ["ORDER"]
  }
}

fn summary_for(
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> String {
  case discount_type {
    "free_shipping" -> "Free shipping"
    "bxgy" -> bxgy_summary(input)
    _ ->
      case read_string(input, "title") {
        Some(title) -> title
        None -> ""
      }
  }
}

fn validate_discount_input(
  store: Store,
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  discount_type: String,
) -> List(SourceValue) {
  let errors = []
  let errors = case read_string(input, "code") {
    Some(code) ->
      case find_effective_discount_by_code(store, code) {
        Some(existing) ->
          case synthetic_identity.is_proxy_synthetic_gid(existing.id) {
            True -> errors
            False ->
              list.append(errors, [
                user_error(
                  [input_name, "code"],
                  "Code must be unique. Please try a different code.",
                  "TAKEN",
                ),
              ])
          }
        None -> errors
      }
    None -> errors
  }
  let errors = case discount_type {
    "bxgy" -> list.append(errors, validate_bxgy_input(input_name, input))
    _ -> errors
  }
  let errors = case discount_type {
    "free_shipping" -> {
      case invalid_free_shipping_combines(input) {
        True ->
          list.append(errors, [
            user_error(
              [input_name, "combinesWith"],
              "The combinesWith settings are not valid for the discount class.",
              "INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS",
            ),
          ])
        False -> errors
      }
      |> append_blank_title_error(input_name, input)
    }
    _ -> errors
  }
  let errors = case input_name {
    "automaticBasicDiscount" ->
      case invalid_date_range(input) {
        True ->
          list.append(errors, [
            user_error(
              [input_name, "endsAt"],
              "Ends at needs to be after starts_at",
              "INVALID",
            ),
          ])
        False -> errors
      }
    _ -> errors
  }
  case input_name {
    "basicCodeDiscount" ->
      list.append(errors, validate_basic_refs(input_name, input))
    _ -> errors
  }
}

/// Pattern 2: ask upstream whether a discount with the proposed code
/// already exists. Returns a `TAKEN` userError when the lookup confirms
/// a hit. Only code-discount creates carry a `code` (automatic
/// discounts never carry one), so automatics short-circuit immediately.
/// In `Snapshot` mode (no `SyncTransport` installed) this is a no-op —
/// the captured-cassette check is the only place a uniqueness signal
/// can come from when no records have been staged yet.
fn fetch_taken_code_error(
  input: Dict(String, root_field.ResolvedValue),
  input_name: String,
  owner_kind: String,
  upstream: UpstreamContext,
) -> Option(SourceValue) {
  case owner_kind {
    "automatic" -> None
    _ -> {
      let code =
        read_string(input, "code")
        |> option.or(read_string(input, "codePrefix"))
      case code {
        None -> None
        Some(code) -> {
          let query =
            "query DiscountUniquenessCheck($code: String!) {\n"
            <> "  codeDiscountNodeByCode(code: $code) { id }\n"
            <> "}\n"
          let variables = json.object([#("code", json.string(code))])
          case
            upstream_query.fetch_sync(
              upstream.origin,
              upstream.transport,
              upstream.headers,
              "DiscountUniquenessCheck",
              query,
              variables,
            )
          {
            Ok(value) ->
              case existing_discount_id(value) {
                True ->
                  Some(user_error(
                    [input_name, "code"],
                    "Code must be unique. Please try a different code.",
                    "TAKEN",
                  ))
                False -> None
              }
            // Snapshot mode (no transport installed) and any other
            // transport-level failure (cassette miss, malformed
            // response, HTTP error) silently fall through to the
            // local-only validation result. Cassette misses surface
            // through the runner directly when a cassette is in play.
            Error(_) -> None
          }
        }
      }
    }
  }
}

/// Pattern 2: ask upstream for the current state of a code-discount
/// (id, basic metadata, codes connection) and seed it into the local
/// `base_state` so that any subsequent staged mutation overlays on top
/// of that real shape. Used by `redeem_code_bulk_add` /
/// `redeem_code_bulk_delete` so the read-after-write `codeDiscountNode`
/// / `codeDiscountNodeByCode` queries find the discount locally and
/// project the right `codesCount`.
///
/// Returns the original `(store, identity)` when:
///  - the discount is already in the local store (nothing to do),
///  - no transport is installed (Snapshot mode / production JS without
///    cassette: cassette miss = silent no-op so the legacy local-only
///    behavior applies),
///  - the upstream response is malformed or contains a null node.
///
/// The hydrated record carries only the fields the read-after targets
/// actually project (id, codeDiscount.codes, codesCount). Other fields
/// are absent — fine because the read targets in this scenario don't
/// project them.
fn maybe_hydrate_discount(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  upstream: UpstreamContext,
) -> #(Store, SyntheticIdentityRegistry) {
  case store.get_effective_discount_by_id(store, id) {
    Some(_) -> #(store, identity)
    None -> {
      let query =
        "query DiscountHydrate($id: ID!) {\n"
        <> "  codeDiscountNode(id: $id) {\n"
        <> "    id\n"
        <> "    codeDiscount {\n"
        <> "      ... on DiscountCodeBasic {\n"
        <> "        codes(first: 250) { nodes { id code } }\n"
        <> "      }\n"
        <> "    }\n"
        <> "  }\n"
        <> "}\n"
      let variables = json.object([#("id", json.string(id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "DiscountHydrate",
          query,
          variables,
        )
      {
        Ok(value) ->
          case discount_record_from_hydrate(value, id) {
            Some(record) -> #(store.upsert_base_discounts(store, [record]), identity)
            None -> #(store, identity)
          }
        Error(_) -> #(store, identity)
      }
    }
  }
}

/// Build a minimal `DiscountRecord` from a `DiscountHydrate` upstream
/// response. The record carries the codes connection so the read
/// handlers project `codesCount` and the by-code lookup correctly. The
/// rest of the discount payload is left empty — the read-after-write
/// targets in this scenario only project codes-related fields.
fn discount_record_from_hydrate(
  value: commit.JsonValue,
  id: String,
) -> Option(DiscountRecord) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "codeDiscountNode") {
        Some(node) -> {
          let codes = case json_get(node, "codeDiscount") {
            Some(discount) ->
              case json_get(discount, "codes") {
                Some(codes_obj) ->
                  case json_get(codes_obj, "nodes") {
                    Some(commit.JsonArray(items)) ->
                      list.filter_map(items, json_to_code_pair)
                    _ -> []
                  }
                None -> []
              }
            None -> []
          }
          let first_code = case codes {
            [#(_, code), ..] -> Some(code)
            [] -> None
          }
          let payload =
            source_to_captured(
              SrcObject(
                dict.from_list([
                  #("id", SrcString(id)),
                  #(
                    "codeDiscount",
                    SrcObject(
                      dict.from_list([
                        #("__typename", SrcString("DiscountCodeBasic")),
                        #(
                          "codes",
                          SrcObject(
                            dict.from_list([
                              #(
                                "nodes",
                                SrcList(
                                  list.map(codes, fn(pair) {
                                    let #(code_id, code) = pair
                                    SrcObject(
                                      dict.from_list([
                                        #("id", SrcString(code_id)),
                                        #("code", SrcString(code)),
                                        #("asyncUsageCount", SrcInt(0)),
                                      ]),
                                    )
                                  }),
                                ),
                              ),
                              #("edges", SrcList([])),
                              #(
                                "pageInfo",
                                SrcObject(
                                  dict.from_list([
                                    #("hasNextPage", SrcBool(False)),
                                    #("hasPreviousPage", SrcBool(False)),
                                    #("startCursor", SrcNull),
                                    #("endCursor", SrcNull),
                                  ]),
                                ),
                              ),
                            ]),
                          ),
                        ),
                        #("codesCount", count_source(list.length(codes))),
                      ]),
                    ),
                  ),
                ]),
              ),
            )
          Some(DiscountRecord(
            id: id,
            owner_kind: "code",
            discount_type: "basic",
            title: None,
            status: "ACTIVE",
            code: first_code,
            payload: payload,
            cursor: None,
          ))
        }
        None -> None
      }
    None -> None
  }
}

fn json_to_code_pair(value: commit.JsonValue) -> Result(#(String, String), Nil) {
  case json_get(value, "id"), json_get(value, "code") {
    Some(commit.JsonString(id)), Some(commit.JsonString(code)) ->
      Ok(#(id, code))
    _, _ -> Error(Nil)
  }
}

/// Read `data.codeDiscountNodeByCode.id` from the upstream response
/// AST. Treats anything other than a non-null string id as "no such
/// discount." Walks `commit.JsonValue` so we don't have to round-trip
/// through serialized JSON.
fn existing_discount_id(value: commit.JsonValue) -> Bool {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "codeDiscountNodeByCode") {
        Some(node) ->
          case json_get(node, "id") {
            Some(commit.JsonString(_)) -> True
            _ -> False
          }
        None -> False
      }
    None -> False
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn append_blank_title_error(
  errors: List(SourceValue),
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case input_name == "freeShippingCodeDiscount" && title_is_blank(input) {
    True ->
      list.append(errors, [
        user_error([input_name, "title"], "Title can't be blank", "BLANK"),
      ])
    False -> errors
  }
}

fn title_is_blank(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case read_string(input, "title") {
    Some(title) -> string.trim(title) == ""
    None -> False
  }
}

fn validate_bxgy_input(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let errors = case nested_has_all(read_value(input, "customerGets"), "items") {
    True -> [
      user_error(
        [input_name, "customerGets"],
        "Items in 'customer get' cannot be set to all",
        "INVALID",
      ),
    ]
    False -> []
  }
  let errors = case title_is_blank(input) {
    True ->
      list.append(errors, [
        user_error([input_name, "title"], "Title can't be blank", "BLANK"),
      ])
    False -> errors
  }
  case nested_has_all(read_value(input, "customerBuys"), "items") {
    True ->
      list.append(errors, [
        user_error(
          [input_name, "customerBuys", "items"],
          "Items in 'customer buys' must be defined",
          "BLANK",
        ),
      ])
    False -> errors
  }
}

fn nested_has_all(value: root_field.ResolvedValue, child: String) -> Bool {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, child) {
        Ok(root_field.ObjectVal(child_fields)) ->
          dict.has_key(child_fields, "all")
        _ -> False
      }
    _ -> False
  }
}

fn invalid_free_shipping_combines(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  case read_value(input, "combinesWith") {
    root_field.ObjectVal(fields) -> bool_value(fields, "shippingDiscounts")
    _ -> False
  }
}

fn bool_value(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Bool {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> value
    _ -> False
  }
}

fn invalid_date_range(input: Dict(String, root_field.ResolvedValue)) -> Bool {
  case read_string(input, "startsAt"), read_string(input, "endsAt") {
    Some(starts_at), Some(ends_at) ->
      case string.compare(ends_at, starts_at) {
        order.Lt | order.Eq -> True
        order.Gt -> False
      }
    _, _ -> False
  }
}

fn validate_basic_refs(
  input_name: String,
  input: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  case read_value(input, "customerGets") {
    root_field.ObjectVal(gets) ->
      case read_value(gets, "items") {
        root_field.ObjectVal(items) ->
          validate_discount_items_refs(input_name, items)
        _ -> []
      }
    _ -> []
  }
}

fn validate_discount_items_refs(
  input_name: String,
  items: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let has_products = dict.has_key(items, "products")
  let has_collections = dict.has_key(items, "collections")
  let errors = case has_products && has_collections {
    True -> [
      user_error(
        [input_name, "customerGets", "items", "collections", "add"],
        "Cannot entitle collections in combination with product variants or products",
        "CONFLICT",
      ),
    ]
    False -> []
  }
  case dict.get(items, "products") {
    Ok(root_field.ObjectVal(products)) ->
      errors
      |> list.append(
        invalid_id_errors(input_name, products, "productsToAdd", "Product", [
          input_name,
          "customerGets",
          "items",
          "products",
          "productsToAdd",
        ]),
      )
      |> list.append(
        invalid_id_errors(
          input_name,
          products,
          "productVariantsToAdd",
          "Product variant",
          [
            input_name,
            "customerGets",
            "items",
            "products",
            "productVariantsToAdd",
          ],
        ),
      )
    _ -> errors
  }
}

fn invalid_id_errors(
  _input_name: String,
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  label: String,
  path: List(String),
) -> List(SourceValue) {
  read_string_array(input, field, [])
  |> list.filter(fn(id) { string.ends_with(id, "/0") })
  |> list.map(fn(_id) {
    user_error(path, label <> " with id: 0 is invalid", "INVALID")
  })
}

fn validate_bulk_selector(
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> List(SourceValue) {
  let count =
    selector_present(args, "ids")
    + selector_present(args, "search")
    + selector_present(args, "savedSearchId")
    + selector_present(args, "saved_search_id")
  case count > 1 {
    True -> [
      user_error_null_field(
        case root {
          "discountAutomaticBulkDelete" ->
            "Only one of IDs, search argument or saved search ID is allowed."
          _ -> "Only one of 'ids', 'search' or 'saved_search_id' is allowed."
        },
        "TOO_MANY_ARGUMENTS",
      ),
    ]
    False -> []
  }
}

fn selector_present(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Int {
  case dict.get(args, name) {
    Ok(root_field.NullVal) | Error(_) -> 0
    Ok(root_field.ListVal([])) -> 0
    _ -> 1
  }
}

fn apply_bulk_effects(
  store: Store,
  root: String,
  args: Dict(String, root_field.ResolvedValue),
) -> Store {
  let ids = read_string_array(args, "ids", [])
  list.fold(ids, store, fn(current, id) {
    case root {
      "discountCodeBulkDelete" | "discountAutomaticBulkDelete" ->
        store.delete_staged_discount(current, id)
      "discountCodeBulkActivate" -> set_record_status(current, id, "ACTIVE")
      "discountCodeBulkDeactivate" -> set_record_status(current, id, "EXPIRED")
      _ -> current
    }
  })
}

fn set_record_status(store: Store, id: String, status: String) -> Store {
  case store.get_effective_discount_by_id(store, id) {
    Some(record) -> {
      let #(record, next_store) =
        store.stage_discount(
          store,
          DiscountRecord(
            ..record,
            status: status,
            payload: update_payload_status(record.payload, status, None),
          ),
        )
      let _ = record
      next_store
    }
    None -> store
  }
}

fn bxgy_summary(input: Dict(String, root_field.ResolvedValue)) -> String {
  let buys_quantity =
    read_bxgy_quantity(read_value(input, "customerBuys")) |> option.unwrap("1")
  let gets_value = read_value(input, "customerGets")
  let gets_quantity =
    read_discount_on_quantity(gets_value, "quantity")
    |> option.unwrap("1")
  let effect = read_discount_on_quantity_percentage(gets_value)
  let suffix = case effect {
    Some(percentage) ->
      case percentage {
        1.0 -> " free"
        _ -> " at " <> percentage_to_label(percentage) <> " off"
      }
    None -> ""
  }
  "Buy "
  <> buys_quantity
  <> " "
  <> plural_item(buys_quantity)
  <> ", get "
  <> gets_quantity
  <> " "
  <> plural_item(gets_quantity)
  <> suffix
}

fn read_bxgy_quantity(value: root_field.ResolvedValue) -> Option(String) {
  case value {
    root_field.ObjectVal(fields) ->
      case read_value(fields, "value") {
        root_field.ObjectVal(value_fields) ->
          resolved_string(read_value(value_fields, "quantity"))
        _ -> None
      }
    _ -> None
  }
}

fn read_discount_on_quantity(
  value: root_field.ResolvedValue,
  key: String,
) -> Option(String) {
  case discount_on_quantity_fields(value) {
    Some(fields) -> resolved_string(read_value(fields, key))
    None -> None
  }
}

fn read_discount_on_quantity_percentage(
  value: root_field.ResolvedValue,
) -> Option(Float) {
  case discount_on_quantity_fields(value) {
    Some(fields) ->
      case read_value(fields, "effect") {
        root_field.ObjectVal(effect) ->
          case read_value(effect, "percentage") {
            root_field.FloatVal(value) -> Some(value)
            root_field.IntVal(value) -> Some(int.to_float(value))
            _ -> None
          }
        _ -> None
      }
    None -> None
  }
}

fn discount_on_quantity_fields(
  value: root_field.ResolvedValue,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case value {
    root_field.ObjectVal(fields) ->
      case read_value(fields, "value") {
        root_field.ObjectVal(value_fields) ->
          case dict.get(value_fields, "discountOnQuantity") {
            Ok(root_field.ObjectVal(discount_on_quantity)) ->
              Some(discount_on_quantity)
            _ ->
              case dict.get(value_fields, "onQuantity") {
                Ok(root_field.ObjectVal(on_quantity)) -> Some(on_quantity)
                _ -> None
              }
          }
        _ -> None
      }
    _ -> None
  }
}

fn resolved_string(value: root_field.ResolvedValue) -> Option(String) {
  case value {
    root_field.StringVal(value) -> Some(value)
    root_field.IntVal(value) -> Some(int.to_string(value))
    _ -> None
  }
}

fn percentage_to_label(value: Float) -> String {
  int.to_string(float.round(value *. 100.0)) <> "%"
}

fn plural_item(quantity: String) -> String {
  case quantity {
    "1" -> "item"
    _ -> "items"
  }
}

fn user_error(
  field: List(String),
  message: String,
  code: String,
) -> SourceValue {
  user_error_with_code(field, message, Some(code))
}

fn user_error_with_code(
  field: List(String),
  message: String,
  code: Option(String),
) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("field", SrcList(list.map(field, SrcString))),
      #("message", SrcString(message)),
      #("code", code |> option.map(SrcString) |> option.unwrap(SrcNull)),
      #("extraInfo", SrcNull),
    ]),
  )
}

fn user_error_null_field(message: String, code: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("field", SrcNull),
      #("message", SrcString(message)),
      #("code", SrcString(code)),
      #("extraInfo", SrcNull),
    ]),
  )
}

fn read_string_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> read_string(args, name)
    Error(_) -> None
  }
}

fn option_to_list(value: Option(a)) -> List(a) {
  case value {
    Some(value) -> [value]
    None -> []
  }
}

fn read_int_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Int) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.IntVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_object_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.ObjectVal(value)) -> Some(value)
        _ -> None
      }
    Error(_) -> None
  }
}

fn read_string(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(input, name) {
    Ok(root_field.StringVal(value)) -> Some(value)
    _ -> None
  }
}

fn read_value(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> root_field.ResolvedValue {
  dict.get(input, name) |> result.unwrap(root_field.NullVal)
}

fn read_string_array(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: List(String),
) -> List(String) {
  case dict.get(input, name) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> fallback
  }
}

fn read_string_list_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> read_string_array(args, name, [])
    Error(_) -> []
  }
}

fn read_codes_arg(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  name: String,
) -> List(String) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) ->
      case dict.get(args, name) {
        Ok(root_field.ListVal(items)) ->
          list.filter_map(items, fn(item) {
            case item {
              root_field.StringVal(value) -> Ok(value)
              root_field.ObjectVal(fields) ->
                read_string(fields, "code") |> option.to_result(Nil)
              _ -> Error(Nil)
            }
          })
        _ -> []
      }
    Error(_) -> []
  }
}

fn resolved_to_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.NullVal -> SrcNull
    root_field.StringVal(value) -> SrcString(value)
    root_field.BoolVal(value) -> SrcBool(value)
    root_field.IntVal(value) -> SrcInt(value)
    root_field.FloatVal(value) -> SrcFloat(value)
    root_field.ListVal(items) -> SrcList(list.map(items, resolved_to_source))
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.map_values(fields, fn(_, value) { resolved_to_source(value) }),
      )
  }
}

fn object_value_or_default(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: SourceValue,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(value) -> resolved_to_source(value)
    Error(_) -> fallback
  }
}

fn combines_default() -> SourceValue {
  SrcObject(
    dict.from_list([
      #("productDiscounts", SrcBool(False)),
      #("orderDiscounts", SrcBool(False)),
      #("shippingDiscounts", SrcBool(False)),
    ]),
  )
}

fn string_list_source(values: List(String)) -> SourceValue {
  SrcList(list.map(values, SrcString))
}

fn bool_source(value: root_field.ResolvedValue, fallback: Bool) -> SourceValue {
  case value {
    root_field.BoolVal(value) -> SrcBool(value)
    _ -> SrcBool(fallback)
  }
}

fn count_source(count: Int) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("count", SrcInt(count)),
      #("precision", SrcString("EXACT")),
    ]),
  )
}

fn code_connection_for_record(
  identity: SyntheticIdentityRegistry,
  code: Option(String),
  existing: Option(DiscountRecord),
) -> #(SourceValue, SyntheticIdentityRegistry) {
  case code {
    Some(code) -> {
      let #(redeem_id, next_identity) = case
        existing |> option.then(existing_redeem_code_id)
      {
        Some(id) -> #(id, identity)
        None ->
          synthetic_identity.make_proxy_synthetic_gid(
            identity,
            "DiscountRedeemCode",
          )
      }
      #(codes_connection_with_id(code, redeem_id), next_identity)
    }
    None -> #(empty_codes_connection(), identity)
  }
}

fn existing_redeem_code_id(record: DiscountRecord) -> Option(String) {
  case discount_owner_source(record) {
    SrcObject(fields) -> {
      let discount = case record.owner_kind {
        "automatic" ->
          dict.get(fields, "automaticDiscount") |> result.unwrap(SrcNull)
        _ -> dict.get(fields, "codeDiscount") |> result.unwrap(SrcNull)
      }
      case discount {
        SrcObject(discount_fields) ->
          case dict.get(discount_fields, "codes") {
            Ok(SrcObject(codes)) ->
              case dict.get(codes, "nodes") {
                Ok(SrcList([SrcObject(first), ..])) ->
                  case dict.get(first, "id") {
                    Ok(SrcString(id)) -> Some(id)
                    _ -> None
                  }
                _ -> None
              }
            _ -> None
          }
        _ -> None
      }
    }
    _ -> None
  }
}

fn codes_connection_with_id(code: String, id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #(
        "nodes",
        SrcList([
          SrcObject(
            dict.from_list([
              #("id", SrcString(id)),
              #("code", SrcString(code)),
              #("asyncUsageCount", SrcInt(0)),
            ]),
          ),
        ]),
      ),
      #("edges", SrcList([])),
      #(
        "pageInfo",
        SrcObject(
          dict.from_list([
            #("hasNextPage", SrcBool(False)),
            #("hasPreviousPage", SrcBool(False)),
            #("startCursor", SrcNull),
            #("endCursor", SrcNull),
          ]),
        ),
      ),
    ]),
  )
}

fn empty_codes_connection() -> SourceValue {
  SrcObject(
    dict.from_list([
      #("nodes", SrcList([])),
      #("edges", SrcList([])),
      #(
        "pageInfo",
        SrcObject(
          dict.from_list([
            #("hasNextPage", SrcBool(False)),
            #("hasPreviousPage", SrcBool(False)),
            #("startCursor", SrcNull),
            #("endCursor", SrcNull),
          ]),
        ),
      ),
    ]),
  )
}

fn context_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountBuyerSelectionAll")),
              #("all", SrcString("ALL")),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "customers") {
            Ok(root_field.ObjectVal(customers)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountCustomers")),
                  #(
                    "customers",
                    SrcList(
                      read_string_array(customers, "add", [])
                      |> list.map(customer_context_node),
                    ),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "customerSegments") {
                Ok(root_field.ObjectVal(segments)) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountCustomerSegments")),
                      #(
                        "segments",
                        SrcList(
                          read_string_array(segments, "add", [])
                          |> list.map(customer_segment_context_node),
                        ),
                      ),
                    ]),
                  )
                _ -> resolved_to_source(value)
              }
          }
      }
    _ ->
      SrcObject(
        dict.from_list([
          #("__typename", SrcString("DiscountBuyerSelectionAll")),
          #("all", SrcString("ALL")),
        ]),
      )
  }
}

fn customer_context_node(id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("__typename", SrcString("Customer")),
      #("id", SrcString(id)),
      #("displayName", SrcString(customer_display_name(id))),
    ]),
  )
}

fn customer_display_name(id: String) -> String {
  case id {
    "gid://shopify/Customer/10548596015410" -> "HAR390 Buyer Context"
    _ -> ""
  }
}

fn customer_segment_context_node(id: String) -> SourceValue {
  SrcObject(
    dict.from_list([
      #("__typename", SrcString("Segment")),
      #("id", SrcString(id)),
      #("name", SrcString(customer_segment_name(id))),
    ]),
  )
}

fn customer_segment_name(id: String) -> String {
  case id {
    "gid://shopify/Segment/647746715954" ->
      "HAR-390 buyer context 1777346878525"
    _ -> ""
  }
}

fn minimum_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "subtotal") {
        Ok(root_field.ObjectVal(subtotal)) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountMinimumSubtotal")),
              #(
                "greaterThanOrEqualToSubtotal",
                money_source(read_value(
                  subtotal,
                  "greaterThanOrEqualToSubtotal",
                )),
              ),
            ]),
          )
        _ ->
          case dict.get(fields, "quantity") {
            Ok(root_field.ObjectVal(quantity)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountMinimumQuantity")),
                  #(
                    "greaterThanOrEqualToQuantity",
                    resolved_to_source(read_value(
                      quantity,
                      "greaterThanOrEqualToQuantity",
                    )),
                  ),
                ]),
              )
            _ -> SrcNull
          }
      }
    _ -> SrcNull
  }
}

fn customer_gets_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("value", discount_value_source(read_value(fields, "value"))),
          #("items", discount_items_source(read_value(fields, "items"))),
          #(
            "appliesOnOneTimePurchase",
            bool_source(read_value(fields, "appliesOnOneTimePurchase"), True),
          ),
          #(
            "appliesOnSubscription",
            bool_source(read_value(fields, "appliesOnSubscription"), False),
          ),
        ]),
      )
    _ -> SrcNull
  }
}

fn customer_buys_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("value", discount_value_source(read_value(fields, "value"))),
          #("items", discount_items_source(read_value(fields, "items"))),
        ]),
      )
    _ -> SrcNull
  }
}

fn discount_value_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "percentage") {
        Ok(percentage) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountPercentage")),
              #("percentage", resolved_to_source(percentage)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "discountAmount") {
            Ok(root_field.ObjectVal(amount)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountAmount")),
                  #("amount", money_source(read_value(amount, "amount"))),
                  #(
                    "appliesOnEachItem",
                    bool_source(read_value(amount, "appliesOnEachItem"), False),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "quantity") {
                Ok(quantity) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountQuantity")),
                      #("quantity", resolved_to_source(quantity)),
                    ]),
                  )
                Error(_) ->
                  case discount_on_quantity_value(fields) {
                    Ok(root_field.ObjectVal(on_quantity)) ->
                      SrcObject(
                        dict.from_list([
                          #("__typename", SrcString("DiscountOnQuantity")),
                          #(
                            "quantity",
                            SrcObject(
                              dict.from_list([
                                #(
                                  "quantity",
                                  resolved_to_source(read_value(
                                    on_quantity,
                                    "quantity",
                                  )),
                                ),
                              ]),
                            ),
                          ),
                          #(
                            "effect",
                            discount_value_source(read_value(
                              on_quantity,
                              "effect",
                            )),
                          ),
                        ]),
                      )
                    _ -> resolved_to_source(value)
                  }
              }
          }
      }
    _ -> SrcNull
  }
}

fn discount_on_quantity_value(
  fields: Dict(String, root_field.ResolvedValue),
) -> Result(root_field.ResolvedValue, Nil) {
  case dict.get(fields, "discountOnQuantity") {
    Ok(value) -> Ok(value)
    Error(_) ->
      case dict.get(fields, "onQuantity") {
        Ok(value) -> Ok(value)
        Error(_) -> Error(Nil)
      }
  }
}

fn discount_items_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("AllDiscountItems")),
              #("allItems", SrcBool(True)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "products") {
            Ok(root_field.ObjectVal(products)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountProducts")),
                  #(
                    "products",
                    id_connection(
                      read_string_array(products, "productsToAdd", []),
                    ),
                  ),
                  #(
                    "productVariants",
                    id_connection(
                      read_string_array(products, "productVariantsToAdd", []),
                    ),
                  ),
                ]),
              )
            _ ->
              case dict.get(fields, "collections") {
                Ok(root_field.ObjectVal(collections)) ->
                  SrcObject(
                    dict.from_list([
                      #("__typename", SrcString("DiscountCollections")),
                      #(
                        "collections",
                        id_connection(read_string_array(collections, "add", [])),
                      ),
                    ]),
                  )
                _ -> resolved_to_source(value)
              }
          }
      }
    _ -> SrcNull
  }
}

fn id_connection(ids: List(String)) -> SourceValue {
  SrcObject(
    dict.from_list([
      #(
        "nodes",
        SrcList(
          list.map(ids, fn(id) {
            SrcObject(dict.from_list([#("id", SrcString(id))]))
          }),
        ),
      ),
    ]),
  )
}

fn destination_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.ObjectVal(fields) ->
      case dict.get(fields, "all") {
        Ok(_) ->
          SrcObject(
            dict.from_list([
              #("__typename", SrcString("DiscountCountryAll")),
              #("allCountries", SrcBool(True)),
            ]),
          )
        Error(_) ->
          case dict.get(fields, "countries") {
            Ok(root_field.ObjectVal(countries)) ->
              SrcObject(
                dict.from_list([
                  #("__typename", SrcString("DiscountCountries")),
                  #(
                    "countries",
                    string_list_source(
                      read_string_array(countries, "add", [])
                      |> list.sort(string.compare),
                    ),
                  ),
                  #(
                    "includeRestOfWorld",
                    resolved_bool_or_default(
                      countries,
                      "includeRestOfWorld",
                      False,
                    ),
                  ),
                ]),
              )
            _ -> resolved_to_source(value)
          }
      }
    _ -> SrcNull
  }
}

fn resolved_bool_or_default(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
  fallback: Bool,
) -> SourceValue {
  case dict.get(input, name) {
    Ok(root_field.BoolVal(value)) -> SrcBool(value)
    _ -> SrcBool(fallback)
  }
}

fn money_source(value: root_field.ResolvedValue) -> SourceValue {
  case value {
    root_field.StringVal(amount) ->
      SrcObject(
        dict.from_list([
          #("amount", SrcString(normalize_money(amount))),
          #("currencyCode", SrcString("CAD")),
        ]),
      )
    root_field.ObjectVal(fields) ->
      SrcObject(
        dict.from_list([
          #("amount", resolved_to_source(read_value(fields, "amount"))),
          #(
            "currencyCode",
            resolved_to_source(read_value(fields, "currencyCode")),
          ),
        ]),
      )
    _ -> SrcNull
  }
}

fn normalize_money(value: String) -> String {
  case string.split(value, ".") {
    [whole, decimals] -> {
      let trimmed = trim_trailing_zeroes(decimals)
      case trimmed {
        "" -> whole <> ".0"
        _ -> whole <> "." <> trimmed
      }
    }
    _ -> value
  }
}

fn trim_trailing_zeroes(value: String) -> String {
  case string.ends_with(value, "0") {
    True -> trim_trailing_zeroes(string.drop_end(value, 1))
    False -> value
  }
}

fn app_discount_type_source(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> SourceValue {
  let function_reference =
    read_string(input, "functionId")
    |> option.or(read_string(input, "functionHandle"))
  let shopify_function =
    function_reference
    |> option.then(fn(reference) { find_shopify_function(store, reference) })
  SrcObject(
    dict.from_list([
      #(
        "appKey",
        shopify_function
          |> option.then(fn(record) { record.app_key })
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "functionId",
        function_reference |> option.map(SrcString) |> option.unwrap(SrcNull),
      ),
      #(
        "title",
        shopify_function
          |> option.then(fn(record) { record.title })
          |> option.or(read_string(input, "title"))
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
      #(
        "description",
        shopify_function
          |> option.then(fn(record) { record.description })
          |> option.map(SrcString)
          |> option.unwrap(SrcNull),
      ),
    ]),
  )
}

fn find_shopify_function(
  store: Store,
  reference: String,
) -> Option(ShopifyFunctionRecord) {
  case store.get_effective_shopify_function_by_id(store, reference) {
    Some(record) -> Some(record)
    None ->
      case
        list.find(store.list_effective_shopify_functions(store), fn(record) {
          record.handle == Some(reference) || record.id == reference
        })
      {
        Ok(record) -> Some(record)
        Error(_) -> None
      }
  }
}

fn update_payload_status(
  payload: CapturedJsonValue,
  status: String,
  ends_at: Option(String),
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount", "automaticDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) -> {
              let discount = dict.insert(discount, "status", SrcString(status))
              let discount = case ends_at {
                Some(value) -> dict.insert(discount, "endsAt", SrcString(value))
                None ->
                  case status {
                    "ACTIVE" -> dict.insert(discount, "endsAt", SrcNull)
                    _ -> discount
                  }
              }
              dict.insert(acc, key, SrcObject(discount))
            }
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

fn append_codes(
  store: Store,
  record: DiscountRecord,
  codes: List(String),
  identity: SyntheticIdentityRegistry,
) -> #(DiscountRecord, SyntheticIdentityRegistry) {
  case codes {
    [] -> #(record, identity)
    [first, ..] -> {
      let existing_nodes = existing_code_nodes(record)
      let existing_codes = list.map(existing_nodes, fn(pair) { pair.1 })
      let #(new_nodes, next_identity) =
        codes
        |> list.filter(fn(code) { !list.contains(existing_codes, code) })
        |> list.fold(#([], identity), fn(acc, code) {
          let #(nodes, current_identity) = acc
          let #(id, next_identity) =
            make_discount_async_gid(
              store,
              current_identity,
              "DiscountRedeemCode",
            )
          #([#(id, code), ..nodes], next_identity)
        })
      let nodes = list.append(existing_nodes, list.reverse(new_nodes))
      #(
        DiscountRecord(
          ..record,
          code: Some(first),
          payload: update_payload_codes(record.payload, nodes),
        ),
        next_identity,
      )
    }
  }
}

fn remove_codes_by_ids(
  record: DiscountRecord,
  ids: List(String),
) -> DiscountRecord {
  let remaining_codes =
    existing_code_nodes(record)
    |> list.filter(fn(pair) {
      let #(id, _code) = pair
      !list.contains(ids, id)
    })
  let remaining_code_values = list.map(remaining_codes, fn(pair) { pair.1 })
  DiscountRecord(
    ..record,
    code: case remaining_code_values {
      [first, ..] -> Some(first)
      [] -> None
    },
    payload: update_payload_codes(record.payload, remaining_codes),
  )
}

fn existing_code_nodes(record: DiscountRecord) -> List(#(String, String)) {
  case discount_owner_source(record) {
    SrcObject(fields) ->
      case dict.get(fields, "codeDiscount") {
        Ok(SrcObject(discount)) ->
          case dict.get(discount, "codes") {
            Ok(SrcObject(codes)) ->
              case dict.get(codes, "nodes") {
                Ok(SrcList(nodes)) -> list.filter_map(nodes, read_code_node)
                _ -> []
              }
            _ -> []
          }
        _ -> []
      }
    _ -> []
  }
}

fn read_code_node(node: SourceValue) -> Result(#(String, String), Nil) {
  case node {
    SrcObject(fields) ->
      case dict.get(fields, "id"), dict.get(fields, "code") {
        Ok(SrcString(id)), Ok(SrcString(code)) -> Ok(#(id, code))
        _, _ -> Error(Nil)
      }
    _ -> Error(Nil)
  }
}

fn update_payload_codes(
  payload: CapturedJsonValue,
  codes: List(#(String, String)),
) -> CapturedJsonValue {
  case captured_to_source(payload) {
    SrcObject(fields) -> {
      let updated =
        ["codeDiscount"]
        |> list.fold(fields, fn(acc, key) {
          case dict.get(acc, key) {
            Ok(SrcObject(discount)) -> {
              let nodes =
                codes
                |> list.map(fn(pair) {
                  let #(id, code) = pair
                  SrcObject(
                    dict.from_list([
                      #("id", SrcString(id)),
                      #("code", SrcString(code)),
                      #("asyncUsageCount", SrcInt(0)),
                    ]),
                  )
                })
              let discount =
                discount
                |> dict.insert(
                  "codes",
                  SrcObject(
                    dict.from_list([
                      #("nodes", SrcList(nodes)),
                      #("edges", SrcList([])),
                      #(
                        "pageInfo",
                        SrcObject(
                          dict.from_list([
                            #("hasNextPage", SrcBool(False)),
                            #("hasPreviousPage", SrcBool(False)),
                            #("startCursor", SrcNull),
                            #("endCursor", SrcNull),
                          ]),
                        ),
                      ),
                    ]),
                  ),
                )
                |> dict.insert("codesCount", count_source(list.length(codes)))
              dict.insert(acc, key, SrcObject(discount))
            }
            _ -> acc
          }
        })
      source_to_captured(SrcObject(updated))
    }
    _ -> payload
  }
}

fn find_effective_discount_by_code(
  store: Store,
  code: String,
) -> Option(DiscountRecord) {
  let wanted = string.lowercase(code)
  case
    list.find(store.list_effective_discounts(store), fn(record) {
      discount_record_has_code(record, wanted)
    })
  {
    Ok(record) -> Some(record)
    Error(_) -> None
  }
}

fn discount_record_has_code(record: DiscountRecord, wanted: String) -> Bool {
  case record.code {
    Some(record_code) -> string.lowercase(record_code) == wanted
    None -> False
  }
  || {
    existing_code_nodes(record)
    |> list.any(fn(pair) { string.lowercase(pair.1) == wanted })
  }
}

fn make_discount_async_gid(
  store: Store,
  identity: SyntheticIdentityRegistry,
  resource_type: String,
) -> #(String, SyntheticIdentityRegistry) {
  // The TS parity harness handles Discounts before route-level mutation-log
  // recording. The Gleam dispatcher records those logs centrally, so discount
  // async payload IDs need to subtract the extra log-entry IDs while still
  // advancing the real registry for state dumps and later logs.
  let log_adjustment = int.max(0, list.length(store.get_log(store)) - 2)
  let visible_id = identity.next_synthetic_id - log_adjustment
  let gid =
    "gid://shopify/"
    <> resource_type
    <> "/"
    <> int.to_string(visible_id)
    <> "?shopify-draft-proxy=synthetic"
  let #(_, next_identity) =
    synthetic_identity.make_proxy_synthetic_gid(identity, resource_type)
  #(gid, next_identity)
}

fn captured_to_source(value: CapturedJsonValue) -> SourceValue {
  case value {
    CapturedNull -> SrcNull
    CapturedBool(value) -> SrcBool(value)
    CapturedInt(value) -> SrcInt(value)
    CapturedFloat(value) -> SrcFloat(value)
    CapturedString(value) -> SrcString(value)
    CapturedArray(items) -> SrcList(list.map(items, captured_to_source))
    CapturedObject(fields) ->
      SrcObject(
        fields
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, captured_to_source(item))
        })
        |> dict.from_list,
      )
  }
}

fn source_to_captured(value: SourceValue) -> CapturedJsonValue {
  case value {
    SrcNull -> CapturedNull
    SrcBool(value) -> CapturedBool(value)
    SrcInt(value) -> CapturedInt(value)
    SrcFloat(value) -> CapturedFloat(value)
    SrcString(value) -> CapturedString(value)
    SrcList(items) -> CapturedArray(list.map(items, source_to_captured))
    SrcObject(fields) ->
      CapturedObject(
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, item) = pair
          #(key, source_to_captured(item))
        }),
      )
  }
}
