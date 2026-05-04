//// Privacy domain port.
////
//// `dataSaleOptOut` is privacy-scoped in the Admin API, but its observable
//// downstream effect lives on `Customer.dataSaleOptOut`. Keep the root under
//// privacy dispatch while staging the read effect against customer state.

import gleam/dict.{type Dict}
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type SourceValue, SrcList, SrcNull, SrcString, get_field_response_key,
  project_graphql_value, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, single_root_log_draft,
}
import shopify_draft_proxy/proxy/upstream_query.{
  type UpstreamContext, empty_upstream_context,
}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type CustomerRecord, CustomerDefaultEmailAddressRecord, CustomerRecord, Money,
}

pub type PrivacyError {
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
  UserError(field: List(String), message: String, code: Option(String))
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
  )
}

pub fn is_privacy_mutation_root(name: String) -> Bool {
  case name {
    "dataSaleOptOut" -> True
    _ -> False
  }
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, PrivacyError) {
  process_mutation_with_upstream(
    store,
    identity,
    request_path,
    document,
    variables,
    empty_upstream_context(),
  )
}

pub fn process_mutation_with_upstream(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> Result(MutationOutcome, PrivacyError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) ->
      Ok(handle_mutation_fields(store, identity, fields, variables, upstream))
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  fields: List(Selection),
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  let initial = #([], store, identity, [], [])
  let #(entries, final_store, final_identity, staged_ids, log_drafts) =
    list.fold(fields, initial, fn(acc, field) {
      let #(current_entries, current_store, current_identity, ids, drafts) = acc
      case field {
        Field(name: name, ..) if name.value == "dataSaleOptOut" -> {
          let #(result, next_store, next_identity) =
            handle_data_sale_opt_out(
              current_store,
              current_identity,
              field,
              variables,
              upstream,
            )
          let next_drafts = case result.staged_resource_ids {
            [] -> drafts
            _ ->
              list.append(drafts, [
                single_root_log_draft(
                  "dataSaleOptOut",
                  result.staged_resource_ids,
                  store.Staged,
                  "privacy",
                  "stage-locally",
                  Some(
                    "Locally staged privacy-domain data sale opt-out mutation in shopify-draft-proxy.",
                  ),
                ),
              ])
          }
          #(
            list.append(current_entries, [#(result.key, result.payload)]),
            next_store,
            next_identity,
            list.append(ids, result.staged_resource_ids),
            next_drafts,
          )
        }
        _ -> acc
      }
    })
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: log_drafts,
  )
}

fn handle_data_sale_opt_out(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let email =
    graphql_helpers.field_args(field, variables)
    |> read_arg_string("email")
    |> option_map(string.trim)
  case email {
    Some(value) -> {
      case is_valid_data_sale_email(value) {
        True ->
          case
            find_customer_by_email(store.list_effective_customers(store), value)
          {
            Some(customer) ->
              opt_out_existing_customer(store, identity, field, customer)
            None -> {
              // Pattern 2: in LiveHybrid, read the existing upstream
              // customer by email so the supported mutation still stages
              // locally but uses Shopify's authoritative customer id.
              // Snapshot/default no-transport execution falls back to a
              // local synthetic customer when no staged customer exists.
              case fetch_upstream_customer_by_email(value, upstream) {
                Some(customer) ->
                  opt_out_existing_customer(store, identity, field, customer)
                None -> opt_out_new_customer(store, identity, field, value)
              }
            }
          }
        False -> failed_data_sale_opt_out(store, identity, field)
      }
    }
    _ -> failed_data_sale_opt_out(store, identity, field)
  }
}

fn failed_data_sale_opt_out(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let payload =
    data_sale_payload(field, None, [
      UserError([], "Data sale opt out failed.", Some("FAILED")),
    ])
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: payload,
      staged_resource_ids: [],
    ),
    store,
    identity,
  )
}

