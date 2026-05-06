//// Mutation handling for marketing roots.

import gleam/dict.{type Dict}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}

import shopify_draft_proxy/graphql/ast.{type Selection, Field}
import shopify_draft_proxy/graphql/root_field

import shopify_draft_proxy/proxy/app_identity
import shopify_draft_proxy/proxy/graphql_helpers.{
  type FragmentMap, type SourceValue, SrcBool, SrcList, SrcNull, SrcString,
  get_document_fragments, get_field_response_key, src_object,
}
import shopify_draft_proxy/proxy/marketing/serializers
import shopify_draft_proxy/proxy/marketing/types as marketing_types
import shopify_draft_proxy/proxy/mutation_helpers.{
  type MutationOutcome, LogDraft, MutationOutcome,
}
import shopify_draft_proxy/proxy/upstream_query.{type UpstreamContext}
import shopify_draft_proxy/state/store.{type Store}
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/synthetic_identity.{
  type SyntheticIdentityRegistry,
}
import shopify_draft_proxy/state/types.{
  type MarketingEngagementRecord, type MarketingRecord, type MarketingValue,
  MarketingBool, MarketingEngagementRecord, MarketingInt, MarketingList,
  MarketingNull, MarketingObject, MarketingRecord, MarketingString,
}

@internal
pub fn is_marketing_mutation_root(name: String) -> Bool {
  marketing_types.is_marketing_mutation_root(name)
}

type UserError {
  UserError(field: Option(List(String)), message: String, code: Option(String))
}

type EngagementIdentifier {
  ActivityIdentifier(value: String, activity: MarketingRecord)
  RemoteIdentifier(value: String, activity: MarketingRecord)
  ChannelIdentifier(value: String)
}

type MutationFieldResult {
  MutationFieldResult(
    key: String,
    payload: SourceValue,
    staged_resource_ids: List(String),
    should_log: Bool,
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
        app_identity.read_requesting_api_client_id(upstream.headers),
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
  requesting_api_client_id: Option(String),
) -> MutationOutcome {
  let initial = #([], store, identity, [], False)
  let #(entries, final_store, final_identity, staged_ids, should_log) =
    list.fold(fields, initial, fn(acc, field) {
      let #(entries, current_store, current_identity, staged_ids, should_log) =
        acc
      case field {
        Field(name: name, ..) ->
          case marketing_types.is_marketing_mutation_root(name.value) {
            False -> acc
            True -> {
              let #(result, next_store, next_identity) =
                handle_marketing_mutation_root(
                  current_store,
                  current_identity,
                  field,
                  fragments,
                  variables,
                  requesting_api_client_id,
                )
              #(
                list.append(entries, [
                  #(
                    result.key,
                    serializers.project_payload(
                      result.payload,
                      field,
                      fragments,
                    ),
                  ),
                ]),
                next_store,
                next_identity,
                list.append(staged_ids, result.staged_resource_ids),
                should_log || result.should_log,
              )
            }
          }
        _ -> acc
      }
    })

  let root_names = serializers.mutation_root_names(fields)
  let final_ids = serializers.dedupe_strings(staged_ids)
  let primary_root = case list.first(root_names) {
    Ok(name) -> Some(name)
    Error(_) -> None
  }
  let log_drafts = case should_log {
    False -> []
    True -> [
      LogDraft(
        operation_name: primary_root,
        root_fields: root_names,
        primary_root_field: primary_root,
        domain: "marketing",
        execution: "stage-locally",
        query: None,
        variables: None,
        staged_resource_ids: final_ids,
        status: store_types.Staged,
        notes: Some("Staged locally in the in-memory marketing draft store."),
      ),
    ]
  }
  MutationOutcome(
    data: json.object([#("data", json.object(entries))]),
    store: final_store,
    identity: final_identity,
    staged_resource_ids: final_ids,
    log_drafts: log_drafts,
  )
}

fn handle_marketing_mutation_root(
  store: Store,
  identity: SyntheticIdentityRegistry,
  field: Selection,
  fragments: FragmentMap,
  variables: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let _ = fragments
  let key = get_field_response_key(field)
  let args = graphql_helpers.field_args(field, variables)
  case field {
    Field(name: name, ..) ->
      case name.value {
        "marketingActivityCreate" ->
          marketing_activity_create(store, identity, key, args)
        "marketingActivityUpdate" ->
          marketing_activity_update(store, identity, key, args)
        "marketingActivityCreateExternal" ->
          marketing_activity_create_external(
            store,
            identity,
            key,
            args,
            requesting_api_client_id,
          )
        "marketingActivityUpdateExternal" ->
          marketing_activity_update_external(store, identity, key, args)
        "marketingActivityUpsertExternal" ->
          marketing_activity_upsert_external(
            store,
            identity,
            key,
            args,
            requesting_api_client_id,
          )
        "marketingActivityDeleteExternal" ->
          marketing_activity_delete_external(store, identity, key, args)
        "marketingActivitiesDeleteAllExternal" ->
          marketing_activities_delete_all_external(store, identity, key)
        "marketingEngagementCreate" ->
          marketing_engagement_create(store, identity, key, args)
        "marketingEngagementsDelete" ->
          marketing_engagements_delete(store, identity, key, args)
        _ -> #(
          MutationFieldResult(key, src_object([]), [], False),
          store,
          identity,
        )
      }
    _ -> #(MutationFieldResult(key, src_object([]), [], False), store, identity)
  }
}

fn marketing_activity_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case
    serializers.is_known_local_marketing_activity_extension(
      serializers.read_value_string(input, "marketingActivityExtensionId"),
    )
  {
    False -> #(
      MutationFieldResult(
        key,
        src_object([
          #(
            "userErrors",
            user_errors_source([missing_marketing_extension_error()]),
          ),
        ]),
        [],
        False,
      ),
      store,
      identity,
    )
    True -> {
      let #(activity, next_identity) =
        build_native_marketing_activity_from_create_input(identity, input)
      let #(staged, next_store) =
        store.stage_marketing_activity(store, activity)
      #(
        MutationFieldResult(
          key,
          src_object([#("userErrors", user_errors_source([]))]),
          [staged.id],
          True,
        ),
        next_store,
        next_identity,
      )
    }
  }
}

fn marketing_activity_update(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case serializers.read_value_string(input, "id") {
    Some(id) ->
      case store.get_effective_marketing_activity_record_by_id(store, id) {
        None -> marketing_missing_activity_result(key, store, identity)
        Some(activity) -> {
          let #(updated, next_identity) =
            apply_native_marketing_activity_update(identity, activity, input)
          let #(staged, next_store) =
            store.stage_marketing_activity(store, updated)
          #(
            MutationFieldResult(
              key,
              src_object([
                #(
                  "marketingActivity",
                  serializers.marketing_data_to_source(staged.data),
                ),
                #("redirectPath", SrcNull),
                #("userErrors", user_errors_source([])),
              ]),
              [staged.id],
              True,
            ),
            next_store,
            next_identity,
          )
        }
      }
    None -> marketing_missing_activity_result(key, store, identity)
  }
}

