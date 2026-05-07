//// Mutation handling for webhook subscription roots.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Definition, type Location, type Selection,
  type VariableDefinition, Argument, Field, OperationDefinition,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/parse_operation
import shopify_draft_proxy/graphql/parser as graphql_parser
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/graphql/source as graphql_source
import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, field_locations_json, get_document_fragments,
  get_field_response_key, locations_json,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type LogDraft, type MutationOutcome, MutationOutcome, RequiredArgument,
  read_optional_string, read_optional_string_array, single_root_log_draft,
  validate_required_field_arguments, validate_required_id_argument,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/proxy/webhooks/filters.{
  endpoint_from_uri, uri_from_endpoint, webhook_subscription_uri,
}
import shopify_draft_proxy/proxy/webhooks/serializers
import shopify_draft_proxy/proxy/webhooks/types as webhook_types
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type WebhookSubscriptionRecord, WebhookSubscriptionRecord,
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Outcome of a webhook-subscription mutation. Mirrors the saved-search
/// outcome shape: a JSON envelope (`{"data": ...}` or `{"errors": ...}`),
/// the updated store and identity registry, and the staged GIDs.
/// User-error payload emitted on validation failure. Mirrors the
/// `WebhookSubscriptionUserError` shape in TS.
type UriInput {
  UriAbsent
  UriBlank
  UriPresent(String)
}

/// Predicate matching `isWebhookSubscriptionMutationRoot`. Three
/// top-level mutations the TS handler dispatches.
@internal
pub fn is_webhook_subscription_mutation_root(name: String) -> Bool {
  name == "webhookSubscriptionCreate"
  || name == "webhookSubscriptionUpdate"
  || name == "webhookSubscriptionDelete"
}

/// Process a webhook-subscription mutation document and return a
/// `MutationOutcome`. Mirrors `handleWebhookSubscriptionMutation`.
/// Top-level argument-validation errors short-circuit the whole
/// envelope to `{"errors": [...]}`; otherwise per-field payloads are
/// collected into `{"data": {...}}`.
@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  process_mutation_with_api_client(
    store,
    identity,
    request_path,
    document,
    variables,
    app_identity.read_requesting_api_client_id(upstream.headers),
  )
}

@internal
pub fn process_mutation_with_headers(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  request_headers: Dict(String, String),
) -> MutationOutcome {
  process_mutation_with_api_client(
    store,
    identity,
    request_path,
    document,
    variables,
    app_identity.read_requesting_api_client_id(request_headers),
  )
}

fn process_mutation_with_api_client(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      let operation_path = get_operation_path_label(document)
      handle_mutation_fields(
        store,
        identity,
        request_path,
        document,
        operation_path,
        fields,
        fragments,
        variables,
        requesting_api_client_id,
      )
    }
  }
}

fn get_operation_path_label(document: String) -> String {
  case parse_operation.parse_operation(document) {
    Ok(parsed) -> {
      let kind = case parsed.type_ {
        parse_operation.QueryOperation -> "query"
        parse_operation.MutationOperation -> "mutation"
      }
      case parsed.name {
        Some(name) -> kind <> " " <> name
        None -> kind
      }
    }
    Error(_) -> "mutation"
  }
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: Json,
    staged_resource_ids: List(String),
    top_level_errors: List(Json),
    log_drafts: List(LogDraft),
  )
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  request_path: String,
  document: String,
  operation_path: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(
    data_entries,
    all_errors,
    final_store,
    final_identity,
    all_staged_ids,
    all_drafts,
  ) =
    list.fold(fields, initial, fn(acc, field) {
      let #(
        entries,
        errors,
        current_store,
        current_identity,
        staged_ids,
        drafts,
      ) = acc
      case field {
        Field(name: name, ..) -> {
          let dispatch = case name.value {
            "webhookSubscriptionCreate" ->
              Some(handle_create(
                current_store,
                current_identity,
                request_path,
                document,
                operation_path,
                field,
                fragments,
                variables,
                requesting_api_client_id,
              ))
            "webhookSubscriptionUpdate" ->
              Some(handle_update(
                current_store,
                current_identity,
                request_path,
                document,
                operation_path,
                field,
                fragments,
                variables,
                requesting_api_client_id,
              ))
            "webhookSubscriptionDelete" ->
              Some(handle_delete(
                current_store,
                current_identity,
                request_path,
                document,
                operation_path,
                field,
                fragments,
                variables,
              ))
            _ -> None
          }
          case dispatch {
            None -> acc
            Some(#(result, next_store, next_identity)) -> {
              let next_errors = list.append(errors, result.top_level_errors)
              let next_entries = case result.top_level_errors {
                [] -> list.append(entries, [#(result.key, result.payload)])
                _ -> entries
              }
              let next_staged_ids = case result.top_level_errors {
                [] -> list.append(staged_ids, result.staged_resource_ids)
                _ -> staged_ids
              }
              let next_drafts = list.append(drafts, result.log_drafts)
              #(
                next_entries,
                next_errors,
                next_store,
                next_identity,
                next_staged_ids,
                next_drafts,
              )
            }
          }
        }
        _ -> acc
      }
    })
  let envelope = case all_errors {
    [] -> json.object([#("data", json.object(data_entries))])
    _ ->
      json.object([
        #("errors", json.preprocessed_array(all_errors)),
      ])
  }
  let final_staged_ids = case all_errors {
    [] -> all_staged_ids
    _ -> []
  }
  MutationOutcome(
    data: envelope,
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_staged_ids,
    log_drafts: all_drafts,
  )
}

// ---- Per-field handlers ---------------------------------------------------

fn handle_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "webhookSubscriptionCreate",
      [
        RequiredArgument(
          name: "topic",
          expected_type: "WebhookSubscriptionTopic!",
        ),
        RequiredArgument(
          name: "webhookSubscription",
          expected_type: "WebhookSubscriptionInput!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> {
      let result =
        MutationFieldResult(
          key: key,
          payload: json.null(),
          staged_resource_ids: [],
          top_level_errors: validation_errors,
          log_drafts: [],
        )
      #(result, store, identity)
    }
    [] -> {
      let args = case root_field.get_field_arguments(field, variables) {
        Ok(d) -> d
        Error(_) -> dict.new()
      }
      let #(topic, topic_errors) =
        read_public_webhook_topic(args, field, document, operation_path)
      case topic_errors {
        [_, ..] -> {
          let result =
            MutationFieldResult(
              key: key,
              payload: json.null(),
              staged_resource_ids: [],
              top_level_errors: topic_errors,
              log_drafts: [],
            )
          #(result, store, identity)
        }
        [] -> {
          let input = read_webhook_subscription_input(args)
          let user_errors = case input {
            Some(input_dict) ->
              validate_webhook_subscription_create_input(
                input_dict,
                topic,
                store,
                requesting_api_client_id,
              )
            None -> []
          }
          let #(record_opt, store_after, identity_after, staged_ids) = case
            input,
            user_errors
          {
            Some(input_dict), [] -> {
              let #(record, identity_after) =
                build_webhook_from_create_input(identity, topic, input_dict)
              let #(_, store_after) =
                store.upsert_staged_webhook_subscription(store, record)
              #(Some(record), store_after, identity_after, [record.id])
            }
            _, _ -> #(None, store, identity, [])
          }
          let payload =
            serializers.project_create_payload(
              record_opt,
              user_errors,
              field,
              fragments,
            )
          let draft =
            single_root_log_draft(
              "webhookSubscriptionCreate",
              staged_ids,
              case user_errors {
                [] -> store_types.Staged
                _ -> store_types.Failed
              },
              "webhooks",
              "stage-locally",
              Some(
                "Locally staged webhookSubscriptionCreate in shopify-draft-proxy.",
              ),
            )
          let result =
            MutationFieldResult(
              key: key,
              payload: payload,
              staged_resource_ids: staged_ids,
              top_level_errors: [],
              log_drafts: [draft],
            )
          #(result, store_after, identity_after)
        }
      }
    }
  }
}

