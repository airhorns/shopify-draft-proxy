//// Payments domain port foundation.
////
//// Mirrors the parity-backed parts of `src/proxy/payments.ts`: payment
//// customization empty/validation behavior, customer payment-method local
//// staging, payment reminder intent staging, no-data finance/POS/dispute/Shop
//// Pay reads, and static payment terms templates.

import gleam/bit_array
import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcBool, SrcInt,
  SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_document_fragments, get_field_response_key, paginate_connection_items,
  project_graphql_value, serialize_connection, serialize_empty_connection,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, LogDraft, MutationOutcome, respond_to_query,
}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerPaymentMethodInstrumentRecord, type CustomerPaymentMethodRecord,
  type CustomerRecord, type Money, type PaymentCustomizationRecord,
  type PaymentScheduleRecord, type PaymentTermsRecord,
  type PaymentTermsTemplateRecord, CustomerPaymentMethodInstrumentRecord,
  CustomerPaymentMethodRecord, CustomerPaymentMethodUpdateUrlRecord,
  CustomerRecord, Money, PaymentCustomizationRecord, PaymentReminderSendRecord,
  PaymentScheduleRecord, PaymentTermsRecord, PaymentTermsTemplateRecord,
}

const customization_app_id: String = "347082227713"

const duplication_prefix: String = "shopify-draft-proxy:customer-payment-method-duplication:"

pub type PaymentsError {
  ParseFailed(root_field.RootFieldError)
}

type UserError {
  UserError(field: List(String), message: String, code: Option(String))
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    root_name: String,
    notes: Option(String),
  )
}

pub fn is_payments_query_root(name: String) -> Bool {
  case name {
    "paymentTermsTemplates"
    | "customerPaymentMethod"
    | "draftOrder"
    | "paymentCustomizations"
    | "paymentCustomization"
    | "cashTrackingSession"
    | "cashTrackingSessions"
    | "pointOfSaleDevice"
    | "dispute"
    | "disputeEvidence"
    | "disputes"
    | "shopPayPaymentRequestReceipt"
    | "shopPayPaymentRequestReceipts"
    | "shopifyPaymentsAccount" -> True
    _ -> False
  }
}

pub fn is_payments_mutation_root(name: String) -> Bool {
  case name {
    "paymentCustomizationCreate"
    | "paymentCustomizationUpdate"
    | "paymentCustomizationDelete"
    | "paymentCustomizationActivation"
    | "customerPaymentMethodCreditCardCreate"
    | "customerPaymentMethodCreditCardUpdate"
    | "customerPaymentMethodRemoteCreate"
    | "customerPaymentMethodPaypalBillingAgreementCreate"
    | "customerPaymentMethodPaypalBillingAgreementUpdate"
    | "customerPaymentMethodGetDuplicationData"
    | "customerPaymentMethodCreateFromDuplicationData"
    | "customerPaymentMethodGetUpdateUrl"
    | "customerPaymentMethodRevoke"
    | "paymentTermsCreate"
    | "paymentTermsUpdate"
    | "paymentTermsDelete"
    | "paymentReminderSend" -> True
    _ -> False
  }
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, PaymentsError) {
  use data <- result.try(handle_payments_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
}

/// Uniform query entrypoint matching the dispatcher's signature.
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

pub fn handle_payments_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, PaymentsError) {
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

fn query_payload(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "paymentTermsTemplates" ->
          serialize_payment_terms_templates(field, fragments, variables)
        "customerPaymentMethod" ->
          serialize_customer_payment_method_by_id(
            store,
            field,
            fragments,
            variables,
          )
        "draftOrder" ->
          serialize_draft_order_payment_terms(
            store,
            field,
            fragments,
            variables,
          )
        "paymentCustomizations" ->
          serialize_payment_customizations(store, field, fragments, variables)
        "paymentCustomization" ->
          serialize_payment_customization_by_id(
            store,
            field,
            fragments,
            variables,
          )
        "customer" ->
          serialize_customer_payment_methods_owner(
            store,
            field,
            fragments,
            variables,
          )
        "cashTrackingSession"
        | "pointOfSaleDevice"
        | "dispute"
        | "disputeEvidence"
        | "shopPayPaymentRequestReceipt"
        | "shopifyPaymentsAccount" -> json.null()
        "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" ->
          serialize_empty_connection(field, default_selected_field_options())
        _ -> json.null()
      }
    _ -> json.null()
  }
}

fn has_key(input: Dict(String, root_field.ResolvedValue), key: String) -> Bool {
  case dict.get(input, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn read_string_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(String) {
  case dict.get(input, key) {
    Ok(root_field.StringVal(value)) ->
      case string.trim(value) {
        "" -> None
        _ -> Some(value)
      }
    _ -> None
  }
}

fn read_bool_field(
  input: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(Bool) {
  case dict.get(input, key) {
    Ok(root_field.BoolVal(value)) -> Some(value)
    _ -> None
  }
}

fn is_shopify_gid(value: Option(String), resource_type: String) -> Bool {
  case value {
    Some(id) -> string.starts_with(id, "gid://shopify/" <> resource_type <> "/")
    None -> False
  }
}

fn gid_tail(id: String) -> String {
  case string.split(id, on: "/") |> list.reverse {
    [tail, ..] -> tail
    [] -> id
  }
}

fn option_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
}

fn option_bool_source(value: Option(Bool)) -> SourceValue {
  case value {
    Some(value) -> SrcBool(value)
    None -> SrcNull
  }
}

fn option_int_source(value: Option(Int)) -> SourceValue {
  case value {
    Some(value) -> SrcInt(value)
    None -> SrcNull
  }
}

fn money_source(value: Money) -> SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(value.amount)),
    #("currencyCode", SrcString(value.currency_code)),
  ])
}

fn option_money_source(value: Option(Money)) -> SourceValue {
  case value {
    Some(money) -> money_source(money)
    None -> SrcNull
  }
}

fn payment_terms_templates() -> List(PaymentTermsTemplateRecord) {
  [
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/1",
      "Due on receipt",
      "Due on receipt",
      None,
      "RECEIPT",
      "Due on receipt",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/9",
      "Due on fulfillment",
      "Due on fulfillment",
      None,
      "FULFILLMENT",
      "Due on fulfillment",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/2",
      "Net 7",
      "Within 7 days",
      Some(7),
      "NET",
      "Net 7",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/3",
      "Net 15",
      "Within 15 days",
      Some(15),
      "NET",
      "Net 15",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/4",
      "Net 30",
      "Within 30 days",
      Some(30),
      "NET",
      "Net 30",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/8",
      "Net 45",
      "Within 45 days",
      Some(45),
      "NET",
      "Net 45",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/5",
      "Net 60",
      "Within 60 days",
      Some(60),
      "NET",
      "Net 60",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/6",
      "Net 90",
      "Within 90 days",
      Some(90),
      "NET",
      "Net 90",
    ),
    PaymentTermsTemplateRecord(
      "gid://shopify/PaymentTermsTemplate/7",
      "Fixed",
      "Fixed date",
      None,
      "FIXED",
      "Fixed",
    ),
  ]
}

fn serialize_payment_terms_templates(
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let type_filter =
    graphql_helpers.read_arg_string_nonempty(args, "paymentTermsType")
  let templates =
    payment_terms_templates()
    |> list.filter(fn(template) {
      case type_filter {
        Some(type_) -> template.payment_terms_type == type_
        None -> True
      }
    })
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      json.array(templates, fn(template) {
        project_graphql_value(
          payment_terms_template_source(template),
          selections,
          fragments,
        )
      })
    _ -> json.array([], fn(x) { x })
  }
}