fn marketing_activity_create_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case store.has_marketing_delete_all_external_in_flight(store) {
    True ->
      validation_result(
        key,
        "marketingActivityCreateExternal",
        [delete_job_enqueued_error()],
        store,
        identity,
      )
    False ->
      case serializers.has_attribution(input) {
        False ->
          validation_result(
            key,
            "marketingActivityCreateExternal",
            [non_hierarchical_utm_error()],
            store,
            identity,
          )
        True ->
          case
            validate_external_activity_create_input(
              store,
              input,
              requesting_api_client_id,
            )
          {
            Some(error) ->
              validation_result(
                key,
                "marketingActivityCreateExternal",
                [error],
                store,
                identity,
              )
            None ->
              create_external_activity_success(store, identity, key, input)
          }
      }
  }
}

fn marketing_activity_update_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  let has_remote_id = dict.has_key(args, "remoteId")
  let has_marketing_activity_id = dict.has_key(args, "marketingActivityId")
  let has_utm_selector = dict.has_key(args, "utm")
  let selector_utm =
    serializers.read_utm(
      graphql_helpers.read_arg_object(args, "utm") |> option.unwrap(dict.new()),
    )
  case store.has_marketing_delete_all_external_in_flight(store) {
    True ->
      validation_result(
        key,
        "marketingActivityUpdateExternal",
        [delete_job_enqueued_error()],
        store,
        identity,
      )
    False ->
      case has_remote_id || has_marketing_activity_id || has_utm_selector {
        False ->
          validation_result(
            key,
            "marketingActivityUpdateExternal",
            [invalid_marketing_activity_external_arguments_error()],
            store,
            identity,
          )
        True -> {
          let activity = case
            graphql_helpers.read_arg_string_nonempty(args, "remoteId")
          {
            Some(remote_id) ->
              store.get_effective_marketing_activity_by_remote_id(
                store,
                remote_id,
              )
            None ->
              case
                graphql_helpers.read_arg_string_nonempty(
                  args,
                  "marketingActivityId",
                )
              {
                Some(id) ->
                  store.get_effective_marketing_activity_record_by_id(store, id)
                None ->
                  serializers.find_marketing_activity_by_utm(
                    store,
                    selector_utm,
                  )
              }
          }
          case activity {
            None ->
              validation_result(
                key,
                "marketingActivityUpdateExternal",
                [marketing_activity_missing_error()],
                store,
                identity,
              )
            Some(activity) -> {
              let requested_utm =
                graphql_helpers.read_arg_object(args, "utm")
                |> option.unwrap(dict.new())
              case
                validate_external_activity_update(
                  store,
                  activity,
                  input,
                  serializers.read_utm(requested_utm),
                  !dict.is_empty(requested_utm),
                )
              {
                Some(error) ->
                  validation_result(
                    key,
                    "marketingActivityUpdateExternal",
                    [error],
                    store,
                    identity,
                  )
                None ->
                  update_external_activity_success(
                    store,
                    identity,
                    key,
                    activity,
                    input,
                  )
              }
            }
          }
        }
      }
  }
}

fn marketing_activity_upsert_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "input") |> option.unwrap(dict.new())
  case store.has_marketing_delete_all_external_in_flight(store) {
    True ->
      validation_result(
        key,
        "marketingActivityUpsertExternal",
        [delete_job_enqueued_error()],
        store,
        identity,
      )
    False -> {
      let existing = case serializers.read_value_string(input, "remoteId") {
        Some(remote_id) ->
          store.get_effective_marketing_activity_by_remote_id(store, remote_id)
        None -> None
      }
      case existing {
        None ->
          case serializers.has_attribution(input) {
            False ->
              validation_result(
                key,
                "marketingActivityUpsertExternal",
                [non_hierarchical_utm_error()],
                store,
                identity,
              )
            True ->
              case
                validate_external_activity_create_input(
                  store,
                  input,
                  requesting_api_client_id,
                )
              {
                Some(error) ->
                  validation_result(
                    key,
                    "marketingActivityUpsertExternal",
                    [error],
                    store,
                    identity,
                  )
                None ->
                  create_external_activity_success(store, identity, key, input)
              }
          }
        Some(activity) ->
          case
            validate_external_activity_update(
              store,
              activity,
              input,
              serializers.read_utm(input),
              True,
            )
          {
            Some(error) ->
              validation_result(
                key,
                "marketingActivityUpsertExternal",
                [error],
                store,
                identity,
              )
            None ->
              update_external_activity_success(
                store,
                identity,
                key,
                activity,
                input,
              )
          }
      }
    }
  }
}