fn handle_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let validation_errors =
    validate_required_field_arguments(
      field,
      variables,
      "webhookSubscriptionUpdate",
      [
        RequiredArgument(name: "id", expected_type: "ID!"),
        RequiredArgument(
          name: "webhookSubscription",
          expected_type: "WebhookSubscriptionInput!",
        ),
      ],
      operation_path,
      document,
    )
  case validation_errors {
    [_, ..] -> {
      let result =
        MutationFieldResult(
          key: key,
          payload: json.null(),
          staged_resource_ids: [],
          top_level_errors: validation_errors,
          log_drafts: [],
        )
      #(result, store, identity)
    }
    [] -> {
      let args = case root_field.get_field_arguments(field, variables) {
        Ok(d) -> d
        Error(_) -> dict.new()
      }
      let id = case dict.get(args, "id") {
        Ok(root_field.StringVal(s)) -> Some(s)
        _ -> None
      }
      let input = read_webhook_subscription_input(args)
      let existing = case id {
        Some(gid) -> store.get_effective_webhook_subscription_by_id(store, gid)
        None -> None
      }
      let user_errors = case existing, input {
        Some(existing_record), Some(input_dict) ->
          validate_webhook_subscription_update_input(
            input_dict,
            existing_record,
            requesting_api_client_id,
          )
        Some(_), None -> []
        None, _ -> [
          webhook_types.UserError(
            field: ["id"],
            message: "Webhook subscription does not exist",
          ),
        ]
      }
      let #(record_opt, store_after, identity_after, staged_ids) = case
        input,
        existing,
        user_errors
      {
        Some(input_dict), Some(existing_record), [] -> {
          let #(record, identity_after) =
            apply_webhook_update_input(identity, existing_record, input_dict)
          let #(_, store_after) =
            store.upsert_staged_webhook_subscription(store, record)
          #(Some(record), store_after, identity_after, [record.id])
        }
        _, _, _ -> #(None, store, identity, [])
      }
      let payload =
        serializers.project_update_payload(
          record_opt,
          user_errors,
          field,
          fragments,
        )
      let draft =
        single_root_log_draft(
          "webhookSubscriptionUpdate",
          staged_ids,
          case user_errors {
            [] -> store_types.Staged
            _ -> store_types.Failed
          },
          "webhooks",
          "stage-locally",
          Some(
            "Locally staged webhookSubscriptionUpdate in shopify-draft-proxy.",
          ),
        )
      let result =
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: staged_ids,
          top_level_errors: [],
          log_drafts: [draft],
        )
      #(result, store_after, identity_after)
    }
  }
}

fn handle_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  operation_path: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let key = get_field_response_key(field)
  let #(resolved_id, id_errors) =
    validate_required_id_argument(
      field,
      variables,
      "webhookSubscriptionDelete",
      operation_path,
      document,
    )
  case id_errors {
    [_, ..] -> {
      let result =
        MutationFieldResult(
          key: key,
          payload: json.null(),
          staged_resource_ids: [],
          top_level_errors: id_errors,
          log_drafts: [],
        )
      #(result, store, identity)
    }
    [] -> {
      let existing = case resolved_id {
        Some(gid) -> store.get_effective_webhook_subscription_by_id(store, gid)
        None -> None
      }
      let user_errors = case resolved_id, existing {
        Some(_), Some(_) -> []
        _, _ -> [
          webhook_types.UserError(
            field: ["id"],
            message: "Webhook subscription does not exist",
          ),
        ]
      }
      let store_after = case resolved_id, existing, user_errors {
        Some(gid), Some(_), [] ->
          store.delete_staged_webhook_subscription(store, gid)
        _, _, _ -> store
      }
      let deleted_id = case user_errors {
        [] -> resolved_id
        _ -> None
      }
      let payload =
        serializers.project_delete_payload(
          deleted_id,
          user_errors,
          field,
          fragments,
        )
      let draft =
        single_root_log_draft(
          "webhookSubscriptionDelete",
          [],
          case user_errors {
            [] -> store_types.Staged
            _ -> store_types.Failed
          },
          "webhooks",
          "stage-locally",
          Some(
            "Locally staged webhookSubscriptionDelete in shopify-draft-proxy.",
          ),
        )
      let result =
        MutationFieldResult(
          key: key,
          payload: payload,
          staged_resource_ids: [],
          top_level_errors: [],
          log_drafts: [draft],
        )
      #(result, store_after, identity)
    }
  }
}

// ---- Top-level argument validation ----------------------------------------
//
// Lifted to `proxy/mutation_helpers` in Pass 14. Handlers above call
// `validate_required_field_arguments` and `validate_required_id_argument`
// directly through the import.

// ---- Builders -------------------------------------------------------------

