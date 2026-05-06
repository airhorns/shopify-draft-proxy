//// Customer domain internals split from proxy/customers.gleam.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/customers/account_mutations.{
  handle_account_invite, handle_activation_url, handle_customer_merge,
  handle_data_erasure, handle_email_consent, handle_order_customer_remove,
  handle_order_customer_set, handle_payment_method_update_email,
  handle_sms_consent, handle_store_credit_adjustment,
}
import shopify_draft_proxy/proxy/customers/customer_mutations.{
  handle_customer_address_create, handle_customer_address_delete,
  handle_customer_address_update, handle_customer_create, handle_customer_delete,
  handle_customer_set, handle_customer_tax_exemptions, handle_customer_update,
  handle_customer_update_default_address,
}
import shopify_draft_proxy/proxy/customers/customer_types.{
  type MutationFieldResult,
}
import shopify_draft_proxy/proxy/customers/hydration.{
  hydrate_before_customer_mutation,
}
import shopify_draft_proxy/proxy/customers/inputs.{
  read_nested_object, read_obj_string, record_mutation_log,
}
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, get_document_fragments,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, MutationOutcome,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}

@internal
pub fn is_customer_mutation_root(name: String) -> Bool {
  case name {
    "customerCreate"
    | "customerUpdate"
    | "customerDelete"
    | "customerSet"
    | "customerAddressCreate"
    | "customerAddressUpdate"
    | "customerAddressDelete"
    | "customerUpdateDefaultAddress"
    | "customerEmailMarketingConsentUpdate"
    | "customerSmsMarketingConsentUpdate"
    | "customerAddTaxExemptions"
    | "customerRemoveTaxExemptions"
    | "customerReplaceTaxExemptions"
    | "customerMerge"
    | "customerRequestDataErasure"
    | "customerCancelDataErasure"
    | "customerGenerateAccountActivationUrl"
    | "customerSendAccountInviteEmail"
    | "customerPaymentMethodSendUpdateEmail"
    | "storeCreditAccountCredit"
    | "storeCreditAccountDebit"
    | "orderCustomerSet"
    | "orderCustomerRemove" -> True
    _ -> False
  }
}

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
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
        request_path,
        document,
        fields,
        fragments,
        variables,
        upstream,
      )
    }
  }
}

@internal
pub fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  case
    first_disallowed_marketing_consent_state_error(fields, variables, document)
  {
    Some(#(root_name, error_json, include_null_data)) -> {
      let entries = case include_null_data {
        True -> [
          #("errors", json.array([error_json], fn(x) { x })),
          #("data", json.object([#(root_name, json.null())])),
        ]
        False -> [#("errors", json.array([error_json], fn(x) { x }))]
      }
      MutationOutcome(
        data: json.object(entries),
        store: store,
        identity: identity,
        staged_resource_ids: [],
        log_drafts: [],
      )
    }
    None ->
      case first_customer_merge_missing_argument_error(fields, variables) {
        Some(error_json) ->
          MutationOutcome(
            data: json.object([
              #("errors", json.array([error_json], fn(x) { x })),
            ]),
            store: store,
            identity: identity,
            staged_resource_ids: [],
            log_drafts: [],
          )
        None ->
          case first_invalid_tax_exemption_error(fields, variables) {
            Some(error_json) ->
              MutationOutcome(
                data: json.object([
                  #("errors", json.array([error_json], fn(x) { x })),
                ]),
                store: store,
                identity: identity,
                staged_resource_ids: [],
                log_drafts: [],
              )
            None ->
              handle_validated_mutation_fields(
                store,
                identity,
                request_path,
                document,
                fields,
                fragments,
                variables,
                upstream,
              )
          }
      }
  }
}

