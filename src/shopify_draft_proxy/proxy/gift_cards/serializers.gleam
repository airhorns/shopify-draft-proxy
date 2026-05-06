//// Gift-card mutation payload serialization.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/gift_cards/queries.{
  project_gift_card, serialize_gift_card_transaction_as,
}
import shopify_draft_proxy/proxy/gift_cards/types as gift_card_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcString, get_field_response_key,
  project_graphql_value, src_object,
}
import shopify_draft_proxy/state/types.{
  type GiftCardRecord, type GiftCardTransactionRecord,
}

@internal
pub type GiftCardPayload {
  GiftCardPayload(
    gift_card: Option(GiftCardRecord),
    gift_card_code: Option(String),
    gift_card_transaction: Option(GiftCardTransactionRecord),
    user_errors: List(gift_card_types.UserError),
  )
}

@internal
pub fn empty_payload(
  user_errors: List(gift_card_types.UserError),
) -> GiftCardPayload {
  GiftCardPayload(
    gift_card: None,
    gift_card_code: None,
    gift_card_transaction: None,
    user_errors: user_errors,
  )
}

// ---------------------------------------------------------------------------
// Payload serialization
// ---------------------------------------------------------------------------

@internal
pub fn gift_card_payload_json(
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
              serialize_gift_card_transaction_as(
                tx,
                graphql_helpers.selection_set_selections(ss),
                payload.gift_card,
                fragments,
                variables,
                case name.value {
                  "giftCardDebitTransaction" -> "GiftCardDebitTransaction"
                  "giftCardCreditTransaction" -> "GiftCardCreditTransaction"
                  _ -> "GiftCardTransaction"
                },
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
  user_errors: List(gift_card_types.UserError),
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  json.array(user_errors, fn(error) {
    let source = user_error_to_source(error)
    project_graphql_value(source, selections, fragments)
  })
}

fn user_error_to_source(error: gift_card_types.UserError) -> SourceValue {
  src_object([
    #("__typename", SrcString("UserError")),
    #("field", SrcList(list.map(error.field, fn(part) { SrcString(part) }))),
    #("code", graphql_helpers.option_string_source(error.code)),
    #("message", SrcString(error.message)),
  ])
}