fn validate_external_activity_create_input(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Option(UserError) {
  case
    validate_external_activity_channel_handle(
      store,
      input,
      requesting_api_client_id,
    )
  {
    Some(error) -> Some(error)
    None ->
      case validate_external_activity_currency(input) {
        Some(error) -> Some(error)
        None -> validate_external_activity_uniqueness(store, input)
      }
  }
}

fn validate_external_activity_channel_handle(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
  requesting_api_client_id: Option(String),
) -> Option(UserError) {
  case serializers.read_value_string(input, "channelHandle") {
    None -> None
    Some(handle) ->
      case
        store.has_known_marketing_channel_handle_for_app(
          store,
          handle,
          requesting_api_client_id,
        )
      {
        True -> None
        False -> Some(invalid_channel_handle_input_error())
      }
  }
}

fn validate_external_activity_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(UserError) {
  case budget_total_currency(input), money_input_currency(input, "adSpend") {
    Some(budget_currency), Some(ad_spend_currency)
      if budget_currency != ad_spend_currency
    -> Some(activity_currency_mismatch_error())
    _, _ -> None
  }
}

fn validate_external_activity_uniqueness(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(UserError) {
  case serializers.read_value_string(input, "remoteId") {
    Some(remote_id) ->
      case
        store.get_effective_marketing_activity_by_remote_id(store, remote_id)
      {
        Some(_) -> Some(duplicate_remote_id_error())
        None -> validate_external_activity_utm_uniqueness(store, input)
      }
    None -> validate_external_activity_utm_uniqueness(store, input)
  }
}

fn validate_external_activity_utm_uniqueness(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(UserError) {
  case
    serializers.find_marketing_activity_by_utm(
      store,
      serializers.read_utm(input),
    )
  {
    Some(_) -> Some(duplicate_utm_campaign_error())
    None -> validate_external_activity_url_parameter_uniqueness(store, input)
  }
}

fn validate_external_activity_url_parameter_uniqueness(
  store: Store,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(UserError) {
  case serializers.read_value_string(input, "urlParameterValue") {
    Some(value) ->
      case find_marketing_activity_by_url_parameter_value(store, value) {
        Some(_) -> Some(duplicate_url_parameter_value_error())
        None -> None
      }
    None -> None
  }
}

fn find_marketing_activity_by_url_parameter_value(
  store: Store,
  value: String,
) -> Option(MarketingRecord) {
  list.find(store.list_effective_marketing_activities(store), fn(activity) {
    serializers.read_marketing_string(activity.data, "urlParameterValue")
    == Some(value)
  })
  |> option.from_result
}

fn validate_external_activity_update(
  store: Store,
  activity: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
  requested_utm: Option(Dict(String, MarketingValue)),
  validate_utm: Bool,
) -> Option(UserError) {
  case serializers.read_marketing_bool(activity.data, "isExternal") {
    False -> Some(activity_not_external_error())
    True ->
      case serializers.read_marketing_object(activity.data, "marketingEvent") {
        None -> Some(marketing_event_does_not_exist_error())
        Some(event) ->
          validate_external_activity_event_update(
            store,
            activity,
            event,
            input,
            requested_utm,
            validate_utm,
          )
      }
  }
}

fn validate_external_activity_event_update(
  store: Store,
  activity: MarketingRecord,
  event: Dict(String, MarketingValue),
  input: Dict(String, root_field.ResolvedValue),
  requested_utm: Option(Dict(String, MarketingValue)),
  validate_utm: Bool,
) -> Option(UserError) {
  case
    supplied_string_differs(
      input,
      "channelHandle",
      serializers.read_marketing_object_string(Some(event), "channelHandle"),
    )
  {
    True -> Some(immutable_channel_handle_error())
    False ->
      case
        supplied_string_differs(
          input,
          "urlParameterValue",
          serializers.read_marketing_string(activity.data, "urlParameterValue"),
        )
      {
        True -> Some(immutable_url_parameter_error())
        False ->
          case
            validate_utm
            && !serializers.same_utm(
              serializers.read_marketing_object(activity.data, "utmParameters"),
              requested_utm,
            )
          {
            True -> Some(immutable_utm_error())
            False ->
              validate_external_activity_parent_and_hierarchy(
                store,
                activity,
                input,
              )
          }
      }
  }
}

fn validate_external_activity_parent_and_hierarchy(
  store: Store,
  activity: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(UserError) {
  let existing_parent_remote_id =
    serializers.read_marketing_string(activity.data, "parentRemoteId")
  case dict.has_key(input, "parentRemoteId") {
    True ->
      case serializers.read_value_string(input, "parentRemoteId") {
        Some(parent_remote_id) ->
          case
            serializers.find_marketing_event_by_remote_id(
              store,
              parent_remote_id,
            )
          {
            None -> Some(invalid_remote_id_error())
            Some(_) ->
              case existing_parent_remote_id == Some(parent_remote_id) {
                True -> validate_external_activity_hierarchy(activity, input)
                False -> Some(immutable_parent_id_error())
              }
          }
        None ->
          case existing_parent_remote_id {
            None -> validate_external_activity_hierarchy(activity, input)
            Some(_) -> Some(immutable_parent_id_error())
          }
      }
    False -> validate_external_activity_hierarchy(activity, input)
  }
}

fn validate_external_activity_hierarchy(
  activity: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> Option(UserError) {
  case
    supplied_string_differs(
      input,
      "hierarchyLevel",
      serializers.read_marketing_string(activity.data, "hierarchyLevel"),
    )
  {
    True -> Some(immutable_hierarchy_level_error())
    False -> None
  }
}

fn supplied_string_differs(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
  existing: Option(String),
) -> Bool {
  dict.has_key(input, field)
  && serializers.read_value_string(input, field) != existing
}

fn marketing_activity_delete_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let remote_id = graphql_helpers.read_arg_string_nonempty(args, "remoteId")
  let marketing_activity_id =
    graphql_helpers.read_arg_string_nonempty(args, "marketingActivityId")
  case remote_id, marketing_activity_id {
    None, None ->
      validation_result(
        key,
        "marketingActivityDeleteExternal",
        [invalid_delete_activity_external_arguments_error()],
        store,
        identity,
      )
    _, _ -> {
      let activity = case remote_id {
        Some(remote_id) ->
          store.get_effective_marketing_activity_by_remote_id(store, remote_id)
        None ->
          case marketing_activity_id {
            Some(id) ->
              store.get_effective_marketing_activity_record_by_id(store, id)
            None -> None
          }
      }
      case activity {
        None ->
          validation_result(
            key,
            "marketingActivityDeleteExternal",
            [marketing_activity_missing_error()],
            store,
            identity,
          )
        Some(activity) ->
          case validate_external_activity_delete(store, activity) {
            Some(error) ->
              validation_result(
                key,
                "marketingActivityDeleteExternal",
                [error],
                store,
                identity,
              )
            None -> {
              let next_store =
                store.stage_delete_marketing_activity(store, activity.id)
              #(
                MutationFieldResult(
                  key,
                  src_object([
                    #("deletedMarketingActivityId", SrcString(activity.id)),
                    #("userErrors", user_errors_source([])),
                  ]),
                  [activity.id],
                  True,
                ),
                next_store,
                identity,
              )
            }
          }
      }
    }
  }
}

fn validate_external_activity_delete(
  store: Store,
  activity: MarketingRecord,
) -> Option(UserError) {
  case serializers.read_marketing_bool(activity.data, "isExternal") {
    False -> Some(activity_not_external_error())
    True ->
      case has_child_marketing_events(store, activity) {
        True -> Some(cannot_delete_activity_with_child_events_error())
        False -> None
      }
  }
}

fn has_child_marketing_events(store: Store, activity: MarketingRecord) -> Bool {
  case nested_marketing_event_has_children(activity) {
    True -> True
    False ->
      store.list_effective_marketing_activities(store)
      |> list.any(fn(candidate) {
        candidate.id != activity.id
        && references_parent_activity(candidate, activity)
      })
  }
}

fn nested_marketing_event_has_children(activity: MarketingRecord) -> Bool {
  case serializers.read_marketing_object(activity.data, "marketingEvent") {
    None -> False
    Some(event) -> marketing_child_events_value_has_items(event)
  }
}

fn marketing_child_events_value_has_items(
  event: Dict(String, MarketingValue),
) -> Bool {
  case dict.get(event, "childEvents") {
    Ok(MarketingList(values)) -> !list.is_empty(values)
    Ok(MarketingObject(fields)) ->
      marketing_child_events_connection_has_items(fields)
    _ -> False
  }
}

fn marketing_child_events_connection_has_items(
  fields: Dict(String, MarketingValue),
) -> Bool {
  case dict.get(fields, "nodes") {
    Ok(MarketingList(nodes)) -> !list.is_empty(nodes)
    _ ->
      case dict.get(fields, "edges") {
        Ok(MarketingList(edges)) -> !list.is_empty(edges)
        _ -> False
      }
  }
}

fn references_parent_activity(
  candidate: MarketingRecord,
  parent: MarketingRecord,
) -> Bool {
  serializers.read_marketing_string(candidate.data, "parentActivityId")
  == Some(parent.id)
  || case serializers.marketing_remote_id(parent.data) {
    Some(remote_id) ->
      serializers.read_marketing_string(candidate.data, "parentRemoteId")
      == Some(remote_id)
    None -> False
  }
}

fn marketing_activities_delete_all_external(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(deleted_ids, next_store) =
    store.stage_delete_all_external_marketing_activities(store)
  let #(job_id, next_identity) =
    synthetic_identity.make_synthetic_gid(identity, "Job")
  #(
    MutationFieldResult(
      key,
      src_object([
        #(
          "job",
          src_object([
            #("__typename", SrcString("Job")),
            #("id", SrcString(job_id)),
            #("done", SrcBool(False)),
          ]),
        ),
        #("userErrors", user_errors_source([])),
      ]),
      [job_id, ..deleted_ids],
      True,
    ),
    next_store,
    next_identity,
  )
}

fn marketing_engagement_create(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let input =
    graphql_helpers.read_arg_object(args, "marketingEngagement")
    |> option.unwrap(dict.new())
  case validate_engagement_input_currency(input) {
    Error(user_error) ->
      validation_result(
        key,
        "marketingEngagementCreate",
        [user_error],
        store,
        identity,
      )
    Ok(engagement_currency_code) ->
      case resolve_marketing_engagement_identifier(store, args) {
        Error(user_error) ->
          validation_result(
            key,
            "marketingEngagementCreate",
            [user_error],
            store,
            identity,
          )
        Ok(identifier) ->
          case
            validate_engagement_activity_currency(
              identifier,
              engagement_currency_code,
            )
          {
            Error(user_error) ->
              validation_result(
                key,
                "marketingEngagementCreate",
                [user_error],
                store,
                identity,
              )
            Ok(Nil) -> {
              let engagement =
                build_marketing_engagement_record(identifier, input)
              let #(staged, next_store) =
                store.stage_marketing_engagement(store, engagement)
              #(
                MutationFieldResult(
                  key,
                  src_object([
                    #(
                      "marketingEngagement",
                      serializers.marketing_data_to_source(staged.data),
                    ),
                    #("userErrors", user_errors_source([])),
                  ]),
                  [staged.id],
                  True,
                ),
                next_store,
                identity,
              )
            }
          }
      }
  }
}