fn build_webhook_from_create_input(
  identity: SyntheticIdentityRegistry,
  topic: Option(String),
  input: Dict(String, root_field.ResolvedValue),
) -> #(WebhookSubscriptionRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let #(id, identity_after_id) =
    synthetic_identity.make_proxy_synthetic_gid(
      identity_after_ts,
      "WebhookSubscription",
    )
  let uri = option.unwrap(normalize_uri_from_input(input), "")
  let record =
    WebhookSubscriptionRecord(
      id: id,
      topic: topic,
      uri: Some(uri),
      name: read_optional_string(input, "name"),
      format: case read_optional_string(input, "format") {
        Some(s) -> Some(s)
        None -> Some("JSON")
      },
      include_fields: option.unwrap(
        read_optional_string_array(input, "includeFields"),
        [],
      ),
      metafield_namespaces: option.unwrap(
        read_optional_string_array(input, "metafieldNamespaces"),
        [],
      ),
      filter: read_optional_string(input, "filter"),
      created_at: Some(timestamp),
      updated_at: Some(timestamp),
      endpoint: Some(endpoint_from_uri(uri)),
    )
  #(record, identity_after_id)
}

fn apply_webhook_update_input(
  identity: SyntheticIdentityRegistry,
  existing: WebhookSubscriptionRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(WebhookSubscriptionRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity_after_ts) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let new_uri = normalize_uri_from_input(input)
  let resolved_uri = case new_uri {
    Some(u) -> Some(u)
    None ->
      case existing.uri {
        Some(_) -> existing.uri
        None -> uri_from_endpoint(existing.endpoint)
      }
  }
  let new_endpoint = case new_uri {
    Some(u) -> Some(endpoint_from_uri(u))
    None -> existing.endpoint
  }
  let record =
    WebhookSubscriptionRecord(
      ..existing,
      uri: resolved_uri,
      name: case read_optional_string(input, "name") {
        Some(s) -> Some(s)
        None -> existing.name
      },
      format: case read_optional_string(input, "format") {
        Some(s) -> Some(s)
        None -> existing.format
      },
      include_fields: option.unwrap(
        read_optional_string_array(input, "includeFields"),
        existing.include_fields,
      ),
      metafield_namespaces: option.unwrap(
        read_optional_string_array(input, "metafieldNamespaces"),
        existing.metafield_namespaces,
      ),
      filter: case read_optional_string(input, "filter") {
        Some(s) -> Some(s)
        None -> existing.filter
      },
      updated_at: Some(timestamp),
      endpoint: new_endpoint,
    )
  #(record, identity_after_ts)
}

// ---- Input readers --------------------------------------------------------

fn read_webhook_subscription_input(
  args: Dict(String, root_field.ResolvedValue),
) -> Option(Dict(String, root_field.ResolvedValue)) {
  case dict.get(args, "webhookSubscription") {
    Ok(root_field.ObjectVal(fields)) -> Some(fields)
    _ -> None
  }
}

fn read_public_webhook_topic(
  args: Dict(String, root_field.ResolvedValue),
  field: Selection,
  document: String,
  operation_path: String,
) -> #(Option(String), List(Json)) {
  case dict.get(args, "topic") {
    Ok(root_field.StringVal(topic)) ->
      case list.contains(public_webhook_subscription_topics(), topic) {
        True -> #(Some(topic), [])
        False -> #(Some(topic), [
          invalid_webhook_topic_error(topic, field, document, operation_path),
        ])
      }
    Ok(_) -> #(None, [
      invalid_webhook_topic_error("", field, document, operation_path),
    ])
    Error(_) -> #(None, [])
  }
}

fn invalid_webhook_topic_error(
  topic: String,
  field: Selection,
  document: String,
  operation_path: String,
) -> Json {
  case topic_argument_variable_name(field) {
    Some(variable_name) ->
      invalid_webhook_topic_variable_error(variable_name, topic, document)
    None ->
      invalid_webhook_topic_literal_error(
        topic,
        field,
        document,
        operation_path,
      )
  }
}

fn invalid_webhook_topic_literal_error(
  topic: String,
  field: Selection,
  document: String,
  operation_path: String,
) -> Json {
  json.object([
    #(
      "message",
      json.string(
        "Argument 'topic' on Field 'webhookSubscriptionCreate' has an invalid value ("
        <> topic
        <> "). Expected type 'WebhookSubscriptionTopic!'.",
      ),
    ),
    #("locations", field_locations_json(field, document)),
    #(
      "path",
      json.array(
        [operation_path, "webhookSubscriptionCreate", "topic"],
        json.string,
      ),
    ),
    #(
      "extensions",
      json.object([
        #("code", json.string("argumentLiteralsIncompatible")),
        #("typeName", json.string("Field")),
        #("argumentName", json.string("topic")),
      ]),
    ),
  ])
}

fn invalid_webhook_topic_variable_error(
  variable_name: String,
  topic: String,
  document: String,
) -> Json {
  let base = [
    #(
      "message",
      json.string(
        "Variable $"
        <> variable_name
        <> " of type WebhookSubscriptionTopic! was provided invalid value",
      ),
    ),
  ]
  let with_locations = case
    variable_definition_location(document, variable_name)
  {
    Some(loc) ->
      list.append(base, [#("locations", locations_json(loc, document))])
    None -> base
  }
  json.object(
    list.append(with_locations, [
      #(
        "extensions",
        json.object([
          #("code", json.string("INVALID_VARIABLE")),
          #("value", json.string(topic)),
          #(
            "problems",
            json.preprocessed_array([
              json.object([
                #("path", json.preprocessed_array([])),
                #(
                  "explanation",
                  json.string(
                    "Expected \""
                    <> topic
                    <> "\" to be one of: "
                    <> public_webhook_subscription_topics_message(),
                  ),
                ),
              ]),
            ]),
          ),
        ]),
      ),
    ]),
  )
}

fn topic_argument_variable_name(field: Selection) -> Option(String) {
  case field {
    Field(arguments: arguments, ..) ->
      case find_ast_argument(arguments, "topic") {
        Some(Argument(value: VariableValue(variable: variable), ..)) ->
          Some(variable.name.value)
        _ -> None
      }
    _ -> None
  }
}

fn find_ast_argument(
  arguments: List(Argument),
  name: String,
) -> Option(Argument) {
  case arguments {
    [] -> None
    [first, ..rest] -> {
      let Argument(name: arg_name, ..) = first
      case arg_name.value == name {
        True -> Some(first)
        False -> find_ast_argument(rest, name)
      }
    }
  }
}

fn variable_definition_location(
  document: String,
  variable_name: String,
) -> Option(Location) {
  case graphql_parser.parse(graphql_source.new(document)) {
    Error(_) -> None
    Ok(doc) ->
      case find_first_operation(doc.definitions) {
        Some(OperationDefinition(variable_definitions: definitions, ..)) ->
          find_variable_definition_location(definitions, variable_name)
        _ -> None
      }
  }
}

fn find_first_operation(definitions: List(Definition)) -> Option(Definition) {
  case definitions {
    [] -> None
    [definition, ..rest] ->
      case definition {
        OperationDefinition(..) -> Some(definition)
        _ -> find_first_operation(rest)
      }
  }
}