fn opt_out_existing_customer(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  customer: CustomerRecord,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(updated_at, next_identity) = case customer.data_sale_opt_out {
    True -> #(customer.updated_at, identity)
    False -> {
      let #(timestamp, after_ts) =
        synthetic_identity.make_synthetic_timestamp(identity)
      #(Some(timestamp), after_ts)
    }
  }
  let updated =
    CustomerRecord(..customer, data_sale_opt_out: True, updated_at: updated_at)
  let #(_, next_store) = store.stage_update_customer(store, updated)
  let payload = data_sale_payload(field, Some(updated.id), [])
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: payload,
      staged_resource_ids: [updated.id],
    ),
    next_store,
    next_identity,
  )
}

fn opt_out_new_customer(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  email: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(id, after_id) =
    synthetic_identity.make_synthetic_gid(identity, "Customer")
  let #(timestamp, after_ts) =
    synthetic_identity.make_synthetic_timestamp(after_id)
  let customer =
    CustomerRecord(
      id: id,
      first_name: None,
      last_name: None,
      display_name: Some(email),
      email: Some(email),
      legacy_resource_id: gid_tail(id),
      locale: None,
      note: None,
      can_delete: Some(True),
      verified_email: Some(True),
      data_sale_opt_out: True,
      tax_exempt: Some(False),
      tax_exemptions: [],
      state: Some("DISABLED"),
      tags: [],
      number_of_orders: Some("0"),
      amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
      default_email_address: Some(CustomerDefaultEmailAddressRecord(
        email_address: Some(email),
        marketing_state: None,
        marketing_opt_in_level: None,
        marketing_updated_at: None,
      )),
      default_phone_number: None,
      email_marketing_consent: None,
      sms_marketing_consent: None,
      default_address: None,
      created_at: Some(timestamp),
      updated_at: Some(timestamp),
    )
  let #(_, next_store) = store.stage_create_customer(store, customer)
  let payload = data_sale_payload(field, Some(id), [])
  #(
    MutationFieldResult(
      key: get_field_response_key(field),
      payload: payload,
      staged_resource_ids: [id],
    ),
    next_store,
    after_ts,
  )
}

fn data_sale_payload(
  field: Selection,
  customer_id: Option(String),
  errors: List(UserError),
) -> Json {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      project_graphql_value(
        src_object([
          #("__typename", SrcString("DataSaleOptOutPayload")),
          #("customerId", optional_string_source(customer_id)),
          #("userErrors", SrcList(list.map(errors, user_error_source))),
        ]),
        selections,
        dict.new(),
      )
    _ -> json.object([])
  }
}

fn user_error_source(err: UserError) -> SourceValue {
  src_object([
    #("field", case err.field {
      [] -> SrcNull
      _ -> SrcList(list.map(err.field, SrcString))
    }),
    #("message", SrcString(err.message)),
    #("code", optional_string_source(err.code)),
  ])
}

fn optional_string_source(value: Option(String)) -> SourceValue {
  case value {
    Some(s) -> SrcString(s)
    None -> SrcNull
  }
}

