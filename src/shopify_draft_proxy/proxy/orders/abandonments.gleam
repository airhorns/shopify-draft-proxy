//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type ObjectField, type Selection, Field, NullValue, ObjectField, ObjectValue,
  SelectionSet, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  SelectedFieldOptions, SerializeConnectionConfig, SrcBool, SrcFloat, SrcInt,
  SrcList, SrcNull, SrcObject, SrcString, default_connection_window_options,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, resolved_value_to_source,
  serialize_connection, source_to_json, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, RequiredArgument,
  find_argument, single_root_log_draft, validate_required_field_arguments,
}
import shopify_draft_proxy/proxy/orders/common.{
  field_arguments, inferred_user_error, read_string_arg, selection_children,
  serialize_user_error,
}
import shopify_draft_proxy/proxy/orders/serializers.{serialize_abandonment_node}
import shopify_draft_proxy/proxy/passthrough
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response, LiveHybrid, Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/user_error_codes
import shopify_draft_proxy/search_query_parser
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry, is_proxy_synthetic_gid,
}
import shopify_draft_proxy/state/types.{
  type AbandonedCheckoutRecord, type AbandonmentRecord, type CapturedJsonValue,
  type CustomerRecord, type DraftOrderRecord,
  type DraftOrderVariantCatalogRecord, type OrderRecord,
  type ProductMetafieldRecord, type ProductRecord, type ProductVariantRecord,
  AbandonmentDeliveryActivityRecord, CapturedArray, CapturedBool, CapturedFloat,
  CapturedInt, CapturedNull, CapturedObject, CapturedString,
  CustomerOrderSummaryRecord, CustomerRecord, DraftOrderRecord,
  DraftOrderVariantCatalogRecord, OrderRecord, ProductVariantRecord,
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
          let #(next_store, updated) =
            store.stage_abandonment_delivery_activity(
              store,
              abandonment_id,
              activity,
            )
          case updated {
            Some(abandonment) -> {
              let payload =
                serialize_abandonment_mutation_payload(
                  next_store,
                  field,
                  Some(abandonment),
                  [],
                  fragments,
                )
              let draft =
                abandonment_log_draft(
                  [abandonment.id],
                  store_types.Staged,
                  Some(
                    "Locally staged abandonmentUpdateActivitiesDeliveryStatuses in shopify-draft-proxy.",
                  ),
                )
              #(key, payload, next_store, identity, [abandonment.id], [], [
                draft,
              ])
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
