//// Gift-card query handling, connection serialization, and read projection.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{type Order, Eq}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/gift_cards/types as gift_card_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcInt,
  SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{respond_to_query}
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, type Response,
}
import shopify_draft_proxy/search_query_parser.{type SearchQueryTerm}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/types.{
  type GiftCardConfigurationRecord, type GiftCardRecipientAttributesRecord,
  type GiftCardRecord, type GiftCardTransactionRecord, type Money,
  GiftCardRecipientAttributesRecord, Money,
}

/// Predicate matching `GIFT_CARD_QUERY_ROOTS`.
pub fn is_gift_card_query_root(name: String) -> Bool {
  case name {
    "giftCard" -> True
    "giftCards" -> True
    "giftCardsCount" -> True
    "giftCardConfiguration" -> True
    _ -> False
  }
}

/// Process a gift-cards query document and return a JSON `data`
/// envelope. Mirrors `handleGiftCardQuery`.
pub fn handle_gift_card_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, gift_card_types.GiftCardsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(gift_card_types.ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(serialize_root_fields(store, fields, fragments, variables))
    }
  }
}

/// Convenience: parse + handle + wrap, for the dispatcher.
pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, gift_card_types.GiftCardsError) {
  use data <- result.try(handle_gift_card_query(store, document, variables))
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
    "Failed to handle gift cards query",
  )
}

// ---------------------------------------------------------------------------
// Query dispatch
// ---------------------------------------------------------------------------

fn serialize_root_fields(
  store: Store,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.map(fields, fn(field) {
      let key = get_field_response_key(field)
      let value = root_payload_for_field(store, field, fragments, variables)
      #(key, value)
    })
  json.object(entries)
}

fn root_payload_for_field(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(name: name, ..) ->
      case name.value {
        "giftCard" ->
          serialize_gift_card_by_id(store, field, fragments, variables)
        "giftCards" ->
          serialize_gift_cards_connection(store, field, fragments, variables)
        "giftCardsCount" ->
          serialize_gift_cards_count(store, field, fragments, variables)
        "giftCardConfiguration" ->
          serialize_gift_card_configuration(store, field, fragments)
        _ -> json.null()
      }
    _ -> json.null()
  }
}

// ---------------------------------------------------------------------------
// Decimal helpers (mirror parseDecimalAmount / formatDecimalAmount)
// ---------------------------------------------------------------------------

@internal
pub fn parse_decimal_amount(value: root_field.ResolvedValue) -> Float {
  case value {
    root_field.IntVal(i) -> int.to_float(i)
    root_field.FloatVal(f) -> f
    root_field.StringVal(s) ->
      case float.parse(s) {
        Ok(f) -> f
        Error(_) ->
          case int.parse(s) {
            Ok(n) -> int.to_float(n)
            Error(_) -> 0.0
          }
      }
    _ -> 0.0
  }
}

/// Format a float as `parseDecimalAmount` does: round to 2 decimals and
/// trim trailing zeros while never going below one fractional digit.
@internal
pub fn format_decimal_amount(value: Float) -> String {
  let rounded = round_to_cents(value)
  let fixed = float_to_fixed_2(rounded)
  case string.ends_with(fixed, "00") {
    True -> string.drop_end(fixed, 3) <> ".0"
    False ->
      case string.ends_with(fixed, "0") {
        True -> string.drop_end(fixed, 1)
        False -> fixed
      }
  }
}

fn round_to_cents(value: Float) -> Float {
  // Multiply by 100, round to nearest, then divide by 100.
  let scaled = value *. 100.0
  let rounded = float.round(scaled)
  int.to_float(rounded) /. 100.0
}

fn float_to_fixed_2(value: Float) -> String {
  let negative = value <. 0.0
  let abs_value = case negative {
    True -> 0.0 -. value
    False -> value
  }
  // Multiply by 100 and round to get total cents.
  let cents = float.round(abs_value *. 100.0)
  let dollars = cents / 100
  let remainder = cents - dollars * 100
  let cents_str = case remainder < 10 {
    True -> "0" <> int.to_string(remainder)
    False -> int.to_string(remainder)
  }
  let sign = case negative {
    True -> "-"
    False -> ""
  }
  sign <> int.to_string(dollars) <> "." <> cents_str
}

