//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}

import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result

import shopify_draft_proxy/graphql/ast.{type Selection, Field}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_field_response_key,
}

import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, RequiredArgument, single_root_log_draft,
  validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  field_arguments, inferred_user_error, read_string_arg, selection_children,
  serialize_user_error, user_error,
}
import shopify_draft_proxy/proxy/orders/serializers.{serialize_abandonment_node}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/state/iso_timestamp

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type AbandonmentDeliveryActivityRecord, type AbandonmentRecord,
  AbandonmentDeliveryActivityRecord, AbandonmentRecord, CapturedNull,
  CapturedObject, CapturedString,
}

@internal
pub fn handle_access_denied_guardrail(
  root_name: String,
  field: Selection,
) -> #(String, Json, List(Json), List(LogDraft)) {
  let key = get_field_response_key(field)
  let required_access = access_denied_required_access(root_name)
  let error = access_denied_error(root_name, required_access)
  let draft =
    single_root_log_draft(
      root_name,
      [],
      store_types.Failed,
      "orders",
      "stage-locally",
      Some(root_name <> " failed local access-denied guardrail."),
    )
  #(key, json.null(), [error], [draft])
}

@internal
pub fn access_denied_required_access(root_name: String) -> String {
  case root_name {
    "orderCreateManualPayment" ->
      "`write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field."
    "taxSummaryCreate" ->
      "`write_taxes` access scope. Also: The caller must be a tax calculations app and the relevant feature must be on."
    _ -> "`write_orders` access scope."
  }
}

@internal
pub fn access_denied_error(root_name: String, required_access: String) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Access denied for "
        <> root_name
        <> " field. Required access: "
        <> required_access,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("ACCESS_DENIED")),
        #(
          "documentation",
          json.string("https://shopify.dev/api/usage/access-scopes"),
        ),
        #("requiredAccess", json.string(required_access)),
      ]),
    ),
    #("path", json.array([root_name], json.string)),
  ])
}

@internal
pub fn handle_abandonment_delivery_status(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "abandonmentUpdateActivitiesDeliveryStatuses",
      [
        RequiredArgument(name: "abandonmentId", expected_type: "ID!"),
        RequiredArgument(name: "marketingActivityId", expected_type: "ID!"),
        RequiredArgument(
          name: "deliveryStatus",
          expected_type: "AbandonmentDeliveryState!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> #(key, json.null(), store, identity, [], validation_errors, [])
    [] -> {
      let args = field_arguments(field, variables)
      let abandonment_id = read_string_arg(args, "abandonmentId")
      let marketing_activity_id = read_string_arg(args, "marketingActivityId")
      let delivery_status = read_string_arg(args, "deliveryStatus")
      case abandonment_id, marketing_activity_id, delivery_status {
        Some(abandonment_id), Some(marketing_activity_id), Some(delivery_status)
        -> {
          let store =
            maybe_hydrate_abandonment_for_delivery_status(
              store,
              abandonment_id,
              upstream,
            )
          let activity =
            AbandonmentDeliveryActivityRecord(
              marketing_activity_id: marketing_activity_id,
              delivery_status: delivery_status,
              delivered_at: read_string_arg(args, "deliveredAt"),
              delivery_status_change_reason: read_string_arg(
                args,
                "deliveryStatusChangeReason",
              ),
            )
          case store.get_abandonment_by_id(store, abandonment_id) {
            Some(abandonment) -> {
              case validate_delivery_activity(abandonment, activity) {
                Some(error) ->
                  invalid_delivery_status_result(
                    key,
                    store,
                    identity,
                    field,
                    fragments,
                    abandonment,
                    [error],
                  )
                None -> {
                  case is_same_delivery_status(abandonment, activity) {
                    True ->
                      same_delivery_status_result(
                        key,
                        store,
                        identity,
                        field,
                        fragments,
                        abandonment,
                      )
                    False -> {
                      let #(next_store, updated) =
                        store.stage_abandonment_delivery_activity(
                          store,
                          abandonment_id,
                          activity,
                        )
                      case updated {
                        Some(updated_abandonment) -> {
                          let payload =
                            serialize_abandonment_mutation_payload(
                              next_store,
                              field,
                              Some(updated_abandonment),
                              [],
                              fragments,
                            )
                          let draft =
                            abandonment_log_draft(
                              [updated_abandonment.id],
                              store_types.Staged,
                              Some(
                                "Locally staged abandonmentUpdateActivitiesDeliveryStatuses in shopify-draft-proxy.",
                              ),
                            )
                          #(
                            key,
                            payload,
                            next_store,
                            identity,
                            [updated_abandonment.id],
                            [],
                            [draft],
                          )
                        }
                        None ->
                          unknown_abandonment_result(
                            key,
                            store,
                            identity,
                            field,
                            fragments,
                          )
                      }
                    }
                  }
                }
              }
            }
            None ->
              unknown_abandonment_result(key, store, identity, field, fragments)
          }
        }
        _, _, _ ->
          unknown_abandonment_result(key, store, identity, field, fragments)
      }
    }
  }
}