fn read_arg_string(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> Option(String) {
  case dict.get(args, name) {
    Ok(root_field.StringVal(s)) ->
      case s {
        "" -> None
        _ -> Some(s)
      }
    _ -> None
  }
}

fn find_customer_by_email(
  customers: List(CustomerRecord),
  email: String,
) -> Option(CustomerRecord) {
  case customers {
    [] -> None
    [customer, ..rest] -> {
      let matches = case customer.email {
        Some(value) -> string.lowercase(email) == string.lowercase(value)
        None -> False
      }
      case matches {
        True -> Some(customer)
        False -> find_customer_by_email(rest, email)
      }
    }
  }
}

fn fetch_upstream_customer_by_email(
  email: String,
  upstream: UpstreamContext,
) -> Option(CustomerRecord) {
  let query =
    "query DataSaleOptOutCustomerLookup($identifier: CustomerIdentifierInput!) {\n"
    <> "  customerByIdentifier(identifier: $identifier) {\n"
    <> "    id\n"
    <> "    email\n"
    <> "    dataSaleOptOut\n"
    <> "    defaultEmailAddress { emailAddress }\n"
    <> "  }\n"
    <> "}\n"
  let variables =
    json.object([
      #("identifier", json.object([#("emailAddress", json.string(email))])),
    ])
  case
    upstream_query.fetch_sync(
      upstream.origin,
      upstream.transport,
      upstream.headers,
      "DataSaleOptOutCustomerLookup",
      query,
      variables,
    )
  {
    Ok(value) -> customer_from_upstream_lookup(value, email)
    Error(_) -> None
  }
}

fn customer_from_upstream_lookup(
  value: commit.JsonValue,
  fallback_email: String,
) -> Option(CustomerRecord) {
  case json_get(value, "data") {
    Some(data) ->
      case json_get(data, "customerByIdentifier") {
        Some(commit.JsonNull) | None -> None
        Some(node) -> customer_record_from_upstream_node(node, fallback_email)
      }
    None -> None
  }
}

fn customer_record_from_upstream_node(
  node: commit.JsonValue,
  fallback_email: String,
) -> Option(CustomerRecord) {
  case json_get_string(node, "id") {
    None -> None
    Some(id) -> {
      let email = option.unwrap(json_get_string(node, "email"), fallback_email)
      let default_email = case json_get(node, "defaultEmailAddress") {
        Some(address) ->
          option.unwrap(json_get_string(address, "emailAddress"), email)
        None -> email
      }
      Some(CustomerRecord(
        id: id,
        first_name: None,
        last_name: None,
        display_name: Some(email),
        email: Some(email),
        legacy_resource_id: gid_tail(id),
        locale: None,
        note: None,
        can_delete: Some(True),
        verified_email: Some(True),
        data_sale_opt_out: option.unwrap(
          json_get_bool(node, "dataSaleOptOut"),
          False,
        ),
        tax_exempt: Some(False),
        tax_exemptions: [],
        state: Some("DISABLED"),
        tags: [],
        number_of_orders: Some("0"),
        amount_spent: Some(Money(amount: "0.0", currency_code: "USD")),
        default_email_address: Some(CustomerDefaultEmailAddressRecord(
          email_address: Some(default_email),
          marketing_state: None,
          marketing_opt_in_level: None,
          marketing_updated_at: None,
        )),
        default_phone_number: None,
        email_marketing_consent: None,
        sms_marketing_consent: None,
        default_address: None,
        created_at: None,
        updated_at: None,
      ))
    }
  }
}

fn is_valid_data_sale_email(email: String) -> Bool {
  let trimmed = string.trim(email)
  case trimmed == email && !string.contains(trimmed, " ") {
    False -> False
    True ->
      case string.split(trimmed, "@") {
        [local, domain] ->
          local != "" && domain_has_dot_with_nonempty_parts(domain)
        _ -> False
      }
  }
}

fn domain_has_dot_with_nonempty_parts(domain: String) -> Bool {
  let parts = string.split(domain, ".")
  list.length(parts) >= 2 && list.all(parts, fn(part) { part != "" })
}

fn gid_tail(id: String) -> Option(String) {
  case string.split(id, "/") |> list.last {
    Ok(value) -> Some(value)
    Error(_) -> None
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

fn json_get(value: commit.JsonValue, key: String) -> Option(commit.JsonValue) {
  case value {
    commit.JsonObject(fields) ->
      list.find_map(fields, fn(pair) {
        case pair {
          #(k, v) if k == key -> Ok(v)
          _ -> Error(Nil)
        }
      })
      |> option.from_result
    _ -> None
  }
}

fn option_map(value: Option(a), mapper: fn(a) -> b) -> Option(b) {
  case value {
    Some(inner) -> Some(mapper(inner))
    None -> None
  }
}