fn find_variable_definition_location(
  definitions: List(VariableDefinition),
  variable_name: String,
) -> Option(Location) {
  case definitions {
    [] -> None
    [definition, ..rest] -> {
      let VariableDefinition(variable: variable, loc: loc, ..) = definition
      case variable.name.value == variable_name {
        True -> loc
        False -> find_variable_definition_location(rest, variable_name)
      }
    }
  }
}

fn public_webhook_subscription_topics() -> List(String) {
  string.split(public_webhook_subscription_topics_message(), ", ")
}

fn public_webhook_subscription_topics_message() -> String {
  // Seeded from the captured Shopify Admin GraphQL 2026-04
  // WebhookSubscriptionTopic introspection fixture.
  "TAX_SUMMARIES_CREATE, APP_UNINSTALLED, APP_SCOPES_UPDATE, CARTS_CREATE, CARTS_UPDATE, CHANNELS_DELETE, CHECKOUTS_CREATE, CHECKOUTS_DELETE, CHECKOUTS_UPDATE, CUSTOMER_PAYMENT_METHODS_CREATE, CUSTOMER_PAYMENT_METHODS_UPDATE, CUSTOMER_PAYMENT_METHODS_REVOKE, COLLECTION_LISTINGS_ADD, COLLECTION_LISTINGS_REMOVE, COLLECTION_LISTINGS_UPDATE, COLLECTION_PUBLICATIONS_CREATE, COLLECTION_PUBLICATIONS_DELETE, COLLECTION_PUBLICATIONS_UPDATE, COLLECTIONS_CREATE, COLLECTIONS_DELETE, COLLECTIONS_UPDATE, CUSTOMER_GROUPS_CREATE, CUSTOMER_GROUPS_DELETE, CUSTOMER_GROUPS_UPDATE, CUSTOMERS_CREATE, CUSTOMERS_DELETE, CUSTOMERS_DISABLE, CUSTOMERS_ENABLE, CUSTOMERS_UPDATE, CUSTOMERS_PURCHASING_SUMMARY, CUSTOMERS_MARKETING_CONSENT_UPDATE, CUSTOMER_TAGS_ADDED, CUSTOMER_TAGS_REMOVED, CUSTOMERS_EMAIL_MARKETING_CONSENT_UPDATE, DISPUTES_CREATE, DISPUTES_UPDATE, DRAFT_ORDERS_CREATE, DRAFT_ORDERS_DELETE, DRAFT_ORDERS_UPDATE, FULFILLMENT_EVENTS_CREATE, FULFILLMENT_EVENTS_DELETE, FULFILLMENTS_CREATE, FULFILLMENTS_UPDATE, ATTRIBUTED_SESSIONS_FIRST, ATTRIBUTED_SESSIONS_LAST, ORDER_TRANSACTIONS_CREATE, ORDERS_CANCELLED, ORDERS_CREATE, ORDERS_DELETE, ORDERS_EDITED, ORDERS_FULFILLED, ORDERS_PAID, ORDERS_PARTIALLY_FULFILLED, ORDERS_UPDATED, ORDERS_LINK_REQUESTED, FULFILLMENT_ORDERS_MOVED, FULFILLMENT_ORDERS_HOLD_RELEASED, FULFILLMENT_ORDERS_SCHEDULED_FULFILLMENT_ORDER_READY, FULFILLMENT_HOLDS_RELEASED, FULFILLMENT_ORDERS_ORDER_ROUTING_COMPLETE, FULFILLMENT_ORDERS_CANCELLED, FULFILLMENT_ORDERS_FULFILLMENT_SERVICE_FAILED_TO_COMPLETE, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_REJECTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_ACCEPTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_REJECTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_ACCEPTED, FULFILLMENT_HOLDS_ADDED, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_LOCAL_DELIVERY, FULFILLMENT_ORDERS_PLACED_ON_HOLD, FULFILLMENT_ORDERS_MERGED, FULFILLMENT_ORDERS_SPLIT, FULFILLMENT_ORDERS_PROGRESS_REPORTED, FULFILLMENT_ORDERS_MANUALLY_REPORTED_PROGRESS_STOPPED, PRODUCT_LISTINGS_ADD, PRODUCT_LISTINGS_REMOVE, PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_ADD, SCHEDULED_PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_REMOVE, PRODUCT_PUBLICATIONS_CREATE, PRODUCT_PUBLICATIONS_DELETE, PRODUCT_PUBLICATIONS_UPDATE, PRODUCTS_CREATE, PRODUCTS_DELETE, PRODUCTS_UPDATE, REFUNDS_CREATE, SEGMENTS_CREATE, SEGMENTS_DELETE, SEGMENTS_UPDATE, SHIPPING_ADDRESSES_CREATE, SHIPPING_ADDRESSES_UPDATE, SHOP_UPDATE, TAX_PARTNERS_UPDATE, TAX_SERVICES_CREATE, TAX_SERVICES_UPDATE, THEMES_CREATE, THEMES_DELETE, THEMES_PUBLISH, THEMES_UPDATE, VARIANTS_IN_STOCK, VARIANTS_OUT_OF_STOCK, INVENTORY_LEVELS_CONNECT, INVENTORY_LEVELS_UPDATE, INVENTORY_LEVELS_DISCONNECT, INVENTORY_ITEMS_CREATE, INVENTORY_ITEMS_UPDATE, INVENTORY_ITEMS_DELETE, LOCATIONS_ACTIVATE, LOCATIONS_DEACTIVATE, LOCATIONS_CREATE, LOCATIONS_UPDATE, LOCATIONS_DELETE, TENDER_TRANSACTIONS_CREATE, APP_PURCHASES_ONE_TIME_UPDATE, APP_SUBSCRIPTIONS_APPROACHING_CAPPED_AMOUNT, APP_SUBSCRIPTIONS_UPDATE, LOCALES_CREATE, LOCALES_UPDATE, LOCALES_DESTROY, DOMAINS_CREATE, DOMAINS_UPDATE, DOMAINS_DESTROY, SUBSCRIPTION_CONTRACTS_CREATE, SUBSCRIPTION_CONTRACTS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_CREATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_DELETE, PROFILES_CREATE, PROFILES_UPDATE, PROFILES_DELETE, SUBSCRIPTION_BILLING_ATTEMPTS_SUCCESS, SUBSCRIPTION_BILLING_ATTEMPTS_FAILURE, SUBSCRIPTION_BILLING_ATTEMPTS_CHALLENGED, RETURNS_CANCEL, RETURNS_CLOSE, RETURNS_REOPEN, RETURNS_REQUEST, RETURNS_APPROVE, RETURNS_UPDATE, RETURNS_PROCESS, RETURNS_DECLINE, REVERSE_DELIVERIES_ATTACH_DELIVERABLE, REVERSE_FULFILLMENT_ORDERS_DISPOSE, PAYMENT_TERMS_CREATE, PAYMENT_TERMS_DELETE, PAYMENT_TERMS_UPDATE, PAYMENT_SCHEDULES_DUE, SELLING_PLAN_GROUPS_CREATE, SELLING_PLAN_GROUPS_UPDATE, SELLING_PLAN_GROUPS_DELETE, BULK_OPERATIONS_FINISH, PRODUCT_FEEDS_CREATE, PRODUCT_FEEDS_UPDATE, PRODUCT_FEEDS_INCREMENTAL_SYNC, PRODUCT_FEEDS_FULL_SYNC, PRODUCT_FEEDS_FULL_SYNC_FINISH, MARKETS_CREATE, MARKETS_UPDATE, MARKETS_DELETE, ORDERS_RISK_ASSESSMENT_CHANGED, ORDERS_SHOPIFY_PROTECT_ELIGIBILITY_CHANGED, FINANCE_KYC_INFORMATION_UPDATE, FULFILLMENT_ORDERS_RESCHEDULED, PUBLICATIONS_DELETE, AUDIT_EVENTS_ADMIN_API_ACTIVITY, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_PICKUP, COMPANIES_CREATE, COMPANIES_UPDATE, COMPANIES_DELETE, COMPANY_LOCATIONS_CREATE, COMPANY_LOCATIONS_UPDATE, COMPANY_LOCATIONS_DELETE, COMPANY_CONTACTS_CREATE, COMPANY_CONTACTS_UPDATE, COMPANY_CONTACTS_DELETE, CUSTOMERS_MERGE, INVENTORY_TRANSFERS_ADD_ITEMS, INVENTORY_TRANSFERS_UPDATE_ITEM_QUANTITIES, INVENTORY_TRANSFERS_REMOVE_ITEMS, INVENTORY_TRANSFERS_READY_TO_SHIP, INVENTORY_TRANSFERS_CANCEL, INVENTORY_TRANSFERS_COMPLETE, INVENTORY_SHIPMENTS_DELETE, INVENTORY_SHIPMENTS_CREATE, INVENTORY_SHIPMENTS_MARK_IN_TRANSIT, INVENTORY_SHIPMENTS_UPDATE_TRACKING, INVENTORY_SHIPMENTS_ADD_ITEMS, INVENTORY_SHIPMENTS_UPDATE_ITEM_QUANTITIES, INVENTORY_SHIPMENTS_REMOVE_ITEMS, INVENTORY_SHIPMENTS_RECEIVE_ITEMS, CUSTOMER_ACCOUNT_SETTINGS_UPDATE, CUSTOMER_JOINED_SEGMENT, CUSTOMER_LEFT_SEGMENT, COMPANY_CONTACT_ROLES_ASSIGN, COMPANY_CONTACT_ROLES_REVOKE, SUBSCRIPTION_CONTRACTS_ACTIVATE, SUBSCRIPTION_CONTRACTS_PAUSE, SUBSCRIPTION_CONTRACTS_CANCEL, SUBSCRIPTION_CONTRACTS_FAIL, SUBSCRIPTION_CONTRACTS_EXPIRE, SUBSCRIPTION_BILLING_CYCLES_SKIP, SUBSCRIPTION_BILLING_CYCLES_UNSKIP, METAOBJECTS_CREATE, METAOBJECTS_UPDATE, METAOBJECTS_DELETE, FINANCE_APP_STAFF_MEMBER_GRANT, FINANCE_APP_STAFF_MEMBER_REVOKE, FINANCE_APP_STAFF_MEMBER_DELETE, FINANCE_APP_STAFF_MEMBER_UPDATE, DISCOUNTS_CREATE, DISCOUNTS_UPDATE, DISCOUNTS_DELETE, DISCOUNTS_REDEEMCODE_ADDED, DISCOUNTS_REDEEMCODE_REMOVED, METAFIELD_DEFINITIONS_CREATE, METAFIELD_DEFINITIONS_UPDATE, METAFIELD_DEFINITIONS_DELETE, DELIVERY_PROMISE_SETTINGS_UPDATE, MARKETS_BACKUP_REGION_UPDATE, CHECKOUT_AND_ACCOUNTS_CONFIGURATIONS_UPDATE"
}