@internal
pub fn first_disallowed_marketing_consent_state_error(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
) -> Option(#(String, Json, Bool)) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case field {
        Field(name: name, ..) as selected_field ->
          case name.value {
            "customerCreate" ->
              case
                customer_create_marketing_consent_state_error(
                  selected_field,
                  variables,
                  document,
                )
              {
                Some(error) -> Some(#("customerCreate", error, True))
                None ->
                  first_disallowed_marketing_consent_state_error(
                    rest,
                    variables,
                    document,
                  )
              }
            _ ->
              case marketing_consent_input_key(name.value) {
                Some(consent_key) -> {
                  let args =
                    graphql_helpers.field_args(selected_field, variables)
                  let input =
                    graphql_helpers.read_arg_object(args, "input")
                    |> option.unwrap(dict.new())
                  let consent = read_nested_object(input, consent_key)
                  case read_obj_string(consent, "marketingState") {
                    Some(value) ->
                      case is_allowed_marketing_consent_update_state(value) {
                        True ->
                          first_disallowed_marketing_consent_state_error(
                            rest,
                            variables,
                            document,
                          )
                        False -> {
                          let #(error_json, include_null_data) =
                            marketing_consent_state_error(
                              name.value,
                              consent_key,
                              value,
                              input,
                              selected_field,
                              document,
                            )
                          Some(#(name.value, error_json, include_null_data))
                        }
                      }
                    None ->
                      first_disallowed_marketing_consent_state_error(
                        rest,
                        variables,
                        document,
                      )
                  }
                }
                None ->
                  first_disallowed_marketing_consent_state_error(
                    rest,
                    variables,
                    document,
                  )
              }
          }
        _ ->
          first_disallowed_marketing_consent_state_error(
            rest,
            variables,
            document,
          )
      }
  }
}

@internal
pub fn customer_create_marketing_consent_state_error(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  document: String,
) -> Option(Json) {
  let args = graphql_helpers.field_args(field, variables)
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case
    customer_create_disallowed_consent_state(input, "emailMarketingConsent")
  {
    Some(#(consent_key, state)) ->
      Some(marketing_consent_resolver_error(
        "customerCreate",
        consent_key,
        state,
        field,
        document,
      ))
    None ->
      case
        customer_create_disallowed_consent_state(input, "smsMarketingConsent")
      {
        Some(#(consent_key, state)) ->
          Some(marketing_consent_resolver_error(
            "customerCreate",
            consent_key,
            state,
            field,
            document,
          ))
        None -> None
      }
  }
}

@internal
pub fn customer_create_disallowed_consent_state(
  input: Dict(String, root_field.ResolvedValue),
  consent_key: String,
) -> Option(#(String, String)) {
  let consent = read_nested_object(input, consent_key)
  case read_obj_string(consent, "marketingState") {
    Some(state) ->
      case is_allowed_marketing_consent_create_state(state) {
        True -> None
        False -> Some(#(consent_key, state))
      }
    None -> None
  }
}

@internal
pub fn marketing_consent_resolver_error(
  root_name: String,
  _consent_key: String,
  state: String,
  field: Selection,
  document: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string("Cannot specify " <> state <> " as a marketing state input"),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #("extensions", json.object([#("code", json.string("INVALID"))])),
    #("path", json.array([json.string(root_name)], fn(x) { x })),
  ])
}

@internal
pub fn is_allowed_marketing_consent_create_state(state: String) -> Bool {
  case state {
    "SUBSCRIBED" | "UNSUBSCRIBED" | "PENDING" | "NOT_SUBSCRIBED" -> True
    _ -> False
  }
}

@internal
pub fn marketing_consent_input_key(root_name: String) -> Option(String) {
  case root_name {
    "customerEmailMarketingConsentUpdate" -> Some("emailMarketingConsent")
    "customerSmsMarketingConsentUpdate" -> Some("smsMarketingConsent")
    _ -> None
  }
}

@internal
pub fn is_allowed_marketing_consent_update_state(state: String) -> Bool {
  case state {
    "SUBSCRIBED" | "UNSUBSCRIBED" | "PENDING" -> True
    _ -> False
  }
}

@internal
pub fn marketing_consent_state_error(
  root_name: String,
  consent_key: String,
  state: String,
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> #(Json, Bool) {
  case root_name, state {
    "customerSmsMarketingConsentUpdate", "INVALID" -> #(
      invalid_sms_marketing_state_variable_error(
        consent_key,
        state,
        input,
        field,
        document,
      ),
      False,
    )
    _, _ -> #(
      marketing_consent_resolver_error(
        root_name,
        consent_key,
        state,
        field,
        document,
      ),
      True,
    )
  }
}

@internal
pub fn invalid_sms_marketing_state_variable_error(
  consent_key: String,
  state: String,
  input: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
) -> Json {
  let input_type = "CustomerSmsMarketingConsentUpdateInput"
  let allowed_values =
    "NOT_SUBSCRIBED, PENDING, SUBSCRIBED, UNSUBSCRIBED, REDACTED"
  let explanation =
    "Expected \"" <> state <> "\" to be one of: " <> allowed_values
  json.object([
    #(
      "message",
      json.string(
        "Variable $input of type "
        <> input_type
        <> "! was provided invalid value for "
        <> consent_key
        <> ".marketingState ("
        <> explanation
        <> ")",
      ),
    ),
    #("locations", graphql_helpers.field_locations_json(field, document)),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #(
          "value",
          root_field.resolved_value_to_json(root_field.ObjectVal(input)),
        ),
        #(
          "problems",
          json.array(
            [
              json.object([
                #(
                  "path",
                  json.array(
                    [json.string(consent_key), json.string("marketingState")],
                    fn(x) { x },
                  ),
                ),
                #("explanation", json.string(explanation)),
              ]),
            ],
            fn(x) { x },
          ),
        ),
      ]),
    ),
  ])
}

