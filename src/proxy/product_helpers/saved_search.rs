use super::*;
use crate::proxy::search::split_search_query_terms;

const SAVED_SEARCH_MUTATION_TARGET_HYDRATE_QUERY: &str =
    "query SavedSearchMutationTargetHydrate($id: ID!) {\n  node(id: $id) {\n    __typename\n    ... on SavedSearch {\n      id\n      legacyResourceId\n      name\n      query\n      resourceType\n      searchTerms\n      filters {\n        key\n        value\n      }\n    }\n  }\n}";

impl DraftProxy {
    pub(crate) fn saved_search_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        debug_assert_eq!(invocation.api_surface, ApiSurface::Admin);
        debug_assert_eq!(invocation.api_version.surface(), invocation.api_surface);
        debug_assert_eq!(invocation.operation.operation_type, OperationType::Query);
        let api_client_id = saved_search_request_api_client_id(invocation.request);
        if self.config.read_mode == ReadMode::LiveHybrid {
            let resource_type = saved_search_resource_type(invocation.root_name);
            let has_overlay = self.store.has_saved_search_overlay(resource_type);
            let mut outcome = self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
            if outcome.errors.is_empty() {
                if !has_overlay {
                    self.observe_saved_search_connection(
                        invocation.root_name,
                        &api_client_id,
                        &outcome.value,
                    );
                    return outcome;
                }
                if saved_search_connection_has_unsupported_overlay_scope(&invocation.arguments) {
                    return outcome;
                }
                let Some(window) = self.saved_search_upstream_window(
                    invocation.request,
                    invocation.root_name,
                    &invocation.arguments,
                    resource_type,
                ) else {
                    return outcome;
                };
                self.observe_saved_search_connection(invocation.root_name, &api_client_id, &window);
                outcome.value = self.saved_search_overlay_connection_value(
                    invocation.root_name,
                    &invocation.arguments,
                    &api_client_id,
                    &window,
                );
                outcome.value_source = crate::admin_graphql::ResolverValueSource::Local;
            }
            return outcome;
        }
        ResolverOutcome::value(self.saved_search_connection_value(
            invocation.root_name,
            &invocation.arguments,
            &api_client_id,
        ))
    }

    pub(crate) fn saved_search_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        debug_assert_eq!(invocation.api_surface, ApiSurface::Admin);
        debug_assert_eq!(invocation.api_version.surface(), invocation.api_surface);
        debug_assert_eq!(invocation.operation.operation_type, OperationType::Mutation);
        let api_client_id = saved_search_request_api_client_id(invocation.request);
        let input = invocation.arguments.get("input").and_then(Value::as_object);
        if matches!(
            invocation.root_name,
            "savedSearchUpdate" | "savedSearchDelete"
        ) {
            self.hydrate_saved_search_mutation_target(invocation.request, input, &api_client_id);
        }
        match invocation.root_name {
            "savedSearchCreate" => self.saved_search_create_outcome(input, &api_client_id),
            "savedSearchUpdate" => self.saved_search_update_outcome(input, &api_client_id),
            "savedSearchDelete" => self.saved_search_delete_outcome(input),
            root_name => {
                ResolverOutcome::error(format!("Unknown saved-search mutation root `{root_name}`"))
            }
        }
    }

    fn hydrate_saved_search_mutation_target(
        &mut self,
        request: &Request,
        input: Option<&serde_json::Map<String, Value>>,
        api_client_id: &str,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let Some(id) = input
            .and_then(|input| input.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return;
        };
        if self.store.saved_searches.staged.is_tombstoned(id)
            || self.store.saved_search_by_id(id).is_some()
        {
            return;
        }

        let response = self.upstream_post(
            request,
            json!({
                "query": SAVED_SEARCH_MUTATION_TARGET_HYDRATE_QUERY,
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return;
        }
        let node = &response.body["data"]["node"];
        if node.get("__typename").and_then(Value::as_str) != Some("SavedSearch") {
            return;
        }
        let Some(record) = saved_search_record_from_node(node, "", api_client_id) else {
            return;
        };
        if record.id != id || self.store.saved_searches.staged.is_tombstoned(id) {
            return;
        }
        self.store
            .saved_searches
            .base
            .insert(record.id.clone(), record);
    }

    fn observe_saved_search_connection(
        &mut self,
        root_name: &str,
        api_client_id: &str,
        connection: &Value,
    ) {
        let resource_type = saved_search_resource_type(root_name);
        for row in observed_connection_rows(connection) {
            let Some(mut record) =
                saved_search_record_from_node(&row.node, resource_type, api_client_id)
            else {
                continue;
            };
            record.cursor = row.cursor.or_else(|| {
                self.store
                    .saved_searches
                    .base
                    .get(&record.id)
                    .and_then(|existing| existing.cursor.clone())
            });
            if !self.store.saved_searches.staged.is_tombstoned(&record.id) {
                self.store
                    .saved_searches
                    .base
                    .insert(record.id.clone(), record);
            }
        }
    }

    pub(in crate::proxy) fn saved_search_connection_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, Value>,
        api_client_id: &str,
    ) -> Value {
        let resource_type = saved_search_resource_type(root_name);
        let mut records = self.saved_search_records_for_resource(resource_type);
        if let Some(query) = arguments.get("query").and_then(Value::as_str) {
            let needle = query.to_lowercase();
            records.retain(|record| {
                record.name.to_lowercase().contains(&needle)
                    || record.query.to_lowercase().contains(&needle)
            });
        }
        if arguments
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            records.reverse();
        }
        let resolved_arguments = resolved_arguments_from_json(arguments);
        let (records, page_info) =
            connection_window(&records, &resolved_arguments, saved_search_cursor);
        typed_connection_value(
            &records,
            |record| saved_search_full_value(record, api_client_id),
            saved_search_cursor,
            page_info,
        )
    }

    fn saved_search_upstream_window(
        &self,
        request: &Request,
        root_name: &str,
        arguments: &BTreeMap<String, Value>,
        resource_type: &str,
    ) -> Option<Value> {
        let first = saved_search_non_negative_window_argument(arguments, "first");
        let last = saved_search_non_negative_window_argument(arguments, "last");
        let (direction, requested_size) = match (first, last) {
            (Some(first), _) => (SavedSearchWindowDirection::Forward, first),
            (None, Some(last)) => (SavedSearchWindowDirection::Backward, last),
            (None, None) => return None,
        };
        let budget = requested_size
            .saturating_add(self.saved_search_overlay_change_count(resource_type))
            .max(1);
        let document = saved_search_window_request(root_name, direction);
        let reverse = arguments
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut after = arguments
            .get("after")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut before = arguments
            .get("before")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut remaining = budget;
        let mut rows = Vec::<ObservedConnectionRow>::new();
        let mut seen = BTreeSet::new();
        let mut has_previous_page = false;
        let mut has_next_page = false;
        let mut fetched_any_page = false;

        while remaining > 0 {
            let page_size = remaining.min(250);
            let mut variables = serde_json::Map::from_iter([
                (
                    "after".to_string(),
                    after.clone().map_or(Value::Null, Value::String),
                ),
                (
                    "before".to_string(),
                    before.clone().map_or(Value::Null, Value::String),
                ),
                ("reverse".to_string(), json!(reverse)),
            ]);
            variables.insert(direction.argument_name().to_string(), json!(page_size));
            let response = self.upstream_post(
                request,
                json!({
                    "query": &document,
                    "operationName": "SavedSearchConnectionWindow",
                    "variables": variables
                }),
            );
            if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
                return None;
            }
            let page = response.body.pointer("/data/savedSearchWindow")?;
            let page_rows = observed_connection_rows(page);
            let page_has_previous = saved_search_page_info_bool(page, "hasPreviousPage");
            let page_has_next = saved_search_page_info_bool(page, "hasNextPage");
            if !fetched_any_page {
                has_previous_page = page_has_previous;
                has_next_page = page_has_next;
                fetched_any_page = true;
            }
            match direction {
                SavedSearchWindowDirection::Forward => {
                    has_next_page = page_has_next;
                    for row in page_rows.iter().cloned() {
                        if seen.insert(saved_search_observed_row_identity(&row)) {
                            rows.push(row);
                        }
                    }
                }
                SavedSearchWindowDirection::Backward => {
                    has_previous_page = page_has_previous;
                    let mut preceding = Vec::new();
                    for row in page_rows.iter().cloned() {
                        if seen.insert(saved_search_observed_row_identity(&row)) {
                            preceding.push(row);
                        }
                    }
                    preceding.extend(rows);
                    rows = preceding;
                }
            }

            let fetched = page_rows.len();
            remaining = remaining.saturating_sub(fetched);
            if fetched == 0 || remaining == 0 {
                break;
            }
            match direction {
                SavedSearchWindowDirection::Forward if page_has_next => {
                    after = connection_end_cursor(page);
                    after.as_ref()?;
                }
                SavedSearchWindowDirection::Backward if page_has_previous => {
                    before = page
                        .pointer("/pageInfo/startCursor")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    before.as_ref()?;
                }
                _ => break,
            }
        }

        fetched_any_page
            .then(|| saved_search_observed_window_value(rows, has_next_page, has_previous_page))
    }

    fn saved_search_overlay_change_count(&self, resource_type: &str) -> usize {
        let staged_records = self
            .store
            .saved_searches
            .staged
            .records
            .values()
            .filter(|record| record.resource_type == resource_type)
            .count();
        let tombstones = self
            .store
            .saved_searches
            .staged
            .tombstones
            .iter()
            .filter(|id| {
                self.store
                    .saved_searches
                    .base
                    .get(id)
                    .is_some_and(|record| record.resource_type == resource_type)
                    || default_saved_search_by_id(id)
                        .is_some_and(|record| record.resource_type == resource_type)
            })
            .count();
        staged_records.saturating_add(tombstones)
    }

    fn saved_search_overlay_connection_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, Value>,
        api_client_id: &str,
        upstream_window: &Value,
    ) -> Value {
        let resource_type = saved_search_resource_type(root_name);
        let mut records = Vec::new();
        let mut semantic_keys = BTreeSet::new();
        for row in observed_connection_rows(upstream_window) {
            let Some(mut record) =
                saved_search_record_from_node(&row.node, resource_type, api_client_id)
            else {
                continue;
            };
            if self.store.saved_searches.staged.is_tombstoned(&record.id) {
                continue;
            }
            if let Some(staged) = self.store.saved_searches.staged.get(&record.id) {
                record = staged.clone();
            }
            if record.cursor.is_none() {
                record.cursor = row.cursor;
            }
            semantic_keys.insert(saved_search_semantic_key(&record));
            records.push(record);
        }

        let mut created = self
            .store
            .saved_searches
            .staged
            .order
            .iter()
            .filter_map(|id| {
                let record = self.store.saved_searches.staged.get(id)?;
                (record.resource_type == resource_type
                    && !self.store.saved_searches.base.records.contains_key(id)
                    && semantic_keys.insert(saved_search_semantic_key(record)))
                .then(|| record.clone())
            })
            .collect::<Vec<_>>();
        let reverse = arguments
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let after = arguments.get("after").and_then(Value::as_str);
        let before = arguments.get("before").and_then(Value::as_str);
        let created_belongs_to_scope = if reverse {
            after.is_none()
        } else {
            before.is_none()
        };
        if created_belongs_to_scope {
            if reverse {
                created.reverse();
                created.extend(records);
                records = created;
            } else {
                records.extend(created);
            }
        }

        let upstream_has_next = saved_search_page_info_bool(upstream_window, "hasNextPage");
        let upstream_has_previous = saved_search_page_info_bool(upstream_window, "hasPreviousPage");
        let total = records.len();
        let mut start = 0;
        let mut end = total;
        if let Some(first) = saved_search_non_negative_window_argument(arguments, "first") {
            end = end.min(first);
        }
        if let Some(last) = saved_search_non_negative_window_argument(arguments, "last") {
            start = start.max(end.saturating_sub(last));
        }
        let page_info = connection_page_info(
            upstream_has_next || end < total,
            upstream_has_previous || start > 0,
            (start < end).then(|| saved_search_cursor(&records[start])),
            (start < end).then(|| saved_search_cursor(&records[end - 1])),
        );
        typed_connection_value(
            &records[start..end],
            |record| saved_search_full_value(record, api_client_id),
            saved_search_cursor,
            page_info,
        )
    }

    fn saved_search_create_outcome(
        &mut self,
        input: Option<&serde_json::Map<String, Value>>,
        api_client_id: &str,
    ) -> ResolverOutcome<Value> {
        let Some(input) = input else {
            return ResolverOutcome::value(saved_search_full_mutation_payload(
                None,
                api_client_id,
                vec![saved_search_input_required_user_error()],
            ));
        };
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let search_query = input
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let resource_type = input
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or("PRODUCT");
        let mut user_errors = self.saved_search_field_user_errors(
            SavedSearchQueryValidationOperation::Create,
            resource_type,
            name,
            None,
        );
        user_errors.extend(saved_search_query_user_errors(
            SavedSearchQueryValidationOperation::Create,
            resource_type,
            search_query,
        ));
        if !user_errors.is_empty() {
            return ResolverOutcome::value(saved_search_full_mutation_payload(
                None,
                api_client_id,
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("SavedSearch");
        let normalized_query =
            normalize_saved_search_query_for_api_client(search_query, api_client_id);
        let record = saved_search_record_with_api_client(
            &id,
            name,
            &normalized_query,
            resource_type,
            api_client_id,
        );
        self.store.stage_saved_search(record.clone());
        ResolverOutcome::value(saved_search_full_mutation_payload(
            Some(&record),
            api_client_id,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "savedSearchCreate",
            "saved_searches",
            vec![id],
        ))
    }

    fn saved_search_update_outcome(
        &mut self,
        input: Option<&serde_json::Map<String, Value>>,
        api_client_id: &str,
    ) -> ResolverOutcome<Value> {
        let Some(input) = input else {
            return ResolverOutcome::value(saved_search_full_mutation_payload(
                None,
                api_client_id,
                vec![saved_search_input_required_user_error()],
            ));
        };
        let id = input.get("id").and_then(Value::as_str).unwrap_or_default();
        let Some(existing) = self.store.saved_search_by_id(id) else {
            return ResolverOutcome::value(saved_search_full_mutation_payload(
                None,
                api_client_id,
                vec![saved_search_missing_user_error()],
            ));
        };
        let requested_name = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&existing.name);
        let requested_query = input
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or(&existing.query);
        let mut updated = existing.clone();
        updated.query = normalize_saved_search_query_for_api_client(requested_query, api_client_id);
        if input.get("query").is_some() {
            updated.search_terms = saved_search_search_terms(&updated.query);
            updated.filters = saved_search_filters_for_api_client(&updated.query, api_client_id);
            updated.api_client_id = api_client_id.to_string();
        }
        let mut user_errors = self.saved_search_field_user_errors(
            SavedSearchQueryValidationOperation::Update,
            &existing.resource_type,
            requested_name,
            Some(id),
        );
        user_errors.extend(saved_search_query_user_errors(
            SavedSearchQueryValidationOperation::Update,
            &existing.resource_type,
            requested_query,
        ));
        if !user_errors.is_empty() {
            return ResolverOutcome::value(saved_search_full_mutation_payload(
                Some(&updated),
                api_client_id,
                user_errors,
            ));
        }
        updated.name = requested_name.to_string();
        self.store.stage_saved_search(updated.clone());
        ResolverOutcome::value(saved_search_full_mutation_payload(
            Some(&updated),
            api_client_id,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "savedSearchUpdate",
            "saved_searches",
            vec![updated.id.clone()],
        ))
    }

    fn saved_search_delete_outcome(
        &mut self,
        input: Option<&serde_json::Map<String, Value>>,
    ) -> ResolverOutcome<Value> {
        let id = input
            .and_then(|input| input.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let deleted = self.store.delete_saved_search(id);
        let value = json!({
            "deletedSavedSearchId": deleted.then_some(id),
            "shop": self.store.effective_shop(),
            "userErrors": if deleted { Vec::new() } else { vec![saved_search_missing_user_error()] }
        });
        let outcome = ResolverOutcome::value(value);
        if deleted {
            outcome.with_log_draft(LogDraft::staged(
                "savedSearchDelete",
                "saved_searches",
                vec![id.to_string()],
            ))
        } else {
            outcome
        }
    }
}

pub(in crate::proxy) fn saved_search_full_value(
    record: &SavedSearchRecord,
    api_client_id: &str,
) -> Value {
    let query = saved_search_read_query_for_api_client(&record.query, api_client_id);
    saved_search_value_with_query(record, query, api_client_id)
}

fn saved_search_mutation_value(record: &SavedSearchRecord, api_client_id: &str) -> Value {
    saved_search_value_with_query(record, record.query.clone(), api_client_id)
}

fn saved_search_value_with_query(
    record: &SavedSearchRecord,
    query: String,
    api_client_id: &str,
) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "query": query,
        "resourceType": record.resource_type,
        "_legacyResourceId": record.legacy_resource_id,
        "_searchTerms": record.search_terms,
        "_filters": record.filters.iter().map(|(key, value)| json!({ "key": key, "value": value })).collect::<Vec<_>>(),
        "_apiClientId": if record.api_client_id.is_empty() { api_client_id } else { &record.api_client_id }
    })
}