@internal
pub fn normalize_money_value(
  raw: Option(root_field.ResolvedValue),
  fallback_currency: String,
) -> Money {
  case raw {
    None ->
      Money(
        amount: format_decimal_amount(0.0),
        currency_code: fallback_currency,
      )
    Some(root_field.StringVal(_))
    | Some(root_field.IntVal(_))
    | Some(root_field.FloatVal(_)) ->
      Money(
        amount: format_decimal_amount(
          parse_decimal_amount(option.unwrap(raw, root_field.NullVal)),
        ),
        currency_code: fallback_currency,
      )
    Some(root_field.ObjectVal(d)) -> {
      let amount_value =
        dict.get(d, "amount")
        |> result.unwrap(root_field.NullVal)
      let currency = case dict.get(d, "currencyCode") {
        Ok(root_field.StringVal(s)) ->
          case s {
            "" -> fallback_currency
            _ -> s
          }
        _ -> fallback_currency
      }
      Money(
        amount: format_decimal_amount(parse_decimal_amount(amount_value)),
        currency_code: currency,
      )
    }
    _ ->
      Money(
        amount: format_decimal_amount(0.0),
        currency_code: fallback_currency,
      )
  }
}

@internal
pub fn read_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case graphql_helpers.read_arg_object(args, "input") {
    Some(d) -> d
    None -> dict.new()
  }
}

// ---------------------------------------------------------------------------
// Gift card -> source projections
// ---------------------------------------------------------------------------

fn serialize_gift_card_by_id(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(id) ->
      case store.get_effective_gift_card_by_id(store, id) {
        Some(record) -> project_gift_card(record, field, fragments, variables)
        None -> json.null()
      }
    None -> json.null()
  }
}

@internal
pub fn project_gift_card(
  record: GiftCardRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_gift_card_value(record, selections, fragments, variables)
    _ -> json.object([])
  }
}

fn project_gift_card_value(
  record: GiftCardRecord,
  selections: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let entries =
    list.flat_map(selections, fn(selection) {
      case selection {
        ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
          let cond = case tc {
            Some(ast.NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case cond {
            None | Some("GiftCard") -> {
              let SelectionSet(selections: inner, ..) = ss
              flatten_gift_card_entries(record, inner, fragments, variables)
            }
            _ -> []
          }
        }
        ast.FragmentSpread(name: name, ..) ->
          case dict.get(fragments, name.value) {
            Ok(ast.FragmentDefinition(
              type_condition: ast.NamedType(name: cond_name, ..),
              selection_set: SelectionSet(selections: inner, ..),
              ..,
            )) ->
              case cond_name.value == "GiftCard" {
                True ->
                  flatten_gift_card_entries(record, inner, fragments, variables)
                False -> []
              }
            _ -> []
          }
        Field(..) -> [
          gift_card_field_entry(record, selection, fragments, variables),
        ]
      }
    })
  json.object(entries)
}

fn flatten_gift_card_entries(
  record: GiftCardRecord,
  selections: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(#(String, Json)) {
  list.flat_map(selections, fn(selection) {
    case selection {
      ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
        let cond = case tc {
          Some(ast.NamedType(name: name, ..)) -> Some(name.value)
          _ -> None
        }
        case cond {
          None | Some("GiftCard") -> {
            let SelectionSet(selections: inner, ..) = ss
            flatten_gift_card_entries(record, inner, fragments, variables)
          }
          _ -> []
        }
      }
      ast.FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(ast.FragmentDefinition(
            type_condition: ast.NamedType(name: cond_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case cond_name.value == "GiftCard" {
              True ->
                flatten_gift_card_entries(record, inner, fragments, variables)
              False -> []
            }
          _ -> []
        }
      Field(..) -> [
        gift_card_field_entry(record, selection, fragments, variables),
      ]
    }
  })
}

fn gift_card_field_entry(
  record: GiftCardRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(key, json.string("GiftCard"))
        "id" -> #(key, json.string(record.id))
        "legacyResourceId" -> #(key, json.string(record.legacy_resource_id))
        "lastCharacters" -> #(key, json.string(record.last_characters))
        "maskedCode" -> #(key, json.string(record.masked_code))
        "enabled" -> #(key, json.bool(record.enabled))
        "disabledAt" | "deactivatedAt" -> #(
          key,
          graphql_helpers.option_string_json(record.deactivated_at),
        )
        "expiresOn" -> #(
          key,
          graphql_helpers.option_string_json(record.expires_on),
        )
        "note" -> #(key, graphql_helpers.option_string_json(record.note))
        "templateSuffix" -> #(
          key,
          graphql_helpers.option_string_json(record.template_suffix),
        )
        "createdAt" -> #(key, json.string(record.created_at))
        "updatedAt" -> #(key, json.string(record.updated_at))
        "initialValue" -> #(
          key,
          serialize_money(
            record.initial_value,
            graphql_helpers.selection_set_selections(ss),
            fragments,
          ),
        )
        "balance" -> #(
          key,
          serialize_money(
            record.balance,
            graphql_helpers.selection_set_selections(ss),
            fragments,
          ),
        )
        "transactions" -> #(
          key,
          serialize_gift_card_transactions_connection(
            record,
            field,
            fragments,
            variables,
          ),
        )
        "customer" -> #(key, customer_object_json(record.customer_id, ss))
        "recipientAttributes" -> {
          let attributes = effective_recipient_attributes(record)
          let payload = case attributes {
            Some(attrs) ->
              serialize_gift_card_recipient_attributes(
                attrs,
                graphql_helpers.selection_set_selections(ss),
                fragments,
              )
            None -> json.null()
          }
          #(key, payload)
        }
        "recipient" -> #(key, customer_object_json(record.recipient_id, ss))
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn customer_object_json(
  customer_id: Option(String),
  ss: Option(ast.SelectionSet),
) -> Json {
  case customer_id {
    None -> json.null()
    Some(_) -> {
      // Match the TS handler — it returns `{ id: customerId }` literally,
      // *not* a projected object. Selections are ignored beyond the
      // top-level field check.
      let _ = ss
      json.object([#("id", graphql_helpers.option_string_json(customer_id))])
    }
  }
}

fn serialize_money(
  money: Money,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("MoneyV2")),
      #("amount", SrcString(money.amount)),
      #("currencyCode", SrcString(money.currency_code)),
    ])
  project_graphql_value(source, selections, fragments)
}