@internal
pub fn first_customer_merge_missing_argument_error(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Json) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case field {
        Field(name: name, ..) ->
          case name.value == "customerMerge" {
            True -> {
              let args = graphql_helpers.field_args(field, variables)
              let missing =
                [
                  #(
                    "customerOneId",
                    graphql_helpers.read_arg_string_nonempty(
                      args,
                      "customerOneId",
                    ),
                  ),
                  #(
                    "customerTwoId",
                    graphql_helpers.read_arg_string_nonempty(
                      args,
                      "customerTwoId",
                    ),
                  ),
                ]
                |> list.filter_map(fn(pair) {
                  case pair {
                    #(argument, None) -> Ok(argument)
                    #(_, Some(_)) -> Error(Nil)
                  }
                })
              case missing {
                [] ->
                  first_customer_merge_missing_argument_error(rest, variables)
                [_, ..] ->
                  Some(customer_merge_missing_argument_error(
                    name.value,
                    missing,
                  ))
              }
            }
            False ->
              first_customer_merge_missing_argument_error(rest, variables)
          }
        _ -> first_customer_merge_missing_argument_error(rest, variables)
      }
  }
}

@internal
pub fn customer_merge_missing_argument_error(
  field_name: String,
  missing_arguments: List(String),
) -> Json {
  let arguments = string.join(missing_arguments, ", ")
  json.object([
    #(
      "message",
      json.string(
        "Field '"
        <> field_name
        <> "' is missing required arguments: "
        <> arguments,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("missingRequiredArguments")),
        #("className", json.string("Field")),
        #("name", json.string(field_name)),
        #("arguments", json.string(arguments)),
      ]),
    ),
  ])
}

@internal
pub fn handle_validated_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(entries, final_store, final_identity, staged_ids, roots) =
    list.fold(fields, initial, fn(acc, field) {
      let #(current_entries, current_store, current_identity, ids, root_names) =
        acc
      case field {
        Field(name: name, ..) -> {
          let handled =
            handle_mutation_field(
              current_store,
              current_identity,
              field,
              name.value,
              fragments,
              variables,
              upstream,
            )
          case handled {
            Some(#(result, next_store, next_identity)) -> #(
              list.append(current_entries, [#(result.key, result.payload)]),
              next_store,
              next_identity,
              list.append(ids, result.staged_resource_ids),
              list.append(root_names, [result.root_name]),
            )
            None -> acc
          }
        }
        _ -> acc
      }
    })
  let #(logged_store, logged_identity) = case staged_ids {
    [] -> #(final_store, final_identity)
    _ ->
      record_mutation_log(
        final_store,
        final_identity,
        request_path,
        document,
        staged_ids,
        roots,
      )
  }
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: logged_store,
    identity: logged_identity,
    staged_resource_ids: staged_ids,
    log_drafts: [],
  )
}

@internal
pub fn first_invalid_tax_exemption_error(
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
) -> Option(Json) {
  case fields {
    [] -> None
    [field, ..rest] ->
      case field {
        Field(name: name, ..) ->
          case is_tax_exemption_root(name.value) {
            True -> {
              let args = graphql_helpers.field_args(field, variables)
              case invalid_tax_exemption_value(args) {
                Some(value) ->
                  Some(invalid_tax_exemption_variable_error(value, 0, args))
                None -> first_invalid_tax_exemption_error(rest, variables)
              }
            }
            False -> first_invalid_tax_exemption_error(rest, variables)
          }
        _ -> first_invalid_tax_exemption_error(rest, variables)
      }
  }
}

@internal
pub fn is_tax_exemption_root(name: String) -> Bool {
  name == "customerAddTaxExemptions"
  || name == "customerRemoveTaxExemptions"
  || name == "customerReplaceTaxExemptions"
}

@internal
pub fn invalid_tax_exemption_value(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(args, "taxExemptions") {
    Ok(root_field.ListVal(items)) -> first_invalid_tax_exemption(items)
    _ -> None
  }
}

