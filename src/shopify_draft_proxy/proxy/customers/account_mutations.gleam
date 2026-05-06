//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/customers/customer_mutations.{
  customer_missing_result,
}
import shopify_draft_proxy/proxy/customers/customer_types.{
  type StoreCreditAccountResolution, type UserError, MutationFieldResult,
  StoreCreditAccountResolutionError, StoreCreditAccountResolved, UserError,
}
import shopify_draft_proxy/proxy/customers/inputs.{
  build_merged_customer, customer_metafield_key, format_cents, gid_tail,
  make_email_consent_from, make_sms_consent_from, parse_cents, read_money,
  read_nested_object, read_obj_array_strings, read_obj_raw_string,
  read_obj_string,
}
import shopify_draft_proxy/proxy/customers/serializers.{
  customer_payload_json, merge_error_payload, merge_payload_json,
  order_customer_payload_json, store_credit_payload_json, user_error_source,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  SrcList, SrcNull, SrcString, get_field_response_key, project_graphql_value,
  src_object,
}
import shopify_draft_proxy/state/iso_timestamp
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerAddressRecord, type CustomerRecord, type Money,
  CustomerAddressRecord, CustomerDefaultEmailAddressRecord,
  CustomerDefaultPhoneNumberRecord, CustomerMergeRequestRecord,
  CustomerMetafieldRecord, CustomerOrderSummaryRecord, CustomerRecord, Money,
  StoreCreditAccountRecord, StoreCreditAccountTransactionRecord,
}

@internal
pub fn handle_email_consent(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let customer_id = read_obj_string(input, "customerId")
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let consent = read_nested_object(input, "emailMarketingConsent")
          case customer.default_email_address {
            Some(default_email_address) -> {
              let updated =
                CustomerRecord(
                  ..customer,
                  default_email_address: Some(CustomerDefaultEmailAddressRecord(
                    email_address: default_email_address.email_address,
                    marketing_state: read_obj_string(consent, "marketingState"),
                    marketing_opt_in_level: read_obj_string(
                      consent,
                      "marketingOptInLevel",
                    ),
                    marketing_updated_at: read_obj_string(
                      consent,
                      "consentUpdatedAt",
                    ),
                  )),
                  email_marketing_consent: make_email_consent_from(consent),
                )
              let #(_, next_store) = store.stage_update_customer(store, updated)
              let payload =
                customer_payload_json(
                  next_store,
                  "CustomerEmailMarketingConsentUpdatePayload",
                  Some(updated),
                  None,
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [id],
                  "customerEmailMarketingConsentUpdate",
                ),
                next_store,
                identity,
              )
            }
            None -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerEmailMarketingConsentUpdatePayload",
                  Some(customer),
                  None,
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerEmailMarketingConsentUpdate",
                ),
                store,
                identity,
              )
            }
          }
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerEmailMarketingConsentUpdatePayload",
            "customerEmailMarketingConsentUpdate",
            ["input", "customerId"],
            "Customer not found",
            Some("INVALID"),
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerEmailMarketingConsentUpdatePayload",
        "customerEmailMarketingConsentUpdate",
        ["input", "customerId"],
        "Customer not found",
        Some("INVALID"),
      )
  }
}

@internal
pub fn handle_sms_consent(store, identity, field, fragments, variables) {
  let input =
    graphql_helpers.read_arg_object(
      graphql_helpers.field_args(field, variables),
      "input",
    )
    |> option.unwrap(dict.new())
  let customer_id = read_obj_string(input, "customerId")
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let consent = read_nested_object(input, "smsMarketingConsent")
          case customer.default_phone_number {
            Some(default_phone_number) -> {
              let updated =
                CustomerRecord(
                  ..customer,
                  default_phone_number: Some(CustomerDefaultPhoneNumberRecord(
                    phone_number: default_phone_number.phone_number,
                    marketing_state: read_obj_string(consent, "marketingState"),
                    marketing_opt_in_level: read_obj_string(
                      consent,
                      "marketingOptInLevel",
                    ),
                    marketing_updated_at: read_obj_string(
                      consent,
                      "consentUpdatedAt",
                    ),
                    marketing_collected_from: Some("OTHER"),
                  )),
                  sms_marketing_consent: make_sms_consent_from(consent),
                )
              let #(_, next_store) = store.stage_update_customer(store, updated)
              let payload =
                customer_payload_json(
                  next_store,
                  "CustomerSmsMarketingConsentUpdatePayload",
                  Some(updated),
                  None,
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [id],
                  "customerSmsMarketingConsentUpdate",
                ),
                next_store,
                identity,
              )
            }
            None -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerSmsMarketingConsentUpdatePayload",
                  None,
                  None,
                  None,
                  [
                    UserError(
                      ["input", "smsMarketingConsent"],
                      "A phone number is required to set the SMS consent state.",
                      Some("INVALID"),
                    ),
                  ],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerSmsMarketingConsentUpdate",
                ),
                store,
                identity,
              )
            }
          }
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerSmsMarketingConsentUpdatePayload",
            "customerSmsMarketingConsentUpdate",
            [],
            "Customer not found",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerSmsMarketingConsentUpdatePayload",
        "customerSmsMarketingConsentUpdate",
        [],
        "Customer not found",
        None,
      )
  }
}