@internal
pub fn effective_recipient_attributes(
  record: GiftCardRecord,
) -> Option(GiftCardRecipientAttributesRecord) {
  case record.recipient_attributes {
    Some(_) -> record.recipient_attributes
    None ->
      case record.recipient_id {
        Some(_) ->
          Some(GiftCardRecipientAttributesRecord(
            id: record.recipient_id,
            message: None,
            preferred_name: None,
            send_notification_at: None,
          ))
        None -> None
      }
  }
}

fn serialize_gift_card_recipient_attributes(
  attributes: GiftCardRecipientAttributesRecord,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  let recipient_source = case attributes.id {
    Some(id) ->
      src_object([
        #("__typename", SrcString("Customer")),
        #("id", SrcString(id)),
      ])
    None -> SrcNull
  }
  let source =
    src_object([
      #("__typename", SrcString("GiftCardRecipientAttributes")),
      #("message", graphql_helpers.option_string_source(attributes.message)),
      #(
        "preferredName",
        graphql_helpers.option_string_source(attributes.preferred_name),
      ),
      #(
        "sendNotificationAt",
        graphql_helpers.option_string_source(attributes.send_notification_at),
      ),
      #("recipient", recipient_source),
    ])
  project_graphql_value(source, selections, fragments)
}

fn serialize_gift_card_transaction(
  transaction: GiftCardTransactionRecord,
  selections: List(Selection),
  giftcard: Option(GiftCardRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  serialize_gift_card_transaction_as(
    transaction,
    selections,
    giftcard,
    fragments,
    variables,
    "GiftCardTransaction",
  )
}

@internal
pub fn serialize_gift_card_transaction_as(
  transaction: GiftCardTransactionRecord,
  selections: List(Selection),
  giftcard: Option(GiftCardRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> Json {
  let entries =
    list.flat_map(selections, fn(selection) {
      case selection {
        ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
          let cond = case tc {
            Some(ast.NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case transaction_selection_matches(cond, typename) {
            True -> {
              let SelectionSet(selections: inner, ..) = ss
              transaction_entries(
                transaction,
                inner,
                giftcard,
                fragments,
                variables,
                typename,
              )
            }
            False -> []
          }
        }
        ast.FragmentSpread(name: name, ..) ->
          case dict.get(fragments, name.value) {
            Ok(ast.FragmentDefinition(
              type_condition: ast.NamedType(name: cond_name, ..),
              selection_set: SelectionSet(selections: inner, ..),
              ..,
            )) ->
              case
                transaction_selection_matches(Some(cond_name.value), typename)
              {
                True ->
                  transaction_entries(
                    transaction,
                    inner,
                    giftcard,
                    fragments,
                    variables,
                    typename,
                  )
                False -> []
              }
            _ -> []
          }
        Field(..) -> [
          transaction_field_entry(
            transaction,
            selection,
            giftcard,
            fragments,
            variables,
            typename,
          ),
        ]
      }
    })
  json.object(entries)
}

fn transaction_selection_matches(
  condition: Option(String),
  typename: String,
) -> Bool {
  case condition {
    None -> True
    Some("GiftCardTransaction") -> True
    Some(value) -> value == typename
  }
}

fn transaction_entries(
  transaction: GiftCardTransactionRecord,
  selections: List(Selection),
  giftcard: Option(GiftCardRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> List(#(String, Json)) {
  list.flat_map(selections, fn(selection) {
    case selection {
      ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
        let cond = case tc {
          Some(ast.NamedType(name: name, ..)) -> Some(name.value)
          _ -> None
        }
        case transaction_selection_matches(cond, typename) {
          True -> {
            let SelectionSet(selections: inner, ..) = ss
            transaction_entries(
              transaction,
              inner,
              giftcard,
              fragments,
              variables,
              typename,
            )
          }
          False -> []
        }
      }
      ast.FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(ast.FragmentDefinition(
            type_condition: ast.NamedType(name: cond_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case
              transaction_selection_matches(Some(cond_name.value), typename)
            {
              True ->
                transaction_entries(
                  transaction,
                  inner,
                  giftcard,
                  fragments,
                  variables,
                  typename,
                )
              False -> []
            }
          _ -> []
        }
      Field(..) -> [
        transaction_field_entry(
          transaction,
          selection,
          giftcard,
          fragments,
          variables,
          typename,
        ),
      ]
    }
  })
}

fn transaction_field_entry(
  transaction: GiftCardTransactionRecord,
  field: Selection,
  giftcard: Option(GiftCardRecord),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  typename: String,
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(key, json.string(typename))
        "id" -> #(key, json.string(transaction.id))
        "kind" -> #(key, json.string(transaction.kind))
        "note" -> #(key, graphql_helpers.option_string_json(transaction.note))
        "processedAt" -> #(key, json.string(transaction.processed_at))
        "amount" -> #(
          key,
          serialize_money(
            transaction.amount,
            graphql_helpers.selection_set_selections(ss),
            fragments,
          ),
        )
        "giftCard" -> #(key, case giftcard {
          Some(gc) -> project_gift_card(gc, field, fragments, variables)
          None -> json.null()
        })
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn serialize_gift_card_transactions_connection(
  record: GiftCardRecord,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let cursor_value = fn(transaction: GiftCardTransactionRecord, _index: Int) -> String {
    transaction.id
  }
  let window =
    paginate_connection_items(
      record.transactions,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: items,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  let serialize_node = fn(
    transaction: GiftCardTransactionRecord,
    node_field: Selection,
    _index: Int,
  ) -> Json {
    serialize_gift_card_transaction(
      transaction,
      graphql_helpers.selection_set_selections(case node_field {
        Field(selection_set: ss, ..) -> ss
        _ -> None
      }),
      Some(record),
      fragments,
      variables,
    )
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: items,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: serialize_node,
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

// ---------------------------------------------------------------------------
// Connection / count / configuration
// ---------------------------------------------------------------------------

fn list_gift_cards_for_connection(
  store: Store,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> List(GiftCardRecord) {
  let args = graphql_helpers.field_args(field, variables)
  let reverse = case graphql_helpers.read_arg_bool(args, "reverse") {
    Some(True) -> True
    _ -> False
  }
  let raw_query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let sort_key = graphql_helpers.read_arg_string_nonempty(args, "sortKey")
  let filtered =
    filter_gift_cards_by_query(
      store.list_effective_gift_cards(store),
      raw_query,
    )
  let sorted =
    list.sort(filtered, fn(left, right) {
      compare_gift_cards(left, right, sort_key)
    })
  case reverse {
    True -> list.reverse(sorted)
    False -> sorted
  }
}

fn serialize_gift_cards_connection(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let items = list_gift_cards_for_connection(store, field, variables)
  let cursor_value = fn(record: GiftCardRecord, _index: Int) -> String {
    record.id
  }
  let window =
    paginate_connection_items(
      items,
      field,
      variables,
      cursor_value,
      default_connection_window_options(),
    )
  let ConnectionWindow(
    items: paged,
    has_next_page: has_next,
    has_previous_page: has_prev,
  ) = window
  let selected_field_options =
    SelectedFieldOptions(include_inline_fragments: True)
  let page_info_options =
    ConnectionPageInfoOptions(
      ..default_connection_page_info_options(),
      include_inline_fragments: True,
    )
  let serialize_node = fn(
    record: GiftCardRecord,
    node_field: Selection,
    _index: Int,
  ) -> Json {
    project_gift_card(record, node_field, fragments, variables)
  }
  serialize_connection(
    field,
    SerializeConnectionConfig(
      items: paged,
      has_next_page: has_next,
      has_previous_page: has_prev,
      get_cursor_value: cursor_value,
      serialize_node: serialize_node,
      selected_field_options: selected_field_options,
      page_info_options: page_info_options,
    ),
  )
}

fn serialize_gift_cards_count(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = graphql_helpers.field_args(field, variables)
  let raw_query = graphql_helpers.read_arg_string_nonempty(args, "query")
  let total =
    list.length(filter_gift_cards_by_query(
      store.list_effective_gift_cards(store),
      raw_query,
    ))
  let limit = graphql_helpers.read_arg_int(args, "limit")
  let limit_clean = case limit {
    Some(n) ->
      case n >= 0 {
        True -> Some(n)
        False -> None
      }
    None -> None
  }
  let visible = case limit_clean {
    None -> total
    Some(n) ->
      case total < n {
        True -> total
        False -> n
      }
  }
  let precision_value = case limit_clean {
    Some(n) ->
      case total > n {
        True -> "AT_LEAST"
        False -> "EXACT"
      }
    None -> "EXACT"
  }
  let source =
    src_object([
      #("__typename", SrcString("Count")),
      #("count", SrcInt(visible)),
      #("precision", SrcString(precision_value)),
    ])
  project_payload(source, field, fragments)
}

fn serialize_gift_card_configuration(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  let configuration = store.get_effective_gift_card_configuration(store)
  let source = gift_card_configuration_to_source(configuration)
  project_payload(source, field, fragments)
}

fn gift_card_configuration_to_source(
  configuration: GiftCardConfigurationRecord,
) -> SourceValue {
  src_object([
    #("__typename", SrcString("GiftCardConfiguration")),
    #("issueLimit", money_to_source(configuration.issue_limit)),
    #("purchaseLimit", money_to_source(configuration.purchase_limit)),
  ])
}

fn money_to_source(money: Money) -> SourceValue {
  src_object([
    #("__typename", SrcString("MoneyV2")),
    #("amount", SrcString(money.amount)),
    #("currencyCode", SrcString(money.currency_code)),
  ])
}

fn project_payload(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(source, selections, fragments)
    _ -> json.object([])
  }
}

// ---------------------------------------------------------------------------
// Search query matching
// ---------------------------------------------------------------------------

fn matches_gift_card_term(
  record: GiftCardRecord,
  term: SearchQueryTerm,
) -> Bool {
  let normalized_value =
    search_query_parser.normalize_search_query_value(term.value)
  let raw_field = case term.field {
    Some(s) -> Some(string.lowercase(s))
    None -> None
  }
  let positive_match = case raw_field {
    None ->
      search_query_parser.matches_search_query_text(
        Some(record.last_characters),
        term,
      )
      || search_query_parser.matches_search_query_text(
        Some(record.masked_code),
        term,
      )
      || search_query_parser.matches_search_query_text(record.code, term)
    Some("id") -> {
      let id_normalized =
        search_query_parser.normalize_search_query_value(record.id)
      let tail = gift_card_tail(record.id)
      normalized_value == id_normalized || normalized_value == tail
    }
    Some("balance_status") -> {
      let initial_value =
        parse_decimal_amount(root_field.StringVal(record.initial_value.amount))
      let balance =
        parse_decimal_amount(root_field.StringVal(record.balance.amount))
      case normalized_value {
        "full" -> balance >=. initial_value && balance >. 0.0
        "partial" -> balance >. 0.0 && balance <. initial_value
        "empty" -> balance <=. 0.0
        "full_or_partial" -> balance >. 0.0
        _ -> False
      }
    }
    Some("status") ->
      case normalized_value {
        "enabled" | "active" | "true" -> record.enabled
        "disabled" | "deactivated" | "inactive" | "false" -> !record.enabled
        _ -> False
      }
    Some("created_at") ->
      search_query_parser.matches_search_query_date(
        Some(record.created_at),
        term,
        0,
      )
    Some("expires_on") ->
      search_query_parser.matches_search_query_date(record.expires_on, term, 0)
    Some("initial_value") ->
      search_query_parser.matches_search_query_number(
        Some(
          parse_decimal_amount(root_field.StringVal(record.initial_value.amount)),
        ),
        term,
      )
    Some("customer_id") ->
      gift_card_id_matches(record.customer_id, normalized_value)
    Some("recipient_id") ->
      gift_card_id_matches(record.recipient_id, normalized_value)
    Some("source") ->
      search_query_parser.normalize_search_query_value(option.unwrap(
        record.source,
        "",
      ))
      == normalized_value
    _ -> True
  }
  case term.negated {
    True -> !positive_match
    False -> positive_match
  }
}

fn gift_card_id_matches(id: Option(String), normalized_value: String) -> Bool {
  case id {
    None -> False
    Some(value) -> {
      let id_normalized =
        search_query_parser.normalize_search_query_value(value)
      let tail = gift_card_tail(value)
      normalized_value == id_normalized || normalized_value == tail
    }
  }
}

fn filter_gift_cards_by_query(
  records: List(GiftCardRecord),
  raw_query: Option(String),
) -> List(GiftCardRecord) {
  case raw_query {
    None -> records
    Some(q) ->
      case string.trim(q) {
        "" -> records
        trimmed -> {
          let opts =
            search_query_parser.SearchQueryTermListOptions(
              quote_characters: ["\"", "'"],
              preserve_quotes_in_terms: False,
              ignored_keywords: ["AND"],
              drop_empty_values: False,
            )
          let terms =
            search_query_parser.parse_search_query_terms(trimmed, opts)
            |> list.filter(fn(term) {
              case term.field {
                None -> True
                Some(name) ->
                  case string.lowercase(name) {
                    "id"
                    | "status"
                    | "balance_status"
                    | "created_at"
                    | "expires_on"
                    | "initial_value"
                    | "customer_id"
                    | "recipient_id"
                    | "source" -> True
                    _ -> False
                  }
              }
            })
          case terms {
            [] -> records
            _ ->
              list.filter(records, fn(record) {
                list.all(terms, fn(term) {
                  matches_gift_card_term(record, term)
                })
              })
          }
        }
      }
  }
}

fn compare_gift_cards(
  left: GiftCardRecord,
  right: GiftCardRecord,
  sort_key: Option(String),
) -> Order {
  case sort_key {
    Some("CREATED_AT") -> {
      let primary = string.compare(left.created_at, right.created_at)
      case primary {
        Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        _ -> primary
      }
    }
    Some("UPDATED_AT") -> {
      let primary = string.compare(left.updated_at, right.updated_at)
      case primary {
        Eq -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
        _ -> primary
      }
    }
    _ -> resource_ids.compare_shopify_resource_ids(left.id, right.id)
  }
}

@internal
pub fn gift_card_tail(id: String) -> String {
  let segments = string.split(id, "/")
  let tail = case list.last(segments) {
    Ok(t) -> t
    Error(_) -> id
  }
  case string.split_once(tail, "?") {
    Ok(#(prefix, _)) -> prefix
    Error(_) -> tail
  }
}
