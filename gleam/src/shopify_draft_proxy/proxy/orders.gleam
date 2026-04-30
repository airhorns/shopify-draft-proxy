//// Minimal Orders payment-domain slice.
////
//// This deliberately ports only the order roots needed by payments parity:
//// local order creation for payment fixtures, capture, void, mandate payment,
//// manual-payment access denied/local staging, and downstream `order(id:)`
//// payment reads.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcString,
  get_document_fragments, get_field_response_key, project_graphql_value,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type LogDraft}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type OrderMandatePaymentRecord, type OrderRecord, type OrderTransactionRecord,
  Money, OrderMandatePaymentRecord, OrderRecord, OrderTransactionRecord,
}

const manual_payment_required_access: String = "`write_orders` access scope. Also: The user must have mark_orders_as_paid permission. The API client must be installed on a Shopify Plus store to use the amount field."

pub type OrdersError {
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

type UserError {
  UserError(field: List(String), message: String)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    should_log: Bool,
    top_level_errors: List(Json),
  )
}

pub fn is_orders_query_root(name: String) -> Bool {
  case name {
    "order" -> True
    _ -> False
  }
}

pub fn is_orders_mutation_root(name: String) -> Bool {
  case name {
    "orderCreate"
    | "orderCapture"
    | "transactionVoid"
    | "orderCreateMandatePayment"
    | "orderCreateManualPayment" -> True
    _ -> False
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, OrdersError) {
  use data <- result.try(handle_orders_query(store, document, variables))
  Ok(wrap_data(data))
}

pub fn handle_orders_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, OrdersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(
        json.object(
          list.map(fields, fn(field) {
            #(
              get_field_response_key(field),
              query_payload(store, field, fragments, variables),
            )
          }),
        ),
      )
    }
  }
}

pub fn wrap_data(data: Json) -> Json {
  json.object([#("data", data)])
}

fn query_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "order" -> {
          let args = field_args(field, variables)
          case read_string_field(args, "id") {
            Some(id) ->
              case store.get_effective_order_by_id(store, id) {
                Some(order) -> serialize_order(order, field, fragments)
                None -> json.null()
              }
            None -> json.null()
          }
        }
        _ -> json.null()
      }
    _ -> json.null()
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, OrdersError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        request_path,
        document,
        fields,
        fragments,
        variables,
      ))
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(entries, final_store, final_identity, staged_ids, top_errors) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entry_acc, current_store, current_identity, staged_acc, error_acc) =
        acc
      let #(result, next_store, next_identity) =
        handle_mutation_field(
          current_store,
          current_identity,
          request_path,
          document,
          field,
          fragments,
          variables,
        )
      #(
        list.append(entry_acc, [#(result.key, result.payload)]),
        next_store,
        next_identity,
        list.append(staged_acc, result.staged_resource_ids),
        list.append(error_acc, result.top_level_errors),
      )
    })
  let envelope = case top_errors {
    [] -> json.object([#("data", json.object(entries))])
    _ ->
      json.object([
        #("data", json.object(entries)),
        #("errors", json.array(top_errors, fn(item) { item })),
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

fn handle_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "orderCreate" ->
          create_order(
            store,
            identity,
            request_path,
            document,
            field,
            fragments,
            variables,
          )
        "orderCapture" ->
          with_reserved_order_payment_log(
            store,
            identity,
            request_path,
            document,
            field,
            fragments,
            variables,
            capture_order,
          )
        "transactionVoid" ->
          with_reserved_order_payment_log(
            store,
            identity,
            request_path,
            document,
            field,
            fragments,
            variables,
            void_transaction,
          )
        "orderCreateMandatePayment" ->
          with_reserved_order_payment_log(
            store,
            identity,
            request_path,
            document,
            field,
            fragments,
            variables,
            create_mandate_payment,
          )
        "orderCreateManualPayment" ->
          with_reserved_order_payment_log(
            store,
            identity,
            request_path,
            document,
            field,
            fragments,
            variables,
            create_manual_payment,
          )
        _ -> #(
          MutationFieldResult(
            get_field_response_key(field),
            json.null(),
            [],
            False,
            [],
          ),
          store,
          identity,
        )
      }
    _ -> #(MutationFieldResult("", json.null(), [], False, []), store, identity)
  }
}

fn with_reserved_order_payment_log(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  handler: fn(
    Store,
    SyntheticIdentityRegistry,
    String,
    String,
    Selection,
    FragmentMap,
    Dict(String, root_field.ResolvedValue),
    String,
    String,
  ) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(result, handled_store, handled_identity) =
    handler(
      store,
      identity,
      request_path,
      document,
      field,
      fragments,
      variables,
      "",
      "",
    )
  let #(log_id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(handled_identity, "MutationLogEntry")
  let #(received_at, identity_after_log) =
    synthetic_identity.make_synthetic_timestamp(identity_after_id)
  case result.should_log {
    False -> #(result, handled_store, identity_after_log)
    True -> {
      let root_field = case field {
        Field(name: name, ..) -> name.value
        _ -> ""
      }
      let logged_store =
        append_order_log(
          handled_store,
          request_path,
          document,
          root_field,
          result.staged_resource_ids,
          "Staged locally in the in-memory order payment draft store.",
          log_id,
          received_at,
        )
      #(result, logged_store, identity_after_log)
    }
  }
}

fn create_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let input = read_object_field(args, "order")
  let #(order, identity_after_order) =
    build_order_from_input(store, identity, input)
  let next_store = store.upsert_staged_order(store, order)
  let payload = serialize_order_create_payload(order, field, fragments, [])
  let #(log_id, identity_after_log_id) =
    synthetic_identity.make_synthetic_gid(
      identity_after_order,
      "MutationLogEntry",
    )
  let #(received_at, identity_after_log) =
    synthetic_identity.make_synthetic_timestamp(identity_after_log_id)
  let logged_store =
    append_order_log(
      next_store,
      request_path,
      document,
      "orderCreate",
      [order.id],
      "Staged locally in the in-memory order draft store.",
      log_id,
      received_at,
    )
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      [order.id],
      True,
      [],
    ),
    logged_store,
    identity_after_log,
  )
}