@internal
pub fn handle_data_erasure(store, identity, field, variables, cancel) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let root = case cancel {
    True -> "customerCancelDataErasure"
    False -> "customerRequestDataErasure"
  }
  let typename = case cancel {
    True -> "CustomerCancelDataErasurePayload"
    False -> "CustomerRequestDataErasurePayload"
  }
  let errors = case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) ->
          case cancel {
            False -> []
            True ->
              case store.get_customer_data_erasure_request(store, id) {
                Some(request) ->
                  case request.canceled_at {
                    None -> []
                    Some(_) -> [
                      UserError(
                        ["customerId"],
                        "Customer's data is not scheduled for erasure",
                        Some("NOT_BEING_ERASED"),
                      ),
                    ]
                  }
                None -> [
                  UserError(
                    ["customerId"],
                    "Customer's data is not scheduled for erasure",
                    Some("NOT_BEING_ERASED"),
                  ),
                ]
              }
          }
        None -> [
          UserError(
            ["customerId"],
            "Customer does not exist",
            Some("DOES_NOT_EXIST"),
          ),
        ]
      }
    None -> [
      UserError(
        ["customerId"],
        "Customer does not exist",
        Some("DOES_NOT_EXIST"),
      ),
    ]
  }
  let next_store = case errors, customer_id {
    [], Some(id) -> {
      let request =
        types.CustomerDataErasureRequestRecord(
          customer_id: id,
          requested_at: "",
          canceled_at: case cancel {
            True -> Some("")
            False -> None
          },
        )
      store.stage_customer_data_erasure_request(store, request)
    }
    _, _ -> store
  }
  let payload = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString(typename)),
          #("customerId", case errors {
            [] -> graphql_helpers.option_string_source(customer_id)
            _ -> SrcNull
          }),
          #("userErrors", SrcList(list.map(errors, user_error_source))),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      case customer_id {
        Some(id) -> [id]
        None -> []
      },
      root,
    ),
    next_store,
    identity,
  )
}

@internal
pub fn handle_activation_url(store, identity, field, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let #(url, errors, next_store) = case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) ->
          case customer.state {
            Some("DISABLED") | Some("INVITED") -> {
              let token =
                customer.account_activation_token
                |> option.unwrap(activation_token_for_customer(id))
              let updated =
                CustomerRecord(
                  ..customer,
                  account_activation_token: Some(token),
                )
              let #(_, staged_store) =
                store.stage_update_customer(store, updated)
              #(Some(activation_url_for_customer(id, token)), [], staged_store)
            }
            _ -> #(
              None,
              [
                UserError(
                  ["customerId"],
                  "Account already enabled.",
                  Some("account_already_enabled"),
                ),
              ],
              store,
            )
          }
        None -> #(None, [missing_customer_activation_error()], store)
      }
    None -> #(None, [missing_customer_activation_error()], store)
  }
  let payload = case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #(
            "__typename",
            SrcString("CustomerGenerateAccountActivationUrlPayload"),
          ),
          #("accountActivationUrl", case errors {
            [] -> option.unwrap(option.map(url, SrcString), SrcNull)
            _ -> SrcNull
          }),
          #("userErrors", SrcList(list.map(errors, user_error_source))),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
  #(
    MutationFieldResult(
      get_field_response_key(field),
      payload,
      case customer_id {
        Some(id) -> [id]
        None -> []
      },
      "customerGenerateAccountActivationUrl",
    ),
    next_store,
    identity,
  )
}