pub(in crate::proxy) fn saved_search_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    let mut registrations = Vec::new();
    for field in ["id", "name", "query", "resourceType"] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "SavedSearch",
            field,
        ));
    }
    registrations.extend([
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "SavedSearch",
            "legacyResourceId",
            saved_search_legacy_resource_id_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "SavedSearch",
            "searchTerms",
            saved_search_search_terms_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "SavedSearch",
            "filters",
            saved_search_filters_field,
        ),
    ]);
    for (parent_type, fields) in [
        ("SavedSearchConnection", &["edges", "nodes", "pageInfo"][..]),
        ("SavedSearchEdge", &["cursor", "node"]),
        ("SearchFilter", &["key", "value"]),
        (
            "PageInfo",
            &["hasNextPage", "hasPreviousPage", "startCursor", "endCursor"],
        ),
        ("SavedSearchCreatePayload", &["savedSearch", "userErrors"]),
        ("SavedSearchUpdatePayload", &["savedSearch", "userErrors"]),
        (
            "SavedSearchDeletePayload",
            &["deletedSavedSearchId", "shop", "userErrors"],
        ),
        ("UserError", &["field", "message"]),
    ] {
        for field in fields {
            registrations.push(FieldResolverRegistration::property(
                ApiSurface::Admin,
                parent_type,
                field,
            ));
        }
    }
    registrations
}