fn capture_order(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  _log_id: String,
  _received_at: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let input = read_object_field(args, "input")
  let order_id = read_string_field(input, "id")
  let parent_id = read_string_field(input, "parentTransactionId")
  let order =
    option_then(order_id, fn(id) { store.get_effective_order_by_id(store, id) })
  let authorization = case order, parent_id {
    Some(record), Some(id) -> find_transaction(record, id)
    _, _ -> None
  }
  case order, authorization {
    None, _ -> #(
      MutationFieldResult(
        get_field_response_key(field),
        serialize_order_capture_payload(None, None, field, fragments, [
          UserError(["input", "id"], "Order does not exist"),
        ]),
        [],
        False,
        [],
      ),
      store,
      identity,
    )
    Some(record), None -> #(
      MutationFieldResult(
        get_field_response_key(field),
        serialize_order_capture_payload(None, Some(record), field, fragments, [
          UserError(
            ["input", "parentTransactionId"],
            "Transaction does not exist",
          ),
        ]),
        [],
        False,
        [],
      ),
      store,
      identity,
    )
    Some(record), Some(auth) -> {
      let remaining = capturable_amount_for_authorization(record, auth)
      let amount = read_payment_input_amount(input, remaining)
      case remaining <=. 0.0 {
        True ->
          validation_result(
            field,
            serialize_order_capture_payload(
              None,
              Some(record),
              field,
              fragments,
              [
                UserError(
                  ["input", "parentTransactionId"],
                  "Transaction is not capturable",
                ),
              ],
            ),
            store,
            identity,
          )
        False ->
          case amount <=. 0.0 {
            True ->
              validation_result(
                field,
                serialize_order_capture_payload(
                  None,
                  Some(record),
                  field,
                  fragments,
                  [
                    UserError(
                      ["input", "amount"],
                      "Amount must be greater than zero",
                    ),
                  ],
                ),
                store,
                identity,
              )
            False ->
              case amount >. remaining {
                True ->
                  validation_result(
                    field,
                    serialize_order_capture_payload(
                      None,
                      Some(record),
                      field,
                      fragments,
                      [
                        UserError(
                          ["input", "amount"],
                          "Amount exceeds capturable amount",
                        ),
                      ],
                    ),
                    store,
                    identity,
                  )
                False -> {
                  let #(payment_reference_id, identity_after_ref) =
                    synthetic_identity.make_synthetic_gid(
                      identity,
                      "PaymentReference",
                    )
                  let #(transaction, identity_after_transaction) =
                    build_payment_transaction(
                      identity_after_ref,
                      "CAPTURE",
                      amount,
                      record.currency_code,
                      auth.gateway,
                      Some(auth.id),
                      Some(payment_reference_id),
                    )
                  let updated =
                    apply_payment_derived_fields(
                      OrderRecord(
                        ..record,
                        payment_gateway_names: append_gateway(
                          record.payment_gateway_names,
                          auth.gateway,
                        ),
                        transactions: list.append(record.transactions, [
                          transaction,
                        ]),
                      ),
                    )
                  let next_store = store.upsert_staged_order(store, updated)
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      serialize_order_capture_payload(
                        Some(transaction),
                        Some(updated),
                        field,
                        fragments,
                        [],
                      ),
                      [updated.id, transaction.id],
                      True,
                      [],
                    ),
                    next_store,
                    identity_after_transaction,
                  )
                }
              }
          }
      }
    }
  }
}

