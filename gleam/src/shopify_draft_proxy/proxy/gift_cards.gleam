//// Mirrors `src/proxy/gift-cards.ts`.
////
//// Pass 19 ships the four query roots (`giftCard`, `giftCards`,
//// `giftCardsCount`, `giftCardConfiguration`) plus the seven mutation
//// roots (`giftCardCreate`/`Update`/`Credit`/`Debit`/`Deactivate`,
//// `giftCardSendNotificationToCustomer`/`Recipient`).
////
//// Gift cards never delete — `giftCardDeactivate` flips an `enabled`
//// flag and stamps `deactivated_at` instead. The store therefore tracks
//// `gift_cards` + `gift_card_order` only (no deleted-id set) and
//// `stage_create_gift_card` doubles as `stageUpdateGiftCard`.
////
//// Currency / decimal formatting follows the TS handler's
//// `formatDecimalAmount` exactly: round to 2dp, then trim a single
//// trailing zero, but never below `<int>.0`. Negative debit amounts on
//// transactions are signed by the handler — the underlying balance
//// math uses unsigned magnitudes.

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
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, ConnectionPageInfoOptions,
  ConnectionWindow, SelectedFieldOptions, SerializeConnectionConfig, SrcInt,
  SrcList, SrcNull, SrcString, default_connection_page_info_options,
  default_connection_window_options, get_document_fragments,
  get_field_response_key, paginate_connection_items, project_graphql_value,
  serialize_connection, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/search_query_parser.{type SearchQueryTerm}