@internal
pub fn missing_customer_activation_error() -> UserError {
  UserError(
    ["customerId"],
    "The customer can't be found.",
    Some("customer_does_not_exist"),
  )
}

@internal
pub fn activation_url_for_customer(
  customer_id: String,
  token: String,
) -> String {
  "https://shopify-draft-proxy.local/customer-account/activate?customer_id="
  <> customer_id
  <> "&account_activation_token="
  <> token
}

@internal
pub fn activation_token_for_customer(customer_id: String) -> String {
  "draft-proxy-activation-"
  <> option.unwrap(gid_tail(customer_id), token_safe_customer_id(customer_id))
}

@internal
pub fn token_safe_customer_id(customer_id: String) -> String {
  customer_id
  |> string.replace(":", "-")
  |> string.replace("/", "-")
  |> string.replace("?", "-")
  |> string.replace("&", "-")
  |> string.replace("=", "-")
}

@internal
pub fn handle_account_invite(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  let email_input = read_account_invite_email_input(args)
  case customer_id {
    Some(id) ->
      case store.get_effective_customer_by_id(store, id) {
        Some(customer) -> {
          let #(payload, next_store, staged_ids) = case
            customer_account_invitable(customer)
          {
            True -> {
              let email_errors =
                validate_account_invite_email(customer, email_input)
              case email_errors {
                [] -> {
                  let updated =
                    CustomerRecord(..customer, state: Some("INVITED"))
                  let #(_, next_store) =
                    store.stage_update_customer(store, updated)
                  #(
                    customer_payload_json(
                      next_store,
                      "CustomerSendAccountInviteEmailPayload",
                      Some(updated),
                      None,
                      None,
                      [],
                      field,
                      fragments,
                    ),
                    next_store,
                    [id],
                  )
                }
                _ -> #(
                  customer_payload_json(
                    store,
                    "CustomerSendAccountInviteEmailPayload",
                    None,
                    None,
                    None,
                    email_errors,
                    field,
                    fragments,
                  ),
                  store,
                  [],
                )
              }
            }
            False -> #(
              customer_payload_json(
                store,
                "CustomerSendAccountInviteEmailPayload",
                Some(customer),
                None,
                None,
                [
                  UserError(
                    ["customerId"],
                    "Account already enabled",
                    Some("ACCOUNT_ALREADY_ENABLED"),
                  ),
                ],
                field,
                fragments,
              ),
              store,
              [],
            )
          }
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              staged_ids,
              "customerSendAccountInviteEmail",
            ),
            next_store,
            identity,
          )
        }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerSendAccountInviteEmailPayload",
            "customerSendAccountInviteEmail",
            ["customerId"],
            "Customer can't be found",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerSendAccountInviteEmailPayload",
        "customerSendAccountInviteEmail",
        ["customerId"],
        "Customer can't be found",
        None,
      )
  }
}

@internal
pub fn read_account_invite_email_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "email") {
    Ok(root_field.ObjectVal(input)) -> Some(input)
    _ -> None
  }
}

@internal
pub fn validate_account_invite_email(
  customer: CustomerRecord,
  email_input: Option(Dict(String, root_field.ResolvedValue)),
) -> List(UserError) {
  case email_input {
    None -> []
    Some(input) -> {
      [
        validate_account_invite_subject(input),
        validate_account_invite_to(customer, input),
        validate_account_invite_from(input),
        validate_account_invite_bcc(input),
        validate_account_invite_custom_message(input),
      ]
      |> list.flatten()
    }
  }
}

@internal
pub fn validate_account_invite_subject(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_obj_raw_string(input, "subject") {
    None | Some("") -> [
      UserError(["email", "subject"], "Subject can't be blank", Some("INVALID")),
    ]
    Some(subject) ->
      case string.length(subject) > 1000 {
        True -> [account_invite_send_error()]
        False -> []
      }
  }
}

