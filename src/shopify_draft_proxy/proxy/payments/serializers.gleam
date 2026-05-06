//// Payments query serializers and GraphQL projection helpers.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SerializeConnectionConfig, SrcBool, SrcInt,
  SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, default_selected_field_options,
  get_field_response_key, get_selected_child_fields, paginate_connection_items,
  project_graphql_field_value, project_graphql_value, serialize_connection,
  serialize_empty_connection, src_object,
}
import shopify_draft_proxy/proxy/metafields
import shopify_draft_proxy/proxy/payments/types.{
  empty_connection_source, gid_tail,
  normalize_payment_customization_metafield_namespace, option_bool_source,
  option_int_source, option_money_source, option_string_source,
  payment_terms_templates,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types as state_types

@internal
pub fn query_payload(
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
  template: state_types.PaymentTermsTemplateRecord,
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
        project_payment_customization(item, node_field, fragments, variables)
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
        Some(record) ->
          project_payment_customization(record, field, fragments, variables)
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn project_payment_customization(
  customization: state_types.PaymentCustomizationRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let source = payment_customization_source(customization)
  let selections =
    get_selected_child_fields(field, default_selected_field_options())
  let entries =
    list.map(selections, fn(selection) {
      let key = get_field_response_key(selection)
      case selection {
        Field(name: name, ..) ->
          case name.value {
            "metafield" -> #(
              key,
              serialize_payment_customization_metafield(
                customization,
                selection,
                variables,
              ),
            )
            "metafields" -> #(
              key,
              serialize_payment_customization_metafields_connection(
                customization,
                selection,
                variables,
              ),
            )
            _ -> #(
              key,
              project_graphql_field_value(source, selection, fragments),
            )
          }
        _ -> #(key, json.null())
      }
    })
  json.object(entries)
}

@internal
pub fn payment_customization_source(
  record: state_types.PaymentCustomizationRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("PaymentCustomization")),
    #("id", SrcString(record.id)),
    #("legacyResourceId", SrcString(gid_tail(record.id))),
    #("title", option_string_source(record.title)),
    #("enabled", option_bool_source(record.enabled)),
    #("functionId", option_string_source(record.function_id)),
    #("functionHandle", option_string_source(record.function_handle)),
    #("shopifyFunction", SrcNull),
    #("errorHistory", SrcNull),
    #("metafield", SrcNull),
    #("metafields", empty_connection_source()),
  ])
}

fn serialize_payment_customization_metafield(
  customization: state_types.PaymentCustomizationRecord,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let namespace =
    graphql_helpers.read_arg_string(args, "namespace")
    |> option.map(normalize_payment_customization_metafield_namespace)
  let key = graphql_helpers.read_arg_string(args, "key")
  let found =
    customization.metafields
    |> list.find(fn(metafield) {
      metafield.namespace == option.unwrap(namespace, "")
      && metafield.key == option.unwrap(key, "")
    })
    |> option.from_result
  case found {
    Some(metafield) ->
      metafields.serialize_metafield_selection(
        payment_customization_metafield_to_core(metafield),
        field,
        default_selected_field_options(),
      )
    None -> json.null()
  }
}

fn serialize_payment_customization_metafields_connection(
  customization: state_types.PaymentCustomizationRecord,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let namespace =
    graphql_helpers.read_arg_string(args, "namespace")
    |> option.map(normalize_payment_customization_metafield_namespace)
  let records =
    customization.metafields
    |> list.filter(fn(metafield) {
      case namespace {
        Some(ns) -> metafield.namespace == ns
        None -> True
      }
    })
    |> list.map(payment_customization_metafield_to_core)
  metafields.serialize_metafields_connection(
    records,
    field,
    variables,
    default_selected_field_options(),
  )
}

fn payment_customization_metafield_to_core(
  record: state_types.PaymentCustomizationMetafieldRecord,
) -> metafields.MetafieldRecordCore {
  metafields.MetafieldRecordCore(
    id: record.id,
    namespace: record.namespace,
    key: record.key,
    type_: record.type_,
    value: record.value,
    compare_digest: record.compare_digest,
    json_value: metafields.parse_metafield_json_value(
      record.type_,
      record.value,
    ),
    created_at: record.created_at,
    updated_at: record.updated_at,
    owner_type: record.owner_type,
  )
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
  customer: state_types.CustomerRecord,
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

@internal
pub fn payment_terms_source(
  record: state_types.PaymentTermsRecord,
) -> SourceValue {
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
  schedules: List(state_types.PaymentScheduleRecord),
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

fn payment_schedule_source(
  schedule: state_types.PaymentScheduleRecord,
) -> SourceValue {
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
  method: state_types.CustomerPaymentMethodRecord,
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

@internal
pub fn payment_method_source(
  store: Store,
  method: state_types.CustomerPaymentMethodRecord,
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

@internal
pub fn instrument_source(
  instrument: state_types.CustomerPaymentMethodInstrumentRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString(instrument.type_name)),
    #("lastDigits", nullable_data_string(instrument.data, "lastDigits")),
    #("maskedNumber", nullable_data_string(instrument.data, "maskedNumber")),
    #("billingAddress", billing_address_source(instrument.data)),
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

fn billing_address_source(data: Dict(String, String)) -> SourceValue {
  case dict.get(data, "billingAddress.address1") {
    Ok(_) ->
      src_object([
        #("firstName", nullable_data_string(data, "billingAddress.firstName")),
        #("lastName", nullable_data_string(data, "billingAddress.lastName")),
        #("company", nullable_data_string(data, "billingAddress.company")),
        #("address1", nullable_data_string(data, "billingAddress.address1")),
        #("address2", nullable_data_string(data, "billingAddress.address2")),
        #("city", nullable_data_string(data, "billingAddress.city")),
        #("zip", nullable_data_string(data, "billingAddress.zip")),
        #("phone", nullable_data_string(data, "billingAddress.phone")),
        #("country", nullable_data_string(data, "billingAddress.country")),
        #(
          "countryCodeV2",
          nullable_data_string(data, "billingAddress.countryCodeV2"),
        ),
        #("province", nullable_data_string(data, "billingAddress.province")),
        #(
          "provinceCode",
          nullable_data_string(data, "billingAddress.provinceCode"),
        ),
      ])
    Error(_) -> SrcNull
  }
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