fn void_transaction(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  _log_id: String,
  _received_at: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let transaction_id = case read_string_field(args, "parentTransactionId") {
    Some(id) -> Some(id)
    None -> read_string_field(args, "id")
  }
  let match =
    option_then(transaction_id, fn(id) {
      store.find_order_with_transaction(store, id)
    })
  case match {
    None ->
      validation_result(
        field,
        serialize_transaction_void_payload(None, None, field, fragments, [
          UserError(["parentTransactionId"], "Transaction does not exist"),
        ]),
        store,
        identity,
      )
    Some(#(order, authorization)) ->
      case !is_successful_authorization(authorization) {
        True ->
          validation_result(
            field,
            serialize_transaction_void_payload(None, None, field, fragments, [
              UserError(["id"], "Transaction is not voidable"),
            ]),
            store,
            identity,
          )
        False ->
          case transaction_has_voiding_child(order, authorization.id) {
            True ->
              validation_result(
                field,
                serialize_transaction_void_payload(
                  None,
                  None,
                  field,
                  fragments,
                  [
                    UserError(["id"], "Transaction has already been voided"),
                  ],
                ),
                store,
                identity,
              )
            False ->
              case
                captured_amount_for_authorization(order, authorization.id)
                >. 0.0
              {
                True ->
                  validation_result(
                    field,
                    serialize_transaction_void_payload(
                      None,
                      None,
                      field,
                      fragments,
                      [
                        UserError(
                          ["id"],
                          "Transaction has already been captured",
                        ),
                      ],
                    ),
                    store,
                    identity,
                  )
                False -> {
                  let #(transaction, identity_after_transaction) =
                    build_payment_transaction(
                      identity,
                      "VOID",
                      parse_decimal(authorization.amount.amount),
                      order.currency_code,
                      authorization.gateway,
                      Some(authorization.id),
                      None,
                    )
                  let updated =
                    apply_payment_derived_fields(
                      OrderRecord(
                        ..order,
                        transactions: list.append(order.transactions, [
                          transaction,
                        ]),
                      ),
                    )
                  let next_store = store.upsert_staged_order(store, updated)
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      serialize_transaction_void_payload(
                        Some(transaction),
                        Some(updated),
                        field,
                        fragments,
                        [],
                      ),
                      [updated.id, transaction.id],
                      True,
                      [],
                    ),
                    next_store,
                    identity_after_transaction,
                  )
                }
              }
          }
      }
  }
}

fn create_mandate_payment(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  _log_id: String,
  _received_at: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let order_id = read_string_field(args, "id")
  let order =
    option_then(order_id, fn(id) { store.get_effective_order_by_id(store, id) })
  case order {
    None ->
      validation_result(
        field,
        serialize_mandate_payload(None, None, field, fragments, [
          UserError(["id"], "Order does not exist"),
        ]),
        store,
        identity,
      )
    Some(record) ->
      case read_string_field(args, "idempotencyKey") {
        None ->
          validation_result(
            field,
            serialize_mandate_payload(None, Some(record), field, fragments, [
              UserError(["idempotencyKey"], "Idempotency key is required"),
            ]),
            store,
            identity,
          )
        Some(idempotency_key) -> {
          case
            store.get_order_mandate_payment(store, record.id, idempotency_key)
          {
            Some(existing) -> {
              let existing_order = order_with_existing_mandate(record, existing)
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  serialize_mandate_payload(
                    Some(existing),
                    Some(existing_order),
                    field,
                    fragments,
                    [],
                  ),
                  [existing_order.id, existing.transaction_id],
                  True,
                  [],
                ),
                store,
                identity,
              )
            }
            None -> {
              let amount =
                read_money_input_amount(
                  args,
                  "amount",
                  parse_decimal(record.total_outstanding),
                )
              case amount <=. 0.0 {
                True ->
                  validation_result(
                    field,
                    serialize_mandate_payload(
                      None,
                      Some(record),
                      field,
                      fragments,
                      [
                        UserError(
                          ["amount"],
                          "Amount must be greater than zero",
                        ),
                      ],
                    ),
                    store,
                    identity,
                  )
                False -> {
                  let #(payment_reference_id, identity_after_ref) =
                    synthetic_identity.make_synthetic_gid(
                      identity,
                      "PaymentReference",
                    )
                  let #(transaction, identity_after_transaction) =
                    build_payment_transaction(
                      identity_after_ref,
                      "MANDATE_PAYMENT",
                      amount,
                      record.currency_code,
                      Some("mandate"),
                      None,
                      Some(payment_reference_id),
                    )
                  let #(job_id, identity_after_job) =
                    synthetic_identity.make_synthetic_gid(
                      identity_after_transaction,
                      "Job",
                    )
                  let mandate =
                    OrderMandatePaymentRecord(
                      order_id: record.id,
                      idempotency_key: idempotency_key,
                      job_id: job_id,
                      payment_reference_id: payment_reference_id,
                      transaction_id: transaction.id,
                    )
                  let updated =
                    apply_payment_derived_fields(
                      OrderRecord(
                        ..record,
                        payment_gateway_names: append_gateway(
                          record.payment_gateway_names,
                          Some("mandate"),
                        ),
                        transactions: list.append(record.transactions, [
                          transaction,
                        ]),
                      ),
                    )
                  let next_store =
                    store.upsert_staged_order(store, updated)
                    |> store.upsert_staged_order_mandate_payment(mandate)
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      serialize_mandate_payload(
                        Some(mandate),
                        Some(updated),
                        field,
                        fragments,
                        [],
                      ),
                      [updated.id, transaction.id, mandate.payment_reference_id],
                      True,
                      [],
                    ),
                    next_store,
                    identity_after_job,
                  )
                }
              }
            }
          }
        }
      }
  }
}