@internal
pub fn validate_account_invite_to(
  customer: CustomerRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_obj_raw_string(input, "to") {
    None | Some("") -> []
    Some(to) ->
      case customer.email {
        Some(email) if email != "" -> [
          UserError(
            ["email", "to"],
            "To must be blank when the customer has an email address",
            Some("INVALID"),
          ),
        ]
        _ ->
          case valid_account_invite_email_address(to) {
            True -> []
            False -> [
              UserError(["email", "to"], "To is invalid", Some("INVALID")),
            ]
          }
      }
  }
}

@internal
pub fn validate_account_invite_from(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case dict.get(input, "from") {
    Ok(_) -> [
      UserError(["email", "from"], "From Sender is invalid", Some("INVALID")),
    ]
    _ -> []
  }
}

@internal
pub fn validate_account_invite_bcc(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  let bcc = read_obj_array_strings(input, "bcc")
  case bcc {
    [] -> []
    _ -> [
      UserError(
        ["email", "bcc"],
        account_invite_bcc_message(bcc),
        Some("INVALID"),
      ),
    ]
  }
}

@internal
pub fn validate_account_invite_custom_message(
  input: Dict(String, root_field.ResolvedValue),
) -> List(UserError) {
  case read_obj_raw_string(input, "customMessage") {
    Some(message) -> {
      let invalid =
        string.length(message) > 5000
        || string.contains(message, "<")
        || string.contains(message, ">")
      case invalid {
        True -> [account_invite_send_error()]
        False -> []
      }
    }
    _ -> []
  }
}

@internal
pub fn account_invite_bcc_message(bcc: List(String)) -> String {
  "Bcc "
  <> string.join(
    list.map(bcc, fn(address) { address <> " is not a valid bcc address" }),
    " and ",
  )
}

@internal
pub fn account_invite_send_error() -> UserError {
  UserError(
    ["customerId"],
    "Error sending account invite to customer.",
    Some("INVALID"),
  )
}

@internal
pub fn valid_account_invite_email_address(email: String) -> Bool {
  case string.split(email, "@") {
    [local, domain] ->
      local != ""
      && domain != ""
      && string.contains(domain, ".")
      && !string.contains(email, " ")
    _ -> False
  }
}

@internal
pub fn customer_account_invitable(customer: CustomerRecord) -> Bool {
  case customer.state {
    Some("DISABLED") | Some("INVITED") -> True
    _ -> False
  }
}

@internal
pub fn handle_payment_method_update_email(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  let id =
    graphql_helpers.read_arg_string_nonempty(args, "customerPaymentMethodId")
  case id {
    Some(payment_id) ->
      case
        store.get_effective_customer_payment_method_by_id(
          store,
          payment_id,
          True,
        )
      {
        Some(method) ->
          case store.get_effective_customer_by_id(store, method.customer_id) {
            Some(customer) -> {
              let payload =
                customer_payload_json(
                  store,
                  "CustomerPaymentMethodSendUpdateEmailPayload",
                  Some(customer),
                  None,
                  None,
                  [],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [payment_id],
                  "customerPaymentMethodSendUpdateEmail",
                ),
                store,
                identity,
              )
            }
            None ->
              customer_missing_result(
                store,
                identity,
                field,
                fragments,
                "CustomerPaymentMethodSendUpdateEmailPayload",
                "customerPaymentMethodSendUpdateEmail",
                ["customerPaymentMethodId"],
                "Customer payment method does not exist",
                None,
              )
          }
        None ->
          customer_missing_result(
            store,
            identity,
            field,
            fragments,
            "CustomerPaymentMethodSendUpdateEmailPayload",
            "customerPaymentMethodSendUpdateEmail",
            ["customerPaymentMethodId"],
            "Customer payment method does not exist",
            None,
          )
      }
    None ->
      customer_missing_result(
        store,
        identity,
        field,
        fragments,
        "CustomerPaymentMethodSendUpdateEmailPayload",
        "customerPaymentMethodSendUpdateEmail",
        ["customerPaymentMethodId"],
        "Customer payment method does not exist",
        None,
      )
  }
}