// `read_optional_string` and `read_optional_string_array` come from
// `proxy/mutation_helpers` (Pass 14 lift).

fn validate_webhook_subscription_input(
  input: Dict(String, root_field.ResolvedValue),
  topic topic: Option(String),
  require_uri require_uri: Bool,
  requesting_api_client_id requesting_api_client_id: Option(String),
) -> List(webhook_types.UserError) {
  list.append(
    validate_webhook_uri_input(input, require_uri, requesting_api_client_id),
    validate_webhook_filter_input(input, topic),
  )
}

const webhook_subscription_name_max_length = 50

fn validate_webhook_subscription_create_input(
  input: Dict(String, root_field.ResolvedValue),
  topic: Option(String),
  store: Store,
  requesting_api_client_id: Option(String),
) -> List(webhook_types.UserError) {
  let errors =
    validate_webhook_subscription_input(
      input,
      topic: topic,
      require_uri: True,
      requesting_api_client_id: requesting_api_client_id,
    )
    |> list.append(validate_webhook_topic_format_input(topic, input))
    |> list.append(validate_webhook_name_input(input))

  case errors {
    [] -> validate_duplicate_webhook_subscription(topic, input, store)
    _ -> errors
  }
}

fn validate_webhook_subscription_update_input(
  input: Dict(String, root_field.ResolvedValue),
  existing: WebhookSubscriptionRecord,
  requesting_api_client_id: Option(String),
) -> List(webhook_types.UserError) {
  validate_webhook_subscription_input(
    input,
    topic: existing.topic,
    require_uri: False,
    requesting_api_client_id: requesting_api_client_id,
  )
  |> list.append(validate_webhook_topic_format(
    existing.topic,
    resolved_webhook_uri(existing, input),
    resolved_webhook_format(existing, input),
  ))
  |> list.append(validate_webhook_name_input(input))
}

fn validate_webhook_topic_format_input(
  topic: Option(String),
  input: Dict(String, root_field.ResolvedValue),
) -> List(webhook_types.UserError) {
  validate_webhook_topic_format(
    topic,
    normalize_uri_from_input(input),
    normalize_webhook_format(read_optional_string(input, "format")),
  )
}