fn payment_terms_template_source(
  template: PaymentTermsTemplateRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentTermsTemplate")),
    #("id", SrcString(template.id)),
    #("name", SrcString(template.name)),
    #("description", SrcString(template.description)),
    #("dueInDays", case template.due_in_days {
      Some(value) -> SrcInt(value)
      None -> SrcNull
    }),
    #("paymentTermsType", SrcString(template.payment_terms_type)),
    #("translatedName", SrcString(template.translated_name)),
  ])
}

fn serialize_payment_customizations(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items = store.list_effective_payment_customizations(store)
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      fn(item, _index) { item.id },
      default_connection_window_options(),
    )
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: window.items,
      has_next_page: window.has_next_page,
      has_previous_page: window.has_previous_page,
      get_cursor_value: fn(item, _index) { item.id },
      serialize_node: fn(item, node_field, _index) {
        project_payment_customization(item, node_field, fragments)
      },
      selected_field_options: default_selected_field_options(),
      page_info_options: default_connection_page_info_options(),
    ),
  )
}

fn serialize_payment_customization_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_payment_customization_by_id(store, id) {
        Some(record) -> project_payment_customization(record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn project_payment_customization(
  customization: PaymentCustomizationRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        payment_customization_source(customization),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn payment_customization_source(
  record: PaymentCustomizationRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentCustomization")),
    #("id", SrcString(record.id)),
    #("legacyResourceId", SrcString(gid_tail(record.id))),
    #("title", option_string_source(record.title)),
    #("enabled", option_bool_source(record.enabled)),
    #("functionId", option_string_source(record.function_id)),
    #("shopifyFunction", SrcNull),
    #("errorHistory", SrcNull),
    #("metafield", SrcNull),
    #("metafields", empty_connection_source()),
  ])
}

fn empty_connection_source() -> SourceValue {
  src_object([
    #("nodes", SrcList([])),
    #("edges", SrcList([])),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", SrcNull),
        #("endCursor", SrcNull),
      ]),
    ),
  ])
}

fn serialize_customer_payment_method_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let show_revoked =
    graphql_helpers.read_arg_bool(args, "showRevoked") |> option.unwrap(False)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case
        store.get_effective_customer_payment_method_by_id(
          store,
          id,
          show_revoked,
        )
      {
        Some(record) ->
          project_customer_payment_method(store, record, field, fragments)
        None -> json.null()
      }
    None -> json.null()
  }
}

fn serialize_customer_payment_methods_owner(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) ->
          project_customer_payment_methods_owner(
            store,
            customer,
            field,
            fragments,
          )
        None -> json.null()
      }
    None -> json.null()
  }
}

fn project_customer_payment_methods_owner(
  store: Store,
  customer: CustomerRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("Customer")),
          #("id", SrcString(customer.id)),
          #(
            "paymentMethods",
            customer_payment_methods_connection_source(store, customer.id),
          ),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn customer_payment_methods_connection_source(
  store: Store,
  customer_id: String,
) -> SourceValue {
  let methods =
    store.list_effective_customer_payment_methods(store, customer_id, True)
  src_object([
    #(
      "nodes",
      SrcList(
        list.map(methods, fn(method) { payment_method_source(store, method) }),
      ),
    ),
    #(
      "edges",
      SrcList(
        list.map(methods, fn(method) {
          src_object([
            #("cursor", SrcString("cursor:" <> method.id)),
            #("node", payment_method_source(store, method)),
          ])
        }),
      ),
    ),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", case methods {
          [first, ..] -> SrcString("cursor:" <> first.id)
          [] -> SrcNull
        }),
        #("endCursor", case list.last(methods) {
          Ok(last) -> SrcString("cursor:" <> last.id)
          Error(_) -> SrcNull
        }),
      ]),
    ),
  ])
}

fn serialize_draft_order_payment_terms(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.payment_terms_owner_exists(store, id) {
        True -> project_draft_order_payment_terms(store, id, field, fragments)
        False -> json.null()
      }
    None -> json.null()
  }
}

fn project_draft_order_payment_terms(
  store: Store,
  owner_id: String,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("DraftOrder")),
          #("id", SrcString(owner_id)),
          #(
            "paymentTerms",
            case
              store.get_effective_payment_terms_by_owner_id(store, owner_id)
            {
              Some(terms) -> payment_terms_source(terms)
              None -> SrcNull
            },
          ),
        ]),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

fn payment_terms_source(record: PaymentTermsRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentTerms")),
    #("id", SrcString(record.id)),
    #("due", SrcBool(record.due)),
    #("overdue", SrcBool(record.overdue)),
    #("dueInDays", option_int_source(record.due_in_days)),
    #("paymentTermsName", SrcString(record.payment_terms_name)),
    #("paymentTermsType", SrcString(record.payment_terms_type)),
    #("translatedName", SrcString(record.translated_name)),
    #(
      "paymentSchedules",
      payment_schedules_connection_source(record.payment_schedules),
    ),
  ])
}

fn payment_schedules_connection_source(
  schedules: List(PaymentScheduleRecord),
) -> SourceValue {
  src_object([
    #("nodes", SrcList(list.map(schedules, payment_schedule_source))),
    #(
      "edges",
      SrcList(
        list.map(schedules, fn(schedule) {
          src_object([
            #("cursor", SrcString("cursor:" <> schedule.id)),
            #("node", payment_schedule_source(schedule)),
          ])
        }),
      ),
    ),
    #(
      "pageInfo",
      src_object([
        #("hasNextPage", SrcBool(False)),
        #("hasPreviousPage", SrcBool(False)),
        #("startCursor", case schedules {
          [first, ..] -> SrcString("cursor:" <> first.id)
          [] -> SrcNull
        }),
        #("endCursor", case list.last(schedules) {
          Ok(last) -> SrcString("cursor:" <> last.id)
          Error(_) -> SrcNull
        }),
      ]),
    ),
  ])
}

fn payment_schedule_source(schedule: PaymentScheduleRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentSchedule")),
    #("id", SrcString(schedule.id)),
    #("issuedAt", option_string_source(schedule.issued_at)),
    #("dueAt", option_string_source(schedule.due_at)),
    #("completedAt", option_string_source(schedule.completed_at)),
    #("due", option_bool_source(schedule.due)),
    #("amount", option_money_source(schedule.amount)),
    #("balanceDue", option_money_source(schedule.balance_due)),
    #("totalBalance", option_money_source(schedule.total_balance)),
  ])
}

fn project_customer_payment_method(
  store: Store,
  method: CustomerPaymentMethodRecord,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        payment_method_source(store, method),
        selections,
        fragments,
      )
    _ -> json.object([])
  }
}

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
      let store =
        hydrate_before_payments_mutation(store, fields, variables, upstream)
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        document,
        variables,
      )
    }
  }
}

fn hydrate_before_payments_mutation(
  store: Store,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  let #(customer_ids, method_ids, owner_ids) =
    list.fold(fields, #([], [], []), fn(acc, field) {
      let #(customer_acc, method_acc, owner_acc) = acc
      let #(customers, methods, owners) =
        payment_mutation_hydrate_inputs(field, variables)
      #(
        list.append(customer_acc, customers),
        list.append(method_acc, methods),
        list.append(owner_acc, owners),
      )
    })
  let with_payment_methods =
    hydrate_customer_payment_method_context(
      store,
      unique_strings(customer_ids, []),
      unique_strings(method_ids, []),
      upstream,
    )
  list.fold(unique_strings(owner_ids, []), with_payment_methods, fn(acc, id) {
    maybe_hydrate_payment_terms_owner(acc, id, upstream)
  })
}