import shopify_draft_proxy/shopify/resource_ids
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type GiftCardConfigurationRecord, type GiftCardRecipientAttributesRecord,
  type GiftCardRecord, type GiftCardTransactionRecord, type Money,
  GiftCardConfigurationRecord, GiftCardRecipientAttributesRecord, GiftCardRecord,
  GiftCardTransactionRecord, Money,
}

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Errors specific to the gift-cards handler.
pub type GiftCardsError {
  ParseFailed(root_field.RootFieldError)
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

/// Predicate matching `GIFT_CARD_MUTATION_ROOTS`.
pub fn is_gift_card_mutation_root(name: String) -> Bool {
  case name {
    "giftCardCreate" -> True
    "giftCardUpdate" -> True
    "giftCardCredit" -> True
    "giftCardDebit" -> True
    "giftCardDeactivate" -> True
    "giftCardSendNotificationToCustomer" -> True
    "giftCardSendNotificationToRecipient" -> True
    _ -> False
  }
}

/// Process a gift-cards query document and return a JSON `data`
/// envelope. Mirrors `handleGiftCardQuery`.
pub fn handle_gift_card_query(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, GiftCardsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
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
) -> Result(Json, GiftCardsError) {
  use data <- result.try(handle_gift_card_query(store, document, variables))
  Ok(graphql_helpers.wrap_data(data))
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

fn parse_decimal_amount(value: root_field.ResolvedValue) -> Float {
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
fn format_decimal_amount(value: Float) -> String {
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

fn normalize_money_value(
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

fn read_input(
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

fn project_gift_card(
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

fn effective_recipient_attributes(
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
      #("preferredName", graphql_helpers.option_string_source(attributes.preferred_name)),
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
  let entries =
    list.flat_map(selections, fn(selection) {
      case selection {
        ast.InlineFragment(type_condition: tc, selection_set: ss, ..) -> {
          let cond = case tc {
            Some(ast.NamedType(name: name, ..)) -> Some(name.value)
            _ -> None
          }
          case cond {
            None | Some("GiftCardTransaction") -> {
              let SelectionSet(selections: inner, ..) = ss
              transaction_entries(
                transaction,
                inner,
                giftcard,
                fragments,
                variables,
              )
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
              case cond_name.value == "GiftCardTransaction" {
                True ->
                  transaction_entries(
                    transaction,
                    inner,
                    giftcard,
                    fragments,
                    variables,
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
          ),
        ]
      }
    })
  json.object(entries)
}

fn transaction_entries(
  transaction: GiftCardTransactionRecord,
  selections: List(Selection),
  giftcard: Option(GiftCardRecord),
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
          None | Some("GiftCardTransaction") -> {
            let SelectionSet(selections: inner, ..) = ss
            transaction_entries(
              transaction,
              inner,
              giftcard,
              fragments,
              variables,
            )
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
            case cond_name.value == "GiftCardTransaction" {
              True ->
                transaction_entries(
                  transaction,
                  inner,
                  giftcard,
                  fragments,
                  variables,
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
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(key, json.string("GiftCardTransaction"))
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

// ===========================================================================
// Mutation path
// ===========================================================================

/// Outcome of a gift-cards mutation.
pub type MutationOutcome {
  MutationOutcome(
    data: Json,
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    log_drafts: List(LogDraft),
  )
}

/// User-error payload. Mirrors `GiftCardUserErrorRecord` (no `code`
/// field — gift-card user errors are field+message only).
pub type UserError {
  UserError(field: List(String), message: String)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

type GiftCardPayload {
  GiftCardPayload(
    gift_card: Option(GiftCardRecord),
    gift_card_code: Option(String),
    gift_card_transaction: Option(GiftCardTransactionRecord),
    user_errors: List(UserError),
  )
}

fn empty_payload(user_errors: List(UserError)) -> GiftCardPayload {
  GiftCardPayload(
    gift_card: None,
    gift_card_code: None,
    gift_card_transaction: None,
    user_errors: user_errors,
  )
}

/// Process a gift-cards mutation document.
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, GiftCardsError) {
  process_mutation_with_upstream(
    store,
    identity,
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

/// Pattern 2: update/credit/debit/deactivate and notification roots need the
/// prior upstream gift-card record before they can stage or short-circuit local
/// effects for an existing Shopify gift card. Snapshot mode/no transport falls
/// back to the local-only not-found behavior; LiveHybrid parity installs a
/// cassette for this narrow read.
pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, GiftCardsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        upstream,
      ))
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(data_entries, final_store, final_identity, all_staged, all_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, drafts) = acc
      case field {
        Field(name: name, ..) -> {
          let current_store =
            maybe_hydrate_gift_card_for_mutation(
              current_store,
              name.value,
              field,
              variables,
              upstream,
            )
          let dispatch = case name.value {
            "giftCardCreate" ->
              Some(handle_gift_card_create(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "giftCardUpdate" ->
              Some(handle_gift_card_update(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "giftCardCredit" ->
              Some(handle_gift_card_transaction(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "CREDIT",
                "creditAmount",
                "creditInput",
                "GiftCardCreditPayload",
              ))
            "giftCardDebit" ->
              Some(handle_gift_card_transaction(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "DEBIT",
                "debitAmount",
                "debitInput",
                "GiftCardDebitPayload",
              ))
            "giftCardDeactivate" ->
              Some(handle_gift_card_deactivate(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
              ))
            "giftCardSendNotificationToCustomer" ->
              Some(handle_gift_card_notification(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "GiftCardSendNotificationToCustomerPayload",
              ))
            "giftCardSendNotificationToRecipient" ->
              Some(handle_gift_card_notification(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "GiftCardSendNotificationToRecipientPayload",
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> {
              let draft =
                single_root_log_draft(
                  name.value,
                  result.staged_resource_ids,
                  gift_cards_status_for(name.value, result.staged_resource_ids),
                  "gift-cards",
                  "stage-locally",
                  Some(gift_cards_notes_for(name.value)),
                )
              #(
                list.append(entries, [#(result.key, result.payload)]),
                next_store,
                next_identity,
                list.append(staged_ids, result.staged_resource_ids),
                list.append(drafts, [draft]),
              )
            }
          }
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(data_entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: all_staged,
    log_drafts: all_drafts,
  )
}

/// Mirror the TS dispatcher: notification root fields are
/// short-circuited (no record staged) but still log a `Staged` entry
/// because the merchant intent was a stage-locally action. Every
/// other gift-cards root field that produced no staged record is a
/// validation `Failed` outcome.
fn gift_cards_status_for(
  root_field_name: String,
  staged_resource_ids: List(String),
) -> store.EntryStatus {
  case root_field_name, staged_resource_ids {
    "giftCardSendNotificationToCustomer", _
    | "giftCardSendNotificationToRecipient", _
    -> store.Staged
    _, [] -> store.Failed
    _, [_, ..] -> store.Staged
  }
}

/// Notes string mirroring the TS `gift-cards` dispatcher in
/// `routes.ts`: notification root fields explicitly call out that
/// they're short-circuited and never invoke a customer-visible
/// notification at runtime.
fn gift_cards_notes_for(root_field_name: String) -> String {
  case root_field_name {
    "giftCardSendNotificationToCustomer"
    | "giftCardSendNotificationToRecipient" ->
      "Short-circuited locally in the in-memory gift-card draft store; no customer-visible notification is sent at runtime."
    _ -> "Staged locally in the in-memory gift-card draft store."
  }
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

fn read_gift_card_id(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  let input = read_input(args)
  case graphql_helpers.read_arg_string_nonempty(args, "id") {
    Some(s) -> Some(s)
    None ->
      case graphql_helpers.read_arg_string_nonempty(args, "giftCardId") {
        Some(s) -> Some(s)
        None ->
          case graphql_helpers.read_arg_string_nonempty(input, "id") {
            Some(s) -> Some(s)
            None ->
              graphql_helpers.read_arg_string_nonempty(input, "giftCardId")
          }
      }
  }
}

fn maybe_hydrate_gift_card_for_mutation(
  store: Store,
  root_field_name: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  case root_field_name {
    "giftCardUpdate"
    | "giftCardCredit"
    | "giftCardDebit"
    | "giftCardDeactivate"
    | "giftCardSendNotificationToCustomer"
    | "giftCardSendNotificationToRecipient" -> {
      let args = graphql_helpers.field_args(field, variables)
      case read_gift_card_id(args) {
        Some(id) -> maybe_hydrate_gift_card(store, id, upstream)
        None -> store
      }
    }
    _ -> store
  }
}

fn maybe_hydrate_gift_card(
  store: Store,
  id: String,
  upstream: UpstreamContext,
) -> Store {
  case store.get_effective_gift_card_by_id(store, id) {
    Some(_) -> store
    None -> {
      let query = "query GiftCardHydrate($id: ID!) {
  giftCard(id: $id) {
    id
    lastCharacters
    maskedCode
    enabled
    deactivatedAt
    expiresOn
    note
    templateSuffix
    createdAt
    updatedAt
    initialValue { amount currencyCode }
    balance { amount currencyCode }
    customer { id }
    recipientAttributes {
      message
      preferredName
      sendNotificationAt
      recipient { id }
    }
    transactions(first: 250) {
      nodes {
        __typename
        id
        note
        processedAt
        amount { amount currencyCode }
      }
    }
  }
  giftCardConfiguration {
    issueLimit { amount currencyCode }
    purchaseLimit { amount currencyCode }
  }
}
"
      let variables = json.object([#("id", json.string(id))])
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "GiftCardHydrate",
          query,
          variables,
        )
      {
        Ok(value) -> hydrate_gift_card_from_upstream_response(store, value)
        Error(_) -> store
      }
    }
  }
}

fn hydrate_gift_card_from_upstream_response(
  store: Store,
  value: commit.JsonValue,
) -> Store {
  case json_get(value, "data") {
    Some(data) -> {
      let store = case
        json_get(data, "giftCard")
        |> non_null_json_node
        |> option.then(gift_card_record_from_json)
      {
        Some(record) -> store.upsert_base_gift_cards(store, [record])
        None -> store
      }
      case
        json_get(data, "giftCardConfiguration")
        |> non_null_json_node
        |> option.then(gift_card_configuration_from_json)
      {
        Some(configuration) ->
          store.upsert_base_gift_card_configuration(store, configuration)
        None -> store
      }
    }
    None -> store
  }
}

fn gift_card_record_from_json(
  node: commit.JsonValue,
) -> Option(GiftCardRecord) {
  case json_get_string(node, "id") {
    Some(id) -> {
      let last_characters =
        option.unwrap(json_get_string(node, "lastCharacters"), "")
      let masked_code =
        option.unwrap(
          json_get_string(node, "maskedCode"),
          masked_code_string(last_characters),
        )
      let initial_value =
        money_from_json(json_get(node, "initialValue"), Money("0.0", "CAD"))
      let balance = money_from_json(json_get(node, "balance"), initial_value)
      let recipient_attributes =
        recipient_attributes_from_json(json_get(node, "recipientAttributes"))
      let recipient_id = case recipient_attributes {
        Some(attributes) -> attributes.id
        None -> None
      }
      Some(GiftCardRecord(
        id: id,
        legacy_resource_id: gift_card_tail(id),
        last_characters: last_characters,
        masked_code: masked_code,
        enabled: option.unwrap(json_get_bool(node, "enabled"), True),
        deactivated_at: json_get_string(node, "deactivatedAt"),
        expires_on: json_get_string(node, "expiresOn"),
        note: json_get_string(node, "note"),
        template_suffix: json_get_string(node, "templateSuffix"),
        created_at: option.unwrap(json_get_string(node, "createdAt"), ""),
        updated_at: option.unwrap(json_get_string(node, "updatedAt"), ""),
        initial_value: initial_value,
        balance: balance,
        customer_id: json_get(node, "customer")
          |> option.then(fn(customer) { json_get_string(customer, "id") }),
        recipient_id: recipient_id,
        source: json_get_string(node, "source") |> option.or(Some("api_client")),
        recipient_attributes: recipient_attributes,
        transactions: gift_card_transactions_from_json(node),
      ))
    }
    None -> None
  }
}

fn gift_card_configuration_from_json(
  node: commit.JsonValue,
) -> Option(GiftCardConfigurationRecord) {
  Some(GiftCardConfigurationRecord(
    issue_limit: money_from_json(
      json_get(node, "issueLimit"),
      Money("0.0", "CAD"),
    ),
    purchase_limit: money_from_json(
      json_get(node, "purchaseLimit"),
      Money("0.0", "CAD"),
    ),
  ))
}

fn recipient_attributes_from_json(
  value: Option(commit.JsonValue),
) -> Option(GiftCardRecipientAttributesRecord) {
  case non_null_json_node(value) {
    Some(node) ->
      Some(GiftCardRecipientAttributesRecord(
        id: json_get(node, "recipient")
          |> option.then(fn(recipient) { json_get_string(recipient, "id") }),
        message: json_get_string(node, "message"),
        preferred_name: json_get_string(node, "preferredName"),
        send_notification_at: json_get_string(node, "sendNotificationAt"),
      ))
    None -> None
  }
}

fn gift_card_transactions_from_json(
  node: commit.JsonValue,
) -> List(GiftCardTransactionRecord) {
  case json_get(node, "transactions") {
    Some(connection) ->
      case json_get(connection, "nodes") {
        Some(commit.JsonArray(items)) ->
          list.filter_map(items, gift_card_transaction_from_json)
        _ -> []
      }
    None -> []
  }
}

fn gift_card_transaction_from_json(
  node: commit.JsonValue,
) -> Result(GiftCardTransactionRecord, Nil) {
  case json_get_string(node, "id") {
    Some(id) -> {
      let amount =
        money_from_json(json_get(node, "amount"), Money("0.0", "CAD"))
      Ok(GiftCardTransactionRecord(
        id: id,
        kind: gift_card_transaction_kind(node, amount),
        amount: amount,
        processed_at: option.unwrap(json_get_string(node, "processedAt"), ""),
        note: json_get_string(node, "note"),
      ))
    }
    None -> Error(Nil)
  }
}

fn gift_card_transaction_kind(node: commit.JsonValue, amount: Money) -> String {
  case json_get_string(node, "__typename") {
    Some("GiftCardDebitTransaction") -> "DEBIT"
    Some("GiftCardCreditTransaction") -> "CREDIT"
    _ ->
      case string.starts_with(amount.amount, "-") {
        True -> "DEBIT"
        False -> "CREDIT"
      }
  }
}

fn money_from_json(value: Option(commit.JsonValue), fallback: Money) -> Money {
  case non_null_json_node(value) {
    Some(node) ->
      Money(
        amount: option.unwrap(
          json_get_scalar_string(node, "amount"),
          fallback.amount,
        ),
        currency_code: option.unwrap(
          json_get_string(node, "currencyCode"),
          fallback.currency_code,
        ),
      )
    None -> fallback
  }
}

fn json_get_scalar_string(
  value: commit.JsonValue,
  key: String,
) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    Some(commit.JsonInt(i)) -> Some(int.to_string(i))
    Some(commit.JsonFloat(f)) -> Some(float.to_string(f))
    _ -> None
  }
}

fn json_get_string(value: commit.JsonValue, key: String) -> Option(String) {
  case json_get(value, key) {
    Some(commit.JsonString(s)) -> Some(s)
    _ -> None
  }
}

fn json_get_bool(value: commit.JsonValue, key: String) -> Option(Bool) {
  case json_get(value, key) {
    Some(commit.JsonBool(b)) -> Some(b)
    _ -> None
  }
}

fn non_null_json_node(
  value: Option(commit.JsonValue),
) -> Option(commit.JsonValue) {
  case value {
    Some(commit.JsonNull) -> None
    Some(node) -> Some(node)
    None -> None
  }
}

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(field, item) if field == key -> Ok(item)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn handle_gift_card_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = read_input(args)
  let #(gid, identity_after_id) =
    synthetic_identity.make_proxy_synthetic_gid(identity, "GiftCard")
  let initial_value =
    normalize_money_value(dict_get(input, "initialValue"), "CAD")
  let initial_amount =
    parse_decimal_amount(root_field.StringVal(initial_value.amount))
  case initial_amount <=. 0.0 {
    True -> {
      let payload =
        empty_payload([
          UserError(
            field: ["input", "initialValue"],
            message: "Initial value must be greater than zero",
          ),
        ])
      let json_payload =
        gift_card_payload_json(
          payload,
          "GiftCardCreatePayload",
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity_after_id,
      )
    }
    False -> {
      let code =
        normalize_gift_card_code(
          graphql_helpers.read_arg_string_nonempty(input, "code"),
          gid,
        )
      let last_chars = last_characters_from_code(code)
      let masked = masked_code_string(last_chars)
      let #(now, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let recipient_attributes =
        read_recipient_attributes(dict_get(input, "recipientAttributes"), None)
      let recipient_id = case
        graphql_helpers.read_arg_string_nonempty(input, "recipientId")
      {
        Some(s) -> Some(s)
        None ->
          case recipient_attributes {
            Some(attrs) -> attrs.id
            None -> None
          }
      }
      let record =
        GiftCardRecord(
          id: gid,
          legacy_resource_id: gift_card_tail(gid),
          last_characters: last_chars,
          masked_code: masked,
          enabled: True,
          deactivated_at: None,
          expires_on: graphql_helpers.read_arg_string_nonempty(
            input,
            "expiresOn",
          ),
          note: graphql_helpers.read_arg_string_nonempty(input, "note"),
          template_suffix: graphql_helpers.read_arg_string_nonempty(
            input,
            "templateSuffix",
          ),
          created_at: now,
          updated_at: now,
          initial_value: initial_value,
          balance: initial_value,
          customer_id: graphql_helpers.read_arg_string_nonempty(
            input,
            "customerId",
          ),
          recipient_id: recipient_id,
          source: Some("api_client"),
          recipient_attributes: recipient_attributes,
          transactions: [],
        )
      let #(_, store_after) = store.stage_create_gift_card(store, record)
      let payload =
        GiftCardPayload(
          gift_card: Some(record),
          gift_card_code: Some(code),
          gift_card_transaction: None,
          user_errors: [],
        )
      let json_payload =
        gift_card_payload_json(
          payload,
          "GiftCardCreatePayload",
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [record.id],
        ),
        store_after,
        identity_after_ts,
      )
    }
  }
}

fn handle_gift_card_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let input = read_input(args)
  let id = case graphql_helpers.read_arg_string_nonempty(input, "id") {
    Some(s) -> Some(s)
    None -> graphql_helpers.read_arg_string_nonempty(args, "id")
  }
  let existing = case id {
    Some(value) -> store.get_effective_gift_card_by_id(store, value)
    None -> None
  }
  case id, existing {
    Some(_), Some(current) -> {
      let #(now, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let new_note = case dict_has_key(input, "note") {
        True -> graphql_helpers.read_arg_string_nonempty(input, "note")
        False -> current.note
      }
      let new_template = case dict_has_key(input, "templateSuffix") {
        True ->
          graphql_helpers.read_arg_string_nonempty(input, "templateSuffix")
        False -> current.template_suffix
      }
      let new_expires = case dict_has_key(input, "expiresOn") {
        True -> graphql_helpers.read_arg_string_nonempty(input, "expiresOn")
        False -> current.expires_on
      }
      let new_customer = case dict_has_key(input, "customerId") {
        True -> graphql_helpers.read_arg_string_nonempty(input, "customerId")
        False -> current.customer_id
      }
      let existing_attrs = effective_recipient_attributes(current)
      let recipient_handling = case
        dict_has_key(input, "recipientId"),
        dict_has_key(input, "recipientAttributes")
      {
        True, _ -> #(
          graphql_helpers.read_arg_string_nonempty(input, "recipientId"),
          current.recipient_attributes,
        )
        False, True -> {
          let attrs =
            read_recipient_attributes(
              dict_get(input, "recipientAttributes"),
              existing_attrs,
            )
          let new_id = case attrs {
            Some(a) -> a.id
            None -> None
          }
          #(new_id, attrs)
        }
        False, False -> #(current.recipient_id, current.recipient_attributes)
      }
      let #(new_recipient_id, new_recipient_attributes) = recipient_handling
      let updated =
        GiftCardRecord(
          ..current,
          note: new_note,
          template_suffix: new_template,
          expires_on: new_expires,
          customer_id: new_customer,
          recipient_id: new_recipient_id,
          recipient_attributes: new_recipient_attributes,
          updated_at: now,
        )
      let #(_, store_after) = store.stage_update_gift_card(store, updated)
      let payload =
        GiftCardPayload(
          gift_card: Some(updated),
          gift_card_code: None,
          gift_card_transaction: None,
          user_errors: [],
        )
      let json_payload =
        gift_card_payload_json(
          payload,
          "GiftCardUpdatePayload",
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [updated.id],
        ),
        store_after,
        identity_after_ts,
      )
    }
    _, _ -> {
      let payload = empty_payload([not_found_user_error()])
      let json_payload =
        gift_card_payload_json(
          payload,
          "GiftCardUpdatePayload",
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_gift_card_transaction(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  kind: String,
  preferred_amount_key: String,
  preferred_input_key: String,
  payload_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = read_gift_card_id(args)
  let existing = case id {
    Some(value) -> store.get_effective_gift_card_by_id(store, value)
    None -> None
  }
  case id, existing {
    Some(_), Some(current) -> {
      let raw_money =
        read_mutation_money(args, preferred_amount_key, preferred_input_key)
      let magnitude =
        parse_decimal_amount(root_field.StringVal(raw_money.amount))
      case magnitude <=. 0.0 {
        True -> {
          let payload =
            empty_payload([
              UserError(
                field: [preferred_amount_key],
                message: "Amount must be greater than zero",
              ),
            ])
          let json_payload =
            gift_card_payload_json(
              payload,
              payload_typename,
              field,
              fragments,
              variables,
            )
          #(
            MutationFieldResult(
              key: key,
              payload: json_payload,
              staged_resource_ids: [],
            ),
            store,
            identity,
          )
        }
        False -> {
          let current_balance =
            parse_decimal_amount(root_field.StringVal(current.balance.amount))
          case kind == "DEBIT" && magnitude >. current_balance {
            True -> {
              let payload =
                empty_payload([
                  UserError(
                    field: [preferred_amount_key],
                    message: "Insufficient balance",
                  ),
                ])
              let json_payload =
                gift_card_payload_json(
                  payload,
                  payload_typename,
                  field,
                  fragments,
                  variables,
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: json_payload,
                  staged_resource_ids: [],
                ),
                store,
                identity,
              )
            }
            False -> {
              let signed = case kind {
                "CREDIT" -> magnitude
                _ -> 0.0 -. magnitude
              }
              let currency = case raw_money.currency_code {
                "" -> current.balance.currency_code
                code -> code
              }
              let #(transaction_id, identity_after_id) =
                synthetic_identity.make_synthetic_gid(
                  identity,
                  "GiftCardTransaction",
                )
              let processed_at_explicit =
                read_mutation_processed_at(args, preferred_input_key)
              let #(processed_at, identity_after_processed) = case
                processed_at_explicit
              {
                Some(value) -> #(value, identity_after_id)
                None ->
                  synthetic_identity.make_synthetic_timestamp(identity_after_id)
              }
              let transaction =
                GiftCardTransactionRecord(
                  id: transaction_id,
                  kind: kind,
                  amount: Money(
                    amount: format_decimal_amount(signed),
                    currency_code: currency,
                  ),
                  processed_at: processed_at,
                  note: read_mutation_note(args, preferred_input_key),
                )
              let new_balance = current_balance +. signed
              let #(now, identity_after_ts) =
                synthetic_identity.make_synthetic_timestamp(
                  identity_after_processed,
                )
              let updated =
                GiftCardRecord(
                  ..current,
                  balance: Money(
                    amount: format_decimal_amount(new_balance),
                    currency_code: currency,
                  ),
                  updated_at: now,
                  transactions: list.append(current.transactions, [transaction]),
                )
              let #(_, store_after) =
                store.stage_update_gift_card(store, updated)
              let payload =
                GiftCardPayload(
                  gift_card: Some(updated),
                  gift_card_code: None,
                  gift_card_transaction: Some(transaction),
                  user_errors: [],
                )
              let json_payload =
                gift_card_payload_json(
                  payload,
                  payload_typename,
                  field,
                  fragments,
                  variables,
                )
              #(
                MutationFieldResult(
                  key: key,
                  payload: json_payload,
                  staged_resource_ids: [updated.id],
                ),
                store_after,
                identity_after_ts,
              )
            }
          }
        }
      }
    }
    _, _ -> {
      let payload = empty_payload([not_found_user_error()])
      let json_payload =
        gift_card_payload_json(
          payload,
          payload_typename,
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_gift_card_deactivate(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = read_gift_card_id(args)
  let existing = case id {
    Some(value) -> store.get_effective_gift_card_by_id(store, value)
    None -> None
  }
  case id, existing {
    Some(_), Some(current) -> {
      let #(now, identity_after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      let deactivated_at = case current.deactivated_at {
        Some(_) -> current.deactivated_at
        None -> Some(now)
      }
      let updated =
        GiftCardRecord(
          ..current,
          enabled: False,
          deactivated_at: deactivated_at,
          updated_at: now,
        )
      let #(_, store_after) = store.stage_update_gift_card(store, updated)
      let payload =
        GiftCardPayload(
          gift_card: Some(updated),
          gift_card_code: None,
          gift_card_transaction: None,
          user_errors: [],
        )
      let json_payload =
        gift_card_payload_json(
          payload,
          "GiftCardDeactivatePayload",
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [updated.id],
        ),
        store_after,
        identity_after_ts,
      )
    }
    _, _ -> {
      let payload = empty_payload([not_found_user_error()])
      let json_payload =
        gift_card_payload_json(
          payload,
          "GiftCardDeactivatePayload",
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

fn handle_gift_card_notification(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  payload_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = read_gift_card_id(args)
  let existing = case id {
    Some(value) -> store.get_effective_gift_card_by_id(store, value)
    None -> None
  }
  case existing {
    None -> {
      let payload = empty_payload([not_found_user_error()])
      let json_payload =
        gift_card_payload_json(
          payload,
          payload_typename,
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
    Some(current) -> {
      let payload =
        GiftCardPayload(
          gift_card: Some(current),
          gift_card_code: None,
          gift_card_transaction: None,
          user_errors: [],
        )
      let json_payload =
        gift_card_payload_json(
          payload,
          payload_typename,
          field,
          fragments,
          variables,
        )
      #(
        MutationFieldResult(
          key: key,
          payload: json_payload,
          staged_resource_ids: [],
        ),
        store,
        identity,
      )
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers used by mutation handlers
// ---------------------------------------------------------------------------

fn not_found_user_error() -> UserError {
  UserError(field: ["id"], message: "Gift card does not exist")
}

fn dict_has_key(
  d: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Bool {
  case dict.get(d, key) {
    Ok(_) -> True
    Error(_) -> False
  }
}

fn dict_get(
  d: Dict(String, root_field.ResolvedValue),
  key: String,
) -> Option(root_field.ResolvedValue) {
  case dict.get(d, key) {
    Ok(v) -> Some(v)
    Error(_) -> None
  }
}

fn read_recipient_attributes(
  raw: Option(root_field.ResolvedValue),
  existing: Option(GiftCardRecipientAttributesRecord),
) -> Option(GiftCardRecipientAttributesRecord) {
  case raw {
    None -> existing
    Some(root_field.NullVal) -> None
    Some(root_field.ObjectVal(d)) -> {
      let id = case graphql_helpers.read_arg_string_nonempty(d, "id") {
        Some(s) -> Some(s)
        None ->
          case graphql_helpers.read_arg_string_nonempty(d, "recipientId") {
            Some(s) -> Some(s)
            None ->
              case existing {
                Some(a) -> a.id
                None -> None
              }
          }
      }
      let message = case dict_has_key(d, "message") {
        True -> graphql_helpers.read_arg_string_nonempty(d, "message")
        False ->
          case existing {
            Some(a) -> a.message
            None -> None
          }
      }
      let preferred_name = case dict_has_key(d, "preferredName") {
        True -> graphql_helpers.read_arg_string_nonempty(d, "preferredName")
        False ->
          case existing {
            Some(a) -> a.preferred_name
            None -> None
          }
      }
      let send_at = case dict_has_key(d, "sendNotificationAt") {
        True ->
          graphql_helpers.read_arg_string_nonempty(d, "sendNotificationAt")
        False ->
          case existing {
            Some(a) -> a.send_notification_at
            None -> None
          }
      }
      Some(GiftCardRecipientAttributesRecord(
        id: id,
        message: message,
        preferred_name: preferred_name,
        send_notification_at: send_at,
      ))
    }
    _ -> existing
  }
}

fn read_mutation_money(
  args: Dict(String, root_field.ResolvedValue),
  preferred_key: String,
  preferred_input_key: String,
) -> Money {
  let input = read_input(args)
  let nested = case graphql_helpers.read_arg_object(args, preferred_input_key) {
    Some(d) -> d
    None -> input
  }
  let raw = case dict_get(args, preferred_key) {
    Some(v) -> Some(v)
    None ->
      case dict_get(args, "amount") {
        Some(v) -> Some(v)
        None ->
          case dict_get(nested, preferred_key) {
            Some(v) -> Some(v)
            None -> dict_get(nested, "amount")
          }
      }
  }
  normalize_money_value(raw, "")
}

fn read_mutation_note(
  args: Dict(String, root_field.ResolvedValue),
  preferred_input_key: String,
) -> Option(String) {
  let input = read_input(args)
  let nested = case graphql_helpers.read_arg_object(args, preferred_input_key) {
    Some(d) -> d
    None -> input
  }
  case graphql_helpers.read_arg_string_nonempty(args, "note") {
    Some(s) -> Some(s)
    None -> graphql_helpers.read_arg_string_nonempty(nested, "note")
  }
}

fn read_mutation_processed_at(
  args: Dict(String, root_field.ResolvedValue),
  preferred_input_key: String,
) -> Option(String) {
  let input = read_input(args)
  let nested = case graphql_helpers.read_arg_object(args, preferred_input_key) {
    Some(d) -> d
    None -> input
  }
  case graphql_helpers.read_arg_string_nonempty(args, "processedAt") {
    Some(s) -> Some(s)
    None -> graphql_helpers.read_arg_string_nonempty(nested, "processedAt")
  }
}

fn gift_card_tail(id: String) -> String {
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

fn normalize_gift_card_code(
  raw: Option(String),
  fallback_id: String,
) -> String {
  case raw {
    None -> proxy_code(fallback_id)
    Some(value) -> {
      let trimmed = remove_whitespace(value)
      case string.length(trimmed) {
        0 -> proxy_code(fallback_id)
        _ -> string.lowercase(trimmed)
      }
    }
  }
}

fn proxy_code(fallback_id: String) -> String {
  "proxy" <> pad_start_zero(gift_card_tail(fallback_id), 8)
}

fn pad_start_zero(value: String, width: Int) -> String {
  let length = string.length(value)
  case length >= width {
    True -> value
    False -> string.repeat("0", width - length) <> value
  }
}

fn last_characters_from_code(code: String) -> String {
  let length = string.length(code)
  let suffix = case length >= 4 {
    True -> string.slice(code, length - 4, 4)
    False -> code
  }
  pad_start_zero(string.uppercase(suffix), 4)
}

fn masked_code_string(last_chars: String) -> String {
  "\u{2022}\u{2022}\u{2022}\u{2022} \u{2022}\u{2022}\u{2022}\u{2022} \u{2022}\u{2022}\u{2022}\u{2022} "
  <> last_chars
}

fn remove_whitespace(value: String) -> String {
  string.to_graphemes(value)
  |> list.filter(fn(g) {
    case g {
      " " | "\t" | "\n" | "\r" -> False
      _ -> True
    }
  })
  |> string.join("")
  |> string.trim
}

// ---------------------------------------------------------------------------
// Payload serialization
// ---------------------------------------------------------------------------

fn gift_card_payload_json(
  payload: GiftCardPayload,
  payload_typename: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let selections = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
  json.object(payload_entries(
    payload,
    payload_typename,
    selections,
    fragments,
    variables,
  ))
}

fn payload_entries(
  payload: GiftCardPayload,
  payload_typename: String,
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
          None -> {
            let SelectionSet(selections: inner, ..) = ss
            payload_entries(
              payload,
              payload_typename,
              inner,
              fragments,
              variables,
            )
          }
          Some(c) ->
            case c == payload_typename {
              True -> {
                let SelectionSet(selections: inner, ..) = ss
                payload_entries(
                  payload,
                  payload_typename,
                  inner,
                  fragments,
                  variables,
                )
              }
              False -> []
            }
        }
      }
      ast.FragmentSpread(name: name, ..) ->
        case dict.get(fragments, name.value) {
          Ok(ast.FragmentDefinition(
            type_condition: ast.NamedType(name: cond_name, ..),
            selection_set: SelectionSet(selections: inner, ..),
            ..,
          )) ->
            case cond_name.value == payload_typename {
              True ->
                payload_entries(
                  payload,
                  payload_typename,
                  inner,
                  fragments,
                  variables,
                )
              False -> []
            }
          _ -> []
        }
      Field(..) -> [
        payload_field_entry(
          payload,
          payload_typename,
          selection,
          fragments,
          variables,
        ),
      ]
    }
  })
}

fn payload_field_entry(
  payload: GiftCardPayload,
  payload_typename: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(String, Json) {
  let key = get_field_response_key(field)
  case field {
    Field(name: name, selection_set: ss, ..) ->
      case name.value {
        "__typename" -> #(key, json.string(payload_typename))
        "giftCard" -> #(key, case payload.gift_card {
          Some(gc) -> project_gift_card(gc, field, fragments, variables)
          None -> json.null()
        })
        "giftCardCode" -> #(
          key,
          graphql_helpers.option_string_json(payload.gift_card_code),
        )
        "giftCardTransaction"
        | "transaction"
        | "giftCardCreditTransaction"
        | "giftCardDebitTransaction" -> #(
          key,
          case payload.gift_card_transaction {
            Some(tx) ->
              serialize_gift_card_transaction(
                tx,
                graphql_helpers.selection_set_selections(ss),
                payload.gift_card,
                fragments,
                variables,
              )
            None -> json.null()
          },
        )
        "userErrors" -> #(
          key,
          serialize_user_errors(
            payload.user_errors,
            graphql_helpers.selection_set_selections(ss),
            fragments,
          ),
        )
        _ -> #(key, json.null())
      }
    _ -> #(key, json.null())
  }
}

fn serialize_user_errors(
  user_errors: List(UserError),
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  json.array(user_errors, fn(error) {
    let source = user_error_to_source(error)
    project_graphql_value(source, selections, fragments)
  })
}

fn user_error_to_source(error: UserError) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("message", SrcString(error.message)),
  ])
}