@internal
pub fn handle_store_credit_adjustment(
  store,
  identity,
  field,
  fragments,
  variables,
  is_credit,
) {
  let args = graphql_helpers.field_args(field, variables)
  let account_id = graphql_helpers.read_arg_string_nonempty(args, "id")
  let input_name = case is_credit {
    True -> "creditInput"
    False -> "debitInput"
  }
  let input =
    graphql_helpers.read_arg_object(args, input_name)
    |> option.unwrap(dict.new())
  let amount = read_money(input)
  let root = case is_credit {
    True -> "storeCreditAccountCredit"
    False -> "storeCreditAccountDebit"
  }
  let typename = case is_credit {
    True -> "StoreCreditAccountCreditPayload"
    False -> "StoreCreditAccountDebitPayload"
  }
  let amount_key = case is_credit {
    True -> "creditAmount"
    False -> "debitAmount"
  }
  case account_id, amount {
    Some(id), Some(money) -> {
      let validation_errors =
        store_credit_adjustment_input_errors(
          input,
          input_name,
          amount_key,
          money,
          is_credit,
        )
      case validation_errors {
        [] ->
          case
            resolve_store_credit_account(store, identity, id, money, is_credit)
          {
            StoreCreditAccountResolved(account, resolved_identity) -> {
              let identity = resolved_identity
              let currency_errors = case
                account.balance.currency_code != money.currency_code
              {
                True -> [
                  UserError(
                    [input_name, amount_key, "currencyCode"],
                    "The currency provided does not match the currency of the store credit account",
                    Some("MISMATCHING_CURRENCY"),
                  ),
                ]
                False -> []
              }
              let balance_cents = parse_cents(account.balance.amount)
              let amount_cents = parse_cents(money.amount)
              let signed = case is_credit {
                True -> amount_cents
                False -> 0 - amount_cents
              }
              let new_balance = balance_cents + signed
              let limit_errors = case !is_credit && new_balance < 0 {
                True -> [
                  UserError(
                    [input_name, amount_key, "amount"],
                    "The store credit account does not have sufficient funds to satisfy the request",
                    Some("INSUFFICIENT_FUNDS"),
                  ),
                ]
                False -> []
              }
              let errors = list.append(currency_errors, limit_errors)
              case errors {
                [] -> {
                  let #(transaction_id, after_id) =
                    synthetic_identity.make_synthetic_gid(
                      identity,
                      "StoreCreditAccountTransaction",
                    )
                  let #(timestamp, after_ts) =
                    synthetic_identity.make_synthetic_timestamp(after_id)
                  let new_account =
                    StoreCreditAccountRecord(
                      ..account,
                      balance: Money(
                        amount: format_cents(new_balance),
                        currency_code: account.balance.currency_code,
                      ),
                    )
                  let transaction =
                    StoreCreditAccountTransactionRecord(
                      id: transaction_id,
                      account_id: account.id,
                      amount: Money(
                        amount: format_cents(signed),
                        currency_code: account.balance.currency_code,
                      ),
                      balance_after_transaction: new_account.balance,
                      created_at: timestamp,
                      event: "ADJUSTMENT",
                    )
                  let next_store =
                    store.stage_store_credit_account(store, new_account)
                    |> store.stage_store_credit_account_transaction(transaction)
                  let payload =
                    store_credit_payload_json(
                      next_store,
                      typename,
                      Some(transaction),
                      [],
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [transaction.id, account.id],
                      root,
                    ),
                    next_store,
                    after_ts,
                  )
                }
                _ -> {
                  let payload =
                    store_credit_payload_json(
                      store,
                      typename,
                      None,
                      errors,
                      field,
                      fragments,
                    )
                  #(
                    MutationFieldResult(
                      get_field_response_key(field),
                      payload,
                      [],
                      root,
                    ),
                    store,
                    identity,
                  )
                }
              }
            }
            StoreCreditAccountResolutionError(error) -> {
              let payload =
                store_credit_payload_json(
                  store,
                  typename,
                  None,
                  [error],
                  field,
                  fragments,
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  root,
                ),
                store,
                identity,
              )
            }
          }
        errors -> {
          let payload =
            store_credit_payload_json(
              store,
              typename,
              None,
              errors,
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              root,
            ),
            store,
            identity,
          )
        }
      }
    }
    _, _ -> {
      let payload =
        store_credit_payload_json(
          store,
          typename,
          None,
          [
            UserError(
              [input_name, "amount"],
              "Amount is invalid",
              Some("INVALID"),
            ),
          ],
          field,
          fragments,
        )
      #(
        MutationFieldResult(get_field_response_key(field), payload, [], root),
        store,
        identity,
      )
    }
  }
}