fn payment_mutation_hydrate_inputs(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(List(String), List(String), List(String)) {
  case field {
    Field(name: name, ..) -> {
      let args = graphql_helpers.field_args(field, variables)
      case name.value {
        "customerPaymentMethodCreditCardCreate"
        | "customerPaymentMethodRemoteCreate"
        | "customerPaymentMethodPaypalBillingAgreementCreate" -> #(
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "customerId",
          )),
          [],
          [],
        )
        "customerPaymentMethodCreditCardUpdate"
        | "customerPaymentMethodPaypalBillingAgreementUpdate" -> #(
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(args, "id")),
          [],
        )
        "customerPaymentMethodGetDuplicationData" -> #(
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "targetCustomerId",
          )),
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "customerPaymentMethodId",
          )),
          [],
        )
        "customerPaymentMethodCreateFromDuplicationData" -> {
          let method_id =
            graphql_helpers.read_arg_string_nonempty(
              args,
              "encryptedDuplicationData",
            )
            |> option.then(fn(raw) {
              case decode_duplication_data(raw) {
                Ok(payload) ->
                  dict_string_to_option(payload, "customerPaymentMethodId")
                Error(_) -> None
              }
            })
          #(
            option_to_list(graphql_helpers.read_arg_string_nonempty(
              args,
              "customerId",
            )),
            option_to_list(method_id),
            [],
          )
        }
        "customerPaymentMethodGetUpdateUrl" | "customerPaymentMethodRevoke" -> #(
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "customerPaymentMethodId",
          )),
          [],
        )
        "paymentTermsCreate" -> #(
          [],
          [],
          option_to_list(graphql_helpers.read_arg_string_nonempty(
            args,
            "referenceId",
          )),
        )
        _ -> #([], [], [])
      }
    }
    _ -> #([], [], [])
  }
}

fn option_to_list(value: Option(String)) -> List(String) {
  case value {
    Some(s) -> [s]
    None -> []
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  query: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], store, identity, [])
  let #(entries, final_store, final_identity, staged_ids) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entry_acc, current_store, current_identity, staged_acc) = acc
      let #(result, next_store, next_identity) =
        handle_mutation_field(
          current_store,
          current_identity,
          field,
          fragments,
          variables,
        )
      let result_staged = result.staged_resource_ids
      #(
        list.append(entry_acc, [#(result.key, result.payload)]),
        next_store,
        next_identity,
        list.append(staged_acc, result_staged),
      )
    })
  let root_names = root_names(fields)
  let drafts = case root_names {
    [] -> []
    [primary, ..] -> [
      LogDraft(
        operation_name: Some(primary),
        root_fields: root_names,
        primary_root_field: Some(primary),
        domain: "payments",
        execution: "stage-locally",
        query: Some(query),
        variables: Some(variables),
        staged_resource_ids: staged_ids,
        status: store.Staged,
        notes: Some(
          "Staged payments mutations locally in the in-memory draft store; payment credentials, gateway side effects, customer-facing URLs, and reminder delivery are scrubbed or synthetic.",
        ),
      ),
    ]
  }
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: drafts,
  )
}

fn root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}

fn handle_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "paymentCustomizationCreate" ->
          create_payment_customization(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "paymentCustomizationUpdate" ->
          update_payment_customization(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "paymentCustomizationDelete" ->
          delete_payment_customization(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "paymentCustomizationActivation" ->
          activate_payment_customizations(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodCreditCardCreate" ->
          create_credit_card_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodCreditCardUpdate" ->
          update_credit_card_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodRemoteCreate" ->
          create_remote_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodPaypalBillingAgreementCreate" ->
          create_paypal_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodPaypalBillingAgreementUpdate" ->
          update_paypal_payment_method(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodGetDuplicationData" ->
          get_payment_method_duplication_data(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodCreateFromDuplicationData" ->
          create_payment_method_from_duplication_data(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodGetUpdateUrl" ->
          get_payment_method_update_url(
            store,
            identity,
            field,
            fragments,
            variables,
          )
        "customerPaymentMethodRevoke" ->
          revoke_payment_method(store, identity, field, fragments, variables)
        "paymentTermsCreate" ->
          create_payment_terms(store, identity, field, fragments, variables)
        "paymentTermsUpdate" ->
          update_payment_terms(store, identity, field, fragments, variables)
        "paymentTermsDelete" ->
          delete_payment_terms(store, identity, field, fragments, variables)
        "paymentReminderSend" ->
          send_payment_reminder(store, identity, field, fragments, variables)
        _ -> #(
          MutationFieldResult(
            get_field_response_key(field),
            json.null(),
            [],
            name.value,
            None,
          ),
          store,
          identity,
        )
      }
    _ -> #(MutationFieldResult("", json.null(), [], "", None), store, identity)
  }
}

fn payment_customization_error(
  field: List(String),
  message: String,
  code: String,
) -> UserError {
  UserError(field: field, message: message, code: Some(code))
}

fn required_customization_input_error(field_name: String) -> UserError {
  payment_customization_error(
    ["paymentCustomization", field_name],
    "Required input field must be present.",
    "REQUIRED_INPUT_FIELD",
  )
}

fn missing_function_error(function_id: String) -> UserError {
  payment_customization_error(
    ["paymentCustomization", "functionId"],
    "Function "
      <> function_id
      <> " not found. Ensure that it is released in the current app ("
      <> customization_app_id
      <> "), and that the app is installed.",
    "FUNCTION_NOT_FOUND",
  )
}

fn customization_not_found_error(field_name: String, id: String) -> UserError {
  payment_customization_error(
    [field_name],
    "Could not find PaymentCustomization with id: " <> id,
    "PAYMENT_CUSTOMIZATION_NOT_FOUND",
  )
}

fn customization_activation_not_found_error(ids: List(String)) -> UserError {
  payment_customization_error(
    ["ids"],
    "Could not find payment customizations with IDs: " <> string.join(ids, ", "),
    "PAYMENT_CUSTOMIZATION_NOT_FOUND",
  )
}

fn validate_create_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case
    has_key(input, "title"),
    has_key(input, "enabled"),
    read_string_field(input, "functionId")
  {
    False, _, _ -> [required_customization_input_error("title")]
    _, False, _ -> [required_customization_input_error("enabled")]
    _, _, None -> [required_customization_input_error("functionId")]
    _, _, Some(function_id) ->
      case gid_tail(function_id) == "0" {
        True -> [missing_function_error(function_id)]
        False -> []
      }
  }
}

fn create_payment_customization(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "paymentCustomization",
    )
    |> option.unwrap(dict.new())
  let errors = validate_create_input(input)
  case errors {
    [_, ..] -> #(
      MutationFieldResult(
        get_field_response_key(field),
        customization_payload(None, errors, field, fragments),
        [],
        "paymentCustomizationCreate",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      ),
      store,
      identity,
    )
    [] -> {
      let #(id, next_identity) =
        synthetic_identity.make_synthetic_gid(identity, "PaymentCustomization")
      let record =
        PaymentCustomizationRecord(
          id: id,
          title: read_string_field(input, "title"),
          enabled: read_bool_field(input, "enabled"),
          function_id: read_string_field(input, "functionId"),
          function_handle: read_string_field(input, "functionHandle"),
          metafields: [],
        )
      let next_store = store.upsert_staged_payment_customization(store, record)
      #(
        MutationFieldResult(
          get_field_response_key(field),
          customization_payload(Some(record), [], field, fragments),
          [id],
          "paymentCustomizationCreate",
          Some(
            "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
          ),
        ),
        next_store,
        next_identity,
      )
    }
  }
}