fn validate_webhook_topic_format(
  topic: Option(String),
  uri: Option(String),
  format: String,
) -> List(webhook_types.UserError) {
  case is_cloud_webhook_uri(uri) && format != "JSON" {
    True -> [cloud_format_user_error()]
    False -> {
      let allowed = supported_webhook_formats(topic)
      case list.contains(allowed, format) {
        True -> []
        False -> [unsupported_format_user_error(format, allowed)]
      }
    }
  }
}

fn supported_webhook_formats(topic: Option(String)) -> List(String) {
  case topic {
    Some("BULK_OPERATIONS_FINISH")
    | Some("COMPANIES_CREATE")
    | Some("DISPUTES_CREATE")
    | Some("PRODUCT_FEEDS_FULL_SYNC")
    | Some("RETURNS_APPROVE")
    | Some("SEGMENTS_CREATE")
    | Some("SELLING_PLAN_GROUPS_CREATE") -> ["JSON"]
    _ -> ["JSON", "XML"]
  }
}

fn normalize_webhook_format(raw: Option(String)) -> String {
  raw
  |> option.unwrap("JSON")
  |> string.trim
  |> string.uppercase
}

fn resolved_webhook_format(
  existing: WebhookSubscriptionRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  case read_optional_string(input, "format") {
    Some(raw) -> normalize_webhook_format(Some(raw))
    None -> normalize_webhook_format(existing.format)
  }
}

fn resolved_webhook_uri(
  existing: WebhookSubscriptionRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case normalize_uri_from_input(input) {
    Some(uri) -> Some(uri)
    None -> webhook_subscription_uri(existing)
  }
}

fn is_cloud_webhook_uri(uri: Option(String)) -> Bool {
  case uri {
    Some(value) ->
      string.starts_with(value, "pubsub://")
      || string.starts_with(value, "arn:aws:events:")
    None -> False
  }
}

fn cloud_format_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "format"],
    message: "Format can only be used with format: 'json'",
  )
}

fn unsupported_format_user_error(
  format: String,
  allowed_formats: List(String),
) -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "format"],
    message: "Format '"
      <> string.lowercase(format)
      <> "' is invalid for this webhook topic. Allowed formats: "
      <> allowed_formats_label(allowed_formats),
  )
}

fn allowed_formats_label(allowed_formats: List(String)) -> String {
  allowed_formats
  |> list.map(string.lowercase)
  |> string.join(", ")
}

fn validate_webhook_name_input(
  input: Dict(String, root_field.ResolvedValue),
) -> List(webhook_types.UserError) {
  case read_optional_string(input, "name") {
    Some(name) -> validate_webhook_name(name)
    None -> []
  }
}

fn validate_webhook_name(name: String) -> List(webhook_types.UserError) {
  case string.length(name) {
    0 -> [
      webhook_types.UserError(
        field: ["webhookSubscription", "name"],
        message: "Name is too short (minimum is 1 character)",
      ),
      invalid_webhook_name_user_error(),
    ]
    length if length > webhook_subscription_name_max_length -> [
      webhook_types.UserError(
        field: ["webhookSubscription", "name"],
        message: "Name is too long (maximum is "
          <> int.to_string(webhook_subscription_name_max_length)
          <> " characters)",
      ),
    ]
    _ ->
      case is_valid_webhook_name(name) {
        True -> []
        False -> [invalid_webhook_name_user_error()]
      }
  }
}

fn invalid_webhook_name_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "name"],
    message: "Name name field can only contain alphanumeric characters, underscores, and hyphens",
  )
}

fn is_valid_webhook_name(name: String) -> Bool {
  list.all(string.to_graphemes(name), fn(char) {
    string.contains(
      "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-",
      char,
    )
  })
}

fn validate_duplicate_webhook_subscription(
  topic: Option(String),
  input: Dict(String, root_field.ResolvedValue),
  store: Store,
) -> List(webhook_types.UserError) {
  let uri = normalize_uri_from_input(input)
  let format =
    Some(normalize_webhook_format(read_optional_string(input, "format")))
  let filter = Some(normalize_webhook_filter(input))
  let address_errors = case
    has_duplicate_webhook_subscription(store, topic, uri, format, filter)
  {
    True -> [duplicate_webhook_subscription_address_user_error()]
    False -> []
  }
  list.append(
    address_errors,
    validate_duplicate_webhook_subscription_name(
      store,
      read_optional_string(input, "name"),
    ),
  )
}

fn has_duplicate_webhook_subscription(
  store: Store,
  topic: Option(String),
  uri: Option(String),
  format: Option(String),
  filter: Option(String),
) -> Bool {
  store.list_effective_webhook_subscriptions(store)
  |> list.any(fn(record) {
    record.topic == topic
    && webhook_subscription_uri(record) == uri
    && normalized_record_format(record) == format
    && normalized_record_filter(record) == filter
  })
}

fn normalized_record_format(
  record: WebhookSubscriptionRecord,
) -> Option(String) {
  Some(normalize_webhook_format(record.format))
}

fn normalized_record_filter(
  record: WebhookSubscriptionRecord,
) -> Option(String) {
  Some(option.unwrap(record.filter, ""))
}

fn normalize_webhook_filter(
  input: Dict(String, root_field.ResolvedValue),
) -> String {
  read_optional_string(input, "filter")
  |> option.unwrap("")
}

fn validate_duplicate_webhook_subscription_name(
  store: Store,
  name: Option(String),
) -> List(webhook_types.UserError) {
  case name {
    Some(name) ->
      case
        store.list_effective_webhook_subscriptions(store)
        |> list.any(fn(record) { record.name == Some(name) })
      {
        True -> [
          webhook_types.UserError(
            field: ["webhookSubscription", "name"],
            message: "Name already exists, no duplicate allowed",
          ),
        ]
        False -> []
      }
    None -> []
  }
}

fn duplicate_webhook_subscription_address_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address for this topic has already been taken",
  )
}

fn validate_webhook_uri_input(
  input: Dict(String, root_field.ResolvedValue),
  require_uri: Bool,
  requesting_api_client_id: Option(String),
) -> List(webhook_types.UserError) {
  case read_uri_input(input), require_uri {
    UriAbsent, True -> [blank_address_user_error()]
    UriAbsent, False -> []
    UriBlank, _ -> [blank_address_user_error()]
    UriPresent(uri), _ -> validate_webhook_uri(uri, requesting_api_client_id)
  }
}