@internal
pub fn store_credit_adjustment_input_errors(
  input: Dict(String, root_field.ResolvedValue),
  input_name: String,
  amount_key: String,
  money: Money,
  is_credit: Bool,
) -> List(UserError) {
  let amount_cents = parse_cents(money.amount)
  let amount_errors = case amount_cents <= 0 {
    True -> [
      UserError(
        [input_name, amount_key, "amount"],
        "A positive amount must be used to credit a store credit account",
        Some("NEGATIVE_OR_ZERO_AMOUNT"),
      ),
    ]
    False -> []
  }
  let currency_errors = case
    is_supported_store_credit_currency(money.currency_code)
  {
    True -> []
    False -> [
      UserError(
        [input_name, amount_key, "currencyCode"],
        "Currency is not supported",
        Some("UNSUPPORTED_CURRENCY"),
      ),
    ]
  }
  let expiry_errors = case is_credit, read_obj_string(input, "expiresAt") {
    True, Some(expires_at) ->
      case store_credit_expires_at_in_past(expires_at) {
        True -> [
          UserError(
            [input_name, "expiresAt"],
            "The expiry date must be in the future",
            Some("EXPIRES_AT_IN_PAST"),
          ),
        ]
        False -> []
      }
    _, _ -> []
  }
  amount_errors
  |> list.append(currency_errors)
  |> list.append(expiry_errors)
}

@internal
pub fn resolve_store_credit_account(
  store: Store,
  identity: SyntheticIdentityRegistry,
  id: String,
  money: Money,
  is_credit: Bool,
) -> StoreCreditAccountResolution {
  case store_credit_id_kind(id) {
    "account" ->
      case store.get_effective_store_credit_account_by_id(store, id) {
        Some(account) -> StoreCreditAccountResolved(account, identity)
        None ->
          StoreCreditAccountResolutionError(store_credit_account_not_found())
      }
    "customer" ->
      case store.get_effective_customer_by_id(store, id) {
        Some(_) ->
          resolve_store_credit_owner_account(
            store,
            identity,
            id,
            money,
            is_credit,
          )
        None ->
          StoreCreditAccountResolutionError(store_credit_owner_not_found())
      }
    "company_location" ->
      case store.get_effective_b2b_company_location_by_id(store, id) {
        Some(_) ->
          resolve_store_credit_owner_account(
            store,
            identity,
            id,
            money,
            is_credit,
          )
        None ->
          StoreCreditAccountResolutionError(store_credit_owner_not_found())
      }
    _ -> StoreCreditAccountResolutionError(store_credit_account_not_found())
  }
}

@internal
pub fn resolve_store_credit_owner_account(
  store: Store,
  identity: SyntheticIdentityRegistry,
  owner_id: String,
  money: Money,
  is_credit: Bool,
) -> StoreCreditAccountResolution {
  case
    store.get_effective_store_credit_account_by_owner_id_and_currency(
      store,
      owner_id,
      money.currency_code,
    )
  {
    Some(account) -> StoreCreditAccountResolved(account, identity)
    None ->
      case is_credit {
        True -> {
          let #(account_id, after_account_id) =
            synthetic_identity.make_synthetic_gid(
              identity,
              "StoreCreditAccount",
            )
          StoreCreditAccountResolved(
            StoreCreditAccountRecord(
              id: account_id,
              customer_id: owner_id,
              cursor: None,
              balance: Money(amount: "0.0", currency_code: money.currency_code),
            ),
            after_account_id,
          )
        }
        False ->
          StoreCreditAccountResolutionError(store_credit_account_not_found())
      }
  }
}

@internal
pub fn store_credit_id_kind(id: String) -> String {
  case string.starts_with(id, "gid://shopify/StoreCreditAccount/") {
    True -> "account"
    False ->
      case string.starts_with(id, "gid://shopify/Customer/") {
        True -> "customer"
        False ->
          case string.starts_with(id, "gid://shopify/CompanyLocation/") {
            True -> "company_location"
            False -> "unknown"
          }
      }
  }
}

@internal
pub fn store_credit_account_not_found() -> UserError {
  UserError(
    ["id"],
    "Store credit account does not exist",
    Some("ACCOUNT_NOT_FOUND"),
  )
}

@internal
pub fn store_credit_owner_not_found() -> UserError {
  UserError(["id"], "Owner does not exist", Some("OWNER_NOT_FOUND"))
}