fn update_payment_customization(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let id =
    graphql_helpers.read_arg_string_nonempty(args, "id") |> option.unwrap("")
  case store.get_effective_payment_customization_by_id(store, id) {
    None -> #(
      MutationFieldResult(
        get_field_response_key(field),
        customization_payload(
          None,
          [customization_not_found_error("id", id)],
          field,
          fragments,
        ),
        [],
        "paymentCustomizationUpdate",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      ),
      store,
      identity,
    )
    Some(current) -> {
      let input =
        graphql_helpers.read_arg_object(args, "paymentCustomization")
        |> option.unwrap(dict.new())
      let updated =
        PaymentCustomizationRecord(
          ..current,
          title: read_string_field(input, "title") |> option.or(current.title),
          enabled: read_bool_field(input, "enabled")
            |> option.or(current.enabled),
          function_id: read_string_field(input, "functionId")
            |> option.or(current.function_id),
        )
      let next_store = store.upsert_staged_payment_customization(store, updated)
      #(
        MutationFieldResult(
          get_field_response_key(field),
          customization_payload(Some(updated), [], field, fragments),
          [updated.id],
          "paymentCustomizationUpdate",
          Some(
            "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
          ),
        ),
        next_store,
        identity,
      )
    }
  }
}

fn delete_payment_customization(store, identity, field, fragments, variables) {
  let id =
    graphql_helpers.read_arg_string_nonempty(
      graphql_helpers.field_args(field, variables),
      "id",
    )
    |> option.unwrap("")
  case store.get_effective_payment_customization_by_id(store, id) {
    None ->
      mutation_payload_result(
        store,
        identity,
        field,
        delete_customization_payload(
          None,
          [customization_not_found_error("id", id)],
          field,
          fragments,
        ),
        [],
        "paymentCustomizationDelete",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      )
    Some(_) ->
      mutation_payload_result(
        store.delete_staged_payment_customization(store, id),
        identity,
        field,
        delete_customization_payload(Some(id), [], field, fragments),
        [id],
        "paymentCustomizationDelete",
        Some(
          "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
        ),
      )
  }
}

fn activate_payment_customizations(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  let ids = read_string_list(args, "ids") |> unique_strings([])
  let enabled =
    graphql_helpers.read_arg_bool(args, "enabled") |> option.unwrap(False)
  let #(next_store, updated_ids, missing_ids) =
    list.fold(ids, #(store, [], []), fn(acc, id) {
      let #(current_store, updated, missing) = acc
      case store.get_effective_payment_customization_by_id(current_store, id) {
        Some(record) -> {
          let next =
            store.upsert_staged_payment_customization(
              current_store,
              PaymentCustomizationRecord(..record, enabled: Some(enabled)),
            )
          #(next, list.append(updated, [id]), missing)
        }
        None -> #(current_store, updated, list.append(missing, [id]))
      }
    })
  let errors = case missing_ids {
    [] -> []
    _ -> [customization_activation_not_found_error(missing_ids)]
  }
  mutation_payload_result(
    next_store,
    identity,
    field,
    activation_payload(updated_ids, errors, field, fragments),
    updated_ids,
    "paymentCustomizationActivation",
    Some(
      "Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.",
    ),
  )
}

fn read_string_list(
  args: Dict(String, root_field.ResolvedValue),
  key: String,
) -> List(String) {
  case dict.get(args, key) {
    Ok(root_field.ListVal(items)) ->
      list.filter_map(items, fn(item) {
        case item {
          root_field.StringVal(value) -> Ok(value)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
}

fn unique_strings(items: List(String), seen: List(String)) -> List(String) {
  case items {
    [] -> []
    [first, ..rest] ->
      case list.contains(seen, first) {
        True -> unique_strings(rest, seen)
        False -> [first, ..unique_strings(rest, [first, ..seen])]
      }
  }
}

fn customization_payload(
  customization: Option(PaymentCustomizationRecord),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(field, fragments, [
    #("paymentCustomization", case customization {
      Some(record) -> payment_customization_source(record)
      None -> SrcNull
    }),
    #("userErrors", user_errors_source(errors)),
  ])
}

fn delete_customization_payload(
  deleted_id: Option(String),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(field, fragments, [
    #("deletedId", option_string_source(deleted_id)),
    #("userErrors", user_errors_source(errors)),
  ])
}

fn activation_payload(
  ids: List(String),
  errors: List(UserError),
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_payload(field, fragments, [
    #("ids", SrcList(list.map(ids, SrcString))),
    #("userErrors", user_errors_source(errors)),
  ])
}

fn payment_method_error(
  field: String,
  message: String,
  code: String,
) -> UserError {
  UserError(field: [field], message: message, code: Some(code))
}

fn payment_method_missing_error(field: String) -> UserError {
  payment_method_error(
    field,
    "Customer payment method does not exist",
    "PAYMENT_METHOD_DOES_NOT_EXIST",
  )
}

fn payment_method_active_contract_error(field: String) -> UserError {
  payment_method_error(
    field,
    "Customer payment method cannot be revoked because it has active subscription contracts",
    "ACTIVE_CONTRACT",
  )
}

fn customer_missing_error(field: String) -> UserError {
  payment_method_error(
    field,
    "Customer does not exist",
    "CUSTOMER_DOES_NOT_EXIST",
  )
}

fn active_payment_method(
  store: Store,
  id: Option(String),
  field_name: String,
) -> Result(CustomerPaymentMethodRecord, UserError) {
  payment_method_by_id(store, id, field_name, False)
}

fn payment_method_by_id(
  store: Store,
  id: Option(String),
  field_name: String,
  show_revoked: Bool,
) -> Result(CustomerPaymentMethodRecord, UserError) {
  case is_shopify_gid(id, "CustomerPaymentMethod"), id {
    True, Some(payment_id) ->
      case
        store.get_effective_customer_payment_method_by_id(
          store,
          payment_id,
          show_revoked,
        )
      {
        Some(record) -> Ok(record)
        None -> Error(payment_method_missing_error(field_name))
      }
    _, _ -> Error(payment_method_missing_error(field_name))
  }
}

fn customer_by_id(
  store: Store,
  id: Option(String),
  field_name: String,
) -> Result(CustomerRecord, UserError) {
  case is_shopify_gid(id, "Customer"), id {
    True, Some(customer_id) ->
      case store.get_effective_customer_by_id(store, customer_id) {
        Some(customer) -> Ok(customer)
        None -> Error(customer_missing_error(field_name))
      }
    _, _ -> Error(customer_missing_error(field_name))
  }
}

fn hydrate_customer_payment_method_context(
  store: Store,
  customer_ids: List(String),
  method_ids: List(String),
  upstream: UpstreamContext,
) -> Store {
  let missing_customer_ids =
    customer_ids
    |> list.filter(fn(id) {
      is_shopify_gid(Some(id), "Customer")
      && case store.get_effective_customer_by_id(store, id) {
        Some(_) -> False
        None -> True
      }
    })
  let missing_method_ids =
    method_ids
    |> list.filter(fn(id) {
      is_shopify_gid(Some(id), "CustomerPaymentMethod")
      && case
        store.get_effective_customer_payment_method_by_id(store, id, True)
      {
        Some(_) -> False
        None -> True
      }
    })
  case missing_customer_ids, missing_method_ids {
    [], [] -> store
    _, _ -> {
      // Pattern 2: local-runtime payment-method flows need existing
      // customers and vaulted-method shells before staging local-only
      // mutations. Snapshot/no-cassette mode keeps the unknown-resource
      // userErrors instead of inventing state.
      let variables =
        json.object([
          #("customerIds", json.array(missing_customer_ids, json.string)),
          #(
            "customerPaymentMethodIds",
            json.array(missing_method_ids, json.string),
          ),
        ])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "CustomerPaymentMethodHydrate",
          customer_payment_method_hydrate_query(),
          variables,
        )
      {
        Ok(value) ->
          hydrate_customer_payment_methods_from_response(store, value)
        Error(_) -> store
      }
    }
  }
}