fn marketing_engagements_delete(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  args: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let channel_handle =
    graphql_helpers.read_arg_string_nonempty(args, "channelHandle")
  let delete_all =
    option.unwrap(
      graphql_helpers.read_arg_bool(args, "deleteEngagementsForAllChannels"),
      False,
    )
  case channel_handle, delete_all {
    Some(_), True ->
      validation_result(
        key,
        "marketingEngagementsDelete",
        [invalid_delete_engagements_arguments_error()],
        store,
        identity,
      )
    None, False ->
      validation_result(
        key,
        "marketingEngagementsDelete",
        [invalid_delete_engagements_arguments_error()],
        store,
        identity,
      )
    Some(handle), False ->
      case store.has_known_marketing_channel_handle(store, handle) {
        False ->
          validation_result(
            key,
            "marketingEngagementsDelete",
            [invalid_channel_handle_error()],
            store,
            identity,
          )
        True -> {
          let #(deleted_ids, next_store) =
            store.stage_delete_marketing_engagements_by_channel_handle(
              store,
              handle,
            )
          #(
            MutationFieldResult(
              key,
              src_object([
                #(
                  "result",
                  SrcString(
                    "Engagement data marked for deletion for 1 channel(s)",
                  ),
                ),
                #("userErrors", user_errors_source([])),
              ]),
              deleted_ids,
              True,
            ),
            next_store,
            identity,
          )
        }
      }
    None, True -> {
      let channel_count =
        store.list_effective_marketing_engagements(store)
        |> list.filter_map(fn(engagement) {
          case engagement.channel_handle {
            Some(handle) -> Ok(handle)
            None -> Error(Nil)
          }
        })
        |> serializers.dedupe_strings
        |> list.length
      let #(deleted_ids, next_store) =
        store.stage_delete_all_channel_marketing_engagements(store)
      #(
        MutationFieldResult(
          key,
          src_object([
            #(
              "result",
              SrcString(
                "Engagement data marked for deletion for "
                <> int.to_string(channel_count)
                <> " channel(s)",
              ),
            ),
            #("userErrors", user_errors_source([])),
          ]),
          deleted_ids,
          True,
        ),
        next_store,
        identity,
      )
    }
  }
}

fn create_external_activity_success(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(activity, event, next_identity) =
    build_marketing_records_from_create_input(identity, input)
  let #(_, next_store) = store.stage_marketing_event(store, event)
  let #(staged_activity, next_store) =
    store.stage_marketing_activity(next_store, activity)
  #(
    MutationFieldResult(
      key,
      src_object([
        #(
          "marketingActivity",
          serializers.marketing_data_to_source(staged_activity.data),
        ),
        #("userErrors", user_errors_source([])),
      ]),
      [staged_activity.id, event.id],
      True,
    ),
    next_store,
    next_identity,
  )
}

fn update_external_activity_success(
  store: Store,
  identity: SyntheticIdentityRegistry,
  key: String,
  activity: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  let #(updated_activity, event, next_identity) =
    apply_external_activity_update(identity, activity, input)
  let #(_, next_store) = store.stage_marketing_event(store, event)
  let #(staged_activity, next_store) =
    store.stage_marketing_activity(next_store, updated_activity)
  #(
    MutationFieldResult(
      key,
      src_object([
        #(
          "marketingActivity",
          serializers.marketing_data_to_source(staged_activity.data),
        ),
        #("userErrors", user_errors_source([])),
      ]),
      [staged_activity.id, event.id],
      True,
    ),
    next_store,
    next_identity,
  )
}

fn marketing_missing_activity_result(
  key: String,
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key,
      src_object([
        #("marketingActivity", SrcNull),
        #("redirectPath", SrcNull),
        #(
          "userErrors",
          user_errors_source([marketing_activity_missing_error()]),
        ),
      ]),
      [],
      False,
    ),
    store,
    identity,
  )
}

fn validation_result(
  key: String,
  root_field: String,
  user_errors: List(UserError),
  store: Store,
  identity: SyntheticIdentityRegistry,
) -> #(MutationFieldResult, Store, SyntheticIdentityRegistry) {
  #(
    MutationFieldResult(
      key,
      marketing_validation_payload(root_field, user_errors),
      [],
      False,
    ),
    store,
    identity,
  )
}