fn create_manual_payment(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  _document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  _log_id: String,
  _received_at: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let args = field_args(field, variables)
  let order =
    option_then(read_string_field(args, "id"), fn(id) {
      store.get_effective_order_by_id(store, id)
    })
  case order {
    None -> #(
      MutationFieldResult(
        get_field_response_key(field),
        json.null(),
        [],
        False,
        [access_denied_manual_payment_error()],
      ),
      store,
      identity,
    )
    Some(record) -> {
      let outstanding = parse_decimal(record.total_outstanding)
      let amount = read_money_input_amount(args, "amount", outstanding)
      case outstanding <=. 0.0 || record.display_financial_status == "PAID" {
        True ->
          validation_result(
            field,
            serialize_order_management_payload(Some(record), field, fragments, [
              UserError(["id"], "Order is already paid"),
            ]),
            store,
            identity,
          )
        False ->
          case amount <=. 0.0 {
            True ->
              validation_result(
                field,
                serialize_order_management_payload(
                  Some(record),
                  field,
                  fragments,
                  [
                    UserError(["amount"], "Amount must be greater than zero"),
                  ],
                ),
                store,
                identity,
              )
            False ->
              case amount >. outstanding {
                True ->
                  validation_result(
                    field,
                    serialize_order_management_payload(
                      Some(record),
                      field,
                      fragments,
                      [
                        UserError(
                          ["amount"],
                          "Amount exceeds outstanding amount",
                        ),
                      ],
                    ),
                    store,
                    identity,
                  )
                False -> {
                  let gateway = case
                    read_string_field(args, "paymentMethodName")
                  {
                    Some(name) -> name
                    None -> "manual"
                  }
                  let #(payment_reference_id, identity_after_ref) =
                    synthetic_identity.make_synthetic_gid(
                      identity,
                      "PaymentReference",
                    )
                  let #(transaction, identity_after_transaction) =
                    build_payment_transaction(
                      identity_after_ref,
                      "SALE",
                      amount,
                      record.currency_code,
                      Some(gateway),
                      None,
                      Some(payment_reference_id),
                    )
                  let updated =
                    apply_payment_derived_fields(
                      OrderRecord(
                        ..record,
                        payment_gateway_names: append_gateway(
                          record.payment_gateway_names,
                          Some(gateway),
                        ),
                        transactions: list.append(record.transactions, [
                          transaction,
                        ]),
                      ),
                    )
                  let next_store = store.upsert_staged_order(store, updated)
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      serialize_order_management_payload(
                        Some(updated),
                        field,
                        fragments,
                        [],
                      ),
                      [updated.id, transaction.id],
                      True,
                      [],
                    ),
                    next_store,
                    identity_after_transaction,
                  )
                }
              }
          }
      }
    }
  }
}