fn customer_payment_method_hydrate_query() -> String {
  "query CustomerPaymentMethodHydrate($customerIds: [ID!]!, $customerPaymentMethodIds: [ID!]!) {\n"
  <> "  customers: nodes(ids: $customerIds) { ... on Customer { id email displayName state } }\n"
  <> "  customerPaymentMethods: nodes(ids: $customerPaymentMethodIds) {\n"
  <> "    ... on CustomerPaymentMethod {\n"
  <> "      id revokedAt revokedReason customer { id }\n"
  <> "      instrument { __typename ... on CustomerCreditCard { lastDigits maskedNumber } ... on CustomerPaypalBillingAgreement { paypalAccountEmail inactive } }\n"
  <> "    }\n"
  <> "  }\n"
  <> "}\n"
}

fn hydrate_customer_payment_methods_from_response(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) -> {
      let customers =
        json_array_items(json_get(data, "customers"))
        |> list.filter_map(customer_from_hydrate_node)
      let methods =
        json_array_items(json_get(data, "customerPaymentMethods"))
        |> list.filter_map(customer_payment_method_from_hydrate_node)
      let with_customers = store.upsert_base_customers(store, customers)
      store.upsert_base_customer_payment_methods(with_customers, methods)
    }
    None -> store
  }
}

fn customer_from_hydrate_node(
  node: commit.JsonValue,
) -> Result(CustomerRecord, Nil) {
  case json_get_string(node, "id") {
    Some(id) ->
      Ok(CustomerRecord(
        id: id,
        first_name: None,
        last_name: None,
        display_name: json_get_string(node, "displayName"),
        email: json_get_string(node, "email"),
        legacy_resource_id: Some(gid_tail(id)),
        locale: None,
        note: None,
        can_delete: None,
        verified_email: None,
        data_sale_opt_out: False,
        tax_exempt: None,
        tax_exemptions: [],
        state: json_get_string(node, "state"),
        tags: [],
        number_of_orders: None,
        amount_spent: None,
        default_email_address: None,
        default_phone_number: None,
        email_marketing_consent: None,
        sms_marketing_consent: None,
        default_address: None,
        created_at: None,
        updated_at: None,
      ))
    None -> Error(Nil)
  }
}

fn customer_payment_method_from_hydrate_node(
  node: commit.JsonValue,
) -> Result(CustomerPaymentMethodRecord, Nil) {
  let customer_id =
    json_get_string(node, "customerId")
    |> option.or(
      json_get(node, "customer")
      |> option.then(fn(customer) { json_get_string(customer, "id") }),
    )
  case json_get_string(node, "id"), customer_id {
    Some(id), Some(owner_id) ->
      Ok(
        CustomerPaymentMethodRecord(
          id: id,
          customer_id: owner_id,
          cursor: None,
          instrument: json_get(node, "instrument")
            |> option.then(instrument_from_hydrate_node),
          revoked_at: json_get_string(node, "revokedAt"),
          revoked_reason: json_get_string(node, "revokedReason"),
          subscription_contracts: [],
        ),
      )
    _, _ -> Error(Nil)
  }
}

fn instrument_from_hydrate_node(
  node: commit.JsonValue,
) -> Option(CustomerPaymentMethodInstrumentRecord) {
  let type_name =
    json_get_string(node, "typeName")
    |> option.or(json_get_string(node, "__typename"))
  use resolved_type <- option.then(type_name)
  let data_node = case json_get(node, "data") {
    Some(data) -> data
    None -> node
  }
  Some(CustomerPaymentMethodInstrumentRecord(
    type_name: resolved_type,
    data: dict.from_list(
      list.filter_map(
        [
          "lastDigits",
          "maskedNumber",
          "paypalAccountEmail",
          "inactive",
        ],
        fn(key) {
          case json_get_data_string(data_node, key) {
            Some(value) -> Ok(#(key, value))
            None -> Error(Nil)
          }
        },
      ),
    ),
  ))
}

fn scrubbed_credit_card_instrument() -> CustomerPaymentMethodInstrumentRecord {
  CustomerPaymentMethodInstrumentRecord(
    type_name: "CustomerCreditCard",
    data: dict.from_list([
      #("lastDigits", "__null"),
      #("maskedNumber", "__null"),
    ]),
  )
}

fn scrubbed_paypal_instrument(
  inactive: Bool,
) -> CustomerPaymentMethodInstrumentRecord {
  CustomerPaymentMethodInstrumentRecord(
    type_name: "CustomerPaypalBillingAgreement",
    data: dict.from_list([
      #("paypalAccountEmail", "__null"),
      #("inactive", case inactive {
        True -> "true"
        False -> "false"
      }),
    ]),
  )
}

fn create_payment_method_record(
  identity: SyntheticIdentityRegistry,
  customer_id: String,
  instrument: Option(CustomerPaymentMethodInstrumentRecord),
) -> #(CustomerPaymentMethodRecord, SyntheticIdentityRegistry) {
  let #(id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "CustomerPaymentMethod")
  #(
    CustomerPaymentMethodRecord(
      id: id,
      customer_id: customer_id,
      cursor: None,
      instrument: instrument,
      revoked_at: None,
      revoked_reason: None,
      subscription_contracts: [],
    ),
    next_identity,
  )
}

fn create_credit_card_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    customer_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerId"),
      "customerId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodCreditCardCreate",
        None,
        [error],
        [#("processing", SrcBool(False))],
      )
    Ok(customer) -> {
      let #(record, next_identity) =
        create_payment_method_record(
          identity,
          customer.id,
          Some(scrubbed_credit_card_instrument()),
        )
      payment_method_result(
        store.stage_customer_payment_method(store, record),
        next_identity,
        field,
        fragments,
        "customerPaymentMethodCreditCardCreate",
        Some(record),
        [],
        [#("processing", SrcBool(False))],
      )
    }
  }
}

fn update_credit_card_payment_method(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "id"),
      "id",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodCreditCardUpdate",
        None,
        [error],
        [#("processing", SrcBool(False))],
      )
    Ok(current) -> {
      let updated =
        CustomerPaymentMethodRecord(
          ..current,
          instrument: Some(scrubbed_credit_card_instrument()),
        )
      payment_method_result(
        store.stage_customer_payment_method(store, updated),
        identity,
        field,
        fragments,
        "customerPaymentMethodCreditCardUpdate",
        Some(updated),
        [],
        [#("processing", SrcBool(False))],
      )
    }
  }
}

fn count_object_values(input: Dict(String, root_field.ResolvedValue)) -> Int {
  input
  |> dict.values
  |> list.filter(fn(value) {
    case value {
      root_field.ObjectVal(_) -> True
      _ -> False
    }
  })
  |> list.length
}

fn create_remote_payment_method(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  case
    customer_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerId"),
      "customerId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodRemoteCreate",
        None,
        [error],
        [],
      )
    Ok(customer) -> {
      let remote_reference =
        graphql_helpers.read_arg_object(args, "remoteReference")
        |> option.unwrap(dict.new())
      case count_object_values(remote_reference) == 1 {
        False ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodRemoteCreate",
            None,
            [
              UserError(
                field: ["remoteReference"],
                message: "Exactly one remote reference is required",
                code: Some("EXACTLY_ONE_REMOTE_REFERENCE_REQUIRED"),
              ),
            ],
            [],
          )
        True -> {
          let #(record, next_identity) =
            create_payment_method_record(identity, customer.id, None)
          payment_method_result(
            store.stage_customer_payment_method(store, record),
            next_identity,
            field,
            fragments,
            "customerPaymentMethodRemoteCreate",
            Some(record),
            [],
            [],
          )
        }
      }
    }
  }
}