fn read_uri_input(input: Dict(String, root_field.ResolvedValue)) -> UriInput {
  case read_uri_input_field(input, "uri") {
    UriAbsent -> read_uri_input_field(input, "callbackUrl")
    found -> found
  }
}

fn read_uri_input_field(
  input: Dict(String, root_field.ResolvedValue),
  name: String,
) -> UriInput {
  case dict.get(input, name) {
    Ok(root_field.StringVal(raw)) ->
      case string.trim(raw) {
        "" -> UriBlank
        trimmed -> UriPresent(trimmed)
      }
    Ok(root_field.NullVal) -> UriBlank
    _ -> UriAbsent
  }
}

fn blank_address_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address can't be blank",
  )
}

fn invalid_url_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid URL",
  )
}

fn unsupported_protocol_user_error(
  protocol: String,
) -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address protocol " <> protocol <> " is not supported",
  )
}

fn invalid_pubsub_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic",
  )
}

fn address_too_long_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is too long",
  )
}

fn validate_webhook_uri(
  uri: String,
  requesting_api_client_id: Option(String),
) -> List(webhook_types.UserError) {
  case string.byte_size(uri) > 4096 {
    True -> [address_too_long_user_error()]
    False ->
      case string.starts_with(uri, "pubsub://") {
        True -> validate_pubsub_uri(uri)
        False ->
          case string.starts_with(uri, "arn:aws:events:") {
            True -> validate_eventbridge_arn(uri, requesting_api_client_id)
            False ->
              case string.starts_with(uri, "kafka://") {
                True -> kafka_user_errors()
                False -> validate_https_uri(uri)
              }
          }
      }
  }
}

fn validate_https_uri(uri: String) -> List(webhook_types.UserError) {
  case string.starts_with(uri, "https://") {
    False ->
      case string.split_once(uri, "://") {
        Ok(#(protocol, _)) -> [
          unsupported_protocol_user_error(protocol <> "://"),
        ]
        Error(_) -> [invalid_url_user_error()]
      }
    True -> {
      let host = https_uri_host(uri)
      case host == "" || has_url_whitespace(uri) {
        True -> [invalid_url_user_error()]
        False ->
          case is_disallowed_host(host) {
            True -> [internal_domain_user_error()]
            False -> []
          }
      }
    }
  }
}

fn https_uri_host(uri: String) -> String {
  uri
  |> string.drop_start(8)
  |> uri_authority
  |> host_without_userinfo
  |> host_without_port
  |> trim_trailing_dot
  |> string.lowercase
}

fn uri_authority(after_scheme: String) -> String {
  after_scheme
  |> string_before("/")
  |> string_before("?")
  |> string_before("#")
}

fn string_before(value: String, delimiter: String) -> String {
  case string.split(value, on: delimiter) {
    [head, ..] -> head
    [] -> value
  }
}

fn host_without_userinfo(authority: String) -> String {
  case string.split(authority, on: "@") |> list.last {
    Ok(host) -> host
    Error(_) -> authority
  }
}