fn build_order_from_input(
  _store: Store,
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(OrderRecord, SyntheticIdentityRegistry) {
  let currency = case read_string_field(input, "currency") {
    Some(value) -> value
    None -> "CAD"
  }
  let #(order_id, identity_after_order) =
    synthetic_identity.make_synthetic_gid(identity, "Order")
  let #(line_total, identity_after_lines) =
    normalize_line_items(input, identity_after_order)
  let #(transactions, identity_after_transactions) =
    normalize_transactions(input, identity_after_lines, currency)
  let has_paid =
    list.any(transactions, fn(transaction) {
      transaction.status == "SUCCESS"
      && { transaction.kind == "SALE" || transaction.kind == "CAPTURE" }
    })
  let has_authorization =
    list.any(transactions, fn(transaction) {
      transaction.status == "SUCCESS" && transaction.kind == "AUTHORIZATION"
    })
  let gateways =
    transactions
    |> list.filter_map(fn(transaction) {
      case transaction.gateway {
        Some(gateway) -> Ok(gateway)
        None -> Error(Nil)
      }
    })
    |> dedupe_strings()
  let total = format_amount(line_total)
  let status = case has_paid, has_authorization {
    True, _ -> "PAID"
    _, True -> "AUTHORIZED"
    _, _ -> "PENDING"
  }
  let received = case has_paid {
    True -> total
    False -> "0.0"
  }
  let capturable_amount = case has_authorization {
    True -> total
    False -> "0.0"
  }
  let order =
    OrderRecord(
      id: order_id,
      currency_code: currency,
      total_price: total,
      display_financial_status: status,
      capturable: has_authorization,
      total_capturable: capturable_amount,
      total_outstanding: case has_paid {
        True -> "0.0"
        False -> total
      },
      total_received: received,
      net_payment: received,
      payment_gateway_names: gateways,
      transactions: transactions,
    )
  #(
    OrderRecord(..order, payment_gateway_names: case gateways {
      [] if has_paid -> ["manual"]
      _ -> gateways
    }),
    identity_after_transactions,
  )
}

fn normalize_line_items(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
) -> #(Float, SyntheticIdentityRegistry) {
  let items = case dict.get(input, "lineItems") {
    Ok(root_field.ListVal(values)) -> values
    _ -> []
  }
  list.fold(items, #(0.0, identity), fn(acc, item) {
    let #(sum, current_identity) = acc
    case item {
      root_field.ObjectVal(fields) -> {
        let #(_line_id, next_identity) =
          synthetic_identity.make_synthetic_gid(current_identity, "LineItem")
        let quantity = read_float_field(fields, "quantity", 0.0)
        let amount = case read_money_bag_amount_optional(fields, "priceSet") {
          Some(value) -> value
          None -> read_money_bag_amount(fields, "originalUnitPriceSet", 0.0)
        }
        #(sum +. amount *. quantity, next_identity)
      }
      _ -> acc
    }
  })
}

fn normalize_transactions(
  input: Dict(String, root_field.ResolvedValue),
  identity: SyntheticIdentityRegistry,
  currency: String,
) -> #(List(OrderTransactionRecord), SyntheticIdentityRegistry) {
  let items = case dict.get(input, "transactions") {
    Ok(root_field.ListVal(values)) -> values
    _ -> []
  }
  list.fold(items, #([], identity), fn(acc, item) {
    let #(transactions, current_identity) = acc
    case item {
      root_field.ObjectVal(fields) -> {
        let #(id, next_identity) =
          synthetic_identity.make_synthetic_gid(
            current_identity,
            "OrderTransaction",
          )
        let amount = read_money_bag_amount(fields, "amountSet", 0.0)
        let transaction =
          OrderTransactionRecord(
            id: id,
            kind: read_string_default(fields, "kind", "AUTHORIZATION"),
            status: read_string_default(fields, "status", "SUCCESS"),
            gateway: read_string_field(fields, "gateway"),
            amount: Money(format_amount(amount), currency),
            parent_transaction_id: read_string_field(
              fields,
              "parentTransactionId",
            ),
            payment_id: read_string_field(fields, "paymentId"),
            payment_reference_id: read_string_field(
              fields,
              "paymentReferenceId",
            ),
          )
        #(list.append(transactions, [transaction]), next_identity)
      }
      _ -> acc
    }
  })
}

fn build_payment_transaction(
  identity: SyntheticIdentityRegistry,
  kind: String,
  amount: Float,
  currency: String,
  gateway: Option(String),
  parent_transaction_id: Option(String),
  payment_reference_id: Option(String),
) -> #(OrderTransactionRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_id) =
    synthetic_identity.make_synthetic_gid(identity, "OrderTransaction")
  let #(payment_id, identity_after_payment) =
    synthetic_identity.make_synthetic_gid(identity_after_id, "Payment")
  #(
    OrderTransactionRecord(
      id: id,
      kind: kind,
      status: "SUCCESS",
      gateway: gateway,
      amount: Money(format_amount(amount), currency),
      parent_transaction_id: parent_transaction_id,
      payment_id: Some(payment_id),
      payment_reference_id: payment_reference_id,
    ),
    identity_after_payment,
  )
}

fn order_with_existing_mandate(
  order: OrderRecord,
  mandate: OrderMandatePaymentRecord,
) -> OrderRecord {
  case find_transaction(order, mandate.transaction_id) {
    Some(_) -> order
    None -> order
  }
}