@internal
pub fn first_invalid_tax_exemption(
  items: List(root_field.ResolvedValue),
) -> Option(String) {
  case items {
    [] -> None
    [item, ..rest] ->
      case item {
        root_field.StringVal(value) ->
          case is_valid_tax_exemption(value) {
            True -> first_invalid_tax_exemption(rest)
            False -> Some(value)
          }
        _ -> Some("")
      }
  }
}

@internal
pub fn is_valid_tax_exemption(value: String) -> Bool {
  list.contains(valid_tax_exemptions(), value)
}

@internal
pub fn valid_tax_exemptions() -> List(String) {
  [
    "CA_STATUS_CARD_EXEMPTION",
    "CA_BC_RESELLER_EXEMPTION",
    "CA_MB_RESELLER_EXEMPTION",
    "CA_SK_RESELLER_EXEMPTION",
    "CA_DIPLOMAT_EXEMPTION",
    "CA_BC_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_MB_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_NS_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_PE_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_SK_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_BC_SUB_CONTRACTOR_EXEMPTION",
    "CA_SK_SUB_CONTRACTOR_EXEMPTION",
    "CA_BC_CONTRACTOR_EXEMPTION",
    "CA_SK_CONTRACTOR_EXEMPTION",
    "CA_ON_PURCHASE_EXEMPTION",
    "CA_MB_FARMER_EXEMPTION",
    "CA_NS_FARMER_EXEMPTION",
    "CA_SK_FARMER_EXEMPTION",
    "EU_REVERSE_CHARGE_EXEMPTION_RULE",
    "US_AL_RESELLER_EXEMPTION",
    "US_AK_RESELLER_EXEMPTION",
    "US_AZ_RESELLER_EXEMPTION",
    "US_AR_RESELLER_EXEMPTION",
    "US_CA_RESELLER_EXEMPTION",
    "US_CO_RESELLER_EXEMPTION",
    "US_CT_RESELLER_EXEMPTION",
    "US_DE_RESELLER_EXEMPTION",
    "US_FL_RESELLER_EXEMPTION",
    "US_GA_RESELLER_EXEMPTION",
    "US_HI_RESELLER_EXEMPTION",
    "US_ID_RESELLER_EXEMPTION",
    "US_IL_RESELLER_EXEMPTION",
    "US_IN_RESELLER_EXEMPTION",
    "US_IA_RESELLER_EXEMPTION",
    "US_KS_RESELLER_EXEMPTION",
    "US_KY_RESELLER_EXEMPTION",
    "US_LA_RESELLER_EXEMPTION",
    "US_ME_RESELLER_EXEMPTION",
    "US_MD_RESELLER_EXEMPTION",
    "US_MA_RESELLER_EXEMPTION",
    "US_MI_RESELLER_EXEMPTION",
    "US_MN_RESELLER_EXEMPTION",
    "US_MS_RESELLER_EXEMPTION",
    "US_MO_RESELLER_EXEMPTION",
    "US_MT_RESELLER_EXEMPTION",
    "US_NE_RESELLER_EXEMPTION",
    "US_NV_RESELLER_EXEMPTION",
    "US_NH_RESELLER_EXEMPTION",
    "US_NJ_RESELLER_EXEMPTION",
    "US_NM_RESELLER_EXEMPTION",
    "US_NY_RESELLER_EXEMPTION",
    "US_NC_RESELLER_EXEMPTION",
    "US_ND_RESELLER_EXEMPTION",
    "US_OH_RESELLER_EXEMPTION",
    "US_OK_RESELLER_EXEMPTION",
    "US_OR_RESELLER_EXEMPTION",
    "US_PA_RESELLER_EXEMPTION",
    "US_RI_RESELLER_EXEMPTION",
    "US_SC_RESELLER_EXEMPTION",
    "US_SD_RESELLER_EXEMPTION",
    "US_TN_RESELLER_EXEMPTION",
    "US_TX_RESELLER_EXEMPTION",
    "US_UT_RESELLER_EXEMPTION",
    "US_VT_RESELLER_EXEMPTION",
    "US_VA_RESELLER_EXEMPTION",
    "US_WA_RESELLER_EXEMPTION",
    "US_WV_RESELLER_EXEMPTION",
    "US_WI_RESELLER_EXEMPTION",
    "US_WY_RESELLER_EXEMPTION",
    "US_DC_RESELLER_EXEMPTION",
  ]
}