fn host_without_port(host_port: String) -> String {
  case string.starts_with(host_port, "[") {
    True ->
      case string.drop_start(host_port, 1) |> string.split_once("]") {
        Ok(#(host, _)) -> host
        Error(_) -> host_port
      }
    False -> string_before(host_port, ":")
  }
}

fn trim_trailing_dot(host: String) -> String {
  case string.ends_with(host, ".") {
    True -> trim_trailing_dot(string.drop_end(host, 1))
    False -> host
  }
}

fn has_url_whitespace(value: String) -> Bool {
  string.contains(value, " ")
  || string.contains(value, "\t")
  || string.contains(value, "\n")
  || string.contains(value, "\r")
}

fn is_disallowed_host(host: String) -> Bool {
  is_disallowed_webhook_domain(host) || is_disallowed_webhook_ipv4(host)
}

fn is_disallowed_webhook_domain(host: String) -> Bool {
  host == "localhost"
  || domain_matches(host, "shopify.com")
  || domain_matches(host, "myshopify.com")
  || domain_matches(host, "shopifypreview.com")
  || domain_matches(host, "myshopify.dev")
}

fn domain_matches(host: String, domain: String) -> Bool {
  host == domain || string.ends_with(host, "." <> domain)
}

fn is_disallowed_webhook_ipv4(host: String) -> Bool {
  case parse_ipv4(host) {
    Some(#(first, second, _, _)) ->
      first == 0
      || first == 10
      || first == 127
      || { first == 192 && second == 168 }
      || { first == 172 && second >= 16 && second <= 31 }
    None -> False
  }
}

fn parse_ipv4(host: String) -> Option(#(Int, Int, Int, Int)) {
  case string.split(host, on: ".") {
    [a, b, c, d] ->
      case int.parse(a), int.parse(b), int.parse(c), int.parse(d) {
        Ok(first), Ok(second), Ok(third), Ok(fourth) ->
          case
            valid_ipv4_octet(first)
            && valid_ipv4_octet(second)
            && valid_ipv4_octet(third)
            && valid_ipv4_octet(fourth)
          {
            True -> Some(#(first, second, third, fourth))
            False -> None
          }
        _, _, _, _ -> None
      }
    _ -> None
  }
}

fn valid_ipv4_octet(value: Int) -> Bool {
  value >= 0 && value <= 255
}

fn internal_domain_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address cannot be a Shopify or an internal domain",
  )
}

fn validate_pubsub_uri(uri: String) -> List(webhook_types.UserError) {
  let tail = string.drop_start(uri, 9)
  case string.split_once(tail, ":") {
    Ok(#(project, topic)) ->
      case project, topic {
        "", _ -> pubsub_format_user_errors()
        _, "" -> pubsub_format_user_errors()
        _, _ ->
          case valid_gcp_project_id(project) {
            False -> [invalid_address_user_error(), gcp_project_id_user_error()]
            True ->
              case valid_gcp_topic_id(topic) {
                True -> []
                False -> [
                  invalid_address_user_error(),
                  gcp_topic_id_user_error(),
                ]
              }
          }
      }
    Error(_) -> pubsub_format_user_errors()
  }
}

fn pubsub_format_user_errors() -> List(webhook_types.UserError) {
  [
    unsupported_protocol_user_error("pubsub://"),
    invalid_pubsub_user_error(),
  ]
}

fn valid_gcp_project_id(project: String) -> Bool {
  case all_digits(project) {
    True -> True
    False -> {
      let length = string.length(project)
      length >= 6
      && length <= 30
      && starts_with_lowercase_alpha(project)
      && !string.ends_with(project, "-")
      && all_gcp_project_chars(project)
    }
  }
}

fn valid_gcp_topic_id(topic: String) -> Bool {
  let length = string.length(topic)
  length >= 3
  && length <= 255
  && starts_with_alpha(topic)
  && !string.starts_with(string.lowercase(topic), "goog")
  && all_gcp_topic_chars(topic)
}

fn validate_eventbridge_arn(
  uri: String,
  requesting_api_client_id: Option(String),
) -> List(webhook_types.UserError) {
  let tail = string.drop_start(uri, 15)
  case string.split_once(tail, "::event-source/aws.partner/shopify.com") {
    Ok(#(region, after_domain)) ->
      case valid_aws_region(region), eventbridge_source_tail(after_domain) {
        True, Some(#(api_client_id, event_source_name)) ->
          case all_digits(api_client_id) && event_source_name != "" {
            True ->
              case requesting_api_client_id {
                Some(expected) if api_client_id != expected -> [
                  invalid_address_user_error(),
                  eventbridge_wrong_api_client_user_error(
                    api_client_id,
                    expected,
                  ),
                ]
                _ -> []
              }
            False -> eventbridge_arn_user_errors()
          }
        _, _ -> eventbridge_arn_user_errors()
      }
    Error(_) -> eventbridge_arn_user_errors()
  }
}

fn eventbridge_source_tail(tail: String) -> Option(#(String, String)) {
  let source = case string.starts_with(tail, ".test/") {
    True -> Some(string.drop_start(tail, 6))
    False ->
      case string.starts_with(tail, "/") {
        True -> Some(string.drop_start(tail, 1))
        False -> None
      }
  }
  case source {
    Some(rest) -> {
      case string.split_once(rest, "/") {
        Ok(#(api_client_id, event_source_name)) ->
          Some(#(api_client_id, event_source_name))
        Error(_) -> None
      }
    }
    None -> None
  }
}

fn valid_aws_region(region: String) -> Bool {
  case string.split(region, on: "-") {
    [country, zone, number] ->
      string.length(country) == 2
      && all_lowercase_alpha(country)
      && zone != ""
      && all_lowercase_alpha(zone)
      && all_digits(number)
    _ -> False
  }
}

fn starts_with_lowercase_alpha(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Ok(#(first, _)) -> is_lowercase_alpha(first)
    Error(_) -> False
  }
}

fn starts_with_alpha(value: String) -> Bool {
  case string.pop_grapheme(value) {
    Ok(#(first, _)) -> is_alpha(first)
    Error(_) -> False
  }
}

fn all_gcp_project_chars(value: String) -> Bool {
  list.all(string.to_graphemes(value), fn(grapheme) {
    is_lowercase_alpha(grapheme) || is_digit(grapheme) || grapheme == "-"
  })
}

fn all_gcp_topic_chars(value: String) -> Bool {
  list.all(string.to_graphemes(value), fn(grapheme) {
    is_alpha(grapheme)
    || is_digit(grapheme)
    || string.contains("-_.~+%", grapheme)
  })
}

fn all_lowercase_alpha(value: String) -> Bool {
  list.all(string.to_graphemes(value), is_lowercase_alpha)
}

fn all_digits(value: String) -> Bool {
  value != "" && list.all(string.to_graphemes(value), is_digit)
}

fn is_alpha(grapheme: String) -> Bool {
  is_lowercase_alpha(grapheme)
  || string.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ", grapheme)
}

fn is_lowercase_alpha(grapheme: String) -> Bool {
  string.contains("abcdefghijklmnopqrstuvwxyz", grapheme)
}

fn is_digit(grapheme: String) -> Bool {
  string.contains("0123456789", grapheme)
}

fn invalid_address_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is invalid",
  )
}

fn gcp_project_id_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid GCP project id.",
  )
}

fn gcp_topic_id_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is not a valid GCP topic id.",
  )
}

fn eventbridge_arn_user_errors() -> List(webhook_types.UserError) {
  [
    invalid_address_user_error(),
    webhook_types.UserError(
      field: ["webhookSubscription", "callbackUrl"],
      message: "Address is not a valid AWS ARN",
    ),
  ]
}

fn eventbridge_wrong_api_client_user_error(
  actual: String,
  expected: String,
) -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription", "callbackUrl"],
    message: "Address is an AWS ARN and includes api_client_id '"
      <> actual
      <> "' instead of '"
      <> expected
      <> "'",
  )
}

fn kafka_user_errors() -> List(webhook_types.UserError) {
  [
    unsupported_protocol_user_error("kafka://"),
    webhook_types.UserError(
      field: ["webhookSubscription", "callbackUrl"],
      message: "Address is not a valid kafka topic",
    ),
  ]
}

fn validate_webhook_filter_input(
  input: Dict(String, root_field.ResolvedValue),
  topic: Option(String),
) -> List(webhook_types.UserError) {
  case dict.has_key(input, "filter") {
    False -> []
    True ->
      case read_optional_string(input, "filter") {
        Some(raw) ->
          case string.trim(raw) {
            "" -> []
            trimmed ->
              case webhook_filter_allowed(topic, trimmed) {
                True -> []
                False -> [invalid_webhook_filter_user_error()]
              }
          }
        None -> []
      }
  }
}

fn invalid_webhook_filter_user_error() -> webhook_types.UserError {
  webhook_types.UserError(
    field: ["webhookSubscription"],
    message: "The specified filter is invalid, please ensure you specify the field(s) you wish to filter on.",
  )
}

fn webhook_filter_allowed(topic: Option(String), filter: String) -> Bool {
  case topic {
    Some("CUSTOMERS_UPDATE") -> customer_id_webhook_filter(filter)
    _ -> False
  }
}

fn customer_id_webhook_filter(filter: String) -> Bool {
  case string.split_once(filter, "customer_id:") {
    Ok(#("", customer_id)) -> decimal_customer_id(customer_id)
    _ -> False
  }
}

fn decimal_customer_id(value: String) -> Bool {
  value != "" && list.all(string.to_graphemes(value), is_digit)
}

fn normalize_uri_from_input(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  // Real Shopify exposes `callbackUrl` on `WebhookSubscriptionInput` as a
  // (deprecated) alias for `uri` — confirmed by introspecting the 2025-01
  // schema with `includeDeprecated: true`. Reading only `uri` made the
  // proxy fabricate a misleading "Address can't be blank" userError when
  // callers used the deprecated field name. (`callback_url` and `endpoint`
  // are NOT real input fields and were stripped after the introspection
  // check; they were artifacts of the search-query alias list in this
  // module.)
  case read_uri_input(input) {
    UriPresent(uri) -> Some(uri)
    UriAbsent | UriBlank -> None
  }
}