fn saved_search_parent_string<'a>(
    invocation: &'a crate::admin_graphql::FieldResolverInvocation,
    field: &str,
) -> Result<&'a str, String> {
    invocation
        .parent
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("SavedSearch field resolver requires canonical `{field}` property"))
}

fn saved_search_legacy_resource_id_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    if let Some(legacy_resource_id) = invocation
        .parent
        .get("_legacyResourceId")
        .and_then(Value::as_str)
    {
        return Ok(json!(legacy_resource_id));
    }
    Ok(json!(saved_search_legacy_resource_id(
        saved_search_parent_string(invocation, "id")?
    )))
}

fn saved_search_search_terms_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    if let Some(search_terms) = invocation.parent.get("_searchTerms") {
        return Ok(search_terms.clone());
    }
    Ok(json!(saved_search_search_terms(
        saved_search_parent_string(invocation, "query")?
    )))
}

fn saved_search_filters_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation,
) -> Result<Value, String> {
    if let Some(filters) = invocation.parent.get("_filters") {
        return Ok(filters.clone());
    }
    let query = saved_search_parent_string(invocation, "query")?;
    let api_client_id = invocation
        .parent
        .get("_apiClientId")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_SAVED_SEARCH_API_CLIENT_ID);
    Ok(Value::Array(
        saved_search_filters_for_api_client(query, api_client_id)
            .into_iter()
            .map(|(key, value)| json!({ "key": key, "value": value }))
            .collect(),
    ))
}