fn apply_payment_derived_fields(order: OrderRecord) -> OrderRecord {
  let received =
    order.transactions
    |> list.filter(is_successful_payment_capture)
    |> list.fold(0.0, fn(sum, transaction) {
      sum +. parse_decimal(transaction.amount.amount)
    })
  let total = parse_decimal(order.total_price)
  let outstanding = max_float(0.0, total -. received)
  let capturable = total_capturable_amount(order)
  let has_voided_authorization =
    list.any(order.transactions, fn(transaction) {
      is_successful_authorization(transaction)
      && transaction_has_voiding_child(order, transaction.id)
    })
  let status = case received >=. total && total >. 0.0 {
    True -> "PAID"
    False ->
      case received >. 0.0 {
        True -> "PARTIALLY_PAID"
        False ->
          case capturable >. 0.0 {
            True -> "AUTHORIZED"
            False ->
              case has_voided_authorization {
                True -> "VOIDED"
                False -> order.display_financial_status
              }
          }
      }
  }
  OrderRecord(
    ..order,
    display_financial_status: status,
    capturable: capturable >. 0.0,
    total_capturable: format_amount(capturable),
    total_outstanding: format_amount(outstanding),
    total_received: format_amount(received),
    net_payment: format_amount(received),
  )
}

fn serialize_order_create_payload(
  order: OrderRecord,
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(UserError),
) -> Json {
  json.object(
    list.map(selected_fields(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, serialize_order(order, selection, fragments))
            "userErrors" -> #(
              key,
              serialize_user_errors(user_errors, selection),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_order_capture_payload(
  transaction: Option(OrderTransactionRecord),
  order: Option(OrderRecord),
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(UserError),
) -> Json {
  json.object(
    list.map(selected_fields(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "transaction" -> #(
              key,
              optional_transaction_json(
                transaction,
                order,
                selection,
                fragments,
              ),
            )
            "order" -> #(key, optional_order_json(order, selection, fragments))
            "userErrors" -> #(
              key,
              serialize_user_errors(user_errors, selection),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_transaction_void_payload(
  transaction: Option(OrderTransactionRecord),
  order: Option(OrderRecord),
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(UserError),
) -> Json {
  json.object(
    list.map(selected_fields(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "transaction" -> #(
              key,
              optional_transaction_json(
                transaction,
                order,
                selection,
                fragments,
              ),
            )
            "userErrors" -> #(
              key,
              serialize_user_errors(user_errors, selection),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_mandate_payload(
  mandate: Option(OrderMandatePaymentRecord),
  order: Option(OrderRecord),
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(UserError),
) -> Json {
  json.object(
    list.map(selected_fields(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "job" -> #(key, optional_job_json(mandate, selection, fragments))
            "paymentReferenceId" -> #(key, case mandate {
              Some(record) -> json.string(record.payment_reference_id)
              None -> json.null()
            })
            "order" -> #(key, optional_order_json(order, selection, fragments))
            "userErrors" -> #(
              key,
              serialize_user_errors(user_errors, selection),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_order_management_payload(
  order: Option(OrderRecord),
  field: Selection,
  fragments: FragmentMap,
  user_errors: List(UserError),
) -> Json {
  json.object(
    list.map(selected_fields(field), fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "order" -> #(key, optional_order_json(order, selection, fragments))
            "userErrors" -> #(
              key,
              serialize_user_errors(user_errors, selection),
            )
            _ -> #(key, json.null())
          }
        _ -> #(key, json.null())
      }
    }),
  )
}

fn serialize_order(
  order: OrderRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(order_source(order), selections, fragments)
    _ -> json.object([])
  }
}

fn optional_order_json(
  order: Option(OrderRecord),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case order {
    Some(record) -> serialize_order(record, field, fragments)
    None -> json.null()
  }
}

fn optional_transaction_json(
  transaction: Option(OrderTransactionRecord),
  order: Option(OrderRecord),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case transaction {
    Some(record) ->
      case field {
        Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
          project_graphql_value(
            transaction_source(record, order),
            selections,
            fragments,
          )
        _ -> json.object([])
      }
    None -> json.null()
  }
}

fn optional_job_json(
  mandate: Option(OrderMandatePaymentRecord),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case mandate {
    Some(record) ->
      case field {
        Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
          project_graphql_value(
            src_object([
              #("__typename", SrcString("Job")),
              #("id", SrcString(record.job_id)),
              #("done", SrcBool(True)),
            ]),
            selections,
            fragments,
          )
        _ -> json.object([])
      }
    None -> json.null()
  }
}

fn order_source(order: OrderRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("Order")),
    #("id", SrcString(order.id)),
    #("displayFinancialStatus", SrcString(order.display_financial_status)),
    #("capturable", SrcBool(order.capturable)),
    #("totalCapturable", SrcString(order.total_capturable)),
    #(
      "totalCapturableSet",
      money_bag_source(order.total_capturable, order.currency_code),
    ),
    #(
      "totalOutstandingSet",
      money_bag_source(order.total_outstanding, order.currency_code),
    ),
    #(
      "totalReceivedSet",
      money_bag_source(order.total_received, order.currency_code),
    ),
    #("netPaymentSet", money_bag_source(order.net_payment, order.currency_code)),
    #(
      "paymentGatewayNames",
      SrcList(list.map(order.payment_gateway_names, SrcString)),
    ),
    #(
      "transactions",
      SrcList(
        list.map(order.transactions, fn(transaction) {
          transaction_source(transaction, Some(order))
        }),
      ),
    ),
  ])
}