fn create_paypal_payment_method(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  case
    customer_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerId"),
      "customerId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementCreate",
        None,
        [error],
        [],
      )
    Ok(customer) -> {
      let inactive =
        graphql_helpers.read_arg_bool(args, "inactive") |> option.unwrap(False)
      let #(record, next_identity) =
        create_payment_method_record(
          identity,
          customer.id,
          Some(scrubbed_paypal_instrument(inactive)),
        )
      payment_method_result(
        store.stage_customer_payment_method(store, record),
        next_identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementCreate",
        Some(record),
        [],
        [],
      )
    }
  }
}

fn update_paypal_payment_method(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "id"),
      "id",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementUpdate",
        None,
        [error],
        [],
      )
    Ok(current) -> {
      let inactive = case current.instrument {
        Some(instrument) ->
          case dict.get(instrument.data, "inactive") {
            Ok("true") -> True
            _ -> False
          }
        None -> False
      }
      let updated =
        CustomerPaymentMethodRecord(
          ..current,
          instrument: Some(scrubbed_paypal_instrument(inactive)),
        )
      payment_method_result(
        store.stage_customer_payment_method(store, updated),
        identity,
        field,
        fragments,
        "customerPaymentMethodPaypalBillingAgreementUpdate",
        Some(updated),
        [],
        [],
      )
    }
  }
}

fn get_payment_method_duplication_data(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId"),
      "customerPaymentMethodId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodGetDuplicationData",
        None,
        [error],
        [#("encryptedDuplicationData", SrcNull)],
      )
    Ok(method) ->
      case
        customer_by_id(
          store,
          graphql_helpers.read_arg_string_nonempty(args, "targetCustomerId"),
          "targetCustomerId",
        )
      {
        Error(error) ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodGetDuplicationData",
            None,
            [error],
            [#("encryptedDuplicationData", SrcNull)],
          )
        Ok(target_customer) -> {
          let target_shop_id =
            graphql_helpers.read_arg_string_nonempty(args, "targetShopId")
            |> option.unwrap("")
          let data =
            encode_duplication_data(
              method.id,
              target_customer.id,
              target_shop_id,
            )
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodGetDuplicationData",
            None,
            [],
            [#("encryptedDuplicationData", SrcString(data))],
          )
        }
      }
  }
}

fn create_payment_method_from_duplication_data(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  case customer_by_id(store, customer_id, "customerId") {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodCreateFromDuplicationData",
        None,
        [error],
        [],
      )
    Ok(customer) -> {
      case
        graphql_helpers.read_arg_string_nonempty(
          args,
          "encryptedDuplicationData",
        )
      {
        None -> invalid_duplication_result(store, identity, field, fragments)
        Some(raw) ->
          case decode_duplication_data(raw) {
            Error(_) ->
              invalid_duplication_result(store, identity, field, fragments)
            Ok(payload) -> {
              let source_id =
                dict_string_to_option(payload, "customerPaymentMethodId")
              let target_id = dict_string_to_option(payload, "targetCustomerId")
              case target_id == Some(customer.id), source_id {
                True, Some(method_id) ->
                  case
                    store.get_effective_customer_payment_method_by_id(
                      store,
                      method_id,
                      True,
                    )
                  {
                    Some(source_method) -> {
                      let #(record, next_identity) =
                        create_payment_method_record(
                          identity,
                          customer.id,
                          source_method.instrument,
                        )
                      payment_method_result(
                        store.stage_customer_payment_method(store, record),
                        next_identity,
                        field,
                        fragments,
                        "customerPaymentMethodCreateFromDuplicationData",
                        Some(record),
                        [],
                        [],
                      )
                    }
                    None ->
                      invalid_duplication_result(
                        store,
                        identity,
                        field,
                        fragments,
                      )
                  }
                _, _ ->
                  invalid_duplication_result(store, identity, field, fragments)
              }
            }
          }
      }
    }
  }
}

fn dict_string_to_option(
  data: Dict(String, String),
  key: String,
) -> Option(String) {
  case dict.get(data, key) {
    Ok(value) -> Some(value)
    Error(_) -> None
  }
}

fn invalid_duplication_result(store, identity, field, fragments) {
  payment_method_result(
    store,
    identity,
    field,
    fragments,
    "customerPaymentMethodCreateFromDuplicationData",
    None,
    [
      UserError(
        field: ["encryptedDuplicationData"],
        message: "Encrypted duplication data is invalid",
        code: Some("INVALID_ENCRYPTED_DUPLICATION_DATA"),
      ),
    ],
    [],
  )
}

fn get_payment_method_update_url(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  case
    active_payment_method(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId"),
      "customerPaymentMethodId",
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodGetUpdateUrl",
        None,
        [error],
        [#("updatePaymentMethodUrl", SrcNull)],
      )
    Ok(method) -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(
          identity,
          "CustomerPaymentMethodUpdateUrl",
        )
      let #(created_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let update_url =
        "https://shopify-draft-proxy.local/customer-payment-methods/"
        <> uri_encode(gid_tail(method.id))
        <> "/update?token=local-only"
      let record =
        CustomerPaymentMethodUpdateUrlRecord(
          id: id,
          customer_payment_method_id: method.id,
          update_payment_method_url: update_url,
          created_at: created_at,
        )
      payment_method_result(
        store.stage_customer_payment_method_update_url(store, record),
        next_identity,
        field,
        fragments,
        "customerPaymentMethodGetUpdateUrl",
        None,
        [],
        [#("updatePaymentMethodUrl", SrcString(update_url))],
      )
    }
  }
}

fn revoke_payment_method(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  case
    payment_method_by_id(
      store,
      graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId"),
      "customerPaymentMethodId",
      True,
    )
  {
    Error(error) ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "customerPaymentMethodRevoke",
        None,
        [error],
        [#("revokedCustomerPaymentMethodId", SrcNull)],
      )
    Ok(method) -> {
      case method.subscription_contracts, method.revoked_at {
        [_, ..], _ ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodRevoke",
            None,
            [payment_method_active_contract_error("customerPaymentMethodId")],
            [#("revokedCustomerPaymentMethodId", SrcNull)],
          )
        _, Some(_) ->
          payment_method_result(
            store,
            identity,
            field,
            fragments,
            "customerPaymentMethodRevoke",
            None,
            [],
            [
              #(
                "revokedCustomerPaymentMethodId",
                SrcString(customer_payment_method_gid_from_token(method.id)),
              ),
            ],
          )
        _, None -> {
          let #(_, identity_after_intent) =
            synthetic_identity.make_synthetic_timestamp(identity)
          let #(revoked_at, next_identity) =
            synthetic_identity.make_synthetic_timestamp(identity_after_intent)
          let revoked =
            CustomerPaymentMethodRecord(
              ..method,
              revoked_at: Some(revoked_at),
              revoked_reason: Some("CUSTOMER_REVOKED"),
            )
          payment_method_result(
            store.stage_customer_payment_method(store, revoked),
            next_identity,
            field,
            fragments,
            "customerPaymentMethodRevoke",
            None,
            [],
            [
              #(
                "revokedCustomerPaymentMethodId",
                SrcString(customer_payment_method_gid_from_token(method.id)),
              ),
            ],
          )
        }
      }
    }
  }
}

fn customer_payment_method_gid_from_token(id: String) -> String {
  "gid://shopify/CustomerPaymentMethod/" <> gid_tail(id)
}

fn payment_terms_error(
  field: List(String),
  message: String,
  code: String,
) -> UserError {
  UserError(field: field, message: message, code: Some(code))
}