fn saved_search_full_mutation_payload(
    record: Option<&SavedSearchRecord>,
    api_client_id: &str,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "savedSearch": record.map(|record| saved_search_mutation_value(record, api_client_id)),
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn saved_search_state_map_json(
    saved_searches: &BTreeMap<String, SavedSearchRecord>,
) -> Value {
    Value::Object(
        saved_searches
            .iter()
            .map(|(id, record)| (id.clone(), saved_search_state_json(record)))
            .collect(),
    )
}

pub(in crate::proxy) fn saved_search_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, SavedSearchRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            saved_search_state_from_json(value).map(|record| (id.clone(), record))
        })
        .collect()
}

pub(in crate::proxy) fn saved_search_state_from_json(value: &Value) -> Option<SavedSearchRecord> {
    let id = value.get("id")?.as_str()?;
    let name = value.get("name")?.as_str()?;
    let query = value.get("query")?.as_str()?;
    let resource_type = value.get("resourceType")?.as_str()?;
    let api_client_id = value
        .get("apiClientId")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_SAVED_SEARCH_API_CLIENT_ID);
    let mut record =
        saved_search_record_with_api_client(id, name, query, resource_type, api_client_id);
    record.cursor = value
        .get("cursor")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(legacy_resource_id) = value.get("legacyResourceId").and_then(Value::as_str) {
        record.legacy_resource_id = legacy_resource_id.to_string();
    }
    if let Some(search_terms) = value.get("searchTerms").and_then(Value::as_str) {
        record.search_terms = search_terms.to_string();
    }
    if let Some(filters) = saved_search_filter_records_from_value(value.get("filters")) {
        record.filters = filters;
    }
    Some(record)
}

