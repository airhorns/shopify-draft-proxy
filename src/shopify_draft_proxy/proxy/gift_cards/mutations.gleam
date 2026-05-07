//// Gift-card mutation handling and local staging.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order.{Lt}
import gleam/result
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/gift_cards/queries.{
  effective_recipient_attributes, format_decimal_amount, gift_card_tail,
  normalize_money_value, parse_decimal_amount, read_input,
}
import shopify_draft_proxy/proxy/gift_cards/serializers.{
  GiftCardPayload, empty_payload, gift_card_payload_json,
}
import shopify_draft_proxy/proxy/gift_cards/types as gift_card_types
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments, get_field_response_key,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationFieldResult, type MutationOutcome, MutationFieldResult,
  MutationOutcome, single_root_log_draft,
}
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerRecord, type GiftCardConfigurationRecord,
  type GiftCardRecipientAttributesRecord, type GiftCardRecord,
  type GiftCardTransactionRecord, type Money, CustomerDefaultEmailAddressRecord,
  CustomerDefaultPhoneNumberRecord, CustomerRecord, GiftCardConfigurationRecord,
  GiftCardRecipientAttributesRecord, GiftCardRecord, GiftCardTransactionRecord,
  Money,
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

/// Process a gift-cards mutation document.
/// Pattern 2: update/credit/debit/deactivate and notification roots need the
/// prior upstream gift-card record before they can stage or short-circuit local
/// effects for an existing Shopify gift card. Snapshot mode/no transport falls
/// back to the local-only not-found behavior; LiveHybrid parity installs a
/// cassette for this narrow read.
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
      handle_mutation_fields(
        store,
        identity,
        fields,
        fragments,
        variables,
        upstream,
      )
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
            |> maybe_hydrate_shop_for_assignment_guard(
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
                upstream,
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
                upstream,
                "CREDIT",
                "creditAmount",
                "creditInput",
                "GiftCardCreditPayload",
                "GiftCardCreditTransaction",
              ))
            "giftCardDebit" ->
              Some(handle_gift_card_transaction(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                upstream,
                "DEBIT",
                "debitAmount",
                "debitInput",
                "GiftCardDebitPayload",
                "GiftCardDebitTransaction",
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
                "customer",
                "GiftCardSendNotificationToCustomerPayload",
              ))
            "giftCardSendNotificationToRecipient" ->
              Some(handle_gift_card_notification(
                current_store,
                current_identity,
                field,
                fragments,
                variables,
                "recipient",
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
) -> store_types.EntryStatus {
  case root_field_name, staged_resource_ids {
    "giftCardSendNotificationToCustomer", _
    | "giftCardSendNotificationToRecipient", _
    -> store_types.Staged
    _, [] -> store_types.Failed
    _, [_, ..] -> store_types.Staged
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

fn maybe_hydrate_shop_for_assignment_guard(
  store: Store,
  root_field_name: String,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Store {
  case root_field_name {
    "giftCardCreate" | "giftCardUpdate" -> {
      let args = graphql_helpers.field_args(field, variables)
      let input = read_input(args)
      case
        gift_card_trial_assignment_target(input),
        store.get_effective_shop(store)
      {
        Some(_), None ->
          store_properties.hydrate_shop_baseline_if_needed(store, upstream)
        _, _ -> store
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
      let query =
        "query GiftCardHydrate($id: ID!) {
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
    customer {
      id
      email
      defaultEmailAddress { emailAddress }
      defaultPhoneNumber { phoneNumber }
    }
    recipientAttributes {
      message
      preferredName
      sendNotificationAt
      recipient {
        id
        email
        defaultEmailAddress { emailAddress }
        defaultPhoneNumber { phoneNumber }
      }
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
      let store = case json_get(data, "giftCard") |> non_null_json_node {
        Some(node) -> hydrate_gift_card_customers(store, node)
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

fn hydrate_gift_card_customers(store: Store, node: commit.JsonValue) -> Store {
  let customer_nodes = [
    json_get(node, "customer") |> non_null_json_node,
    json_get(node, "recipientAttributes")
      |> non_null_json_node
      |> option.then(fn(attributes) {
        json_get(attributes, "recipient") |> non_null_json_node
      }),
  ]
  let customers =
    customer_nodes
    |> list.filter_map(fn(customer) {
      case customer {
        Some(value) -> customer_record_from_json(value)
        None -> Error(Nil)
      }
    })
  case customers {
    [] -> store
    records -> store.upsert_base_customers(store, records)
  }
}

fn customer_record_from_json(
  node: commit.JsonValue,
) -> Result(CustomerRecord, Nil) {
  case json_get_string(node, "id") {
    Some(id) -> {
      let email = json_get_string(node, "email")
      let default_email =
        json_get(node, "defaultEmailAddress")
        |> non_null_json_node
        |> option.then(fn(value) { json_get_string(value, "emailAddress") })
        |> option.or(email)
      let default_phone =
        json_get(node, "defaultPhoneNumber")
        |> non_null_json_node
        |> option.then(fn(value) { json_get_string(value, "phoneNumber") })
      Ok(CustomerRecord(
        id: id,
        first_name: None,
        last_name: None,
        display_name: None,
        email: email,
        legacy_resource_id: None,
        locale: None,
        note: None,
        can_delete: None,
        verified_email: None,
        data_sale_opt_out: False,
        tax_exempt: None,
        tax_exemptions: [],
        state: None,
        tags: [],
        number_of_orders: None,
        amount_spent: None,
        default_email_address: Some(CustomerDefaultEmailAddressRecord(
          email_address: default_email,
          marketing_state: None,
          marketing_opt_in_level: None,
          marketing_updated_at: None,
        )),
        default_phone_number: Some(CustomerDefaultPhoneNumberRecord(
          phone_number: default_phone,
          marketing_state: None,
          marketing_opt_in_level: None,
          marketing_updated_at: None,
          marketing_collected_from: None,
        )),
        email_marketing_consent: None,
        sms_marketing_consent: None,
        default_address: None,
        account_activation_token: None,
        created_at: None,
        updated_at: None,
      ))
    }
    None -> Error(Nil)
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
        code: None,
        enabled: option.unwrap(json_get_bool(node, "enabled"), True),
        notify: option.unwrap(json_get_bool(node, "notify"), True),
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
  upstream: UpstreamContext,
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
          gift_card_types.UserError(
            field: ["input", "initialValue"],
            code: Some("GREATER_THAN"),
            message: "must be greater than 0",
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
      let raw_code = graphql_helpers.read_arg_string(input, "code")
      let customer_id =
        graphql_helpers.read_arg_string_nonempty(input, "customerId")
      case gift_card_trial_assignment_user_error(store, input) {
        Some(error) ->
          gift_card_create_error_result(
            key,
            field,
            fragments,
            variables,
            store,
            identity_after_id,
            error,
          )
        None -> {
          case validate_gift_card_create_code(store, raw_code, gid) {
            Error(error) ->
              gift_card_create_error_result(
                key,
                field,
                fragments,
                variables,
                store,
                identity_after_id,
                error,
              )
            Ok(code) -> {
              case gift_card_create_customer_user_error(store, customer_id) {
                Some(error) ->
                  gift_card_create_error_result(
                    key,
                    field,
                    fragments,
                    variables,
                    store,
                    identity_after_id,
                    error,
                  )
                None -> {
                  let store =
                    maybe_hydrate_gift_card_configuration_for_create(
                      store,
                      upstream,
                    )
                  case
                    gift_card_create_issue_limit_user_error(
                      store,
                      initial_value,
                    )
                  {
                    Some(error) ->
                      gift_card_create_error_result(
                        key,
                        field,
                        fragments,
                        variables,
                        store,
                        identity_after_id,
                        error,
                      )
                    None -> {
                      let recipient_errors =
                        gift_card_recipient_attribute_user_errors(input)
                      case recipient_errors {
                        [first, ..] ->
                          gift_card_create_error_result(
                            key,
                            field,
                            fragments,
                            variables,
                            store,
                            identity_after_id,
                            first,
                          )
                        [] -> {
                          let last_chars = last_characters_from_code(code)
                          let masked = masked_code_string(last_chars)
                          let #(now, identity_after_ts) =
                            synthetic_identity.make_synthetic_timestamp(
                              identity_after_id,
                            )
                          let recipient_attributes =
                            read_recipient_attributes(
                              dict_get(input, "recipientAttributes"),
                              None,
                            )
                          let recipient_id = case
                            graphql_helpers.read_arg_string_nonempty(
                              input,
                              "recipientId",
                            )
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
                              code: Some(code),
                              enabled: True,
                              notify: True,
                              deactivated_at: None,
                              expires_on: graphql_helpers.read_arg_string_nonempty(
                                input,
                                "expiresOn",
                              ),
                              note: graphql_helpers.read_arg_string_nonempty(
                                input,
                                "note",
                              ),
                              template_suffix: graphql_helpers.read_arg_string_nonempty(
                                input,
                                "templateSuffix",
                              ),
                              created_at: now,
                              updated_at: now,
                              initial_value: initial_value,
                              balance: initial_value,
                              customer_id: customer_id,
                              recipient_id: recipient_id,
                              source: Some("api_client"),
                              recipient_attributes: recipient_attributes,
                              transactions: [],
                            )
                          let #(_, store_after) =
                            store.stage_create_gift_card(store, record)
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
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}

fn maybe_hydrate_gift_card_configuration_for_create(
  store: Store,
  upstream: UpstreamContext,
) -> Store {
  case configured_issue_limit(store) {
    Some(_) -> store
    None -> {
      let query =
        "
query GiftCardCreateConfiguration {
  giftCardConfiguration {
    issueLimit { amount currencyCode }
    purchaseLimit { amount currencyCode }
  }
}
"
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "GiftCardCreateConfiguration",
          query,
          json.object([]),
        )
      {
        Ok(value) -> hydrate_gift_card_from_upstream_response(store, value)
        Error(_) -> store
      }
    }
  }
}

fn maybe_hydrate_gift_card_configuration_for_credit(
  store: Store,
  upstream: UpstreamContext,
) -> Store {
  case configured_issue_limit(store) {
    Some(_) -> store
    None -> {
      let query =
        "
query GiftCardCreditConfiguration {
  giftCardConfiguration {
    issueLimit { amount currencyCode }
    purchaseLimit { amount currencyCode }
  }
}
"
      case
        upstream_query.fetch_sync(
          upstream.origin,
          upstream.transport,
          upstream.headers,
          "GiftCardCreditConfiguration",
          query,
          json.object([]),
        )
      {
        Ok(value) -> hydrate_gift_card_from_upstream_response(store, value)
        Error(_) -> store
      }
    }
  }
}

fn gift_card_create_issue_limit_user_error(
  store: Store,
  initial_value: Money,
) -> Option(gift_card_types.UserError) {
  case configured_issue_limit(store) {
    Some(#(issue_limit, limit_amount)) -> {
      let requested_amount =
        parse_decimal_amount(root_field.StringVal(initial_value.amount))
      case requested_amount >. limit_amount {
        True ->
          Some(gift_card_types.UserError(
            field: ["input", "initialValue"],
            code: Some("GIFT_CARD_LIMIT_EXCEEDED"),
            message: gift_card_limit_exceeded_message(issue_limit, limit_amount),
          ))
        False -> None
      }
    }
    None -> None
  }
}

fn configured_issue_limit(store: Store) -> Option(#(Money, Float)) {
  let configuration = store.get_effective_gift_card_configuration(store)
  let issue_limit = configuration.issue_limit
  let limit_amount =
    parse_decimal_amount(root_field.StringVal(issue_limit.amount))
  case limit_amount <=. 0.0 {
    True -> None
    False -> Some(#(issue_limit, limit_amount))
  }
}

fn gift_card_limit_exceeded_message(
  issue_limit: Money,
  limit_amount: Float,
) -> String {
  "can't exceed $"
  <> format_currency_limit_amount(limit_amount)
  <> " "
  <> issue_limit.currency_code
}

fn format_currency_limit_amount(amount: Float) -> String {
  let cents = float.round(amount *. 100.0)
  let dollars = cents / 100
  let remainder = cents - dollars * 100
  let cents_str = case remainder < 10 {
    True -> "0" <> int.to_string(remainder)
    False -> int.to_string(remainder)
  }
  add_thousands_separators(int.to_string(dollars)) <> "." <> cents_str
}

fn add_thousands_separators(digits: String) -> String {
  let length = string.length(digits)
  case length <= 3 {
    True -> digits
    False ->
      add_thousands_separators(string.drop_end(digits, 3))
      <> ","
      <> string.slice(digits, length - 3, 3)
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
      let early_errors = case
        gift_card_trial_assignment_user_error(store, input)
      {
        Some(error) -> [error]
        None -> gift_card_update_deactivated_user_errors(input, current)
      }
      case early_errors {
        [_first, ..] ->
          gift_card_update_error_result(
            key,
            field,
            fragments,
            variables,
            store,
            identity,
            early_errors,
          )
        [] -> {
          let customer_error =
            gift_card_update_customer_user_error(store, current, input)
          case customer_error {
            Some(error) ->
              gift_card_update_error_result(
                key,
                field,
                fragments,
                variables,
                store,
                identity,
                [error],
              )
            None -> {
              let has_update_argument =
                gift_card_update_has_editable_argument(input)
              case has_update_argument {
                False ->
                  gift_card_update_error_result(
                    key,
                    field,
                    fragments,
                    variables,
                    store,
                    identity,
                    [gift_card_update_missing_arguments_user_error()],
                  )
                True -> {
                  let recipient_errors =
                    gift_card_update_recipient_attribute_user_errors(
                      store,
                      current,
                      input,
                    )
                  case recipient_errors {
                    [_first, ..] ->
                      gift_card_update_error_result(
                        key,
                        field,
                        fragments,
                        variables,
                        store,
                        identity,
                        recipient_errors,
                      )
                    [] -> {
                      let #(now, identity_after_ts) =
                        synthetic_identity.make_synthetic_timestamp(identity)
                      let updated =
                        GiftCardRecord(
                          ..current,
                          note: new_note,
                          template_suffix: new_template,
                          expires_on: new_expires,
                          customer_id: new_customer,
                          notify: current.notify,
                          recipient_id: new_recipient_id,
                          recipient_attributes: new_recipient_attributes,
                          updated_at: now,
                        )
                      let #(_, store_after) =
                        store.stage_update_gift_card(store, updated)
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
                  }
                }
              }
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
  upstream: UpstreamContext,
  kind: String,
  preferred_amount_key: String,
  preferred_input_key: String,
  payload_typename: String,
  transaction_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = read_gift_card_id(args)
  let raw_money =
    read_mutation_money(args, preferred_amount_key, preferred_input_key)
  let magnitude = parse_decimal_amount(root_field.StringVal(raw_money.amount))
  let store_for_validation = case kind {
    "CREDIT" ->
      maybe_hydrate_gift_card_configuration_for_credit(store, upstream)
    _ -> store
  }
  let existing =
    id
    |> option.then(fn(value) {
      store.get_effective_gift_card_by_id(store_for_validation, value)
    })
  let processed_at_explicit =
    read_mutation_processed_at(args, preferred_input_key)
  let validation_error =
    validate_gift_card_transaction(
      store_for_validation,
      existing,
      raw_money,
      magnitude,
      processed_at_explicit,
      kind,
      preferred_amount_key,
      preferred_input_key,
    )

  case existing, validation_error {
    _, Some(user_error) ->
      gift_card_transaction_error_result(
        key,
        user_error,
        payload_typename,
        field,
        fragments,
        variables,
        store_for_validation,
        identity,
      )
    Some(current), None -> {
      let current_balance =
        parse_decimal_amount(root_field.StringVal(current.balance.amount))
      let signed = case kind {
        "CREDIT" -> magnitude
        _ -> 0.0 -. magnitude
      }
      let currency = current.balance.currency_code
      let #(transaction_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, transaction_typename)
      let #(processed_at, identity_after_processed) = case
        processed_at_explicit
      {
        Some(value) -> #(value, identity_after_id)
        None -> synthetic_identity.make_synthetic_timestamp(identity_after_id)
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
        synthetic_identity.make_synthetic_timestamp(identity_after_processed)
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
        store.stage_update_gift_card(store_for_validation, updated)
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
    None, None ->
      gift_card_transaction_error_result(
        key,
        not_found_user_error(),
        payload_typename,
        field,
        fragments,
        variables,
        store_for_validation,
        identity,
      )
  }
}

fn gift_card_transaction_error_result(
  key: String,
  user_error: gift_card_types.UserError,
  payload_typename: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload = empty_payload([user_error])
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

fn validate_gift_card_transaction(
  store: Store,
  existing: Option(GiftCardRecord),
  raw_money: Money,
  magnitude: Float,
  processed_at: Option(String),
  kind: String,
  preferred_amount_key: String,
  preferred_input_key: String,
) -> Option(gift_card_types.UserError) {
  case magnitude <=. 0.0 {
    True ->
      Some(gift_card_types.UserError(
        field: [preferred_input_key, preferred_amount_key, "amount"],
        code: Some("NEGATIVE_OR_ZERO_AMOUNT"),
        message: "A positive amount must be used.",
      ))
    False ->
      case existing {
        None -> Some(not_found_user_error())
        Some(current) ->
          validate_existing_gift_card_transaction(
            store,
            current,
            raw_money,
            magnitude,
            processed_at,
            kind,
            preferred_amount_key,
            preferred_input_key,
          )
      }
  }
}

fn validate_existing_gift_card_transaction(
  store: Store,
  current: GiftCardRecord,
  raw_money: Money,
  magnitude: Float,
  processed_at: Option(String),
  kind: String,
  preferred_amount_key: String,
  preferred_input_key: String,
) -> Option(gift_card_types.UserError) {
  case validate_processed_at(processed_at, preferred_input_key) {
    Some(error) -> Some(error)
    None ->
      case gift_card_is_expired(current) {
        True -> Some(invalid_user_error(["id"], "The gift card has expired."))
        False ->
          case current.enabled {
            False ->
              Some(invalid_user_error(["id"], "The gift card is deactivated."))
            True ->
              validate_gift_card_transaction_money(
                store,
                current,
                raw_money,
                magnitude,
                kind,
                preferred_amount_key,
                preferred_input_key,
              )
          }
      }
  }
}

fn validate_processed_at(
  processed_at: Option(String),
  preferred_input_key: String,
) -> Option(gift_card_types.UserError) {
  case processed_at {
    Some(value) ->
      case iso_timestamp.parse_iso(value) {
        Ok(ms) ->
          case ms < 0 {
            True ->
              Some(invalid_user_error(
                [preferred_input_key, "processedAt"],
                "A valid processed date must be used.",
              ))
            False -> {
              let now_ms =
                iso_timestamp.now_iso()
                |> iso_timestamp.parse_iso
                |> result.unwrap(0)
              case ms > now_ms {
                True ->
                  Some(invalid_user_error(
                    [preferred_input_key, "processedAt"],
                    "The processed date must not be in the future.",
                  ))
                False -> None
              }
            }
          }
        Error(_) ->
          Some(invalid_user_error(
            [preferred_input_key, "processedAt"],
            "A valid processed date must be used.",
          ))
      }
    None -> None
  }
}

fn validate_gift_card_transaction_money(
  store: Store,
  current: GiftCardRecord,
  raw_money: Money,
  magnitude: Float,
  kind: String,
  preferred_amount_key: String,
  preferred_input_key: String,
) -> Option(gift_card_types.UserError) {
  case
    raw_money.currency_code != ""
    && raw_money.currency_code != current.balance.currency_code
  {
    True ->
      Some(gift_card_types.UserError(
        field: [preferred_input_key, preferred_amount_key, "currencyCode"],
        code: Some("MISMATCHING_CURRENCY"),
        message: "The currency provided does not match the currency of the gift card.",
      ))
    False -> {
      let current_balance =
        parse_decimal_amount(root_field.StringVal(current.balance.amount))
      case kind == "DEBIT" && magnitude >. current_balance {
        True ->
          Some(gift_card_types.UserError(
            field: [preferred_input_key, preferred_amount_key, "amount"],
            code: Some("INSUFFICIENT_FUNDS"),
            message: "The gift card does not have sufficient funds to satisfy the request.",
          ))
        False ->
          gift_card_credit_limit_user_error(
            store,
            current,
            magnitude,
            kind,
            preferred_amount_key,
            preferred_input_key,
          )
      }
    }
  }
}

fn gift_card_credit_limit_user_error(
  store: Store,
  current: GiftCardRecord,
  magnitude: Float,
  kind: String,
  preferred_amount_key: String,
  preferred_input_key: String,
) -> Option(gift_card_types.UserError) {
  case kind {
    "CREDIT" -> {
      let current_balance =
        parse_decimal_amount(root_field.StringVal(current.balance.amount))
      case configured_issue_limit(store) {
        Some(#(_, limit_amount)) ->
          case current_balance +. magnitude >. limit_amount {
            True ->
              Some(gift_card_types.UserError(
                field: [preferred_input_key, preferred_amount_key, "amount"],
                code: Some("GIFT_CARD_LIMIT_EXCEEDED"),
                message: "The gift card's value exceeds the allowed limits.",
              ))
            False -> None
          }
        None -> None
      }
    }
    _ -> None
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
  notification_target: String,
  payload_typename: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  let id = read_gift_card_id(args)
  let existing = case id {
    Some(value) -> store.get_effective_gift_card_by_id(store, value)
    None -> None
  }
  let validation_error = case shop_is_trial(store), existing {
    True, _ -> Some(notification_trial_user_error())
    False, None -> Some(not_found_user_error())
    False, Some(current) ->
      gift_card_notification_user_error(store, current, notification_target)
  }
  case validation_error {
    Some(error) -> {
      let payload = empty_payload([error])
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
    None -> {
      let assert Some(current) = existing
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

fn not_found_user_error() -> gift_card_types.UserError {
  gift_card_types.UserError(
    field: ["id"],
    code: Some("GIFT_CARD_NOT_FOUND"),
    message: "The gift card could not be found.",
  )
}

fn invalid_user_error(
  field: List(String),
  message: String,
) -> gift_card_types.UserError {
  gift_card_types.UserError(
    field: field,
    code: Some("INVALID"),
    message: message,
  )
}

fn gift_card_trial_assignment_user_error(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(gift_card_types.UserError) {
  case shop_is_trial(store), gift_card_trial_assignment_target(input) {
    True, Some(#(field, type_)) ->
      Some(invalid_user_error(
        ["input", field],
        "A trial shop cannot assign a " <> type_ <> " to a gift card.",
      ))
    _, _ -> None
  }
}

fn gift_card_trial_assignment_target(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(#(String, String)) {
  case graphql_helpers.read_arg_string_nonempty(input, "customerId") {
    Some(_) -> Some(#("customerId", "customer"))
    None -> {
      let has_recipient_attributes = case
        dict_get(input, "recipientAttributes")
      {
        Some(root_field.ObjectVal(_)) -> True
        _ -> False
      }
      let has_recipient_id = case
        graphql_helpers.read_arg_string_nonempty(input, "recipientId")
      {
        Some(_) -> True
        None -> False
      }
      case has_recipient_attributes || has_recipient_id {
        True -> Some(#("recipientAttributes", "recipient"))
        False -> None
      }
    }
  }
}

fn gift_card_update_error_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
  errors: List(gift_card_types.UserError),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload = empty_payload(errors)
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

fn gift_card_update_deactivated_user_errors(
  input: Dict(String, root_field.ResolvedValue),
  current: GiftCardRecord,
) -> List(gift_card_types.UserError) {
  case current.enabled {
    True -> []
    False ->
      case
        [
          "expiresOn",
          "customerId",
          "recipientAttributes",
          "crossCurrencyRedemptionStrategy",
        ]
        |> list.find(fn(field) { dict_has_key(input, field) })
      {
        Ok(field) -> [
          invalid_user_error(["input", field], "The gift card is deactivated."),
        ]
        Error(Nil) -> []
      }
  }
}

fn gift_card_update_customer_user_error(
  store: Store,
  current: GiftCardRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(gift_card_types.UserError) {
  case dict_has_key(input, "customerId") {
    False -> None
    True -> {
      let new_customer =
        graphql_helpers.read_arg_string_nonempty(input, "customerId")
      case new_customer == current.customer_id, new_customer {
        True, _ -> None
        _, None -> None
        False, Some(id) ->
          case store.get_effective_customer_by_id(store, id) {
            Some(_) -> None
            None -> Some(gift_card_create_customer_not_found_user_error())
          }
      }
    }
  }
}

fn gift_card_update_has_editable_argument(
  input: Dict(String, root_field.ResolvedValue),
) -> Bool {
  [
    "note",
    "expiresOn",
    "customerId",
    "templateSuffix",
    "recipientId",
    "recipientAttributes",
    "crossCurrencyRedemptionStrategy",
    "notify",
  ]
  |> list.any(fn(field) { dict_has_key(input, field) })
}

fn gift_card_update_missing_arguments_user_error() -> gift_card_types.UserError {
  invalid_user_error(
    ["input"],
    "At least one argument is required in the input.",
  )
}

fn gift_card_update_recipient_attribute_user_errors(
  store: Store,
  current: GiftCardRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(gift_card_types.UserError) {
  case dict_get(input, "recipientAttributes") {
    Some(root_field.ObjectVal(attributes)) -> {
      let validation_errors = gift_card_recipient_attribute_user_errors(input)
      case validation_errors {
        [_first, ..] -> validation_errors
        [] -> {
          let recipient_error =
            gift_card_update_recipient_customer_user_error(
              store,
              current,
              attributes,
            )
          case recipient_error {
            Some(error) -> [error]
            None -> []
          }
        }
      }
    }
    _ -> []
  }
}

fn gift_card_recipient_attribute_user_errors(
  input: Dict(String, root_field.ResolvedValue),
) -> List(gift_card_types.UserError) {
  case dict_get(input, "recipientAttributes") {
    Some(root_field.ObjectVal(attributes)) ->
      recipient_text_length_user_errors(attributes)
      |> list.append(recipient_text_html_user_errors(attributes))
      |> list.append(recipient_send_at_user_errors(attributes))
    _ -> []
  }
}

fn recipient_text_length_user_errors(
  attributes: Dict(String, root_field.ResolvedValue),
) -> List(gift_card_types.UserError) {
  [
    #("preferredName", 255),
    #("message", 200),
  ]
  |> list.filter_map(fn(pair) {
    let #(field, max_length) = pair
    case graphql_helpers.read_arg_string(attributes, field) {
      Some(value) ->
        case string.length(value) > max_length {
          True ->
            Ok(gift_card_types.UserError(
              field: ["input", "recipientAttributes", field],
              code: Some("TOO_LONG"),
              message: field
                <> " is too long (maximum is "
                <> int.to_string(max_length)
                <> ")",
            ))
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

fn recipient_text_html_user_errors(
  attributes: Dict(String, root_field.ResolvedValue),
) -> List(gift_card_types.UserError) {
  [
    #("preferredName", "Preferred name cannot contain HTML tags"),
    #("message", "Message cannot contain HTML tags"),
  ]
  |> list.filter_map(fn(pair) {
    let #(field, message) = pair
    case graphql_helpers.read_arg_string(attributes, field) {
      Some(value) ->
        case contains_html_tag(value) {
          True ->
            Ok(invalid_user_error(
              ["input", "recipientAttributes", field],
              message,
            ))
          False -> Error(Nil)
        }
      _ -> Error(Nil)
    }
  })
}

fn recipient_send_at_user_errors(
  attributes: Dict(String, root_field.ResolvedValue),
) -> List(gift_card_types.UserError) {
  case graphql_helpers.read_arg_string(attributes, "sendNotificationAt") {
    Some(value) ->
      case send_notification_at_in_range(value) {
        True -> []
        False -> [
          invalid_user_error(
            ["input", "recipientAttributes", "sendNotificationAt"],
            "Send notification at must be within 90 days from now",
          ),
        ]
      }
    None -> []
  }
}

fn send_notification_at_in_range(value: String) -> Bool {
  let max_offset_ms = 90 * 24 * 60 * 60 * 1000
  case
    iso_timestamp.parse_iso(value),
    iso_timestamp.parse_iso(iso_timestamp.now_iso())
  {
    Ok(send_at_ms), Ok(now_ms) ->
      send_at_ms >= now_ms && send_at_ms <= now_ms + max_offset_ms
    _, _ -> False
  }
}

fn contains_html_tag(value: String) -> Bool {
  contains_html_tag_in_graphemes(string.to_graphemes(value))
}

fn contains_html_tag_in_graphemes(chars: List(String)) -> Bool {
  case chars {
    [] -> False
    ["<", ..rest] ->
      case scan_html_tag(rest) {
        True -> True
        False -> contains_html_tag_in_graphemes(rest)
      }
    [_, ..rest] -> contains_html_tag_in_graphemes(rest)
  }
}

fn scan_html_tag(chars: List(String)) -> Bool {
  case chars {
    [] -> False
    [first, ..rest] ->
      case first == "/" {
        True ->
          case rest {
            [after_slash, ..tail] ->
              ascii_letter(after_slash) && scan_until_tag_close(tail)
            [] -> False
          }
        False -> ascii_letter(first) && scan_until_tag_close(rest)
      }
  }
}

fn scan_until_tag_close(chars: List(String)) -> Bool {
  case chars {
    [] -> False
    [">", ..] -> True
    [_, ..rest] -> scan_until_tag_close(rest)
  }
}

fn ascii_letter(value: String) -> Bool {
  let lower = string.lowercase(value)
  list.contains(
    [
      "a",
      "b",
      "c",
      "d",
      "e",
      "f",
      "g",
      "h",
      "i",
      "j",
      "k",
      "l",
      "m",
      "n",
      "o",
      "p",
      "q",
      "r",
      "s",
      "t",
      "u",
      "v",
      "w",
      "x",
      "y",
      "z",
    ],
    lower,
  )
}

fn gift_card_update_recipient_customer_user_error(
  store: Store,
  current: GiftCardRecord,
  attributes: Dict(String, root_field.ResolvedValue),
) -> Option(gift_card_types.UserError) {
  let new_recipient = case
    graphql_helpers.read_arg_string_nonempty(attributes, "id")
  {
    Some(id) -> Some(id)
    None -> graphql_helpers.read_arg_string_nonempty(attributes, "recipientId")
  }
  case new_recipient == current.recipient_id, new_recipient {
    True, _ -> None
    _, None -> None
    False, Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) -> None
        None ->
          Some(gift_card_types.UserError(
            field: ["input", "recipientAttributes", "id"],
            code: Some("CUSTOMER_NOT_FOUND"),
            message: "The customer could not be found.",
          ))
      }
  }
}

fn gift_card_create_error_result(
  key: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  store: Store,
  identity: SyntheticIdentityRegistry,
  error: gift_card_types.UserError,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload = empty_payload([error])
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
    identity,
  )
}

fn gift_card_code_user_error(
  code: String,
  message: String,
) -> gift_card_types.UserError {
  gift_card_types.UserError(
    field: ["input", "code"],
    code: Some(code),
    message: message,
  )
}

fn gift_card_duplicate_code_user_error() -> gift_card_types.UserError {
  gift_card_types.UserError(
    field: ["input", "code"],
    code: None,
    message: "Code has already been taken",
  )
}

fn gift_card_create_customer_not_found_user_error() -> gift_card_types.UserError {
  gift_card_types.UserError(
    field: ["input", "customerId"],
    code: Some("CUSTOMER_NOT_FOUND"),
    message: "The customer could not be found.",
  )
}

fn gift_card_create_customer_user_error(
  store: Store,
  customer_id: Option(String),
) -> Option(gift_card_types.UserError) {
  case customer_id {
    None -> None
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) -> None
        None -> Some(gift_card_create_customer_not_found_user_error())
      }
  }
}

fn notification_trial_user_error() -> gift_card_types.UserError {
  invalid_user_error(
    ["base"],
    "Gift card notifications are not available for trial shops.",
  )
}

fn gift_card_notification_user_error(
  store: Store,
  current: GiftCardRecord,
  notification_target: String,
) -> Option(gift_card_types.UserError) {
  case current.notify {
    False ->
      Some(invalid_user_error(["id"], "Gift card notifications are disabled."))
    True ->
      gift_card_notification_lifecycle_user_error(
        store,
        current,
        notification_target,
      )
  }
}

fn gift_card_notification_lifecycle_user_error(
  store: Store,
  current: GiftCardRecord,
  notification_target: String,
) -> Option(gift_card_types.UserError) {
  case gift_card_is_expired(current) {
    True -> Some(invalid_user_error(["id"], "The gift card has expired."))
    False ->
      case current.enabled {
        False ->
          Some(invalid_user_error(["id"], "The gift card is deactivated."))
        True ->
          gift_card_notification_owner_user_error(
            store,
            current,
            notification_target,
          )
      }
  }
}

fn gift_card_notification_owner_user_error(
  store: Store,
  current: GiftCardRecord,
  notification_target: String,
) -> Option(gift_card_types.UserError) {
  let owner_id = case notification_target {
    "recipient" -> current.recipient_id
    _ -> current.customer_id
  }
  case owner_id {
    None -> Some(missing_notification_owner_user_error(notification_target))
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        None ->
          Some(notification_owner_not_found_user_error(notification_target))
        Some(customer) ->
          case customer_has_contact_information(customer) {
            True -> None
            False ->
              Some(notification_owner_no_contact_user_error(notification_target))
          }
      }
  }
}

fn missing_notification_owner_user_error(
  notification_target: String,
) -> gift_card_types.UserError {
  case notification_target {
    "recipient" ->
      invalid_user_error(["base"], "The gift card has no recipient.")
    _ -> invalid_user_error(["base"], "The gift card has no customer.")
  }
}

fn notification_owner_not_found_user_error(
  notification_target: String,
) -> gift_card_types.UserError {
  case notification_target {
    "recipient" ->
      gift_card_types.UserError(
        field: ["base"],
        code: Some("RECIPIENT_NOT_FOUND"),
        message: "The recipient could not be found.",
      )
    _ ->
      gift_card_types.UserError(
        field: ["base"],
        code: Some("CUSTOMER_NOT_FOUND"),
        message: "The customer could not be found.",
      )
  }
}

fn notification_owner_no_contact_user_error(
  notification_target: String,
) -> gift_card_types.UserError {
  case notification_target {
    "recipient" ->
      invalid_user_error(
        ["base"],
        "The recipient has no contact information (e.g. email address or phone number).",
      )
    _ ->
      invalid_user_error(
        ["base"],
        "The customer has no contact information (e.g. email address or phone number).",
      )
  }
}

fn gift_card_is_expired(record: GiftCardRecord) -> Bool {
  case record.expires_on {
    Some(expires_on) -> {
      let today = string.slice(iso_timestamp.now_iso(), 0, 10)
      string.compare(expires_on, today) == Lt
    }
    None -> False
  }
}

fn shop_is_trial(store: Store) -> Bool {
  case store.get_effective_shop(store) {
    Some(shop) ->
      string.contains(string.lowercase(shop.plan.public_display_name), "trial")
    None -> False
  }
}

fn customer_has_contact_information(customer: CustomerRecord) -> Bool {
  option_string_has_value(customer.email)
  || case customer.default_email_address {
    Some(email) -> option_string_has_value(email.email_address)
    None -> False
  }
  || case customer.default_phone_number {
    Some(phone) -> option_string_has_value(phone.phone_number)
    None -> False
  }
}

fn option_string_has_value(value: Option(String)) -> Bool {
  case value {
    Some(raw) -> string.trim(raw) != ""
    None -> False
  }
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

fn normalize_gift_card_code(
  raw: Option(String),
  fallback_id: String,
) -> Result(String, gift_card_types.UserError) {
  case raw {
    None -> Ok(proxy_code(fallback_id))
    Some(value) -> {
      let normalized = normalize_provided_gift_card_code(value)
      let length = string.length(normalized)
      case length < 8 {
        True ->
          Error(gift_card_code_user_error(
            "TOO_SHORT",
            "Code must be at least 8 characters long",
          ))
        False ->
          case length > 20 {
            True ->
              Error(gift_card_code_user_error(
                "TOO_LONG",
                "Code must be at most 20 characters long",
              ))
            False ->
              case gift_card_code_is_alphanumeric(normalized) {
                True -> Ok(normalized)
                False ->
                  Error(gift_card_code_user_error(
                    "INVALID",
                    "Code can only contain letters(a-z) and numbers(0-9)",
                  ))
              }
          }
      }
    }
  }
}

fn validate_gift_card_create_code(
  store: Store,
  raw: Option(String),
  fallback_id: String,
) -> Result(String, gift_card_types.UserError) {
  case normalize_gift_card_code(raw, fallback_id) {
    Error(error) -> Error(error)
    Ok(code) ->
      case gift_card_code_is_taken(store, code) {
        True -> Error(gift_card_duplicate_code_user_error())
        False -> Ok(code)
      }
  }
}

fn proxy_code(fallback_id: String) -> String {
  let padded = pad_start_zero(gift_card_tail(fallback_id), 16)
  let length = string.length(padded)
  let hex = case length > 16 {
    True -> string.slice(padded, length - 16, 16)
    False -> padded
  }
  string.to_graphemes(hex)
  |> list.map(substitute_shopify_generated_code_char)
  |> string.join("")
}

fn pad_start_zero(value: String, width: Int) -> String {
  let length = string.length(value)
  case length >= width {
    True -> value
    False -> string.repeat("0", width - length) <> value
  }
}

fn normalize_provided_gift_card_code(value: String) -> String {
  string.to_graphemes(value)
  |> list.filter(fn(g) {
    case g {
      " " | "\t" | "\n" | "\r" | "-" -> False
      _ -> True
    }
  })
  |> string.join("")
  |> string.trim
  |> string.lowercase
}

fn gift_card_code_is_alphanumeric(code: String) -> Bool {
  string.to_graphemes(code)
  |> list.all(is_gift_card_code_character)
}

fn is_gift_card_code_character(char: String) -> Bool {
  case char {
    "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" -> True
    "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" -> True
    "u" | "v" | "w" | "x" | "y" | "z" -> True
    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" -> True
    _ -> False
  }
}

fn substitute_shopify_generated_code_char(char: String) -> String {
  case char {
    "0" -> "g"
    "1" -> "h"
    _ -> string.lowercase(char)
  }
}

fn gift_card_code_is_taken(store: Store, code: String) -> Bool {
  store.list_effective_gift_cards(store)
  |> list.any(fn(record) { record.code == Some(code) })
}

fn last_characters_from_code(code: String) -> String {
  let length = string.length(code)
  let suffix = case length >= 4 {
    True -> string.slice(code, length - 4, 4)
    False -> code
  }
  pad_start_zero(suffix, 4)
}

fn masked_code_string(last_chars: String) -> String {
  "\u{2022}\u{2022}\u{2022}\u{2022} \u{2022}\u{2022}\u{2022}\u{2022} \u{2022}\u{2022}\u{2022}\u{2022} "
  <> last_chars
}
// ---------------------------------------------------------------------------
// Payload serialization
// ---------------------------------------------------------------------------