fn marketing_validation_payload(
  root_field: String,
  user_errors: List(UserError),
) -> SourceValue {
  case root_field {
    "marketingEngagementCreate" ->
      src_object([
        #("marketingEngagement", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    "marketingEngagementsDelete" ->
      src_object([
        #("result", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    "marketingActivityDeleteExternal" ->
      src_object([
        #("deletedMarketingActivityId", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    "marketingActivitiesDeleteAllExternal" ->
      src_object([
        #("job", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
    _ ->
      src_object([
        #("marketingActivity", SrcNull),
        #("userErrors", user_errors_source(user_errors)),
      ])
  }
}

fn user_errors_source(user_errors: List(UserError)) -> SourceValue {
  SrcList(list.map(user_errors, user_error_source))
}

fn user_error_source(user_error: UserError) -> SourceValue {
  src_object([
    #("field", serializers.optional_string_list_source(user_error.field)),
    #("message", SrcString(user_error.message)),
    #("code", graphql_helpers.option_string_source(user_error.code)),
  ])
}

fn non_hierarchical_utm_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.",
    code: Some("NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER"),
  )
}

fn marketing_activity_missing_error() -> UserError {
  UserError(
    field: None,
    message: "Marketing activity does not exist.",
    code: Some("MARKETING_ACTIVITY_DOES_NOT_EXIST"),
  )
}

fn invalid_marketing_activity_external_arguments_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Either marketing activity ID, remote ID, or UTM parameters must be provided.",
    code: Some("INVALID_MARKETING_ACTIVITY_EXTERNAL_ARGUMENTS"),
  )
}

fn invalid_delete_activity_external_arguments_error() -> UserError {
  UserError(
    field: None,
    message: "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.",
    code: Some("INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"),
  )
}

fn activity_not_external_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Marketing activity is not external.",
    code: Some("ACTIVITY_NOT_EXTERNAL"),
  )
}

fn cannot_delete_activity_with_child_events_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Cannot delete a marketing activity with child events.",
    code: Some("CANNOT_DELETE_ACTIVITY_WITH_CHILD_EVENTS"),
  )
}

fn delete_job_enqueued_error() -> UserError {
  UserError(
    field: None,
    message: "Cannot perform this operation because a job to delete all external activities has been enqueued, which happens either from calling the marketingActivitiesDeleteAllExternal mutation or as a result of an app uninstall. Please either check the status of the job returned by the mutation or try again later.",
    code: Some("DELETE_JOB_ENQUEUED"),
  )
}

fn marketing_event_does_not_exist_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Marketing event does not exist.",
    code: Some("MARKETING_EVENT_DOES_NOT_EXIST"),
  )
}

fn immutable_channel_handle_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Channel handle cannot be modified.",
    code: Some("IMMUTABLE_CHANNEL_HANDLE"),
  )
}

fn immutable_url_parameter_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "URL parameter value cannot be modified.",
    code: Some("IMMUTABLE_URL_PARAMETER"),
  )
}

fn immutable_utm_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "UTM parameters cannot be modified.",
    code: Some("IMMUTABLE_UTM_PARAMETERS"),
  )
}

fn invalid_remote_id_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Remote ID does not correspond to an activity.",
    code: Some("INVALID_REMOTE_ID"),
  )
}

fn immutable_parent_id_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Parent marketing activity cannot be modified.",
    code: Some("IMMUTABLE_PARENT_ID"),
  )
}

fn immutable_hierarchy_level_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Hierarchy level cannot be modified.",
    code: Some("IMMUTABLE_HIERARCHY_LEVEL"),
  )
}

fn activity_currency_mismatch_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Currency code is not matching between budget and ad spend",
    code: None,
  )
}

fn duplicate_remote_id_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Validation failed: Remote ID has already been taken",
    code: None,
  )
}

fn duplicate_utm_campaign_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Validation failed: Utm campaign has already been taken",
    code: None,
  )
}

fn duplicate_url_parameter_value_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "Validation failed: Url parameter value has already been taken",
    code: None,
  )
}

fn missing_marketing_extension_error() -> UserError {
  UserError(
    field: Some(["input", "marketingActivityExtensionId"]),
    message: "Could not find the marketing extension",
    code: None,
  )
}

fn engagement_missing_identifier_error() -> UserError {
  UserError(
    field: None,
    message: "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
    code: Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"),
  )
}

fn engagement_invalid_identifier_error() -> UserError {
  UserError(
    field: None,
    message: "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
    code: Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"),
  )
}

fn invalid_channel_handle_error() -> UserError {
  UserError(
    field: Some(["channelHandle"]),
    message: "The channel handle is not recognized. Please contact your partner manager for more information.",
    code: Some("INVALID_CHANNEL_HANDLE"),
  )
}

fn invalid_channel_handle_input_error() -> UserError {
  UserError(
    field: Some(["input"]),
    message: "The channel handle is not recognized. Please contact your partner manager for more information.",
    code: Some("INVALID_CHANNEL_HANDLE"),
  )
}

fn invalid_delete_engagements_arguments_error() -> UserError {
  UserError(
    field: None,
    message: "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.",
    code: Some("INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"),
  )
}

fn currency_code_mismatch_input_error() -> UserError {
  UserError(
    field: Some(["marketingEngagement"]),
    message: "Currency codes in the marketing engagement input do not match.",
    code: Some("CURRENCY_CODE_MISMATCH_INPUT"),
  )
}

fn marketing_activity_currency_code_mismatch_error() -> UserError {
  UserError(
    field: Some(["marketingEngagement"]),
    message: "Marketing activity currency code does not match the currency code in the marketing engagement input.",
    code: Some("MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"),
  )
}

// ===========================================================================
// Record builders
// ===========================================================================