pub(in crate::proxy) fn saved_search_record_from_node(
    node: &Value,
    fallback_resource_type: &str,
    api_client_id: &str,
) -> Option<SavedSearchRecord> {
    let query = node
        .get("query")
        .and_then(Value::as_str)
        .map(|query| normalize_saved_search_query_for_api_client(query, api_client_id))
        .unwrap_or_default();
    let id = node.get("id")?.as_str()?;
    let name = node.get("name")?.as_str()?;
    let resource_type = node
        .get("resourceType")
        .and_then(Value::as_str)
        .unwrap_or(fallback_resource_type);
    let mut record =
        saved_search_record_with_api_client(id, name, &query, resource_type, api_client_id);
    if let Some(legacy_resource_id) = node.get("legacyResourceId").and_then(Value::as_str) {
        record.legacy_resource_id = legacy_resource_id.to_string();
    }
    if let Some(search_terms) = node.get("searchTerms").and_then(Value::as_str) {
        record.search_terms = search_terms.to_string();
    }
    if let Some(filters) = saved_search_filter_records_from_value(node.get("filters")) {
        record.filters = filters;
    }
    Some(record)
}

pub(in crate::proxy) fn saved_search_state_json(record: &SavedSearchRecord) -> Value {
    let mut value = json!({
        "id": record.id,
        "legacyResourceId": record.legacy_resource_id,
        "name": record.name,
        "query": record.query,
        "resourceType": record.resource_type,
        "searchTerms": record.search_terms,
        "filters": record.filters.iter().map(|(key, value)| json!({ "key": key, "value": value })).collect::<Vec<_>>(),
        "apiClientId": record.api_client_id
    });
    if let Some(cursor) = &record.cursor {
        value["cursor"] = json!(cursor);
    }
    value
}

fn saved_search_filter_records_from_value(value: Option<&Value>) -> Option<Vec<(String, String)>> {
    Some(
        value?
            .as_array()?
            .iter()
            .filter_map(|filter| {
                Some((
                    filter.get("key")?.as_str()?.to_string(),
                    filter.get("value")?.as_str()?.to_string(),
                ))
            })
            .collect(),
    )
}

pub(in crate::proxy) fn saved_search_name_taken_user_error() -> Value {
    user_error_omit_code(["input", "name"], "Name has already been taken", None)
}

fn saved_search_input_required_user_error() -> Value {
    user_error_omit_code(["input"], "Saved search input is required", None)
}

fn saved_search_missing_user_error() -> Value {
    user_error_omit_code(["input", "id"], "Saved Search does not exist", None)
}

fn saved_search_name_blank_user_error() -> Value {
    user_error_omit_code(["input", "name"], "Name can't be blank", None)
}

fn saved_search_name_too_long_user_error() -> Value {
    user_error_omit_code(
        ["input", "name"],
        "Name is too long (maximum is 40 characters)",
        None,
    )
}

fn saved_search_customer_deprecated_user_error() -> Value {
    user_error_omit_code(
        Value::Null,
        "Customer saved searches have been deprecated. Use Segmentation API instead.",
        None,
    )
}

#[derive(Clone, Copy)]
pub(in crate::proxy) enum SavedSearchQueryValidationOperation {
    Create,
    Update,
}

pub(in crate::proxy) fn saved_search_query_user_errors(
    operation: SavedSearchQueryValidationOperation,
    resource_type: &str,
    query: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resource_type == "ORDER" && query.contains("reference_location_id:") {
        let field = match operation {
            SavedSearchQueryValidationOperation::Create => json!(["input", "query"]),
            SavedSearchQueryValidationOperation::Update => json!(["input", "searchTerms"]),
        };
        errors.push(user_error_omit_code(
            field,
            "Search terms is invalid, 'reference_location_id' is a reserved filter name",
            None,
        ));
    }
    let filters = saved_search_filters(query);
    let mut invalid_filters: Vec<String> = filters
        .iter()
        .filter_map(|(key, _)| {
            if saved_search_known_filter(resource_type, key)
                || saved_search_reserved_filter(resource_type, key)
            {
                None
            } else {
                Some(saved_search_base_filter_key(key).to_string())
            }
        })
        .collect();
    invalid_filters.sort();
    invalid_filters.dedup();
    for key in invalid_filters {
        errors.push(user_error_omit_code(
            ["input", "query"],
            &format!("Query is invalid, '{key}' is not a valid filter"),
            None,
        ));
    }
    if resource_type == "PRODUCT" {
        let has_collection = filters.iter().any(|(key, _)| key == "collection_id");
        let incompatible: Vec<&str> = ["tag", "published_status", "error_feedback"]
            .iter()
            .copied()
            .filter(|needle| filters.iter().any(|(key, _)| key == *needle))
            .collect();
        if has_collection && !incompatible.is_empty() {
            let mut keys = vec!["collection_id"];
            keys.extend(incompatible);
            errors.push(user_error_omit_code(
                ["input", "query"],
                &format!("Query has incompatible filters: {}", keys.join(", ")),
                None,
            ));
        }
    }
    errors
}

fn saved_search_reserved_filter(resource_type: &str, key: &str) -> bool {
    resource_type == "ORDER" && saved_search_base_filter_key(key) == "reference_location_id"
}

