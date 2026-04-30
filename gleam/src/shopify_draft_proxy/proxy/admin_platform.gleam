//// Mirrors the utility subset of `src/proxy/admin-platform.ts`.
////
//// This pass ports the Admin Platform roots that are safe to model without
//// product/customer/order substrate: public API versions, generic null Node
//// fallbacks, Job echo reads, backup region reads/updates, empty taxonomy
//// search/catalog shapes, staff access blockers, and local Flow utility
//// mutations.

import gleam/dict.{type Dict}
import gleam/float
import gleam/int
import gleam/json.{type Json}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shopify_draft_proxy/crypto
import shopify_draft_proxy/graphql/ast.{type Selection, Field, SelectionSet}
import shopify_draft_proxy/graphql/root_field
import shopify_draft_proxy/proxy/apps
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcString,
  default_selected_field_options, get_document_fragments, get_field_response_key,
  get_selected_child_fields, project_graphql_value, serialize_empty_connection,
  src_object,
}
import shopify_draft_proxy/proxy/mutation_helpers.{type LogDraft, LogDraft}
import shopify_draft_proxy/proxy/store_properties
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type BackupRegionRecord, AdminPlatformFlowSignatureRecord,
  AdminPlatformFlowTriggerRecord, BackupRegionRecord,
}

const flow_trigger_payload_limit_bytes = 100_000

const flow_signature_secret = "shopify-draft-proxy-flow-signature-local-secret-v1"