@internal
pub fn is_supported_store_credit_currency(currency_code: String) -> Bool {
  currency_code != "" && currency_code != "XXX" && currency_code != "XTS"
}

@internal
pub fn store_credit_expires_at_in_past(expires_at: String) -> Bool {
  case iso_timestamp.parse_iso(expires_at) {
    Ok(expires_ms) ->
      case iso_timestamp.parse_iso(iso_timestamp.now_iso()) {
        Ok(now_ms) -> expires_ms < now_ms
        Error(_) -> False
      }
    Error(_) -> False
  }
}

@internal
pub fn handle_order_customer_set(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let order_id = graphql_helpers.read_arg_string_nonempty(args, "orderId")
  let customer_id = graphql_helpers.read_arg_string_nonempty(args, "customerId")
  case order_id, customer_id {
    Some(order_id), Some(customer_id) ->
      case
        store.get_effective_customer_order_summary_by_id(store, order_id),
        store.get_effective_customer_by_id(store, customer_id)
      {
        Some(order), Some(customer) -> {
          let linked =
            CustomerOrderSummaryRecord(..order, customer_id: Some(customer.id))
          let next_store = store.stage_customer_order_summary(store, linked)
          let payload =
            order_customer_payload_json(
              next_store,
              "OrderCustomerSetPayload",
              Some(linked),
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [order_id],
              "orderCustomerSet",
            ),
            next_store,
            identity,
          )
        }
        _, _ -> {
          let payload =
            order_customer_payload_json(
              store,
              "OrderCustomerSetPayload",
              None,
              [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "orderCustomerSet",
            ),
            store,
            identity,
          )
        }
      }
    _, _ -> {
      let payload =
        order_customer_payload_json(
          store,
          "OrderCustomerSetPayload",
          None,
          [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "orderCustomerSet",
        ),
        store,
        identity,
      )
    }
  }
}

@internal
pub fn handle_order_customer_remove(
  store,
  identity,
  field,
  fragments,
  variables,
) {
  let args = graphql_helpers.field_args(field, variables)
  case graphql_helpers.read_arg_string_nonempty(args, "orderId") {
    Some(order_id) ->
      case store.get_effective_customer_order_summary_by_id(store, order_id) {
        Some(order) -> {
          let unlinked = CustomerOrderSummaryRecord(..order, customer_id: None)
          let next_store = store.stage_customer_order_summary(store, unlinked)
          let payload =
            order_customer_payload_json(
              next_store,
              "OrderCustomerRemovePayload",
              Some(unlinked),
              [],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [order_id],
              "orderCustomerRemove",
            ),
            next_store,
            identity,
          )
        }
        None -> {
          let payload =
            order_customer_payload_json(
              store,
              "OrderCustomerRemovePayload",
              None,
              [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
              field,
              fragments,
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "orderCustomerRemove",
            ),
            store,
            identity,
          )
        }
      }
    None -> {
      let payload =
        order_customer_payload_json(
          store,
          "OrderCustomerRemovePayload",
          None,
          [UserError(["orderId"], "Order does not exist", Some("INVALID"))],
          field,
          fragments,
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "orderCustomerRemove",
        ),
        store,
        identity,
      )
    }
  }
}