pub(in crate::proxy) fn saved_search_known_filter(resource_type: &str, key: &str) -> bool {
    let base_key = saved_search_base_filter_key(key);
    if base_key == "default" {
        return true;
    }
    match resource_type {
        "PRODUCT" => {
            matches!(
                base_key,
                "collection_id"
                    | "created_at"
                    | "error_feedback"
                    | "handle"
                    | "id"
                    | "inventory_total"
                    | "product_type"
                    | "published_at"
                    | "published_status"
                    | "sku"
                    | "status"
                    | "tag"
                    | "title"
                    | "updated_at"
                    | "vendor"
            ) || base_key.starts_with("metafields.")
        }
        "COLLECTION" => matches!(
            base_key,
            "collection_type"
                | "handle"
                | "id"
                | "product_id"
                | "product_publication_status"
                | "publishable_status"
                | "published_at"
                | "published_status"
                | "title"
                | "updated_at"
        ),
        "ORDER" => matches!(
            base_key,
            "channel_id"
                | "created_at"
                | "customer_id"
                | "email"
                | "financial_status"
                | "fulfillment_status"
                | "id"
                | "location_id"
                | "name"
                | "processed_at"
                | "sales_channel"
                | "status"
                | "tag"
                | "test"
                | "updated_at"
        ),
        "DRAFT_ORDER" => matches!(
            base_key,
            "created_at"
                | "customer_id"
                | "email"
                | "id"
                | "name"
                | "status"
                | "tag"
                | "updated_at"
        ),
        "FILE" => matches!(
            base_key,
            "created_at"
                | "filename"
                | "id"
                | "ids"
                | "media_type"
                | "original_source"
                | "original_upload_size"
                | "product_id"
                | "status"
                | "updated_at"
                | "used_in"
        ),
        "DISCOUNT_REDEEM_CODE" => matches!(
            base_key,
            "code" | "created_at" | "discount_id" | "id" | "status" | "updated_at"
        ),
        _ => true,
    }
}

fn saved_search_base_filter_key(key: &str) -> &str {
    key.trim_end_matches("_not")
        .trim_end_matches("_min")
        .trim_end_matches("_max")
}

const DEFAULT_SAVED_SEARCH_API_CLIENT_ID: &str = "shopify-draft-proxy-local-app";

pub(in crate::proxy) fn saved_search_request_api_client_id(request: &Request) -> String {
    request_header(request, API_CLIENT_ID_HEADER)
        .map(|value| saved_search_namespace_api_client_id(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SAVED_SEARCH_API_CLIENT_ID.to_string())
}

fn saved_search_namespace_api_client_id(value: &str) -> String {
    let trimmed = value.trim();
    let tail = shopify_gid_tail_for_type(trimmed, "App").unwrap_or(trimmed);
    tail.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect()
}

pub(in crate::proxy) fn normalize_saved_search_query_for_api_client(
    query: &str,
    api_client_id: &str,
) -> String {
    let api_client_id = saved_search_namespace_api_client_id(api_client_id);
    let api_client_id = if api_client_id.is_empty() {
        DEFAULT_SAVED_SEARCH_API_CLIENT_ID
    } else {
        api_client_id.as_str()
    };
    query.replace(
        "metafields.$app.",
        &format!("metafields.app--{api_client_id}."),
    )
}

pub(in crate::proxy) fn saved_search_read_query_for_api_client(
    query: &str,
    api_client_id: &str,
) -> String {
    let namespace_normalized = normalize_saved_search_query_for_api_client(query, api_client_id);
    let quote_normalized = namespace_normalized.replace('\'', "\"");
    let canonical = canonical_saved_search_query(&quote_normalized);
    if saved_search_filters_for_api_client(&canonical, api_client_id).is_empty()
        && canonical.contains('-')
    {
        canonical.replace('-', "\\-")
    } else {
        canonical
    }
}

pub(in crate::proxy) fn canonical_saved_search_query(query: &str) -> String {
    let tokens = saved_search_query_tokens(query);
    if tokens.len() == 2 {
        let first_is_filter = saved_search_filter_from_token(tokens[0].as_str()).is_some();
        let second_is_filter = saved_search_filter_from_token(tokens[1].as_str()).is_some();
        if first_is_filter && !second_is_filter {
            return format!("{} {}", tokens[1], tokens[0]);
        }
    }
    if let Some((key, value)) = saved_search_filter_from_token(query) {
        if key == "inventory_total_min" && query.starts_with("-inventory_total:<") {
            return format!("inventory_total:>={}", value);
        }
    }
    query.to_string()
}

pub(in crate::proxy) fn saved_search_search_terms(query: &str) -> String {
    let display_query = query.replace('\'', "\"");
    let tokens = saved_search_query_tokens(&display_query);
    let has_grouping = display_query.contains(" OR ")
        || display_query.contains('(')
        || display_query.contains(')');
    let mut terms = Vec::new();
    for token in tokens {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')');
        if has_grouping && token.starts_with('-') {
            continue;
        }
        if !has_grouping && saved_search_filter_from_token(trimmed).is_some() {
            continue;
        }
        terms.push(token);
    }
    terms.join(" ").replace("\\-", "-")
}

pub(in crate::proxy) fn is_reserved_saved_search_name(resource_type: &str, name: &str) -> bool {
    let normalized = name.to_lowercase();
    let reserved = match resource_type {
        "PRODUCT" => &["all products"][..],
        "ORDER" => &["all"][..],
        "DRAFT_ORDER" => &["all drafts"][..],
        "FILE" => &["all files"][..],
        "COLLECTION" => &["all collections"][..],
        "PRICE_RULE" => &["all price rules"][..],
        "DISCOUNT_REDEEM_CODE" => &["all codes"][..],
        _ => &[],
    };
    reserved
        .iter()
        .any(|reserved_name| normalized == *reserved_name)
}