fn build_marketing_records_from_create_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, MarketingRecord, SyntheticIdentityRegistry) {
  let #(activity_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "MarketingActivity")
  let #(event_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "MarketingEvent")
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let title = option.unwrap(serializers.read_value_string(input, "title"), "")
  let remote_id = serializers.read_value_string(input, "remoteId")
  let status =
    option.unwrap(serializers.read_value_string(input, "status"), "UNDEFINED")
  let tactic =
    option.unwrap(serializers.read_value_string(input, "tactic"), "NEWSLETTER")
  let channel_type =
    option.unwrap(
      serializers.read_value_string(input, "marketingChannelType"),
      "EMAIL",
    )
  let source_medium = serializers.source_and_medium(channel_type, tactic)
  let utm = serializers.read_utm(input)
  let started_at =
    option.unwrap(
      serializers.read_value_string(input, "start")
        |> option.or(serializers.read_value_string(input, "scheduledStart")),
      timestamp,
    )
  let ended_at =
    serializers.read_value_string(input, "end")
    |> option.or(serializers.event_ended_at_for_status(status, timestamp))
  let event_data =
    dict.from_list([
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(event_id)),
      #(
        "legacyResourceId",
        MarketingInt(option.unwrap(serializers.id_number(event_id), 0)),
      ),
      #("type", MarketingString(tactic)),
      #("remoteId", serializers.optional_marketing_string(remote_id)),
      #("startedAt", MarketingString(started_at)),
      #("endedAt", serializers.optional_marketing_string(ended_at)),
      #(
        "scheduledToEndAt",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "scheduledEnd",
        )),
      ),
      #(
        "manageUrl",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "remoteUrl",
        )),
      ),
      #(
        "previewUrl",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "remotePreviewImageUrl",
        )),
      ),
      #(
        "utmCampaign",
        serializers.optional_marketing_string(
          serializers.read_marketing_object_string(utm, "campaign"),
        ),
      ),
      #(
        "utmMedium",
        serializers.optional_marketing_string(
          serializers.read_marketing_object_string(utm, "medium"),
        ),
      ),
      #(
        "utmSource",
        serializers.optional_marketing_string(
          serializers.read_marketing_object_string(utm, "source"),
        ),
      ),
      #("description", MarketingString(title)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #(
        "channelHandle",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "channelHandle",
        )),
      ),
    ])
  let activity_data =
    dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(activity_id)),
      #("title", MarketingString(title)),
      #("createdAt", MarketingString(timestamp)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(serializers.status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #("isExternal", MarketingBool(True)),
      #("inMainWorkflowVersion", MarketingBool(False)),
      #(
        "urlParameterValue",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "urlParameterValue",
        )),
      ),
      #(
        "parentActivityId",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "parentActivityId",
        )),
      ),
      #(
        "parentRemoteId",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "parentRemoteId",
        )),
      ),
      #(
        "hierarchyLevel",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "hierarchyLevel",
        )),
      ),
      #("remoteId", serializers.optional_marketing_string(remote_id)),
      #(
        "currencyCode",
        serializers.optional_marketing_string(activity_input_currency(input)),
      ),
      #("utmParameters", serializers.optional_marketing_object(utm)),
      #("marketingEvent", MarketingObject(event_data)),
    ])
  #(
    MarketingRecord(id: activity_id, cursor: None, data: activity_data),
    MarketingRecord(id: event_id, cursor: None, data: event_data),
    identity,
  )
}

fn build_native_marketing_activity_from_create_input(
  identity: SyntheticIdentityRegistry,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, SyntheticIdentityRegistry) {
  let #(activity_id, identity) =
    synthetic_identity.make_synthetic_gid(identity, "MarketingActivity")
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let status =
    option.unwrap(serializers.read_value_string(input, "status"), "UNDEFINED")
  let title =
    option.unwrap(
      serializers.read_value_string(input, "marketingActivityTitle")
        |> option.or(serializers.read_value_string(input, "title")),
      "Marketing activity",
    )
  let tactic =
    option.unwrap(serializers.read_value_string(input, "tactic"), "NEWSLETTER")
  let channel_type =
    option.unwrap(
      serializers.read_value_string(input, "marketingChannelType"),
      "EMAIL",
    )
  let source_medium = serializers.source_and_medium(channel_type, tactic)
  let data =
    dict.from_list([
      #("__typename", MarketingString("MarketingActivity")),
      #("id", MarketingString(activity_id)),
      #("title", MarketingString(title)),
      #("createdAt", MarketingString(timestamp)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(serializers.status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #("isExternal", MarketingBool(False)),
      #("inMainWorkflowVersion", MarketingBool(True)),
      #(
        "urlParameterValue",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "urlParameterValue",
        )),
      ),
      #(
        "parentActivityId",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "parentActivityId",
        )),
      ),
      #(
        "parentRemoteId",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "parentRemoteId",
        )),
      ),
      #(
        "hierarchyLevel",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "hierarchyLevel",
        )),
      ),
      #(
        "marketingActivityExtensionId",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "marketingActivityExtensionId",
        )),
      ),
      #(
        "context",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "context",
        )),
      ),
      #(
        "formData",
        serializers.optional_marketing_string(serializers.read_value_string(
          input,
          "formData",
        )),
      ),
      #(
        "currencyCode",
        serializers.optional_marketing_string(activity_input_currency(input)),
      ),
      #(
        "utmParameters",
        serializers.optional_marketing_object(serializers.read_utm(input)),
      ),
      #("marketingEvent", MarketingNull),
    ])
  #(MarketingRecord(id: activity_id, cursor: None, data: data), identity)
}

fn apply_native_marketing_activity_update(
  identity: SyntheticIdentityRegistry,
  record: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let status =
    option.unwrap(
      serializers.read_value_string(input, "status")
        |> option.or(serializers.read_marketing_string(record.data, "status")),
      "UNDEFINED",
    )
  let tactic =
    option.unwrap(
      serializers.read_value_string(input, "tactic")
        |> option.or(serializers.read_marketing_string(record.data, "tactic")),
      "NEWSLETTER",
    )
  let channel_type =
    option.unwrap(
      serializers.read_value_string(input, "marketingChannelType")
        |> option.or(serializers.read_marketing_string(
          record.data,
          "marketingChannelType",
        )),
      "EMAIL",
    )
  let title =
    option.unwrap(
      serializers.read_value_string(input, "marketingActivityTitle")
        |> option.or(serializers.read_value_string(input, "title"))
        |> option.or(serializers.read_marketing_string(record.data, "title")),
      "Marketing activity",
    )
  let source_medium = serializers.source_and_medium(channel_type, tactic)
  let data =
    serializers.overlay_marketing_data(record.data, [
      #("title", MarketingString(title)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(serializers.status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #(
        "urlParameterValue",
        serializers.optional_marketing_string(
          serializers.read_value_string(input, "urlParameterValue")
          |> option.or(serializers.read_marketing_string(
            record.data,
            "urlParameterValue",
          )),
        ),
      ),
      #(
        "context",
        serializers.optional_marketing_string(
          serializers.read_value_string(input, "context")
          |> option.or(serializers.read_marketing_string(record.data, "context")),
        ),
      ),
      #(
        "formData",
        serializers.optional_marketing_string(
          serializers.read_value_string(input, "formData")
          |> option.or(serializers.read_marketing_string(
            record.data,
            "formData",
          )),
        ),
      ),
      #(
        "utmParameters",
        serializers.optional_marketing_object(
          serializers.read_utm(input)
          |> option.or(serializers.read_marketing_object(
            record.data,
            "utmParameters",
          )),
        ),
      ),
      #(
        "currencyCode",
        serializers.optional_marketing_string(
          activity_input_currency(input)
          |> option.or(serializers.read_marketing_string(
            record.data,
            "currencyCode",
          )),
        ),
      ),
    ])
  #(MarketingRecord(..record, data: data), identity)
}