fn transaction_source(
  transaction: OrderTransactionRecord,
  order: Option(OrderRecord),
) -> SourceValue {
  src_object([
    #("__typename", SrcString("OrderTransaction")),
    #("id", SrcString(transaction.id)),
    #("kind", SrcString(transaction.kind)),
    #("status", SrcString(transaction.status)),
    #("gateway", option_string_source(transaction.gateway)),
    #("paymentId", option_string_source(transaction.payment_id)),
    #(
      "paymentReferenceId",
      option_string_source(transaction.payment_reference_id),
    ),
    #("parentTransaction", parent_transaction_source(transaction, order)),
    #(
      "amountSet",
      money_bag_source(
        transaction.amount.amount,
        transaction.amount.currency_code,
      ),
    ),
  ])
}

fn parent_transaction_source(
  transaction: OrderTransactionRecord,
  order: Option(OrderRecord),
) -> SourceValue {
  case transaction.parent_transaction_id, order {
    Some(parent_id), Some(record) ->
      case find_transaction(record, parent_id) {
        Some(parent) ->
          src_object([
            #("__typename", SrcString("OrderTransaction")),
            #("id", SrcString(parent.id)),
            #("kind", SrcString(parent.kind)),
            #("status", SrcString(parent.status)),
          ])
        None -> SrcNull
      }
    _, _ -> SrcNull
  }
}

fn money_bag_source(amount: String, currency: String) -> SourceValue {
  src_object([
    #(
      "shopMoney",
      src_object([
        #("amount", SrcString(amount)),
        #("currencyCode", SrcString(currency)),
      ]),
    ),
  ])
}

fn option_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(text) -> SrcString(text)
    None -> SrcNull
  }
}

fn serialize_user_errors(errors: List(UserError), field: Selection) -> Json {
  json.array(errors, fn(error) {
    json.object(
      list.map(selected_fields(field), fn(selection) {
        let key = get_field_response_key(selection)
        case selection {
          Field(name: name, ..) ->
            case name.value {
              "field" -> #(key, json.array(error.field, json.string))
              "message" -> #(key, json.string(error.message))
              _ -> #(key, json.null())
            }
          _ -> #(key, json.null())
        }
      }),
    )
  })
}

fn validation_result(
  field: Selection,
  payload: Json,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(get_field_response_key(field), payload, [], False, []),
    store,
    identity,
  )
}

pub fn access_denied_manual_payment_response() -> Json {
  json.object([
    #("data", json.object([#("orderCreateManualPayment", json.null())])),
    #(
      "errors",
      json.array([access_denied_manual_payment_error()], fn(item) { item }),
    ),
  ])
}

fn access_denied_manual_payment_error() -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Access denied for orderCreateManualPayment field. Required access: "
        <> manual_payment_required_access,
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
        #("requiredAccess", json.string(manual_payment_required_access)),
      ]),
    ),
    #("path", json.array(["orderCreateManualPayment"], json.string)),
  ])
}

fn append_order_log(
  store: Store,
  request_path: String,
  document: String,
  root_field: String,
  staged_ids: List(String),
  notes: String,
  log_id: String,
  received_at: String,
) -> Store {
  let entry =
    store.MutationLogEntry(
      id: log_id,
      received_at: received_at,
      operation_name: Some(root_field),
      path: request_path,
      query: document,
      variables: dict.new(),
      staged_resource_ids: staged_ids,
      status: store.Staged,
      interpreted: store.InterpretedMetadata(
        operation_type: store.Mutation,
        operation_name: Some(root_field),
        root_fields: [root_field],
        primary_root_field: Some(root_field),
        capability: store.Capability(
          operation_name: Some(root_field),
          domain: "orders",
          execution: "stage-locally",
        ),
      ),
      notes: Some(notes),
    )
  store.record_mutation_log_entry(store, entry)
}

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> args
    Error(_) -> dict.new()
  }
}

fn read_object_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(input, key) {
    Ok(root_field.ObjectVal(value)) -> value
    _ -> dict.new()
  }
}

fn read_string_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(value)) if value != "" -> Some(value)
    _ -> None
  }
}

fn read_string_default(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
  fallback: String,
) -> String {
  case read_string_field(input, key) {
    Some(value) -> value
    None -> fallback
  }
}

fn read_float_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
  fallback: Float,
) -> Float {
  case read_float_field_optional(input, key) {
    Some(value) -> value
    None -> fallback
  }
}