const abandonment_delivery_hydrate_query: String = "
query OrdersAbandonmentDeliveryHydrate($id: ID!) {
  abandonment(id: $id) {
    id
    emailState
    emailSentAt
  }
}
"

fn maybe_hydrate_abandonment_for_delivery_status(
  store: Store,
  abandonment_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_proxy_synthetic_gid(abandonment_id)
    || option.is_some(store.get_abandonment_by_id(store, abandonment_id))
  {
    True -> store
    False -> {
      let variables = json.object([#("id", json.string(abandonment_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "OrdersAbandonmentDeliveryHydrate",
          abandonment_delivery_hydrate_query,
          variables,
        )
      {
        Ok(value) -> hydrate_abandonment_delivery_response(store, value)
        Error(_) -> store
      }
    }
  }
}

fn hydrate_abandonment_delivery_response(
  store: Store,
  body: commit.JsonValue,
) -> Store {
  case json_get(body, "data") {
    Some(data) ->
      case json_get(data, "abandonment") {
        Some(commit.JsonObject(_) as node) ->
          case abandonment_record_from_json(node) {
            Some(record) -> store.upsert_base_abandonments(store, [record])
            None -> store
          }
        _ -> store
      }
    None -> store
  }
}

fn abandonment_record_from_json(
  node: commit.JsonValue,
) -> Option(AbandonmentRecord) {
  case json_get_string(node, "id") {
    Some(id) ->
      Some(AbandonmentRecord(
        id: id,
        abandoned_checkout_id: None,
        cursor: None,
        data: CapturedObject([
          #("id", CapturedString(id)),
          #(
            "emailState",
            option_to_captured_string(json_get_string(node, "emailState")),
          ),
          #(
            "emailSentAt",
            option_to_captured_string(json_get_string(node, "emailSentAt")),
          ),
        ]),
        delivery_activities: delivery_activities_from_json(node),
      ))
    None -> None
  }
}

fn delivery_activities_from_json(
  node: commit.JsonValue,
) -> Dict(String, AbandonmentDeliveryActivityRecord) {
  case json_get(node, "deliveryActivities") {
    Some(commit.JsonArray(items)) ->
      items
      |> list.filter_map(delivery_activity_pair_from_json)
      |> dict.from_list
    _ -> dict.new()
  }
}

fn delivery_activity_pair_from_json(
  value: commit.JsonValue,
) -> Result(#(String, AbandonmentDeliveryActivityRecord), Nil) {
  case json_get_string(value, "marketingActivityId") {
    Some(marketing_activity_id) ->
      case json_get_string(value, "deliveryStatus") {
        Some(delivery_status) ->
          Ok(#(
            marketing_activity_id,
            AbandonmentDeliveryActivityRecord(
              marketing_activity_id: marketing_activity_id,
              delivery_status: delivery_status,
              delivered_at: json_get_string(value, "deliveredAt"),
              delivery_status_change_reason: json_get_string(
                value,
                "deliveryStatusChangeReason",
              ),
            ),
          ))
        None -> Error(Nil)
      }
    None -> Error(Nil)
  }
}

fn option_to_captured_string(value: Option(String)) {
  case value {
    Some(value) -> CapturedString(value)
    None -> CapturedNull
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(field_key, field_value) if field_key == key -> Ok(field_value)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(value)) -> Some(value)
    _ -> None
  }
}

@internal
pub fn invalid_delivery_status_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  abandonment: AbandonmentRecord,
  user_errors: List(#(List(String), String, Option(String))),
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let payload =
    serialize_abandonment_mutation_payload(
      store,
      field,
      Some(abandonment),
      user_errors,
      fragments,
    )
  let draft =
    abandonment_log_draft(
      [],
      store_types.Failed,
      Some(
        "abandonmentUpdateActivitiesDeliveryStatuses failed local validation.",
      ),
    )
  #(key, payload, store, identity, [], [], [draft])
}

@internal
pub fn same_delivery_status_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  abandonment: AbandonmentRecord,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let payload =
    serialize_abandonment_mutation_payload(
      store,
      field,
      Some(abandonment),
      [],
      fragments,
    )
  let draft =
    abandonment_log_draft(
      [],
      store_types.Staged,
      Some(
        "abandonmentUpdateActivitiesDeliveryStatuses accepted same-status no-op without staging a state change.",
      ),
    )
  #(key, payload, store, identity, [], [], [draft])
}