pub type AdminPlatformError {
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

pub fn is_admin_platform_query_root(name: String) -> Bool {
  list.contains(
    [
      "backupRegion",
      "domain",
      "job",
      "node",
      "nodes",
      "publicApiVersions",
      "staffMember",
      "staffMembers",
      "taxonomy",
    ],
    name,
  )
}

pub fn is_admin_platform_mutation_root(name: String) -> Bool {
  list.contains(
    ["backupRegionUpdate", "flowGenerateSignature", "flowTriggerReceive"],
    name,
  )
}

fn captured_backup_region() -> BackupRegionRecord {
  BackupRegionRecord(
    id: "gid://shopify/MarketRegionCountry/4062110417202",
    name: "Canada",
    code: "CA",
  )
}

fn backup_region_for_country(code: String) -> Option(BackupRegionRecord) {
  case string.uppercase(code) {
    "CA" -> Some(captured_backup_region())
    _ -> None
  }
}

fn backup_region_source(region: BackupRegionRecord) -> SourceValue {
  src_object([
    #("__typename", SrcString("MarketRegionCountry")),
    #("id", SrcString(region.id)),
    #("name", SrcString(region.name)),
    #("code", SrcString(region.code)),
  ])
}

fn public_api_versions() -> List(SourceValue) {
  [
    api_version("2025-07", "2025-07", True),
    api_version("2025-10", "2025-10", True),
    api_version("2026-01", "2026-01", True),
    api_version("2026-04", "2026-04 (Latest)", True),
    api_version("2026-07", "2026-07 (Release candidate)", False),
    api_version("unstable", "unstable", False),
  ]
}

fn api_version(handle: String, display_name: String, supported: Bool) {
  src_object([
    #("__typename", SrcString("ApiVersion")),
    #("handle", SrcString(handle)),
    #("displayName", SrcString(display_name)),
    #("supported", SrcBool(supported)),
  ])
}

pub fn process(
  store: Store,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(Json, AdminPlatformError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  let #(data_entries, errors) =
    list.fold(fields, #([], []), fn(acc, field) {
      let #(entries, errs) = acc
      let key = get_field_response_key(field)
      case field {
        Field(name: name, ..) -> {
          let #(value, field_errors) =
            serialize_query_field(
              store,
              field,
              name.value,
              fragments,
              variables,
            )
          #(
            list.append(entries, [#(key, value)]),
            list.append(errs, field_errors),
          )
        }
        _ -> #(entries, errs)
      }
    })
  let data = json.object(data_entries)
  let envelope_entries = case errors {
    [] -> [#("data", data)]
    _ -> [#("data", data), #("errors", json.array(errors, fn(x) { x }))]
  }
  Ok(json.object(envelope_entries))
}

fn serialize_query_field(
  store: Store,
  field: Selection,
  name: String,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> #(Json, List(Json)) {
  case name {
    "publicApiVersions" -> #(
      json.array(public_api_versions(), fn(version) {
        project_selection(version, field, fragments)
      }),
      [],
    )
    "node" -> #(serialize_node(store, field, fragments, variables), [])
    "nodes" -> #(serialize_nodes(store, field, fragments, variables), [])
    "job" -> #(serialize_job(field, fragments, variables), [])
    "domain" -> #(serialize_domain(store, field, fragments, variables), [])
    "backupRegion" -> {
      let region = case store.get_effective_backup_region(store) {
        Some(region) -> region
        None -> captured_backup_region()
      }
      #(project_selection(backup_region_source(region), field, fragments), [])
    }
    "taxonomy" -> #(serialize_taxonomy(field, fragments), [])
    "staffMember" -> #(json.null(), [staff_access_error(field)])
    "staffMembers" -> #(json.null(), [staff_access_error(field)])
    _ -> #(json.null(), [])
  }
}

fn project_selection(
  source: SourceValue,
  field: Selection,
  fragments: FragmentMap,
) -> Json {
  project_graphql_value(source, selection_children(field), fragments)
}

fn selection_children(field: Selection) -> List(Selection) {
  case field {
    Field(selection_set: Some(SelectionSet(selections: selections, ..)), ..) ->
      selections
    _ -> []
  }
}

fn serialize_node(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case dict.get(args, "id") {
    Ok(root_field.StringVal(id)) ->
      serialize_node_by_id(store, id, selection_children(field), fragments)
    _ -> json.null()
  }
}

fn serialize_nodes(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  let ids = case dict.get(args, "ids") {
    Ok(root_field.ListVal(values)) ->
      list.filter_map(values, fn(value) {
        case value {
          root_field.StringVal(id) -> Ok(id)
          _ -> Error(Nil)
        }
      })
    _ -> []
  }
  json.array(ids, fn(id) {
    serialize_node_by_id(store, id, selection_children(field), fragments)
  })
}

fn serialize_node_by_id(
  store: Store,
  id: String,
  selections: List(Selection),
  fragments: FragmentMap,
) -> Json {
  case gid_resource_type(id) {
    "App" -> apps.serialize_app_node_by_id(store, id, selections, fragments)
    "AppInstallation" ->
      apps.serialize_app_installation_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "AppPurchaseOneTime" ->
      apps.serialize_app_one_time_purchase_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "AppSubscription" ->
      apps.serialize_app_subscription_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "AppUsageRecord" ->
      apps.serialize_app_usage_record_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "Shop" ->
      store_properties.serialize_shop_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ShopAddress" ->
      store_properties.serialize_shop_address_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    "ShopPolicy" ->
      store_properties.serialize_shop_policy_node_by_id(
        store,
        id,
        selections,
        fragments,
      )
    _ -> json.null()
  }
}

fn serialize_domain(
  store: Store,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case dict.get(args, "id") {
    Ok(root_field.StringVal(id)) ->
      case store_properties.primary_domain_for_id(store, id) {
        Some(domain) ->
          project_graphql_value(
            store_properties.shop_domain_source(domain),
            selection_children(field),
            fragments,
          )
        None -> json.null()
      }
    _ -> json.null()
  }
}

fn gid_resource_type(id: String) -> String {
  case string.split(id, on: "/") {
    ["gid:", "", "shopify", resource_type, ..] -> resource_type
    _ -> ""
  }
}

fn serialize_job(
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> Json {
  let args = field_args(field, variables)
  case dict.get(args, "id") {
    Ok(root_field.StringVal(id)) ->
      case id {
        "" -> json.null()
        _ -> project_selection(job_source(id), field, fragments)
      }
    _ -> json.null()
  }
}

fn job_source(id: String) -> SourceValue {
  src_object([
    #("__typename", SrcString("Job")),
    #("id", SrcString(id)),
    #("done", SrcBool(True)),
    #("query", src_object([#("__typename", SrcString("QueryRoot"))])),
  ])
}

fn serialize_taxonomy(field: Selection, fragments: FragmentMap) -> Json {
  let source =
    src_object([
      #("__typename", SrcString("Taxonomy")),
      #("categories", SrcNull),
      #("children", SrcNull),
      #("descendants", SrcNull),
      #("siblings", SrcNull),
    ])
  let child_entries =
    list.map(
      get_selected_child_fields(field, default_selected_field_options()),
      fn(child) {
        let key = get_field_response_key(child)
        case child {
          Field(name: name, ..) ->
            case name.value {
              "__typename" -> #(key, json.string("Taxonomy"))
              "categories" | "children" | "descendants" | "siblings" -> #(
                key,
                serialize_empty_connection(
                  child,
                  default_selected_field_options(),
                ),
              )
              _ -> #(key, project_selection(source, child, fragments))
            }
          _ -> #(key, json.null())
        }
      },
    )
  json.object(child_entries)
}

fn staff_access_error(field: Selection) -> Json {
  let path = get_field_response_key(field)
  let message = case path {
    "staffMember" ->
      "Access denied for staffMember field. Required access: `read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app."
    _ -> "Access denied for staffMembers field."
  }
  json.object([
    #("message", json.string(message)),
    #("path", json.array([path], json.string)),
    #(
      "extensions",
      json.object([
        #("code", json.string("ACCESS_DENIED")),
        #(
          "documentation",
          json.string("https://shopify.dev/api/usage/access-scopes"),
        ),
      ]),
    ),
  ])
}

pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, AdminPlatformError) {
  use fields <- result.try(
    root_field.get_root_fields(document)
    |> result.map_error(ParseFailed),
  )
  let fragments = get_document_fragments(document)
  Ok(handle_mutation_fields(store, identity, fields, fragments, variables))
}