fn maybe_hydrate_payment_terms_owner(
  store: Store,
  owner_id: String,
  upstream: UpstreamContext,
) -> Store {
  case
    is_shopify_gid(Some(owner_id), "DraftOrder"),
    store.payment_terms_owner_exists(store, owner_id)
  {
    True, False -> {
      // Pattern 2: paymentTermsCreate needs the upstream draft-order
      // reference to exist before staging local payment terms. Snapshot
      // or no-cassette mode preserves Shopify-like REFERENCE_DOES_NOT_EXIST.
      let variables = json.object([#("id", json.string(owner_id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "PaymentTermsOwnerHydrate",
          payment_terms_owner_hydrate_query(),
          variables,
        )
      {
        Ok(value) ->
          case payment_terms_owner_exists_in_response(value) {
            True -> store.register_payment_terms_owner(store, owner_id)
            False -> store
          }
        Error(_) -> store
      }
    }
    _, _ -> store
  }
}

fn payment_terms_owner_hydrate_query() -> String {
  "query PaymentTermsOwnerHydrate($id: ID!) {\n"
  <> "  draftOrder(id: $id) { id paymentTerms { id } }\n"
  <> "}\n"
}

fn payment_terms_owner_exists_in_response(value: commit.JsonValue) -> Bool {
  json_get(value, "data")
  |> option.then(fn(data) { json_get(data, "draftOrder") })
  |> non_null_json
  |> option.is_some
}

fn create_payment_terms(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let reference_id =
    graphql_helpers.read_arg_string_nonempty(args, "referenceId")
  let attrs =
    graphql_helpers.read_arg_object(args, "paymentTermsAttributes")
    |> option.unwrap(dict.new())
  case
    reference_id,
    reference_id |> option.map(store.payment_terms_owner_exists(store, _))
  {
    Some(owner_id), Some(True) -> {
      let #(record, next_identity) =
        build_payment_terms(identity, owner_id, attrs, None)
      payment_terms_result(
        store.upsert_staged_payment_terms(store, record),
        next_identity,
        field,
        fragments,
        "paymentTermsCreate",
        Some(record),
        [],
        [record.id],
      )
    }
    _, _ ->
      payment_terms_result(
        store,
        identity,
        field,
        fragments,
        "paymentTermsCreate",
        None,
        [
          payment_terms_error(
            ["referenceId"],
            "Reference does not exist",
            "REFERENCE_DOES_NOT_EXIST",
          ),
        ],
        [],
      )
  }
}

fn update_payment_terms(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_string_field(input, "paymentTermsId")
  let attrs =
    graphql_helpers.read_arg_object(input, "paymentTermsAttributes")
    |> option.unwrap(dict.new())
  case id {
    Some(payment_terms_id) ->
      case store.get_effective_payment_terms_by_id(store, payment_terms_id) {
        Some(current) -> {
          let #(record, next_identity) =
            build_payment_terms(
              identity,
              current.owner_id,
              attrs,
              Some(current.id),
            )
          payment_terms_result(
            store.upsert_staged_payment_terms(store, record),
            next_identity,
            field,
            fragments,
            "paymentTermsUpdate",
            Some(record),
            [],
            [record.id],
          )
        }
        None ->
          payment_terms_result(
            store,
            identity,
            field,
            fragments,
            "paymentTermsUpdate",
            None,
            [
              payment_terms_error(
                ["input", "paymentTermsId"],
                "Payment terms do not exist",
                "PAYMENT_TERMS_NOT_FOUND",
              ),
            ],
            [],
          )
      }
    None ->
      payment_terms_result(
        store,
        identity,
        field,
        fragments,
        "paymentTermsUpdate",
        None,
        [
          payment_terms_error(
            ["input", "paymentTermsId"],
            "Payment terms do not exist",
            "PAYMENT_TERMS_NOT_FOUND",
          ),
        ],
        [],
      )
  }
}

fn delete_payment_terms(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let id = read_string_field(input, "paymentTermsId")
  case id {
    Some(payment_terms_id) ->
      case store.get_effective_payment_terms_by_id(store, payment_terms_id) {
        Some(_) ->
          mutation_payload_result(
            store.delete_staged_payment_terms(store, payment_terms_id),
            identity,
            field,
            project_payload(field, fragments, [
              #("deletedId", SrcString(payment_terms_id)),
              #("userErrors", user_errors_source([])),
            ]),
            [payment_terms_id],
            "paymentTermsDelete",
            Some(
              "Staged payment terms deletion locally in the in-memory payment terms draft store.",
            ),
          )
        None ->
          mutation_payload_result(
            store,
            identity,
            field,
            project_payload(field, fragments, [
              #("deletedId", SrcNull),
              #(
                "userErrors",
                user_errors_source([
                  payment_terms_error(
                    ["input", "paymentTermsId"],
                    "Payment terms do not exist",
                    "PAYMENT_TERMS_NOT_FOUND",
                  ),
                ]),
              ),
            ]),
            [],
            "paymentTermsDelete",
            Some(
              "Staged payment terms deletion locally in the in-memory payment terms draft store.",
            ),
          )
      }
    None ->
      mutation_payload_result(
        store,
        identity,
        field,
        project_payload(field, fragments, [
          #("deletedId", SrcNull),
          #(
            "userErrors",
            user_errors_source([
              payment_terms_error(
                ["input", "paymentTermsId"],
                "Payment terms do not exist",
                "PAYMENT_TERMS_NOT_FOUND",
              ),
            ]),
          ),
        ]),
        [],
        "paymentTermsDelete",
        Some(
          "Staged payment terms deletion locally in the in-memory payment terms draft store.",
        ),
      )
  }
}

fn build_payment_terms(
  identity: SyntheticIdentityRegistry,
  owner_id: String,
  attrs: Dict(String, root_field.ResolvedValue),
  existing_id: Option(String),
) -> #(PaymentTermsRecord, SyntheticIdentityRegistry) {
  let #(id, identity_after_terms) = case existing_id {
    Some(value) -> #(value, identity)
    None -> synthetic_identity.make_synthetic_gid(identity, "PaymentTerms")
  }
  let #(schedule_id, next_identity) =
    synthetic_identity.make_synthetic_gid(
      identity_after_terms,
      "PaymentSchedule",
    )
  let template =
    read_string_field(attrs, "paymentTermsTemplateId")
    |> option.then(find_payment_terms_template)
    |> option.unwrap(
      payment_terms_templates()
      |> list.first
      |> result.unwrap(PaymentTermsTemplateRecord(
        "gid://shopify/PaymentTermsTemplate/4",
        "Net 30",
        "Within 30 days",
        Some(30),
        "NET",
        "Net 30",
      )),
    )
  let schedule_attrs = first_schedule_attrs(attrs)
  let issued_at = read_string_field(schedule_attrs, "issuedAt")
  let due_at =
    read_string_field(schedule_attrs, "dueAt")
    |> option.or(case template.due_in_days, issued_at {
      Some(days), Some(issued) -> add_days(issued, days)
      _, _ -> None
    })
  let amount = Money(amount: "18.5", currency_code: "CAD")
  #(
    PaymentTermsRecord(
      id: id,
      owner_id: owner_id,
      due: False,
      overdue: False,
      due_in_days: template.due_in_days,
      payment_terms_name: template.name,
      payment_terms_type: template.payment_terms_type,
      translated_name: template.translated_name,
      payment_schedules: [
        PaymentScheduleRecord(
          id: schedule_id,
          due_at: due_at,
          issued_at: case template.payment_terms_type {
            "FIXED" -> None
            _ -> issued_at
          },
          completed_at: None,
          due: Some(False),
          amount: Some(amount),
          balance_due: Some(amount),
          total_balance: Some(amount),
        ),
      ],
    ),
    next_identity,
  )
}

fn first_schedule_attrs(
  attrs: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case dict.get(attrs, "paymentSchedules") {
    Ok(root_field.ListVal([root_field.ObjectVal(first), ..])) -> first
    _ -> dict.new()
  }
}