fn read_float_field_optional(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Float) {
  case dict.get(input, key) {
    Ok(root_field.IntVal(value)) -> Some(int.to_float(value))
    Ok(root_field.FloatVal(value)) -> Some(value)
    Ok(root_field.StringVal(value)) -> Some(parse_decimal_or(value, 0.0))
    _ -> None
  }
}

fn read_money_bag_amount(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
  fallback: Float,
) -> Float {
  case read_money_bag_amount_optional(input, key) {
    Some(value) -> value
    None -> fallback
  }
}

fn read_money_bag_amount_optional(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Float) {
  let bag = read_object_field(input, key)
  let shop_money = read_object_field(bag, "shopMoney")
  case read_float_field_optional(shop_money, "amount") {
    Some(value) -> Some(value)
    None -> read_float_field_optional(bag, "amount")
  }
}

fn read_payment_input_amount(
  input: Dict(String, root_field.ResolvedValue),
  fallback: Float,
) -> Float {
  read_float_field(input, "amount", fallback)
}

fn read_money_input_amount(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
  fallback: Float,
) -> Float {
  let money = read_object_field(input, key)
  read_float_field(money, "amount", fallback)
}

fn selected_fields(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      list.filter(selections, fn(selection) {
        case selection {
          Field(..) -> True
          _ -> False
        }
      })
    _ -> []
  }
}

fn find_transaction(
  order: OrderRecord,
  transaction_id: String,
) -> Option(OrderTransactionRecord) {
  order.transactions
  |> list.find(fn(transaction) { transaction.id == transaction_id })
  |> option_from_result()
}

fn option_from_result(value: Result(a, b)) -> Option(a) {
  case value {
    Ok(found) -> Some(found)
    Error(_) -> None
  }
}

fn option_then(value: Option(a), mapper: fn(a) -> Option(b)) -> Option(b) {
  case value {
    Some(inner) -> mapper(inner)
    None -> None
  }
}

fn is_successful_authorization(transaction: OrderTransactionRecord) -> Bool {
  transaction.kind == "AUTHORIZATION" && transaction.status == "SUCCESS"
}

fn is_successful_payment_capture(transaction: OrderTransactionRecord) -> Bool {
  transaction.status == "SUCCESS"
  && {
    transaction.kind == "SALE"
    || transaction.kind == "CAPTURE"
    || transaction.kind == "MANDATE_PAYMENT"
  }
}

fn transaction_has_voiding_child(
  order: OrderRecord,
  parent_id: String,
) -> Bool {
  list.any(order.transactions, fn(transaction) {
    transaction.kind == "VOID"
    && transaction.status == "SUCCESS"
    && transaction.parent_transaction_id == Some(parent_id)
  })
}

fn captured_amount_for_authorization(
  order: OrderRecord,
  parent_id: String,
) -> Float {
  order.transactions
  |> list.filter(fn(transaction) {
    transaction.kind == "CAPTURE"
    && transaction.status == "SUCCESS"
    && transaction.parent_transaction_id == Some(parent_id)
  })
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. parse_decimal(transaction.amount.amount)
  })
}

fn capturable_amount_for_authorization(
  order: OrderRecord,
  authorization: OrderTransactionRecord,
) -> Float {
  case
    is_successful_authorization(authorization)
    && !transaction_has_voiding_child(order, authorization.id)
  {
    False -> 0.0
    True ->
      max_float(
        0.0,
        parse_decimal(authorization.amount.amount)
          -. captured_amount_for_authorization(order, authorization.id),
      )
  }
}

fn total_capturable_amount(order: OrderRecord) -> Float {
  order.transactions
  |> list.filter(is_successful_authorization)
  |> list.fold(0.0, fn(sum, transaction) {
    sum +. capturable_amount_for_authorization(order, transaction)
  })
}

fn append_gateway(
  gateways: List(String),
  gateway: Option(String),
) -> List(String) {
  case gateway {
    Some(value) if value != "" -> dedupe_strings(list.append(gateways, [value]))
    _ -> gateways
  }
}

fn dedupe_strings(values: List(String)) -> List(String) {
  list.fold(values, [], fn(acc, value) {
    case list.contains(acc, value) {
      True -> acc
      False -> list.append(acc, [value])
    }
  })
}

fn parse_decimal(raw: String) -> Float {
  parse_decimal_or(raw, 0.0)
}

fn parse_decimal_or(raw: String, fallback: Float) -> Float {
  case float.parse(raw) {
    Ok(value) -> value
    Error(_) -> fallback
  }
}

fn format_amount(value: Float) -> String {
  float.to_string(value)
}

fn max_float(left: Float, right: Float) -> Float {
  case left >. right {
    True -> left
    False -> right
  }
}