@internal
pub fn handle_customer_merge(store, identity, field, fragments, variables) {
  let args = graphql_helpers.field_args(field, variables)
  let one = graphql_helpers.read_arg_string_nonempty(args, "customerOneId")
  let two = graphql_helpers.read_arg_string_nonempty(args, "customerTwoId")
  let override =
    graphql_helpers.read_arg_object(args, "overrideFields")
    |> option.unwrap(dict.new())
  case one, two {
    Some(one_id), Some(two_id) ->
      case one_id == two_id {
        True -> {
          let payload =
            merge_error_payload(
              field,
              fragments,
              [],
              "Customers IDs should not match",
              Some("INVALID_CUSTOMER_ID"),
            )
          #(
            MutationFieldResult(
              get_field_response_key(field),
              payload,
              [],
              "customerMerge",
            ),
            store,
            identity,
          )
        }
        False ->
          case
            store.get_effective_customer_by_id(store, one_id),
            store.get_effective_customer_by_id(store, two_id)
          {
            Some(c1), Some(c2) -> {
              let #(job_id, after_id) =
                synthetic_identity.make_synthetic_gid(identity, "Job")
              let #(timestamp, after_ts) =
                synthetic_identity.make_synthetic_timestamp(after_id)
              let merged = build_merged_customer(c1, c2, override, timestamp)
              let request =
                CustomerMergeRequestRecord(
                  job_id: job_id,
                  resulting_customer_id: merged.id,
                  status: "COMPLETED",
                  customer_merge_errors: [],
                )
              let payload_request =
                CustomerMergeRequestRecord(
                  job_id: job_id,
                  resulting_customer_id: merged.id,
                  status: "IN_PROGRESS",
                  customer_merge_errors: [],
                )
              let source_addresses =
                store.list_effective_customer_addresses(store, c1.id)
              let next_store =
                store.stage_merge_customers(store, c1.id, merged, request)
                |> stage_customer_merge_attached_resources(
                  c1,
                  c2,
                  merged,
                  source_addresses,
                )
              let payload =
                merge_payload_json(payload_request, field, fragments)
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [c1.id, c2.id, job_id],
                  "customerMerge",
                ),
                next_store,
                after_ts,
              )
            }
            None, _ -> {
              let payload =
                merge_error_payload(
                  field,
                  fragments,
                  ["customerOneId"],
                  "Customer does not exist with ID "
                    <> option.unwrap(gid_tail(one_id), one_id),
                  Some("INVALID_CUSTOMER_ID"),
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerMerge",
                ),
                store,
                identity,
              )
            }
            _, None -> {
              let payload =
                merge_error_payload(
                  field,
                  fragments,
                  ["customerTwoId"],
                  "Customer does not exist with ID "
                    <> option.unwrap(gid_tail(two_id), two_id),
                  Some("INVALID_CUSTOMER_ID"),
                )
              #(
                MutationFieldResult(
                  get_field_response_key(field),
                  payload,
                  [],
                  "customerMerge",
                ),
                store,
                identity,
              )
            }
          }
      }
    _, _ -> {
      let payload =
        merge_error_payload(
          field,
          fragments,
          ["customerId"],
          "Required argument missing",
          Some("CUSTOMER_DOES_NOT_EXIST"),
        )
      #(
        MutationFieldResult(
          get_field_response_key(field),
          payload,
          [],
          "customerMerge",
        ),
        store,
        identity,
      )
    }
  }
}

@internal
pub fn stage_customer_merge_attached_resources(
  store: Store,
  source: CustomerRecord,
  result: CustomerRecord,
  merged: CustomerRecord,
  source_addresses: List(CustomerAddressRecord),
) -> Store {
  let with_source_addresses =
    source_addresses
    |> list.index_map(fn(address, index) {
      CustomerAddressRecord(
        ..address,
        customer_id: merged.id,
        position: -1000 + index,
      )
    })
    |> list.fold(store, fn(acc, address) {
      let #(_, next_store) = store.stage_upsert_customer_address(acc, address)
      next_store
    })
  let result_metafields =
    store.get_effective_metafields_by_customer_id(
      with_source_addresses,
      result.id,
    )
  let result_keys =
    result_metafields
    |> list.map(customer_metafield_key)
  let copied_source_metafields =
    store.get_effective_metafields_by_customer_id(
      with_source_addresses,
      source.id,
    )
    |> list.filter(fn(metafield) {
      !list.contains(result_keys, customer_metafield_key(metafield))
    })
    |> list.index_map(fn(metafield, index) {
      CustomerMetafieldRecord(
        ..metafield,
        id: "gid://shopify/Metafield/900000000000" <> int.to_string(index),
        customer_id: merged.id,
      )
    })
  let with_source_orders =
    store.list_effective_customer_order_summaries(
      with_source_addresses,
      source.id,
    )
    |> list.fold(with_source_addresses, fn(acc, order) {
      store.stage_customer_order_summary(
        acc,
        CustomerOrderSummaryRecord(
          ..order,
          customer_id: Some(merged.id),
          email: merged.email |> option.or(order.email),
        ),
      )
    })
  case copied_source_metafields {
    [] -> with_source_orders
    [_, ..] ->
      store.stage_customer_metafields(
        with_source_orders,
        merged.id,
        list.append(result_metafields, copied_source_metafields),
      )
  }
}