#[derive(Clone, Copy)]
enum SavedSearchWindowDirection {
    Forward,
    Backward,
}

impl SavedSearchWindowDirection {
    fn argument_name(self) -> &'static str {
        match self {
            Self::Forward => "first",
            Self::Backward => "last",
        }
    }
}

fn saved_search_window_request(root_name: &str, direction: SavedSearchWindowDirection) -> String {
    let (window_definition, window_argument) = match direction {
        SavedSearchWindowDirection::Forward => ("$first: Int!", "first: $first"),
        SavedSearchWindowDirection::Backward => ("$last: Int!", "last: $last"),
    };
    format!(
        "query SavedSearchConnectionWindow({window_definition}, $after: String, $before: String, $reverse: Boolean!) {{\n  savedSearchWindow: {root_name}({window_argument}, after: $after, before: $before, reverse: $reverse) {{\n    edges {{ cursor node {{ id name query resourceType }} }}\n    pageInfo {{ hasNextPage hasPreviousPage startCursor endCursor }}\n  }}\n}}"
    )
}

fn saved_search_non_negative_window_argument(
    arguments: &BTreeMap<String, Value>,
    name: &str,
) -> Option<usize> {
    arguments
        .get(name)
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
        .and_then(|value| usize::try_from(value).ok())
}