fn find_payment_terms_template(
  id: String,
) -> Option(PaymentTermsTemplateRecord) {
  case
    payment_terms_templates() |> list.find(fn(template) { template.id == id })
  {
    Ok(template) -> Some(template)
    Error(_) -> None
  }
}

fn add_days(iso: String, days: Int) -> Option(String) {
  case iso_timestamp.parse_iso(iso) {
    Ok(ms) ->
      Some(
        iso_timestamp.format_iso(ms + days * 86_400_000)
        |> string.replace(".000Z", "Z"),
      )
    Error(_) -> None
  }
}

fn payment_terms_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  terms: Option(PaymentTermsRecord),
  errors: List(UserError),
  staged_ids: List(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  mutation_payload_result(
    store,
    identity,
    field,
    project_payload(field, fragments, [
      #("paymentTerms", case terms {
        Some(record) -> payment_terms_source(record)
        None -> SrcNull
      }),
      #("userErrors", user_errors_source(errors)),
    ]),
    staged_ids,
    root_name,
    Some(
      "Staged payment terms locally in the in-memory payment terms draft store.",
    ),
  )
}

fn send_payment_reminder(store, identity, field, fragments, variables) {
  let payment_schedule_id =
    graphql_helpers.read_arg_string_nonempty(
      graphql_helpers.field_args(field, variables),
      "paymentScheduleId",
    )
  case
    is_shopify_gid(payment_schedule_id, "PaymentSchedule"),
    payment_schedule_id
  {
    True, Some(schedule_id) -> {
      let #(id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "PaymentReminderSend")
      let #(sent_at, next_identity) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let record =
        PaymentReminderSendRecord(
          id: id,
          payment_schedule_id: schedule_id,
          sent_at: sent_at,
        )
      payment_method_result(
        store.stage_payment_reminder_send(store, record),
        next_identity,
        field,
        fragments,
        "paymentReminderSend",
        None,
        [],
        [#("success", SrcBool(True))],
      )
    }
    _, _ ->
      payment_method_result(
        store,
        identity,
        field,
        fragments,
        "paymentReminderSend",
        None,
        [
          UserError(
            field: ["paymentScheduleId"],
            message: "Payment reminder could not be sent",
            code: Some("PAYMENT_REMINDER_SEND_UNSUCCESSFUL"),
          ),
        ],
        [#("success", SrcBool(False))],
      )
  }
}

fn payment_method_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  root_name: String,
  method: Option(CustomerPaymentMethodRecord),
  errors: List(UserError),
  extra_fields: List(#(String, SourceValue)),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let method_fields = [
    #("customerPaymentMethod", case method {
      Some(record) -> payment_method_source(store, record)
      None -> SrcNull
    }),
    #("userErrors", user_errors_source(errors)),
  ]
  let staged = case method {
    Some(record) -> [record.id]
    None -> []
  }
  #(
    MutationFieldResult(
      get_field_response_key(field),
      project_payload(
        field,
        fragments,
        list.append(method_fields, extra_fields),
      ),
      staged,
      root_name,
      Some(case root_name {
        "paymentReminderSend" ->
          "Staged a local payment reminder intent only; no customer email is sent at runtime."
        _ ->
          "Staged locally in the in-memory customer payment-method draft store; payment credentials, gateway secrets, and customer-facing update URLs are scrubbed or synthetic."
      }),
    ),
    store,
    identity,
  )
}

fn payment_method_source(
  store: Store,
  method: CustomerPaymentMethodRecord,
) -> SourceValue {
  let customer = store.get_effective_customer_by_id(store, method.customer_id)
  src_object([
    #("__typename", SrcString("CustomerPaymentMethod")),
    #("id", SrcString(method.id)),
    #("customer", case customer {
      Some(c) ->
        src_object([
          #("__typename", SrcString("Customer")),
          #("id", SrcString(c.id)),
        ])
      None -> SrcNull
    }),
    #("instrument", case method.instrument {
      Some(instrument) -> instrument_source(instrument)
      None -> SrcNull
    }),
    #("revokedAt", option_string_source(method.revoked_at)),
    #("revokedReason", option_string_source(method.revoked_reason)),
    #("subscriptionContracts", empty_connection_source()),
  ])
}

pub fn instrument_source(
  instrument: CustomerPaymentMethodInstrumentRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString(instrument.type_name)),
    #("lastDigits", nullable_data_string(instrument.data, "lastDigits")),
    #("maskedNumber", nullable_data_string(instrument.data, "maskedNumber")),
    #(
      "paypalAccountEmail",
      nullable_data_string(instrument.data, "paypalAccountEmail"),
    ),
    #("inactive", case dict.get(instrument.data, "inactive") {
      Ok("true") -> SrcBool(True)
      Ok("false") -> SrcBool(False)
      _ -> SrcNull
    }),
  ])
}

fn nullable_data_string(
  data: Dict(String, String),
  key: String,
) -> SourceValue {
  case dict.get(data, key) {
    Ok("__null") -> SrcNull
    Ok(value) -> SrcString(value)
    Error(_) -> SrcNull
  }
}

fn project_payload(
  field: Selection,
  fragments: FragmentMap,
  entries: List(#(String, SourceValue)),
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(src_object(entries), selections, fragments)
    _ -> json.object([])
  }
}

fn user_errors_source(errors: List(UserError)) -> SourceValue {
  SrcList(
    list.map(errors, fn(error) {
      src_object([
        #("field", SrcList(list.map(error.field, SrcString))),
        #("message", SrcString(error.message)),
        #("code", case error.code {
          Some(code) -> SrcString(code)
          None -> SrcNull
        }),
      ])
    }),
  )
}

fn mutation_payload_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  payload: Json,
  staged_ids: List(String),
  root_name: String,
  notes: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      staged_ids,
      root_name,
      notes,
    ),
    store,
    identity,
  )
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(name, child) if name == key -> Ok(child)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn non_null_json(value: Option(commit.JsonValue)) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(v) -> Some(v)
    None -> None
  }
}

fn json_array_items(value: Option(commit.JsonValue)) -> List(commit.JsonValue) {
  case non_null_json(value) {
    Some(commit.JsonArray(items)) -> items
    _ -> []
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  json_get(value, key) |> option.then(json_scalar_string)
}

fn json_scalar_string(value: commit.JsonValue) -> Option(String) {
  case value {
    commit.JsonString(s) -> Some(s)
    _ -> None
  }
}

fn json_get_data_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonNull) -> Some("__null")
    Some(commit.JsonString(s)) -> Some(s)
    Some(commit.JsonBool(True)) -> Some("true")
    Some(commit.JsonBool(False)) -> Some("false")
    _ -> None
  }
}

fn encode_duplication_data(
  method_id: String,
  target_customer_id: String,
  target_shop_id: String,
) -> String {
  let body =
    json.object([
      #("customerPaymentMethodId", json.string(method_id)),
      #("targetCustomerId", json.string(target_customer_id)),
      #("targetShopId", json.string(target_shop_id)),
    ])
    |> json.to_string
    |> bit_array.from_string
    |> bit_array.base64_url_encode(False)
  duplication_prefix <> body
}

fn decode_duplication_data(raw: String) -> Result(Dict(String, String), Nil) {
  case string.starts_with(raw, duplication_prefix) {
    False -> Error(Nil)
    True -> {
      let encoded =
        string.drop_start(raw, up_to: string.length(duplication_prefix))
      use bits <- result.try(bit_array.base64_url_decode(encoded))
      use text <- result.try(bit_array.to_string(bits))
      json.parse(text, decode.dict(decode.string, decode.string))
      |> result.replace_error(Nil)
    }
  }
}

fn uri_encode(value: String) -> String {
  value
  |> string.replace(" ", "%20")
  |> string.replace("/", "%2F")
  |> string.replace("?", "%3F")
  |> string.replace("&", "%26")
}
