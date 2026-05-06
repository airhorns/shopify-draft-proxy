//// Incremental Orders-domain port.
////
//// This module is being expanded slice-by-slice from executable parity
//// fixtures. Broad order creation/payment, order editing, fulfillment
//// creation, and returns remain intentionally narrow until their lifecycle
//// effects are modeled together.

import gleam/dict.{type Dict}

import gleam/json
import gleam/list

import shopify_draft_proxy/graphql/ast.{Field}

import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/graphql_helpers.{get_document_fragments}

import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/orders/abandonments.{
  handle_abandonment_delivery_status, handle_access_denied_guardrail,
}
import shopify_draft_proxy/proxy/orders/common.{get_operation_path_label}
import shopify_draft_proxy/proxy/orders/draft_order_admin.{
  handle_draft_order_bulk_helper, handle_draft_order_calculate,
  handle_draft_order_complete, handle_draft_order_delete,
  handle_draft_order_duplicate, handle_draft_order_invoice_preview,
  handle_draft_order_invoice_send, handle_draft_order_update,
  handle_order_delete_mutation,
}
import shopify_draft_proxy/proxy/orders/draft_orders.{
  handle_draft_order_create, handle_draft_order_create_from_order,
}
import shopify_draft_proxy/proxy/orders/fulfillment_orders.{
  handle_fulfillment_order_bulk_mutation,
  handle_fulfillment_order_lifecycle_mutation,
  handle_fulfillment_order_request_mutation,
}
import shopify_draft_proxy/proxy/orders/fulfillments.{
  handle_fulfillment_create_mutation, handle_fulfillment_event_create_mutation,
}
import shopify_draft_proxy/proxy/orders/hydration.{handle_fulfillment_mutation}
import shopify_draft_proxy/proxy/orders/order_create.{
  handle_order_create_mutation,
}
import shopify_draft_proxy/proxy/orders/order_edit.{
  handle_order_edit_add_variant_mutation, handle_order_edit_begin_mutation,
  handle_order_edit_commit_mutation, handle_order_edit_residual_mutation,
  handle_order_edit_set_quantity_mutation,
}
import shopify_draft_proxy/proxy/orders/order_transactions.{
  handle_order_cancel_mutation, handle_order_capture_mutation,
  handle_order_create_mandate_payment_mutation, handle_order_invoice_send,
  handle_order_lifecycle_mutation, handle_order_mark_as_paid_mutation,
  handle_transaction_void_mutation,
}
import shopify_draft_proxy/proxy/orders/order_update_refund.{
  handle_order_update_mutation, handle_refund_create_mutation,
}
import shopify_draft_proxy/proxy/orders/returns_core.{
  handle_return_lifecycle_mutation,
}

import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}