fn saved_search_page_info_bool(connection: &Value, field: &str) -> bool {
    connection
        .pointer(&format!("/pageInfo/{field}"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn saved_search_observed_row_identity(row: &ObservedConnectionRow) -> String {
    row.node
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| row.node.to_string())
}

fn saved_search_observed_window_value(
    rows: Vec<ObservedConnectionRow>,
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    let start_cursor = rows.first().and_then(|row| row.cursor.clone());
    let end_cursor = rows.last().and_then(|row| row.cursor.clone());
    let nodes = rows.iter().map(|row| row.node.clone()).collect::<Vec<_>>();
    let edges = rows
        .into_iter()
        .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
        .collect::<Vec<_>>();
    json!({
        "nodes": nodes,
        "edges": edges,
        "pageInfo": connection_page_info(
            has_next_page,
            has_previous_page,
            start_cursor,
            end_cursor,
        )
    })
}

fn saved_search_connection_has_unsupported_overlay_scope(
    arguments: &BTreeMap<String, Value>,
) -> bool {
    arguments.get("query").is_some_and(|query| !query.is_null())
        || arguments
            .get("sortKey")
            .and_then(Value::as_str)
            .is_some_and(|sort_key| sort_key != "ID")
}

pub(in crate::proxy) fn saved_search_resource_type(root: &str) -> &'static str {
    match root {
        "automaticDiscountSavedSearches" => "PRICE_RULE",
        "codeDiscountSavedSearches" => "PRICE_RULE",
        "collectionSavedSearches" => "COLLECTION",
        "customerSavedSearches" => "CUSTOMER",
        "discountRedeemCodeSavedSearches" => "DISCOUNT_REDEEM_CODE",
        "draftOrderSavedSearches" => "DRAFT_ORDER",
        "fileSavedSearches" => "FILE",
        "orderSavedSearches" => "ORDER",
        "productSavedSearches" => "PRODUCT",
        _ => "UNKNOWN",
    }
}

pub(in crate::proxy) fn default_saved_searches(resource_type: &str) -> Vec<SavedSearchRecord> {
    match resource_type {
        "ORDER" => vec![
            default_saved_search_record(
                "default-order-unfulfilled",
                "Unfulfilled",
                "status:open fulfillment_status:unshipped,partial",
                "ORDER",
            ),
            default_saved_search_record(
                "default-order-unpaid",
                "Unpaid",
                "status:open financial_status:unpaid",
                "ORDER",
            ),
            default_saved_search_record("default-order-open", "Open", "status:open", "ORDER"),
            default_saved_search_record(
                "default-order-archived",
                "Archived",
                "status:closed",
                "ORDER",
            ),
        ],
        "DRAFT_ORDER" => vec![
            default_saved_search_record(
                "default-draft-order-open-and-invoice-sent",
                "Open and invoice sent",
                "status:open_and_invoice_sent",
                "DRAFT_ORDER",
            ),
            default_saved_search_record(
                "default-draft-order-open",
                "Open",
                "status:open",
                "DRAFT_ORDER",
            ),
            default_saved_search_record(
                "default-draft-order-invoice-sent",
                "Invoice sent",
                "status:invoice_sent",
                "DRAFT_ORDER",
            ),
            default_saved_search_record(
                "default-draft-order-completed",
                "Completed",
                "status:completed",
                "DRAFT_ORDER",
            ),
            default_saved_search_record(
                "default-draft-order-submitted-for-review",
                "Submitted for review",
                "status:open source:online_store",
                "DRAFT_ORDER",
            ),
        ],
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn default_saved_search_by_id(id: &str) -> Option<SavedSearchRecord> {
    [
        "ORDER",
        "DRAFT_ORDER",
        "PRODUCT",
        "COLLECTION",
        "CUSTOMER",
        "FILE",
        "DISCOUNT_REDEEM_CODE",
        "DISCOUNT",
    ]
    .iter()
    .flat_map(|resource_type| default_saved_searches(resource_type))
    .find(|record| record.id == id)
}

pub(in crate::proxy) fn saved_search_record(
    id: &str,
    name: &str,
    query: &str,
    resource_type: &str,
) -> SavedSearchRecord {
    saved_search_record_with_api_client(
        id,
        name,
        query,
        resource_type,
        DEFAULT_SAVED_SEARCH_API_CLIENT_ID,
    )
}

fn saved_search_record_with_api_client(
    id: &str,
    name: &str,
    query: &str,
    resource_type: &str,
    api_client_id: &str,
) -> SavedSearchRecord {
    SavedSearchRecord {
        id: id.to_string(),
        cursor: None,
        legacy_resource_id: saved_search_legacy_resource_id(id),
        name: name.to_string(),
        query: query.to_string(),
        resource_type: resource_type.to_string(),
        search_terms: saved_search_search_terms(query),
        filters: saved_search_filters_for_api_client(query, api_client_id),
        api_client_id: api_client_id.to_string(),
    }
}

pub(in crate::proxy) fn default_saved_search_record(
    id_suffix: &str,
    name: &str,
    query: &str,
    resource_type: &str,
) -> SavedSearchRecord {
    saved_search_record(
        &synthetic_shopify_gid("SavedSearch", id_suffix),
        name,
        query,
        resource_type,
    )
}

pub(in crate::proxy) fn saved_search_cursor(record: &SavedSearchRecord) -> String {
    record
        .cursor
        .clone()
        .unwrap_or_else(|| format!("cursor:{}", record.id))
}

pub(in crate::proxy) fn saved_search_legacy_resource_id(id: &str) -> String {
    resource_id_tail(id).to_string()
}

pub(in crate::proxy) fn saved_search_filters(query: &str) -> Vec<(String, String)> {
    saved_search_filters_for_api_client(query, DEFAULT_SAVED_SEARCH_API_CLIENT_ID)
}

pub(in crate::proxy) fn saved_search_filters_for_api_client(
    query: &str,
    api_client_id: &str,
) -> Vec<(String, String)> {
    let query = normalize_saved_search_query_for_api_client(query, api_client_id);
    let tokens = saved_search_query_tokens(&query);
    let grouped = query.contains(" OR ") || query.contains('(') || query.contains(')');
    tokens
        .iter()
        .filter_map(|term| {
            let trimmed = term.trim_matches(|ch| ch == '(' || ch == ')');
            if grouped && !trimmed.starts_with('-') {
                return None;
            }
            saved_search_filter_from_token(trimmed)
        })
        .collect()
}

pub(in crate::proxy) fn saved_search_filter_from_token(term: &str) -> Option<(String, String)> {
    if term == "*" {
        return Some(("default".to_string(), "true".to_string()));
    }
    let (raw_key, raw_value) = term.split_once(':')?;
    if raw_key.is_empty() || raw_value.is_empty() {
        return None;
    }
    let mut key = raw_key.to_string();
    let mut value = raw_value.trim_matches('"').to_string();
    let negated = key.starts_with('-');
    if negated {
        key = key.trim_start_matches('-').to_string();
    }
    if value == "*" {
        value = "true".to_string();
    }
    if let Some(stripped) = value.strip_prefix(">=").or_else(|| value.strip_prefix('>')) {
        key = if negated {
            format!("{}_max", key)
        } else {
            format!("{}_min", key)
        };
        value = stripped.to_string();
    } else if let Some(stripped) = value.strip_prefix("<=").or_else(|| value.strip_prefix('<')) {
        key = if negated {
            format!("{}_min", key)
        } else {
            format!("{}_max", key)
        };
        value = stripped.to_string();
    } else if negated {
        key = format!("{}_not", key);
    }
    Some((key, value))
}

pub(in crate::proxy) fn saved_search_query_tokens(query: &str) -> Vec<String> {
    split_search_query_terms(query, '"')
}

impl DraftProxy {
    pub(in crate::proxy) fn saved_search_records_for_resource(
        &self,
        resource_type: &str,
    ) -> Vec<SavedSearchRecord> {
        self.store.saved_searches_for_resource(resource_type)
    }

    pub(in crate::proxy) fn saved_search_name_exists(
        &self,
        resource_type: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> bool {
        self.saved_search_records_for_resource(resource_type)
            .iter()
            .any(|record| Some(record.id.as_str()) != except_id && record.name == name)
    }

    fn saved_search_field_user_errors(
        &self,
        operation: SavedSearchQueryValidationOperation,
        resource_type: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let name_is_blank = name.trim().is_empty();
        match operation {
            SavedSearchQueryValidationOperation::Create => {
                if !name_is_blank && is_reserved_saved_search_name(resource_type, name) {
                    errors.push(saved_search_name_taken_user_error());
                }
                if name_is_blank {
                    errors.push(saved_search_name_blank_user_error());
                }
                if !name_is_blank && self.saved_search_name_exists(resource_type, name, except_id) {
                    errors.push(saved_search_name_taken_user_error());
                }
                if resource_type == "CUSTOMER" {
                    errors.push(saved_search_customer_deprecated_user_error());
                }
            }
            SavedSearchQueryValidationOperation::Update => {
                if name_is_blank {
                    errors.push(saved_search_name_blank_user_error());
                }
                if !name_is_blank
                    && (is_reserved_saved_search_name(resource_type, name)
                        || self.saved_search_name_exists(resource_type, name, except_id))
                {
                    errors.push(saved_search_name_taken_user_error());
                }
            }
        }
        if name.chars().count() > 40 {
            errors.push(saved_search_name_too_long_user_error());
        }
        errors
    }
}
