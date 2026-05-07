//// Mutation handling for admin-platform roots.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import gleam/uri
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/admin_platform/queries
import shopify_draft_proxy/proxy/commit
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcList, SrcNull, SrcString,
  get_document_fragments, get_field_response_key, src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, LogDraft, MutationOutcome, RequiredArgument,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type BackupRegionRecord, AdminPlatformFlowSignatureRecord,
  AdminPlatformFlowTriggerRecord,
}

const flow_trigger_payload_limit_bytes = 50_000

const flow_signature_secret = "shopify-draft-proxy-flow-signature-local-secret-v1"

@internal
pub fn is_admin_platform_mutation_root(name: String) -> Bool {
  list.contains(
    ["backupRegionUpdate", "flowGenerateSignature", "flowTriggerReceive"],
    name,
  )
}

@internal
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
  upstream: UpstreamContext,
) -> MutationOutcome {
  process_mutation_with_shop_origin(
    store,
    identity,
    upstream.origin,
    document,
    variables,
  )
}

@internal
pub fn process_mutation_with_shop_origin(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shop_origin: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  case root_field.get_root_fields(document) {
    Error(err) -> mutation_helpers.parse_failed_outcome(store, identity, err)
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      handle_mutation_fields(
        store,
        identity,
        shop_origin,
        document,
        fields,
        fragments,
        variables,
      )
    }
  }
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shop_origin: String,
  document: String,
  fields: List(Selection),
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationOutcome {
  let initial = #([], [], store, identity, [], [])
  let #(data_entries, errors, final_store, final_identity, staged_ids, notes) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, errs, current_store, current_identity, ids, current_notes) =
        acc
      let key = get_field_response_key(field)
      case field {
        Field(name: name, ..) -> {
          let result =
            handle_mutation_field(
              current_store,
              current_identity,
              shop_origin,
              document,
              field,
              name.value,
              fragments,
              variables,
            )
          let MutationFieldResult(
            payload,
            field_errors,
            next_store,
            next_identity,
            next_ids,
            next_notes,
          ) = result
          #(
            list.append(entries, [#(key, payload)]),
            list.append(errs, field_errors),
            next_store,
            next_identity,
            list.append(ids, next_ids),
            list.append(current_notes, next_notes),
          )
        }
        _ -> acc
      }
    })
  let root_names = mutation_root_names(fields)
  let primary_root = case list.first(root_names) {
    Ok(name) -> Some(name)
    Error(_) -> None
  }
  let log_drafts = case staged_ids {
    [] -> []
    _ -> [
      LogDraft(
        operation_name: primary_root,
        root_fields: root_names,
        primary_root_field: primary_root,
        domain: "admin-platform",
        execution: "stage-locally",
        query: None,
        variables: None,
        staged_resource_ids: staged_ids,
        status: store_types.Staged,
        notes: case notes {
          [] -> Some("Handled Admin Platform utility mutation locally.")
          _ -> Some(string.join(notes, " "))
        },
      ),
    ]
  }
  let data = json.object(data_entries)
  let body_entries = case errors {
    [] -> [#("data", data)]
    _ -> [#("data", data), #("errors", json.array(errors, fn(x) { x }))]
  }
  MutationOutcome(
    data: json.object(body_entries),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: staged_ids,
    log_drafts: log_drafts,
  )
}

type MutationFieldResult {
  MutationFieldResult(
    payload: Json,
    errors: List(Json),
    store: Store,
    identity: SyntheticIdentityRegistry,
    staged_resource_ids: List(String),
    notes: List(String),
  )
}

fn handle_mutation_field(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shop_origin: String,
  document: String,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  case name {
    "flowGenerateSignature" ->
      handle_flow_generate_signature(
        store,
        identity,
        document,
        field,
        fragments,
        variables,
      )
    "flowTriggerReceive" ->
      handle_flow_trigger_receive(store, identity, field, fragments, variables)
    "backupRegionUpdate" ->
      handle_backup_region_update(
        store,
        identity,
        shop_origin,
        field,
        fragments,
        variables,
      )
    _ -> MutationFieldResult(json.null(), [], store, identity, [], [])
  }
}

fn handle_flow_generate_signature(
  store: Store,
  identity: SyntheticIdentityRegistry,
  document: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let argument_errors =
    mutation_helpers.validate_required_field_arguments(
      field,
      variables,
      "flowGenerateSignature",
      [
        RequiredArgument("id", "ID!"),
        RequiredArgument("payload", "String!"),
      ],
      "mutation",
      document,
    )

  case argument_errors {
    [_, ..] ->
      MutationFieldResult(json.null(), argument_errors, store, identity, [], [])
    [] -> {
      let args = graphql_helpers.field_args(field, variables)
      let id = queries.read_string_arg(args, "id")
      let payload = queries.read_string_arg(args, "payload")
      case valid_flow_trigger_id(id) {
        False ->
          MutationFieldResult(
            json.null(),
            [resource_not_found_error(field, document, id)],
            store,
            identity,
            [],
            [],
          )
        True -> {
          let signature =
            crypto.sha256_hex(
              flow_signature_secret <> "|" <> id <> "|" <> payload,
            )
          let #(record_id, identity_after_id) =
            synthetic_identity.make_synthetic_gid(
              identity,
              "FlowGenerateSignature",
            )
          let #(created_at, identity_after_time) =
            synthetic_identity.make_synthetic_timestamp(identity_after_id)
          let record =
            AdminPlatformFlowSignatureRecord(
              id: record_id,
              flow_trigger_id: id,
              payload_sha256: crypto.sha256_hex(payload),
              signature_sha256: crypto.sha256_hex(signature),
              created_at: created_at,
            )
          let #(_, next_store) =
            store.stage_admin_platform_flow_signature(store, record)
          MutationFieldResult(
            queries.project_selection(
              flow_generate_signature_source(payload, signature),
              field,
              fragments,
            ),
            [],
            next_store,
            identity_after_time,
            [record_id],
            [
              "Generated a deterministic proxy-local Flow signature without exposing or storing a Shopify secret.",
            ],
          )
        }
      }
    }
  }
}

fn valid_flow_trigger_id(id: String) -> Bool {
  string.starts_with(id, "gid://shopify/FlowTrigger/")
  && string.drop_start(id, string.length("gid://shopify/FlowTrigger/")) != "0"
}

fn flow_generate_signature_source(payload: String, signature: String) {
  src_object([
    #("__typename", SrcString("FlowGenerateSignaturePayload")),
    #("payload", SrcString(payload)),
    #("signature", SrcString(signature)),
    #("userErrors", SrcList([])),
  ])
}

fn handle_flow_trigger_receive(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let args = graphql_helpers.field_args(field, variables)
  let body = graphql_helpers.read_arg_string(args, "body")
  let handle = graphql_helpers.read_arg_string(args, "handle")
  let payload = case dict.get(args, "payload") {
    Ok(value) -> value
    Error(_) -> root_field.NullVal
  }
  let payload_json = resolved_value_to_json_string(payload)
  let payload_bytes = string.byte_size(payload_json)
  let body_present = string_option_present(body)
  let handle_present = string_option_present(handle)
  let payload_present = resolved_value_present(payload)
  let body_validation = case body_present, body {
    True, Some(value) -> validate_flow_trigger_body(value, store)
    _, _ -> Ok("")
  }
  let user_errors = case body_present, handle_present, payload_present {
    True, True, _ -> [flow_trigger_body_conflict_error()]
    True, _, True -> [flow_trigger_body_conflict_error()]
    True, False, False ->
      case body_validation {
        Ok(_) -> []
        Error(errors) -> errors
      }
    False, False, _ -> [flow_trigger_missing_handle_error()]
    _, _, _ ->
      case payload_bytes > flow_trigger_payload_limit_bytes {
        True -> [
          user_error(
            ["body"],
            "Errors validating schema:\n  Properties size exceeds the limit of "
              <> int.to_string(flow_trigger_payload_limit_bytes)
              <> " bytes.\n",
            None,
          ),
        ]
        False -> {
          let handle_value = handle |> option.unwrap("")
          case is_known_missing_flow_trigger_handle(handle_value) {
            True -> [flow_trigger_invalid_handle_error(handle_value)]
            False -> []
          }
        }
      }
  }
  case user_errors {
    [] -> {
      let record_handle = case handle {
        Some(value) -> value
        None -> "legacy-body"
      }
      let audit_payload = case body, body_validation {
        Some(_), Ok(canonical_body) -> canonical_body
        Some(value), Error(_) -> value
        None, _ -> payload_json
      }
      let audit_payload_bytes = string.byte_size(audit_payload)
      let #(record_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "FlowTriggerReceive")
      let #(received_at, identity_after_time) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let record =
        AdminPlatformFlowTriggerRecord(
          id: record_id,
          handle: record_handle,
          payload_bytes: audit_payload_bytes,
          payload_sha256: crypto.sha256_hex(audit_payload),
          received_at: received_at,
        )
      let #(_, next_store) =
        store.stage_admin_platform_flow_trigger(store, record)
      MutationFieldResult(
        queries.project_selection(
          flow_trigger_receive_source([]),
          field,
          fragments,
        ),
        [],
        next_store,
        identity_after_time,
        [record_id],
        [
          "Recorded a local Flow trigger receipt without delivering any external Flow side effects.",
        ],
      )
    }
    _ ->
      MutationFieldResult(
        queries.project_selection(
          flow_trigger_receive_source(user_errors),
          field,
          fragments,
        ),
        [],
        store,
        identity,
        [],
        [],
      )
  }
}

fn flow_trigger_body_conflict_error() -> SourceValue {
  user_error(
    ["body"],
    "Cannot use `handle` and `payload` arguments with `body` argument",
    None,
  )
}

fn flow_trigger_missing_handle_error() -> SourceValue {
  user_error(["handle"], "`handle` and `payload` arguments are required", None)
}

fn flow_trigger_invalid_handle_error(handle: String) -> SourceValue {
  user_error(
    ["body"],
    "Errors validating schema:\n  Invalid handle '" <> handle <> "'.\n",
    None,
  )
}

fn is_known_missing_flow_trigger_handle(handle: String) -> Bool {
  handle == "har-374-missing"
}

fn validate_flow_trigger_body(
  raw_body: String,
  store: Store,
) -> Result(String, List(SourceValue)) {
  case json.parse(raw_body, commit.json_value_decoder()) {
    Ok(value) ->
      case validate_flow_trigger_body_value(value, store) {
        Ok(Nil) -> Ok(json.to_string(commit.json_value_to_json(value)))
        Error(messages) -> Error([flow_trigger_body_schema_error(messages)])
      }
    Error(_) ->
      Error([
        flow_trigger_body_schema_error([
          flow_trigger_json_parse_message(raw_body),
        ]),
      ])
  }
}

fn validate_flow_trigger_body_value(
  value: commit.JsonValue,
  store: Store,
) -> Result(Nil, List(String)) {
  case value {
    commit.JsonObject(fields) ->
      validate_flow_trigger_body_fields(fields, store)
    _ -> Error(["Type error: body is not an Object."])
  }
}

fn validate_flow_trigger_body_fields(
  fields: List(#(String, commit.JsonValue)),
  store: Store,
) -> Result(Nil, List(String)) {
  let property_errors = validate_flow_trigger_properties(fields)
  let trigger_type_errors =
    list.append(
      validate_optional_flow_trigger_string(fields, "trigger_id"),
      validate_optional_flow_trigger_string(fields, "trigger_title"),
    )
  let resource_errors = validate_optional_flow_trigger_resources(fields)
  let structural_errors =
    []
    |> list.append(property_errors)
    |> list.append(validate_unknown_flow_trigger_body_fields(fields))
    |> list.append(trigger_type_errors)
    |> list.append(resource_errors)
  let reference_errors =
    validate_flow_trigger_reference(fields, store, structural_errors)
  case list.append(structural_errors, reference_errors) {
    [] -> Ok(Nil)
    errors -> Error(errors)
  }
}

fn validate_flow_trigger_properties(
  fields: List(#(String, commit.JsonValue)),
) -> List(String) {
  case json_object_field(fields, "properties") {
    Some(commit.JsonObject(_)) -> []
    Some(value) -> [
      "Type error for field 'properties': "
      <> flow_trigger_json_error_value(value)
      <> " is not an Object.",
    ]
    None -> ["Required field missing: 'properties'."]
  }
}

fn validate_unknown_flow_trigger_body_fields(
  fields: List(#(String, commit.JsonValue)),
) -> List(String) {
  case fields {
    [] -> []
    [#(key, _), ..rest] -> {
      let rest_errors = validate_unknown_flow_trigger_body_fields(rest)
      case is_flow_trigger_body_field(key) {
        True -> rest_errors
        False -> list.append(["Invalid field: '" <> key <> "'."], rest_errors)
      }
    }
  }
}

fn is_flow_trigger_body_field(key: String) -> Bool {
  key == "trigger_id"
  || key == "trigger_title"
  || key == "resources"
  || key == "properties"
}

fn validate_optional_flow_trigger_string(
  fields: List(#(String, commit.JsonValue)),
  name: String,
) -> List(String) {
  case json_object_field(fields, name) {
    Some(commit.JsonString(_)) -> []
    Some(value) -> [
      "Type error for field '"
      <> name
      <> "': "
      <> flow_trigger_json_error_value(value)
      <> " is not a String.",
    ]
    None -> []
  }
}

fn validate_optional_flow_trigger_resources(
  fields: List(#(String, commit.JsonValue)),
) -> List(String) {
  case json_object_field(fields, "resources") {
    Some(commit.JsonArray(resources)) ->
      validate_flow_trigger_resources(resources)
    Some(value) -> [
      "Type error for field 'resources': "
      <> flow_trigger_json_error_value(value)
      <> " is not an Array.",
    ]
    None -> []
  }
}

fn validate_flow_trigger_resources(
  resources: List(commit.JsonValue),
) -> List(String) {
  case resources {
    [] -> []
    [resource, ..rest] -> {
      list.append(
        validate_flow_trigger_resource(resource),
        validate_flow_trigger_resources(rest),
      )
    }
  }
}

fn validate_flow_trigger_resource(resource: commit.JsonValue) -> List(String) {
  case resource {
    commit.JsonObject(fields) -> {
      list.append(
        validate_missing_flow_trigger_resource_fields(fields),
        validate_present_flow_trigger_resource_fields(fields),
      )
    }
    _ -> [
      "Type error for field 'resources': "
      <> flow_trigger_json_error_value(resource)
      <> " is not an Object.",
    ]
  }
}

fn validate_missing_flow_trigger_resource_fields(
  fields: List(#(String, commit.JsonValue)),
) -> List(String) {
  []
  |> list.append(validate_missing_flow_trigger_resource_field(fields, "url"))
  |> list.append(validate_missing_flow_trigger_resource_field(fields, "name"))
}

fn validate_missing_flow_trigger_resource_field(
  fields: List(#(String, commit.JsonValue)),
  name: String,
) -> List(String) {
  case json_object_field(fields, name) {
    Some(_) -> []
    None -> ["Required field missing: '" <> name <> "'."]
  }
}

fn validate_present_flow_trigger_resource_fields(
  fields: List(#(String, commit.JsonValue)),
) -> List(String) {
  []
  |> list.append(validate_present_flow_trigger_resource_field(fields, "url"))
  |> list.append(validate_present_flow_trigger_resource_field(fields, "name"))
}

fn validate_present_flow_trigger_resource_field(
  fields: List(#(String, commit.JsonValue)),
  name: String,
) -> List(String) {
  case json_object_field(fields, name) {
    Some(commit.JsonString(value)) ->
      case name == "url" && !flow_trigger_url_is_absolute(value) {
        True -> [
          "Type error for field 'url': " <> value <> " is not an absolute URL.",
        ]
        False -> []
      }
    Some(value) -> [
      "Type error for field '"
      <> name
      <> "': "
      <> flow_trigger_json_error_value(value)
      <> " is not a String.",
    ]
    None -> []
  }
}

fn flow_trigger_url_is_absolute(value: String) -> Bool {
  case uri.parse(value) {
    Ok(uri.Uri(scheme: Some(scheme), ..)) -> string.trim(scheme) != ""
    _ -> False
  }
}

fn validate_flow_trigger_reference(
  fields: List(#(String, commit.JsonValue)),
  store: Store,
  structural_errors: List(String),
) -> List(String) {
  case structural_errors {
    [] -> validate_flow_trigger_reference_after_shape(fields, store)
    _ -> []
  }
}

fn validate_flow_trigger_reference_after_shape(
  fields: List(#(String, commit.JsonValue)),
  store: Store,
) -> List(String) {
  case json_object_field(fields, "trigger_id") {
    Some(commit.JsonString(id)) ->
      case known_flow_trigger_id(store, id) {
        True -> []
        False -> ["Invalid trigger_id '" <> id <> "'."]
      }
    _ ->
      case json_object_field(fields, "trigger_title") {
        Some(commit.JsonString(title)) ->
          case known_flow_trigger_title(store, title) {
            True -> []
            False -> ["Invalid trigger_title '" <> title <> "'."]
          }
        _ -> ["Required field missing: 'trigger_id'."]
      }
  }
}

fn known_flow_trigger_id(_store: Store, _id: String) -> Bool {
  False
}

fn known_flow_trigger_title(_store: Store, _title: String) -> Bool {
  False
}

fn json_object_field(
  fields: List(#(String, commit.JsonValue)),
  name: String,
) -> Option(commit.JsonValue) {
  case fields {
    [] -> None
    [#(key, value), ..rest] ->
      case key == name {
        True -> Some(value)
        False -> json_object_field(rest, name)
      }
  }
}

fn flow_trigger_body_schema_error(messages: List(String)) -> SourceValue {
  user_error(
    ["body"],
    "Errors validating schema:\n  " <> string.join(messages, "\n  ") <> "\n",
    None,
  )
}

fn flow_trigger_json_parse_message(raw_body: String) -> String {
  let trimmed = string.trim(raw_body)
  let token = case string.split(trimmed, " ") {
    [first, ..] -> first
    [] -> trimmed
  }
  "unexpected token '" <> token <> "' at line 1 column 1"
}

fn flow_trigger_json_error_value(value: commit.JsonValue) -> String {
  case value {
    commit.JsonString(value) -> value
    _ -> json.to_string(commit.json_value_to_json(value))
  }
}

fn string_option_present(value: Option(String)) -> Bool {
  case value {
    Some(value) -> string.trim(value) != ""
    None -> False
  }
}

fn resolved_value_present(value: root_field.ResolvedValue) -> Bool {
  case value {
    root_field.NullVal -> False
    root_field.StringVal(value) -> string.trim(value) != ""
    root_field.BoolVal(value) -> value
    root_field.IntVal(_) -> True
    root_field.FloatVal(_) -> True
    root_field.ListVal(items) -> !list.is_empty(items)
    root_field.ObjectVal(fields) -> !dict.is_empty(fields)
  }
}

fn resolved_value_to_json_string(value: root_field.ResolvedValue) -> String {
  value
  |> root_field.resolved_value_to_json
  |> json.to_string
}

fn flow_trigger_receive_source(errors: List(SourceValue)) {
  src_object([
    #("__typename", SrcString("FlowTriggerReceivePayload")),
    #("userErrors", SrcList(errors)),
  ])
}

fn handle_backup_region_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shop_origin: String,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let args = graphql_helpers.field_args(field, variables)
  case dict.get(args, "region") {
    Ok(root_field.ObjectVal(region_args)) ->
      handle_backup_region_update_to_country(
        store,
        identity,
        shop_origin,
        field,
        fragments,
        queries.read_string_arg(region_args, "countryCode"),
      )
    Ok(root_field.NullVal) | Error(_) ->
      MutationFieldResult(
        queries.project_selection(
          backup_region_update_source(
            queries.effective_backup_region(store, shop_origin),
            [],
          ),
          field,
          fragments,
        ),
        [],
        store,
        identity,
        [],
        [],
      )
    _ -> backup_region_not_found_result(store, identity, field, fragments)
  }
}

fn handle_backup_region_update_to_country(
  store: Store,
  identity: SyntheticIdentityRegistry,
  shop_origin: String,
  field: Selection,
  fragments: FragmentMap,
  code: String,
) -> MutationFieldResult {
  case queries.backup_region_for_country(store, shop_origin, code) {
    None -> backup_region_not_found_result(store, identity, field, fragments)
    Some(region) -> {
      let #(_, next_store) = store.stage_backup_region(store, region)
      MutationFieldResult(
        queries.project_selection(
          backup_region_update_source(Some(region), []),
          field,
          fragments,
        ),
        [],
        next_store,
        identity,
        [region.id],
        [
          "Staged the shop backup region locally; no market or regional setting was changed upstream.",
        ],
      )
    }
  }
}

fn backup_region_not_found_result(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
) -> MutationFieldResult {
  MutationFieldResult(
    queries.project_selection(
      backup_region_update_source(None, [
        user_error_with_typename(
          "MarketUserError",
          ["region"],
          "Region not found.",
          Some("REGION_NOT_FOUND"),
        ),
      ]),
      field,
      fragments,
    ),
    [],
    store,
    identity,
    [],
    [],
  )
}

fn backup_region_update_source(
  region: Option(BackupRegionRecord),
  errors: List(SourceValue),
) {
  let region_value = case region {
    Some(value) -> queries.backup_region_source(value)
    None -> SrcNull
  }
  src_object([
    #("__typename", SrcString("BackupRegionUpdatePayload")),
    #("backupRegion", region_value),
    #("userErrors", SrcList(errors)),
  ])
}

fn user_error(
  field: List(String),
  message: String,
  code: Option(String),
) -> SourceValue {
  user_error_with_typename("UserError", field, message, code)
}

fn user_error_with_typename(
  typename: String,
  field: List(String),
  message: String,
  code: Option(String),
) -> SourceValue {
  src_object([
    #("__typename", SrcString(typename)),
    #("field", SrcList(list.map(field, SrcString))),
    #("message", SrcString(message)),
    #("code", option_string(code)),
  ])
}

fn option_string(value: Option(String)) -> SourceValue {
  case value {
    Some(value) -> SrcString(value)
    None -> SrcNull
  }
}

fn resource_not_found_error(
  field: Selection,
  document: String,
  id: String,
) -> Json {
  json.object([
    #("message", json.string("Invalid id: " <> id)),
    #(
      "locations",
      json.array(queries.field_locations(field, document), fn(pair) {
        let #(line, column) = pair
        json.object([#("line", json.int(line)), #("column", json.int(column))])
      }),
    ),
    #("path", json.array([get_field_response_key(field)], json.string)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
  ])
}

fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}