import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      let initial = #([], [], store, identity, [], [])
      let #(
        data_entries,
        all_errors,
        final_store,
        final_identity,
        staged_ids,
        log_drafts,
      ) =
        list.fold(fields, initial, fn(acc, field) {
          let #(entries, errors, current_store, current_identity, ids, drafts) =
            acc
          case field {
            Field(name: name, ..)
              if name.value == "abandonmentUpdateActivitiesDeliveryStatuses"
            -> {
              let result =
                handle_abandonment_delivery_status(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "draftOrderCreate" -> {
              let result =
                handle_draft_order_create(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..)
              if name.value == "draftOrderCreateFromOrder"
            -> {
              let result =
                handle_draft_order_create_from_order(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "draftOrderComplete" -> {
              let result =
                handle_draft_order_complete(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "draftOrderDelete" -> {
              let result =
                handle_draft_order_delete(
                  current_store,
                  document,
                  operation_path,
                  field,
                  variables,
                  upstream,
                )
              let #(key, payload, next_store, next_errors, next_drafts) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  current_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "draftOrderDuplicate" -> {
              let result =
                handle_draft_order_duplicate(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_drafts,
              ) = result
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                list.append(ids, next_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..) if name.value == "draftOrderCalculate" -> {
              let result =
                handle_draft_order_calculate(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(key, payload, next_errors, next_drafts) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  current_store,
                  current_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..)
              if name.value == "draftOrderBulkAddTags"
              || name.value == "draftOrderBulkRemoveTags"
              || name.value == "draftOrderBulkDelete"
            -> {
              let result =
                handle_draft_order_bulk_helper(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_drafts,
              ) = result
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                list.append(ids, next_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..) if name.value == "draftOrderInvoicePreview" -> {
              let result =
                handle_draft_order_invoice_preview(
                  current_store,
                  document,
                  operation_path,
                  field,
                  variables,
                )
              let #(key, payload, next_errors, next_drafts) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  current_store,
                  current_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "draftOrderInvoiceSend" -> {
              let result =
                handle_draft_order_invoice_send(
                  current_store,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(key, payload, next_errors, next_drafts) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  current_store,
                  current_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "draftOrderUpdate" -> {
              let result =
                handle_draft_order_update(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..)
              if name.value == "fulfillmentCancel"
              || name.value == "fulfillmentTrackingInfoUpdate"
            -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_fulfillment_mutation(
                  name.value,
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "fulfillmentCreate" -> {
              let result =
                handle_fulfillment_create_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  list.append(entries, [#(key, payload)]),
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
              }
            }
            Field(name: name, ..) if name.value == "fulfillmentEventCreate" -> {
              let result =
                handle_fulfillment_event_create_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  list.append(entries, [#(key, payload)]),
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
              }
            }
            Field(name: name, ..)
              if name.value == "fulfillmentOrderMerge"
              || name.value == "fulfillmentOrderSplit"
              || name.value == "fulfillmentOrdersSetFulfillmentDeadline"
            -> {
              let result =
                handle_fulfillment_order_bulk_mutation(
                  name.value,
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  list.append(entries, [#(key, payload)]),
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
              }
            }
            Field(name: name, ..)
              if name.value == "fulfillmentOrderCancel"
              || name.value == "fulfillmentOrderClose"
              || name.value == "fulfillmentOrderHold"
              || name.value == "fulfillmentOrderMove"
              || name.value == "fulfillmentOrderOpen"
              || name.value == "fulfillmentOrderReleaseHold"
              || name.value == "fulfillmentOrderReportProgress"
              || name.value == "fulfillmentOrderReschedule"
            -> {
              let result =
                handle_fulfillment_order_lifecycle_mutation(
                  name.value,
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  list.append(entries, [#(key, payload)]),
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
              }
            }
            Field(name: name, ..)
              if name.value == "fulfillmentOrderAcceptCancellationRequest"
              || name.value == "fulfillmentOrderAcceptFulfillmentRequest"
              || name.value == "fulfillmentOrderRejectCancellationRequest"
              || name.value == "fulfillmentOrderRejectFulfillmentRequest"
              || name.value == "fulfillmentOrderSubmitCancellationRequest"
              || name.value == "fulfillmentOrderSubmitFulfillmentRequest"
            -> {
              let result =
                handle_fulfillment_order_request_mutation(
                  name.value,
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  list.append(entries, [#(key, payload)]),
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
              }
            }
            Field(name: name, ..) if name.value == "orderCreate" -> {
              let result =
                handle_order_create_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderDelete" -> {
              let result =
                handle_order_delete_mutation(current_store, field, variables)
              let #(key, payload, next_store, next_ids, next_drafts) = result
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                current_identity,
                list.append(ids, next_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..)
              if name.value == "orderClose" || name.value == "orderOpen"
            -> {
              let result =
                handle_order_lifecycle_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderCancel" -> {
              let result =
                handle_order_cancel_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  variables,
                  upstream,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_errors,
                next_drafts,
              ) = result
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, next_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  next_store,
                  next_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderCapture" -> {
              let result =
                handle_order_capture_mutation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_drafts,
              ) = result
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                list.append(ids, next_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..) if name.value == "transactionVoid" -> {
              let result =
                handle_transaction_void_mutation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_drafts,
              ) = result
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                list.append(ids, next_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..)
              if name.value == "orderCreateMandatePayment"
            -> {
              let result =
                handle_order_create_mandate_payment_mutation(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                )
              let #(
                key,
                payload,
                next_store,
                next_identity,
                next_ids,
                next_drafts,
              ) = result
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                list.append(ids, next_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..) if name.value == "orderInvoiceSend" -> {
              let #(key, payload, next_errors) =
                handle_order_invoice_send(
                  current_store,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderMarkAsPaid" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_order_mark_as_paid_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  list.append(drafts, next_drafts),
                )
              }
            }
            Field(name: name, ..) if name.value == "orderUpdate" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_order_update_mutation(
                  current_store,
                  current_identity,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "refundCreate" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_refund_create_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderEditBegin" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_order_edit_begin_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderEditAddVariant" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_order_edit_add_variant_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderEditSetQuantity" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_order_edit_set_quantity_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..) if name.value == "orderEditCommit" -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_errors,
                next_drafts,
              ) =
                handle_order_edit_commit_mutation(
                  current_store,
                  current_identity,
                  document,
                  operation_path,
                  field,
                  fragments,
                  variables,
                )
              case next_errors {
                [] -> #(
                  list.append(entries, [#(key, payload)]),
                  errors,
                  next_store,
                  next_identity,
                  list.append(ids, staged_ids),
                  list.append(drafts, next_drafts),
                )
                _ -> #(
                  entries,
                  list.append(errors, next_errors),
                  current_store,
                  current_identity,
                  ids,
                  drafts,
                )
              }
            }
            Field(name: name, ..)
              if name.value == "orderEditAddCustomItem"
              || name.value == "orderEditAddLineItemDiscount"
              || name.value == "orderEditRemoveDiscount"
              || name.value == "orderEditAddShippingLine"
              || name.value == "orderEditUpdateShippingLine"
              || name.value == "orderEditRemoveShippingLine"
            -> {
              let #(key, payload, next_store, next_identity) =
                handle_order_edit_residual_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                )
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                ids,
                drafts,
              )
            }
            Field(name: name, ..)
              if name.value == "returnCreate"
              || name.value == "returnRequest"
              || name.value == "returnCancel"
              || name.value == "returnClose"
              || name.value == "returnReopen"
              || name.value == "removeFromReturn"
              || name.value == "returnDeclineRequest"
              || name.value == "returnApproveRequest"
              || name.value == "returnProcess"
              || name.value == "reverseDeliveryCreateWithShipping"
              || name.value == "reverseDeliveryShippingUpdate"
              || name.value == "reverseFulfillmentOrderDispose"
            -> {
              let #(
                key,
                payload,
                next_store,
                next_identity,
                staged_ids,
                next_drafts,
              ) =
                handle_return_lifecycle_mutation(
                  current_store,
                  current_identity,
                  name.value,
                  field,
                  fragments,
                  variables,
                  upstream,
                )
              #(
                list.append(entries, [#(key, payload)]),
                errors,
                next_store,
                next_identity,
                list.append(ids, staged_ids),
                list.append(drafts, next_drafts),
              )
            }
            Field(name: name, ..)
              if name.value == "orderCreateManualPayment"
              || name.value == "taxSummaryCreate"
            -> {
              let #(key, payload, next_errors, next_drafts) =
                handle_access_denied_guardrail(name.value, field)
              #(
                list.append(entries, [#(key, payload)]),
                list.append(errors, next_errors),
                current_store,
                current_identity,
                ids,
                list.append(drafts, next_drafts),
              )
            }
            _ -> acc
          }
        })
      let envelope = case all_errors {
        [] -> json.object([#("data", json.object(data_entries))])
        _ ->
          case data_entries {
            [] ->
              json.object([#("errors", json.preprocessed_array(all_errors))])
            _ ->
              json.object([
                #("errors", json.preprocessed_array(all_errors)),
                #("data", json.object(data_entries)),
              ])
          }
      }
      MutationOutcome(
        data: envelope,
        store: final_store,
        identity: final_identity,
        staged_resource_ids: staged_ids,
        log_drafts: log_drafts,
      )
    }
  }
}