fn apply_external_activity_update(
  identity: SyntheticIdentityRegistry,
  record: MarketingRecord,
  input: Dict(String, root_field.ResolvedValue),
) -> #(MarketingRecord, MarketingRecord, SyntheticIdentityRegistry) {
  let #(timestamp, identity) =
    synthetic_identity.make_synthetic_timestamp(identity)
  let existing_event =
    option.unwrap(
      serializers.read_marketing_object(record.data, "marketingEvent"),
      dict.new(),
    )
  let #(event_id, identity) = case
    serializers.read_marketing_object_string(Some(existing_event), "id")
  {
    Some(id) -> #(id, identity)
    None -> synthetic_identity.make_synthetic_gid(identity, "MarketingEvent")
  }
  let status =
    option.unwrap(
      serializers.read_value_string(input, "status")
        |> option.or(serializers.read_marketing_string(record.data, "status")),
      "UNDEFINED",
    )
  let tactic =
    option.unwrap(
      serializers.read_value_string(input, "tactic")
        |> option.or(serializers.read_marketing_string(record.data, "tactic")),
      "NEWSLETTER",
    )
  let channel_type =
    option.unwrap(
      serializers.read_value_string(input, "marketingChannelType")
        |> option.or(serializers.read_marketing_string(
          record.data,
          "marketingChannelType",
        )),
      "EMAIL",
    )
  let title =
    option.unwrap(
      serializers.read_value_string(input, "title")
        |> option.or(serializers.read_marketing_string(record.data, "title")),
      "",
    )
  let source_medium = serializers.source_and_medium(channel_type, tactic)
  let existing_utm =
    serializers.read_marketing_object(record.data, "utmParameters")
  let ended_at =
    serializers.read_value_string(input, "end")
    |> option.or({
      case
        status
        == option.unwrap(
          serializers.read_marketing_string(record.data, "status"),
          "",
        )
      {
        True ->
          serializers.read_marketing_object_string(
            Some(existing_event),
            "endedAt",
          )
        False -> serializers.event_ended_at_for_status(status, timestamp)
      }
    })
  let event_data =
    serializers.overlay_marketing_data(existing_event, [
      #("__typename", MarketingString("MarketingEvent")),
      #("id", MarketingString(event_id)),
      #(
        "legacyResourceId",
        MarketingInt(option.unwrap(serializers.id_number(event_id), 0)),
      ),
      #("type", MarketingString(tactic)),
      #(
        "remoteId",
        serializers.optional_marketing_string(serializers.marketing_remote_id(
          record.data,
        )),
      ),
      #(
        "startedAt",
        MarketingString(option.unwrap(
          serializers.read_value_string(input, "start")
            |> option.or(serializers.read_value_string(input, "scheduledStart"))
            |> option.or(serializers.read_marketing_object_string(
              Some(existing_event),
              "startedAt",
            )),
          timestamp,
        )),
      ),
      #("endedAt", serializers.optional_marketing_string(ended_at)),
      #(
        "scheduledToEndAt",
        serializers.optional_marketing_string(
          serializers.read_value_string(input, "scheduledEnd")
          |> option.or(serializers.read_marketing_object_string(
            Some(existing_event),
            "scheduledToEndAt",
          )),
        ),
      ),
      #(
        "manageUrl",
        serializers.optional_marketing_string(
          serializers.read_value_string(input, "remoteUrl")
          |> option.or(serializers.read_marketing_object_string(
            Some(existing_event),
            "manageUrl",
          )),
        ),
      ),
      #(
        "previewUrl",
        serializers.optional_marketing_string(
          serializers.read_value_string(input, "remotePreviewImageUrl")
          |> option.or(serializers.read_marketing_object_string(
            Some(existing_event),
            "previewUrl",
          )),
        ),
      ),
      #(
        "utmCampaign",
        serializers.optional_marketing_string(
          serializers.read_marketing_object_string(existing_utm, "campaign"),
        ),
      ),
      #(
        "utmMedium",
        serializers.optional_marketing_string(
          serializers.read_marketing_object_string(existing_utm, "medium"),
        ),
      ),
      #(
        "utmSource",
        serializers.optional_marketing_string(
          serializers.read_marketing_object_string(existing_utm, "source"),
        ),
      ),
      #("description", MarketingString(title)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
    ])
  let activity_data =
    serializers.overlay_marketing_data(record.data, [
      #("title", MarketingString(title)),
      #("updatedAt", MarketingString(timestamp)),
      #("status", MarketingString(status)),
      #("statusLabel", MarketingString(serializers.status_label(status))),
      #("tactic", MarketingString(tactic)),
      #("marketingChannelType", MarketingString(channel_type)),
      #("sourceAndMedium", MarketingString(source_medium)),
      #(
        "currencyCode",
        serializers.optional_marketing_string(
          activity_input_currency(input)
          |> option.or(serializers.read_marketing_string(
            record.data,
            "currencyCode",
          )),
        ),
      ),
      #("marketingEvent", MarketingObject(event_data)),
    ])
  #(
    MarketingRecord(..record, data: activity_data),
    MarketingRecord(id: event_id, cursor: None, data: event_data),
    identity,
  )
}

fn build_marketing_engagement_record(
  identifier: EngagementIdentifier,
  input: Dict(String, root_field.ResolvedValue),
) -> MarketingEngagementRecord {
  let occurred_on =
    option.unwrap(serializers.read_value_string(input, "occurredOn"), "")
  let activity = engagement_activity(identifier)
  let channel_handle = case identifier {
    ChannelIdentifier(value) -> Some(value)
    _ -> None
  }
  let data =
    dict.from_list([
      #("__typename", MarketingString("MarketingEngagement")),
      #("occurredOn", MarketingString(occurred_on)),
      #(
        "utcOffset",
        MarketingString(option.unwrap(
          serializers.read_value_string(input, "utcOffset"),
          "+00:00",
        )),
      ),
      #(
        "isCumulative",
        MarketingBool(option.unwrap(
          serializers.read_value_bool(input, "isCumulative"),
          False,
        )),
      ),
      #("channelHandle", serializers.optional_marketing_string(channel_handle)),
      #(
        "marketingActivity",
        serializers.optional_marketing_object(
          option.map(activity, fn(a) { a.data }),
        ),
      ),
    ])
    |> serializers.overlay_marketing_data(integer_engagement_entries(input))
    |> serializers.overlay_marketing_data(money_engagement_entries(input))
    |> serializers.overlay_marketing_data(decimal_engagement_entries(input))
  MarketingEngagementRecord(
    id: engagement_record_id(identifier, occurred_on),
    marketing_activity_id: option.map(activity, fn(a) { a.id }),
    remote_id: case identifier {
      RemoteIdentifier(value, ..) -> Some(value)
      _ ->
        option.flatten(
          option.map(activity, fn(a) { serializers.marketing_remote_id(a.data) }),
        )
    },
    channel_handle: channel_handle,
    occurred_on: occurred_on,
    data: data,
  )
}