fn handle_mutation_fields(
  store: Store,
  identity: SyntheticIdentityRegistry,
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
        staged_resource_ids: staged_ids,
        status: store.Staged,
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
        field,
        fragments,
        variables,
      )
    "flowTriggerReceive" ->
      handle_flow_trigger_receive(store, identity, field, fragments, variables)
    "backupRegionUpdate" ->
      handle_backup_region_update(store, identity, field, fragments, variables)
    _ -> MutationFieldResult(json.null(), [], store, identity, [], [])
  }
}

fn handle_flow_generate_signature(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let args = field_args(field, variables)
  let id = read_string_arg(args, "id")
  let payload = read_string_arg(args, "payload")
  case valid_flow_trigger_id(id) {
    False ->
      MutationFieldResult(
        json.null(),
        [resource_not_found_error(field, id)],
        store,
        identity,
        [],
        [],
      )
    True -> {
      let signature =
        crypto.sha256_hex(flow_signature_secret <> "|" <> id <> "|" <> payload)
      let #(record_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "FlowGenerateSignature")
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
        project_selection(
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
  let args = field_args(field, variables)
  let handle = read_string_arg(args, "handle")
  let payload = case dict.get(args, "payload") {
    Ok(value) -> value
    Error(_) -> root_field.NullVal
  }
  let payload_string = resolved_value_to_string(payload)
  let payload_bytes = string.length(payload_string)
  let user_errors = case payload_bytes > flow_trigger_payload_limit_bytes {
    True -> [
      user_error(
        ["body"],
        "Errors validating schema:\n  Properties size exceeds the limit of "
          <> int.to_string(flow_trigger_payload_limit_bytes)
          <> " bytes.\n",
        None,
      ),
    ]
    False ->
      case is_local_flow_trigger_handle(handle) {
        True -> []
        False -> [
          user_error(
            ["body"],
            "Errors validating schema:\n  Invalid handle '" <> handle <> "'.\n",
            None,
          ),
        ]
      }
  }
  case user_errors {
    [] -> {
      let #(record_id, identity_after_id) =
        synthetic_identity.make_synthetic_gid(identity, "FlowTriggerReceive")
      let #(received_at, identity_after_time) =
        synthetic_identity.make_synthetic_timestamp(identity_after_id)
      let record =
        AdminPlatformFlowTriggerRecord(
          id: record_id,
          handle: handle,
          payload_bytes: payload_bytes,
          payload_sha256: crypto.sha256_hex(payload_string),
          received_at: received_at,
        )
      let #(_, next_store) =
        store.stage_admin_platform_flow_trigger(store, record)
      MutationFieldResult(
        project_selection(flow_trigger_receive_source([]), field, fragments),
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
        project_selection(
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

fn is_local_flow_trigger_handle(handle: String) -> Bool {
  string.starts_with(handle, "local-")
  || string.starts_with(handle, "har-374-local")
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
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
) -> MutationFieldResult {
  let args = field_args(field, variables)
  let code = case dict.get(args, "region") {
    Ok(root_field.ObjectVal(region)) -> read_string_arg(region, "countryCode")
    _ -> ""
  }
  case backup_region_for_country(code) {
    None ->
      MutationFieldResult(
        project_selection(
          backup_region_update_source(None, [
            user_error(
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
    Some(region) -> {
      let #(_, next_store) = store.stage_backup_region(store, region)
      MutationFieldResult(
        project_selection(
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

fn backup_region_update_source(
  region: Option(BackupRegionRecord),
  errors: List(SourceValue),
) {
  let region_value = case region {
    Some(value) -> backup_region_source(value)
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
  src_object([
    #("__typename", SrcString("UserError")),
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

fn resource_not_found_error(field: Selection, id: String) -> Json {
  json.object([
    #("message", json.string("Invalid id: " <> id)),
    #("path", json.array([get_field_response_key(field)], json.string)),
    #("extensions", json.object([#("code", json.string("RESOURCE_NOT_FOUND"))])),
  ])
}

fn field_args(
  field: Selection,
  variables: Dict(String, root_field.ResolvedValue),
) -> Dict(String, root_field.ResolvedValue) {
  case root_field.get_field_arguments(field, variables) {
    Ok(args) -> args
    Error(_) -> dict.new()
  }
}

fn read_string_arg(
  args: Dict(String, root_field.ResolvedValue),
  name: String,
) -> String {
  case dict.get(args, name) {
    Ok(root_field.StringVal(value)) -> value
    _ -> ""
  }
}

fn resolved_value_to_string(value: root_field.ResolvedValue) -> String {
  case value {
    root_field.NullVal -> "null"
    root_field.StringVal(value) -> "\"" <> value <> "\""
    root_field.BoolVal(value) ->
      case value {
        True -> "true"
        False -> "false"
      }
    root_field.IntVal(value) -> int.to_string(value)
    root_field.FloatVal(value) -> float.to_string(value)
    root_field.ListVal(values) ->
      "[" <> string.join(list.map(values, resolved_value_to_string), ",") <> "]"
    root_field.ObjectVal(fields) -> {
      let entries =
        dict.to_list(fields)
        |> list.map(fn(pair) {
          let #(key, child) = pair
          "\"" <> key <> "\":" <> resolved_value_to_string(child)
        })
      "{" <> string.join(entries, ",") <> "}"
    }
  }
}

fn mutation_root_names(fields: List(Selection)) -> List(String) {
  list.filter_map(fields, fn(field) {
    case field {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}