@internal
pub fn invalid_tax_exemption_variable_error(
  value: String,
  index: Int,
  args: Dict(String, root_field.ResolvedValue),
) -> Json {
  let expected_values = string.join(valid_tax_exemptions(), ", ")
  let explanation =
    "Expected \"" <> value <> "\" to be one of: " <> expected_values
  let message =
    "Variable $taxExemptions of type [TaxExemption!]! was provided invalid value for "
    <> int.to_string(index)
    <> " ("
    <> explanation
    <> ")"
  json.object([
    #("message", json.string(message)),
    #(
      "extensions",
      json.object([
        #("code", json.string("INVALID_VARIABLE")),
        #("value", tax_exemptions_value_json(args)),
        #(
          "problems",
          json.array(
            [
              json.object([
                #("path", json.array([json.int(index)], fn(x) { x })),
                #("explanation", json.string(explanation)),
              ]),
            ],
            fn(x) { x },
          ),
        ),
      ]),
    ),
  ])
}

@internal
pub fn tax_exemptions_value_json(
  args: Dict(String, root_field.ResolvedValue),
) -> Json {
  case dict.get(args, "taxExemptions") {
    Ok(root_field.ListVal(items)) ->
      json.array(items, fn(item) {
        case item {
          root_field.StringVal(s) -> json.string(s)
          root_field.IntVal(i) -> json.int(i)
          root_field.BoolVal(b) -> json.bool(b)
          _ -> json.null()
        }
      })
    _ -> json.null()
  }
}

@internal
pub fn handle_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  root_name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Option(#(MutationFieldResult, Store, SyntheticIdentityRegistry)) {
  let #(store, identity) =
    hydrate_before_customer_mutation(
      store,
      identity,
      field,
      root_name,
      variables,
      upstream,
    )
  case root_name {
    "customerCreate" ->
      Some(handle_customer_create(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      ))
    "customerUpdate" ->
      Some(handle_customer_update(
        store,
        identity,
        field,
        fragments,
        variables,
        upstream,
      ))
    "customerSet" ->
      Some(handle_customer_set(store, identity, field, fragments, variables))
    "customerDelete" ->
      Some(handle_customer_delete(store, identity, field, fragments, variables))
    "customerAddressCreate" ->
      Some(handle_customer_address_create(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerAddressUpdate" ->
      Some(handle_customer_address_update(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerAddressDelete" ->
      Some(handle_customer_address_delete(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerUpdateDefaultAddress" ->
      Some(handle_customer_update_default_address(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerAddTaxExemptions" ->
      Some(handle_customer_tax_exemptions(
        store,
        identity,
        field,
        fragments,
        variables,
        "add",
      ))
    "customerRemoveTaxExemptions" ->
      Some(handle_customer_tax_exemptions(
        store,
        identity,
        field,
        fragments,
        variables,
        "remove",
      ))
    "customerReplaceTaxExemptions" ->
      Some(handle_customer_tax_exemptions(
        store,
        identity,
        field,
        fragments,
        variables,
        "replace",
      ))
    "customerEmailMarketingConsentUpdate" ->
      Some(handle_email_consent(store, identity, field, fragments, variables))
    "customerSmsMarketingConsentUpdate" ->
      Some(handle_sms_consent(store, identity, field, fragments, variables))
    "customerRequestDataErasure" ->
      Some(handle_data_erasure(store, identity, field, variables, False))
    "customerCancelDataErasure" ->
      Some(handle_data_erasure(store, identity, field, variables, True))
    "customerGenerateAccountActivationUrl" ->
      Some(handle_activation_url(store, identity, field, variables))
    "customerSendAccountInviteEmail" ->
      Some(handle_account_invite(store, identity, field, fragments, variables))
    "customerPaymentMethodSendUpdateEmail" ->
      Some(handle_payment_method_update_email(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "storeCreditAccountCredit" ->
      Some(handle_store_credit_adjustment(
        store,
        identity,
        field,
        fragments,
        variables,
        True,
      ))
    "storeCreditAccountDebit" ->
      Some(handle_store_credit_adjustment(
        store,
        identity,
        field,
        fragments,
        variables,
        False,
      ))
    "orderCustomerSet" ->
      Some(handle_order_customer_set(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "orderCustomerRemove" ->
      Some(handle_order_customer_remove(
        store,
        identity,
        field,
        fragments,
        variables,
      ))
    "customerMerge" ->
      Some(handle_customer_merge(store, identity, field, fragments, variables))
    _ -> None
  }
}