fn resolve_marketing_engagement_identifier(
  store: Store,
  args: Dict(String, root_field.ResolvedValue),
) -> Result(EngagementIdentifier, UserError) {
  let marketing_activity_id =
    graphql_helpers.read_arg_string_nonempty(args, "marketingActivityId")
  let remote_id = graphql_helpers.read_arg_string_nonempty(args, "remoteId")
  let channel_handle =
    graphql_helpers.read_arg_string_nonempty(args, "channelHandle")
  let count =
    serializers.option_count(marketing_activity_id)
    + serializers.option_count(remote_id)
    + serializers.option_count(channel_handle)
  case count {
    0 -> Error(engagement_missing_identifier_error())
    n if n > 1 -> Error(engagement_invalid_identifier_error())
    _ ->
      case marketing_activity_id, remote_id, channel_handle {
        Some(id), _, _ ->
          case store.get_effective_marketing_activity_record_by_id(store, id) {
            Some(activity) -> Ok(ActivityIdentifier(id, activity))
            None -> Error(marketing_activity_missing_error())
          }
        _, Some(remote_id), _ ->
          case
            store.get_effective_marketing_activity_by_remote_id(
              store,
              remote_id,
            )
          {
            Some(activity) -> Ok(RemoteIdentifier(remote_id, activity))
            None -> Error(marketing_activity_missing_error())
          }
        _, _, Some(handle) ->
          case store.has_known_marketing_channel_handle(store, handle) {
            True -> Ok(ChannelIdentifier(handle))
            False -> Error(invalid_channel_handle_error())
          }
        _, _, _ -> Error(engagement_missing_identifier_error())
      }
  }
}

fn validate_engagement_input_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Result(Option(String), UserError) {
  let ad_spend_currency = money_input_currency(input, "adSpend")
  let sales_currency = money_input_currency(input, "sales")
  case ad_spend_currency, sales_currency {
    Some(ad_spend_currency), Some(sales_currency)
      if ad_spend_currency != sales_currency
    -> Error(currency_code_mismatch_input_error())
    Some(currency), _ -> Ok(Some(currency))
    _, Some(currency) -> Ok(Some(currency))
    _, _ -> Ok(None)
  }
}

fn validate_engagement_activity_currency(
  identifier: EngagementIdentifier,
  engagement_currency_code: Option(String),
) -> Result(Nil, UserError) {
  case engagement_currency_code, engagement_activity(identifier) {
    Some(engagement_currency_code), Some(activity) ->
      case marketing_activity_currency(activity.data) {
        Some(activity_currency_code)
          if activity_currency_code != engagement_currency_code
        -> Error(marketing_activity_currency_code_mismatch_error())
        _ -> Ok(Nil)
      }
    _, _ -> Ok(Nil)
  }
}

fn engagement_activity(
  identifier: EngagementIdentifier,
) -> Option(MarketingRecord) {
  case identifier {
    ActivityIdentifier(activity: activity, ..) -> Some(activity)
    RemoteIdentifier(activity: activity, ..) -> Some(activity)
    ChannelIdentifier(..) -> None
  }
}

fn engagement_record_id(
  identifier: EngagementIdentifier,
  occurred_on: String,
) -> String {
  let target = case identifier {
    ChannelIdentifier(value) -> "channel:" <> value
    ActivityIdentifier(activity: activity, ..) -> "activity:" <> activity.id
    RemoteIdentifier(activity: activity, ..) -> "activity:" <> activity.id
  }
  "gid://shopify/MarketingEngagement/"
  <> serializers.url_encode(target <> ":" <> occurred_on)
}

fn integer_engagement_entries(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, MarketingValue)) {
  let fields = [
    "impressionsCount",
    "viewsCount",
    "clicksCount",
    "sharesCount",
    "favoritesCount",
    "commentsCount",
    "unsubscribesCount",
    "complaintsCount",
    "failsCount",
    "sendsCount",
    "uniqueViewsCount",
    "uniqueClicksCount",
    "sessionsCount",
  ]
  list.filter_map(fields, fn(field) {
    case serializers.read_value_int(input, field) {
      Some(value) -> Ok(#(field, MarketingInt(value)))
      None -> Error(Nil)
    }
  })
}

fn money_engagement_entries(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, MarketingValue)) {
  ["adSpend", "sales"]
  |> list.filter_map(fn(field) {
    case serializers.read_money_input(input, field) {
      Some(value) -> Ok(#(field, MarketingObject(value)))
      None -> Error(Nil)
    }
  })
}

fn activity_input_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  money_input_currency(input, "budget")
  |> option.or(budget_total_currency(input))
  |> option.or(money_input_currency(input, "adSpend"))
}

fn budget_total_currency(
  input: Dict(String, root_field.ResolvedValue),
) -> Option(String) {
  case dict.get(input, "budget") {
    Ok(root_field.ObjectVal(budget)) -> money_input_currency(budget, "total")
    _ -> None
  }
}

fn money_input_currency(
  input: Dict(String, root_field.ResolvedValue),
  field: String,
) -> Option(String) {
  case dict.get(input, field) {
    Ok(root_field.ObjectVal(money)) ->
      serializers.read_value_string(money, "currencyCode")
    _ -> None
  }
}

fn marketing_activity_currency(
  data: Dict(String, MarketingValue),
) -> Option(String) {
  serializers.read_marketing_string(data, "currencyCode")
  |> option.or(
    serializers.read_marketing_object(data, "adSpend")
    |> serializers.read_marketing_object_string("currencyCode"),
  )
  |> option.or(
    marketing_budget_currency(serializers.read_marketing_object(data, "budget")),
  )
}

fn marketing_budget_currency(
  budget: Option(Dict(String, MarketingValue)),
) -> Option(String) {
  case budget {
    Some(budget) ->
      serializers.read_marketing_object_string(Some(budget), "currencyCode")
      |> option.or(
        serializers.read_marketing_object(budget, "total")
        |> serializers.read_marketing_object_string("currencyCode"),
      )
    None -> None
  }
}

fn decimal_engagement_entries(
  input: Dict(String, root_field.ResolvedValue),
) -> List(#(String, MarketingValue)) {
  [
    "orders",
    "primaryConversions",
    "allConversions",
    "firstTimeCustomers",
    "returningCustomers",
  ]
  |> list.filter_map(fn(field) {
    case serializers.read_decimal_input(input, field) {
      Some(value) -> Ok(#(field, MarketingString(value)))
      None -> Error(Nil)
    }
  })
}
// ===========================================================================
// Shared helpers
// ===========================================================================