@internal
pub fn unknown_abandonment_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> #(
  String,
  Json,
  Store,
  SyntheticIdentityRegistry,
  List(String),
  List(Json),
  List(LogDraft),
) {
  let user_errors = [
    inferred_user_error(["abandonmentId"], "abandonment_not_found"),
  ]
  let payload =
    serialize_abandonment_mutation_payload(
      store,
      field,
      None,
      user_errors,
      fragments,
    )
  let draft =
    abandonment_log_draft(
      [],
      store_types.Failed,
      Some(
        "abandonmentUpdateActivitiesDeliveryStatuses failed local validation.",
      ),
    )
  #(key, payload, store, identity, [], [], [draft])
}

fn validate_delivery_activity(
  abandonment: AbandonmentRecord,
  activity: AbandonmentDeliveryActivityRecord,
) -> Option(#(List(String), String, Option(String))) {
  case existing_delivery_activity(abandonment, activity.marketing_activity_id) {
    None ->
      Some(user_error(
        ["deliveryStatuses", "0", "marketingActivityId"],
        "invalid",
        Some(user_error_codes.not_found),
      ))
    Some(existing) -> {
      case
        is_backwards_delivery_transition(
          existing.delivery_status,
          activity.delivery_status,
        )
      {
        True ->
          Some(user_error(
            ["deliveryStatuses", "0", "deliveryStatus"],
            "invalid_transition",
            Some(user_error_codes.invalid),
          ))
        False -> future_delivered_at_error(activity)
      }
    }
  }
}

fn existing_delivery_activity(
  abandonment: AbandonmentRecord,
  marketing_activity_id: String,
) -> Option(AbandonmentDeliveryActivityRecord) {
  case dict.get(abandonment.delivery_activities, marketing_activity_id) {
    Ok(activity) -> Some(activity)
    Error(_) -> None
  }
}

fn is_same_delivery_status(
  abandonment: AbandonmentRecord,
  activity: AbandonmentDeliveryActivityRecord,
) -> Bool {
  case existing_delivery_activity(abandonment, activity.marketing_activity_id) {
    Some(existing) -> existing.delivery_status == activity.delivery_status
    None -> False
  }
}

fn is_backwards_delivery_transition(current: String, next: String) -> Bool {
  case delivery_status_rank(current), delivery_status_rank(next) {
    Some(current_rank), Some(next_rank) -> next_rank < current_rank
    _, _ -> False
  }
}

fn delivery_status_rank(status: String) -> Option(Int) {
  case status {
    "SENDING" -> Some(10)
    "SENT" -> Some(20)
    "DELIVERED" -> Some(30)
    _ -> None
  }
}

fn future_delivered_at_error(
  activity: AbandonmentDeliveryActivityRecord,
) -> Option(#(List(String), String, Option(String))) {
  case activity.delivery_status, activity.delivered_at {
    "DELIVERED", Some(value) -> {
      case iso_timestamp.parse_iso(value) {
        Ok(delivered_at_ms) -> {
          let now_ms =
            iso_timestamp.now_iso()
            |> iso_timestamp.parse_iso
            |> result.unwrap(0)
          case delivered_at_ms > now_ms {
            True ->
              Some(user_error(
                ["deliveryStatuses", "0", "deliveredAt"],
                "invalid",
                Some(user_error_codes.invalid),
              ))
            False -> None
          }
        }
        Error(_) -> None
      }
    }
    _, _ -> None
  }
}

@internal
pub fn abandonment_log_draft(
  staged_resource_ids: List(String),
  status: store.EntryStatus,
  notes: Option(String),
) -> LogDraft {
  single_root_log_draft(
    "abandonmentUpdateActivitiesDeliveryStatuses",
    staged_resource_ids,
    status,
    "orders",
    "stage-locally",
    notes,
  )
}

@internal
pub fn serialize_abandonment_mutation_payload(
  store: Store,
  field: Selection,
  abandonment: Option(AbandonmentRecord),
  user_errors: List(#(List(String), String, Option(String))),
  fragments: FragmentMap,
) -> Json {
  let entries =
    list.map(selection_children(field), fn(child) {
      let key = get_field_response_key(child)
      case child {
        Field(name: name, ..) ->
          case name.value {
            "abandonment" -> #(key, case abandonment {
              Some(record) ->
                serialize_abandonment_node(store, child, record, fragments)
              None -> json.null()
            })
            "userErrors" -> #(
              key,
              json.array(user_errors, fn(error) {
                serialize_user_error(child, error)
              }),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}
