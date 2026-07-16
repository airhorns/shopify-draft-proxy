use super::*;
use crate::graphql::ParsedDocument;

fn segment_gid_tail_sort_value(segment: &Value) -> StagedSortValue {
    let tail = segment
        .get("id")
        .and_then(Value::as_str)
        .map(resource_id_tail)
        .unwrap_or_default();
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn segment_string_sort_value(segment: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(
        segment
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn segment_staged_sort_key(segment: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let primary = match sort_key {
        Some("CREATION_DATE") => segment_string_sort_value(segment, "creationDate"),
        Some("LAST_EDIT_DATE") => segment_string_sort_value(segment, "lastEditDate"),
        None | Some("ID") | Some("RELEVANCE") => segment_gid_tail_sort_value(segment),
        Some(_) => segment_gid_tail_sort_value(segment),
    };
    vec![primary, segment_gid_tail_sort_value(segment)]
}

impl DraftProxy {
    pub(crate) fn resolve_segments_graphql(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            query,
            variables,
            root_name,
            mode,
            ..
        } = invocation;
        match mode {
            LocalResolverMode::OverlayRead => {
                let mut fields = match self.root_fields_or_error(query, variables) {
                    Ok(fields) => fields,
                    Err(_) => return ResolverOutcome::error("Could not parse GraphQL operation"),
                };
                fields.retain(|field| field.response_key == response_key);
                if let Some(field) = fields.first_mut() {
                    field.arguments = arguments
                        .iter()
                        .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                        .collect();
                }
                if root_name == "customerSegmentMembersQuery" {
                    let mut data = self.customer_segment_members_query_read_data(&fields);
                    return ResolverOutcome::value(
                        data.as_object_mut()
                            .and_then(|data| data.remove(response_key))
                            .unwrap_or(Value::Null),
                    );
                }
                if self.store.staged.segments.is_empty() {
                    // With no local segment lifecycle effects, Shopify owns the
                    // catalog, count, detail, cursors, and suggestion taxonomy.
                    return self.forward_upstream_root_outcome(request, response_key);
                }
                let mut upstream_body = None;
                if self.segment_read_needs_upstream_data(&fields) {
                    let outcome = self.forward_upstream_root_outcome(request, response_key);
                    if !outcome.errors.is_empty() {
                        return outcome;
                    }
                    let body = json!({
                        "data": { (response_key): outcome.value }
                    });
                    self.observe_upstream_segment_read_data(&fields, &body);
                    upstream_body = Some(body);
                }
                let (mut data, errors) = self.segment_read_data(&fields, upstream_body.as_ref());
                let value = data
                    .as_object_mut()
                    .and_then(|data| data.remove(response_key))
                    .unwrap_or(Value::Null);
                ResolverOutcome::value(value)
                    .with_errors(root_field_errors_from_json(&errors, response_key))
            }
            LocalResolverMode::StageLocally if root_name == "customerSegmentMembersQueryCreate" => {
                self.customer_segment_members_query_create_outcome(
                    response_key,
                    &arguments,
                    query,
                    variables,
                )
            }
            LocalResolverMode::StageLocally => {
                self.segment_mutation_outcome(response_key, root_name, &arguments, query, variables)
            }
        }
    }

    pub(in crate::proxy) fn segment_read_needs_upstream_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return false;
        }
        fields.iter().any(|field| match field.name.as_str() {
            "segment" => resolved_string_field(&field.arguments, "id").is_some_and(|id| {
                !self.store.staged.segments.is_tombstoned(&id)
                    && self.store.segment_by_id(&id).is_none()
            }),
            "segments" | "segmentsCount" => true,
            name if segment_taxonomy_root(name) => true,
            _ => false,
        })
    }

    pub(in crate::proxy) fn observe_upstream_segment_read_data(
        &mut self,
        fields: &[RootFieldSelection],
        upstream_body: &Value,
    ) {
        for field in fields {
            match field.name.as_str() {
                "segment" => {
                    if let Some(segment) = upstream_segment_root_field(field, upstream_body) {
                        if !segment.is_null() {
                            self.store
                                .observe_base_segment(normalize_segment_record(segment));
                        }
                    }
                }
                "segments" => {
                    for segment in connection_nodes(
                        &upstream_segment_root_field(field, upstream_body).unwrap_or(Value::Null),
                    ) {
                        self.store
                            .observe_base_segment(normalize_segment_record(segment));
                    }
                }
                _ => {}
            }
        }
    }

    fn segment_field_uses_upstream(&self, field: &RootFieldSelection) -> bool {
        match field.name.as_str() {
            "segment" => resolved_string_field(&field.arguments, "id").is_some_and(|id| {
                !self.store.staged.segments.is_tombstoned(&id)
                    && self.store.segment_by_id(&id).is_none()
            }),
            "segments" | "segmentsCount" => true,
            name if segment_taxonomy_root(name) => true,
            _ => false,
        }
    }

    fn segment_upstream_errors(
        &self,
        fields: &[RootFieldSelection],
        upstream_body: Option<&Value>,
    ) -> Vec<Value> {
        let Some(upstream_body) = upstream_body else {
            return Vec::new();
        };
        let response_keys = fields
            .iter()
            .filter(|field| self.segment_field_uses_upstream(field))
            .map(|field| field.response_key.as_str())
            .collect::<Vec<_>>();
        upstream_body["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|error| {
                let response_key = error["path"].as_array()?.first()?.as_str()?;
                response_keys.contains(&response_key).then(|| error.clone())
            })
            .collect()
    }

    pub(in crate::proxy) fn segment_read_data(
        &self,
        fields: &[RootFieldSelection],
        upstream_body: Option<&Value>,
    ) -> (Value, Vec<Value>) {
        let errors = self.segment_upstream_errors(fields, upstream_body);
        let data = root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "segment" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .segment_by_id(&id)
                        .map(|segment| selected_json(segment, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "segments" => {
                    let records = self.segment_overlay_records(field, upstream_body);
                    selected_staged_connection_with_args(
                        records,
                        &field.arguments,
                        &field.selection,
                        segment_overlay_search_decision,
                        segment_staged_sort_key,
                        selected_json,
                        value_id_cursor,
                    )
                }
                "segmentsCount" => self.segment_count_field(field, upstream_body),
                name if segment_taxonomy_root(name) => {
                    upstream_segment_root_field(field, upstream_body.unwrap_or(&Value::Null))
                        .map(|connection| {
                            project_seeded_connection(
                                &connection,
                                &field.arguments,
                                &field.selection,
                            )
                        })
                        .unwrap_or(Value::Null)
                }
                _ => return None,
            })
        });
        (data, errors)
    }

    fn segment_overlay_records(
        &self,
        field: &RootFieldSelection,
        upstream_body: Option<&Value>,
    ) -> Vec<Value> {
        let mut records = self
            .store
            .base
            .segments
            .ordered_values()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        merge_segment_records_from_connection(
            &mut records,
            upstream_segment_root_field(field, upstream_body.unwrap_or(&Value::Null)).as_ref(),
        );
        effective_segment_records_from_base(records, &self.store.staged.segments)
    }

    fn segment_count_field(
        &self,
        field: &RootFieldSelection,
        upstream_body: Option<&Value>,
    ) -> Value {
        let query = resolved_string_field(&field.arguments, "query");
        if let Some((base_count, precision)) = segment_upstream_count_field(field, upstream_body) {
            let mut count = base_count as usize;
            let base_matching_ids = segment_matching_record_ids(
                self.store
                    .base
                    .segments
                    .ordered_values()
                    .into_iter()
                    .cloned(),
                query.as_deref(),
            );
            for id in &self.store.staged.segments.tombstones {
                if base_matching_ids.contains(id) {
                    count = count.saturating_sub(1);
                }
            }
            for (id, segment) in self.store.staged.segments.iter() {
                let matches = segment_overlay_search_decision(segment, query.as_deref())
                    == StagedSearchDecision::Match;
                match self.store.base.segments.get(id) {
                    Some(base) => {
                        let base_matches = segment_overlay_search_decision(base, query.as_deref())
                            == StagedSearchDecision::Match;
                        if base_matches && !matches {
                            count = count.saturating_sub(1);
                        } else if !base_matches && matches {
                            count = count.saturating_add(1);
                        }
                    }
                    None if matches => count = count.saturating_add(1),
                    None => {}
                }
            }
            return selected_json(
                &count_object_with_precision(count, &precision),
                &field.selection,
            );
        }

        let records = self.segment_overlay_records(field, upstream_body);
        let result = staged_connection_query(
            records,
            &field.arguments,
            segment_overlay_search_decision,
            segment_staged_sort_key,
            value_id_cursor,
        );
        selected_json(
            &snapshot_count_with_limit_precision(result.total_count, &field.arguments),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn segment_mutation_outcome(
        &mut self,
        response_key: &str,
        root_field: &str,
        coerced_arguments: &BTreeMap<String, Value>,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let Some(document) = parsed_document(query, variables) else {
            return resolver_http_error_outcome(400, "Could not parse GraphQL operation");
        };
        let Some(mut execution_fields) = self.execution_root_fields(query, variables) else {
            return resolver_http_error_outcome(400, "Operation has no root field");
        };
        execution_fields.retain(|field| {
            field.response_key == response_key
                && matches!(
                    field.name.as_str(),
                    "segmentCreate" | "segmentUpdate" | "segmentDelete"
                )
        });
        let fields = execution_fields;
        if fields.is_empty() {
            return resolver_http_error_outcome(400, "Operation has no root field");
        }
        let now = self.next_product_timestamp();
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for mut field in fields {
            field.arguments = coerced_arguments
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect();
            if let Some(error) =
                segment_required_argument_error(&field.name, &field, &document.operation_path)
            {
                return graphql_error_outcome(vec![error], response_key);
            }
            let payload_selection = field.selection.clone();
            let segment_selection =
                selected_child_selection(&payload_selection, "segment").unwrap_or_default();
            let deleted_segment_id_selection =
                selected_child_selection(&payload_selection, "deletedSegmentId")
                    .unwrap_or_default();
            let arguments = field.arguments.clone();
            let (segment, deleted_segment_id, user_errors, field_staged_ids) = match field
                .name
                .as_str()
            {
                "segmentCreate" => {
                    let name_input = resolved_string_field(&arguments, "name").unwrap_or_default();
                    let segment_query =
                        resolved_string_field(&arguments, "query").unwrap_or_default();
                    let mut user_errors = segment_name_user_errors(&name_input);
                    user_errors.extend(segment_query_change_user_errors(&segment_query));
                    if user_errors.is_empty() {
                        user_errors.extend(segment_query_grammar_user_errors(&segment_query));
                    }
                    let name = name_input.trim().to_string();
                    if user_errors.is_empty() && self.store.effective_segment_count() >= 6000 {
                        user_errors.push(segment_user_error(
                            Value::Null,
                            "Segment limit reached. Delete an existing segment to create more.",
                        ));
                    }
                    let name = if user_errors.is_empty() {
                        match self.segment_available_name(&name, None) {
                            Ok(name) => name,
                            Err(error) => {
                                user_errors.push(error);
                                name
                            }
                        }
                    } else {
                        name
                    };
                    if user_errors.is_empty() {
                        let id = self.next_proxy_synthetic_gid("Segment");
                        let segment = json!({
                            "__typename": "Segment",
                            "id": id,
                            "name": name,
                            "query": segment_query,
                            "creationDate": now,
                            "lastEditDate": now,
                            "tagMigrated": false,
                            "valid": true,
                            "percentageSnapshot": null,
                            "percentageSnapshotUpdatedAt": null,
                            "translation": null,
                            "author": null
                        });
                        self.store
                            .staged
                            .segments
                            .stage(id.clone(), segment.clone());
                        (segment, Value::Null, vec![], vec![id])
                    } else {
                        (Value::Null, Value::Null, user_errors, Vec::new())
                    }
                }
                "segmentUpdate" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(errors) =
                        segment_id_top_level_errors(&id, &field.response_key, &field)
                    {
                        return graphql_error_outcome(errors, response_key);
                    }
                    match self.store.segment_by_id(&id).cloned() {
                        None => (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        ),
                        Some(_)
                            if !segment_update_attribute_present(&arguments, "name")
                                && !segment_update_attribute_present(&arguments, "query") =>
                        {
                            (
                                Value::Null,
                                Value::Null,
                                vec![segment_user_error(
                                    Value::Null,
                                    "At least one attribute to change must be present",
                                )],
                                Vec::new(),
                            )
                        }
                        Some(existing_segment) => {
                            let mut user_errors = Vec::new();
                            let name_input = resolved_string_field(&arguments, "name");
                            let query_input = resolved_string_field(&arguments, "query");
                            if let Some(name) = name_input.as_deref() {
                                user_errors.extend(segment_name_user_errors(name));
                            }
                            if let Some(segment_query) = query_input.as_deref() {
                                user_errors.extend(segment_query_change_user_errors(segment_query));
                            }
                            if user_errors.is_empty() {
                                if let Some(segment_query) = query_input.as_deref() {
                                    user_errors
                                        .extend(segment_query_grammar_user_errors(segment_query));
                                }
                            }
                            let mut new_name =
                                name_input.as_deref().map(str::trim).map(str::to_string);
                            if user_errors.is_empty() {
                                if let Some(name) = new_name.as_deref() {
                                    match self.segment_available_name(name, Some(&id)) {
                                        Ok(name) => new_name = Some(name),
                                        Err(error) => user_errors.push(error),
                                    }
                                }
                            }
                            if user_errors.is_empty() {
                                let mut segment = existing_segment;
                                if let Some(name) = new_name {
                                    segment["name"] = json!(name);
                                }
                                if let Some(segment_query) = query_input {
                                    segment["query"] = json!(segment_query);
                                }
                                segment["lastEditDate"] = json!(now);
                                self.store
                                    .staged
                                    .segments
                                    .stage(id.clone(), segment.clone());
                                (segment, Value::Null, vec![], vec![id])
                            } else {
                                (Value::Null, Value::Null, user_errors, Vec::new())
                            }
                        }
                    }
                }
                "segmentDelete" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(errors) =
                        segment_id_top_level_errors(&id, &field.response_key, &field)
                    {
                        return graphql_error_outcome(errors, response_key);
                    }
                    if self.store.segment_by_id(&id).is_some() {
                        self.store.staged.segments.remove_staged(&id);
                        self.store.staged.segments.tombstone(id.clone());
                        (Value::Null, json!(id.clone()), vec![], vec![id])
                    } else {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        )
                    }
                }
                _ => (Value::Null, Value::Null, vec![], Vec::new()),
            };
            staged_ids.extend(field_staged_ids);
            data.insert(
                field.response_key.clone(),
                segment_payload_json(
                    segment,
                    deleted_segment_id,
                    &payload_selection,
                    &segment_selection,
                    &deleted_segment_id_selection,
                    user_errors,
                ),
            );
        }
        let value = data.remove(response_key).unwrap_or(Value::Null);
        let outcome = ResolverOutcome::value(value);
        if staged_ids.is_empty() {
            outcome
        } else {
            outcome.with_log_draft(LogDraft::staged(root_field, "segments", staged_ids))
        }
    }

    fn segment_available_name(
        &self,
        requested_name: &str,
        exclude_id: Option<&str>,
    ) -> Result<String, Value> {
        if !self.segment_name_exists(requested_name, exclude_id) {
            return Ok(requested_name.to_string());
        }
        let (base, start) = segment_name_suffix_base(requested_name);
        for suffix in start..=100 {
            let candidate = format!("{base} ({suffix})");
            if !self.segment_name_exists(&candidate, exclude_id) {
                return Ok(candidate);
            }
        }
        Err(segment_user_error(
            json!(["name"]),
            "Name has already been taken",
        ))
    }

    fn segment_name_exists(&self, name: &str, exclude_id: Option<&str>) -> bool {
        let matches_name = |id: &str, segment: &Value| {
            let id = segment.get("id").and_then(Value::as_str).unwrap_or(id);
            exclude_id != Some(id) && segment["name"].as_str() == Some(name)
        };
        self.store
            .staged
            .segments
            .iter()
            .any(|(id, segment)| matches_name(id, segment))
            || self
                .store
                .base
                .segments
                .records
                .iter()
                .any(|(id, segment)| {
                    !self.store.staged.segments.is_tombstoned(id)
                        && !self.store.staged.segments.contains_staged(id)
                        && matches_name(id, segment)
                })
    }

    pub(in crate::proxy) fn customer_segment_members_query_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            if field.name != "customerSegmentMembersQuery" {
                return None;
            }
            let value = field
                .arguments
                .get("id")
                .and_then(resolved_value_string)
                .and_then(|id| {
                    self.store
                        .staged
                        .customer_segment_member_queries
                        .get(&id)
                        .cloned()
                })
                .map(|query| selected_json(&query, &field.selection))
                .unwrap_or(Value::Null);
            Some(value)
        })
    }

    pub(in crate::proxy) fn customer_segment_members_query_create_outcome(
        &mut self,
        response_key: &str,
        coerced_arguments: &BTreeMap<String, Value>,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let Some(field) = self
            .execution_root_fields(query, variables)
            .and_then(|fields| {
                fields
                    .into_iter()
                    .find(|field| field.response_key == response_key)
            })
        else {
            return ResolverOutcome::error("No customerSegmentMembersQueryCreate root field found");
        };
        let payload_selection = field.selection;
        let arguments = coerced_arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect::<BTreeMap<_, _>>();
        let query_selection =
            selected_child_selection(&payload_selection, "customerSegmentMembersQuery")
                .unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let query_input = resolved_string_field(&input, "query");
        let segment_id_input = resolved_string_field(&input, "segmentId");
        if let Some(errors) =
            member_query_segment_id_top_level_error(query, variables, segment_id_input.as_deref())
        {
            return graphql_error_outcome(errors, response_key);
        }
        let user_errors = match (query_input.as_deref(), segment_id_input.as_deref()) {
            (Some(_), Some(_)) => vec![member_query_user_error(
                json!(["input"]),
                "Providing both segment_id and query is not supported.",
            )],
            (None, None) => vec![member_query_user_error(
                json!(["input"]),
                "You must provide one of segment_id or query.",
            )],
            // A direct query goes through the Customer Data Platform grammar; a
            // malformed query returns a CDP-shaped error (field null) while broad
            // valid grammar stages an async job.
            (Some(direct_query), None) => member_query_direct_query_error(direct_query)
                .into_iter()
                .collect(),
            // A segment_id reuses a stored segment's query without revalidating it,
            // but the segment must exist in the shop.
            (None, Some(segment_id)) => {
                if self.store.segment_by_id(segment_id).is_some() {
                    Vec::new()
                } else {
                    vec![member_query_user_error(Value::Null, "Invalid segment ID.")]
                }
            }
        };
        if !user_errors.is_empty() {
            return ResolverOutcome::value(customer_segment_members_query_payload_json(
                Value::Null,
                &payload_selection,
                &query_selection,
                user_errors,
            ));
        }

        let id = self.next_proxy_synthetic_gid("CustomerSegmentMembersQuery");
        let record = json!({
            "id": id,
            "currentCount": 0,
            "done": false,
            "status": "INITIALIZED"
        });
        self.store
            .staged
            .customer_segment_member_queries
            .insert(id.clone(), record.clone());
        ResolverOutcome::value(customer_segment_members_query_payload_json(
            record,
            &payload_selection,
            &query_selection,
            vec![],
        ))
        .with_log_draft(LogDraft::staged(
            "customerSegmentMembersQueryCreate",
            "segments",
            vec![id],
        ))
    }
}

fn segment_taxonomy_root(name: &str) -> bool {
    matches!(
        name,
        "segmentFilters"
            | "segmentFilterSuggestions"
            | "segmentValueSuggestions"
            | "segmentMigrations"
    )
}

fn upstream_segment_root_field(field: &RootFieldSelection, upstream_body: &Value) -> Option<Value> {
    upstream_body
        .get("data")
        .and_then(|data| data.get(field.response_key.as_str()))
        .cloned()
}

fn segment_upstream_count_field(
    field: &RootFieldSelection,
    upstream_body: Option<&Value>,
) -> Option<(u64, String)> {
    let value = upstream_segment_root_field(field, upstream_body?)?;
    let count = value.get("count").and_then(Value::as_u64)?;
    let precision = value
        .get("precision")
        .and_then(Value::as_str)
        .unwrap_or("EXACT")
        .to_string();
    Some((count, precision))
}

fn segment_record_id(segment: &Value) -> Option<String> {
    segment
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn normalize_segment_record(mut segment: Value) -> Value {
    if let Some(object) = segment.as_object_mut() {
        object
            .entry("__typename".to_string())
            .or_insert_with(|| json!("Segment"));
    }
    segment
}

fn merge_segment_records_from_connection(records: &mut Vec<Value>, connection: Option<&Value>) {
    let mut by_id = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| segment_record_id(record).map(|id| (id, index)))
        .collect::<BTreeMap<_, _>>();
    for upstream in connection_nodes(connection.unwrap_or(&Value::Null)) {
        let upstream = normalize_segment_record(upstream);
        let Some(id) = segment_record_id(&upstream) else {
            continue;
        };
        if let Some(index) = by_id.get(&id).copied() {
            merge_segment_record_fields(&mut records[index], &upstream);
        } else {
            by_id.insert(id, records.len());
            records.push(upstream);
        }
    }
}

fn merge_segment_record_fields(target: &mut Value, source: &Value) {
    let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) else {
        return;
    };
    for (key, value) in source {
        if !value.is_null() {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn effective_segment_records_from_base(
    base_records: Vec<Value>,
    staged: &StagedRecords<Value>,
) -> Vec<Value> {
    let mut records_by_id = BTreeMap::new();
    let mut ordered_ids = Vec::new();
    for record in base_records {
        let Some(id) = segment_record_id(&record) else {
            continue;
        };
        if staged.is_tombstoned(&id) {
            continue;
        }
        if !records_by_id.contains_key(&id) {
            ordered_ids.push(id.clone());
        }
        let record = staged.get(&id).cloned().unwrap_or(record);
        records_by_id.insert(id, record);
    }
    for (id, segment) in staged.iter() {
        if staged.is_tombstoned(id) {
            continue;
        }
        if !records_by_id.contains_key(id) {
            ordered_ids.push(id.clone());
        }
        records_by_id.insert(id.clone(), segment.clone());
    }
    ordered_ids
        .into_iter()
        .filter_map(|id| records_by_id.remove(&id))
        .collect()
}

fn segment_matching_record_ids(
    records: impl IntoIterator<Item = Value>,
    query: Option<&str>,
) -> BTreeSet<String> {
    records
        .into_iter()
        .filter(|segment| {
            segment_overlay_search_decision(segment, query) == StagedSearchDecision::Match
        })
        .filter_map(|segment| segment_record_id(&segment))
        .collect()
}

fn segment_overlay_search_decision(segment: &Value, query: Option<&str>) -> StagedSearchDecision {
    match segment_search_decision(segment, query) {
        StagedSearchDecision::Unsupported => StagedSearchDecision::Match,
        decision => decision,
    }
}

fn segment_search_decision(segment: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    let mut saw_supported = false;
    for term in query.split_whitespace() {
        match segment_search_term_decision(segment, term) {
            StagedSearchDecision::Match => saw_supported = true,
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::from_bool(saw_supported)
}

fn segment_search_term_decision(segment: &Value, term: &str) -> StagedSearchDecision {
    let Some((field, value)) = term.split_once(':') else {
        return StagedSearchDecision::from_bool(
            segment_text_field_contains(segment, "id", term)
                || segment_text_field_contains(segment, "name", term)
                || segment_text_field_contains(segment, "query", term),
        );
    };
    match field.to_ascii_lowercase().as_str() {
        "id" => StagedSearchDecision::from_bool(segment_text_field_contains(segment, "id", value)),
        "name" => {
            StagedSearchDecision::from_bool(segment_text_field_contains(segment, "name", value))
        }
        "query" => {
            StagedSearchDecision::from_bool(segment_text_field_contains(segment, "query", value))
        }
        _ => StagedSearchDecision::Unsupported,
    }
}

fn segment_text_field_contains(segment: &Value, field: &str, needle: &str) -> bool {
    let needle = needle
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    segment
        .get(field)
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase().contains(&needle))
        .unwrap_or(false)
}

pub(in crate::proxy) fn segment_payload_json(
    segment: Value,
    deleted_segment_id: Value,
    payload_selection: &[SelectedField],
    segment_selection: &[SelectedField],
    deleted_segment_id_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "segment" => Some(if segment.is_null() {
                Value::Null
            } else {
                selected_json(&segment, segment_selection)
            }),
            "deletedSegmentId" => Some(if deleted_segment_id_selection.is_empty() {
                deleted_segment_id.clone()
            } else {
                selected_json(&deleted_segment_id, deleted_segment_id_selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn customer_segment_members_query_payload_json(
    query_record: Value,
    payload_selection: &[SelectedField],
    query_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selection, |selection| {
        match selection.name.as_str() {
            "customerSegmentMembersQuery" => Some(if query_record.is_null() {
                Value::Null
            } else {
                selected_json(&query_record, query_selection)
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

fn segment_user_error(field: Value, message: &str) -> Value {
    user_error_typed_omit_code("UserError", field, message, None)
}

fn segment_presence_user_error(field: impl Into<UserErrorField>, field_name: &str) -> Value {
    let mut error = presence_user_error(field, field_name);
    error["__typename"] = json!("UserError");
    error
}

fn segment_length_user_error(
    field: impl Into<UserErrorField>,
    field_name: &str,
    bound: LengthUserErrorBound,
) -> Value {
    let mut error = length_user_error(field, field_name, bound);
    error["__typename"] = json!("UserError");
    error
}

fn segment_name_user_errors(name: &str) -> Vec<Value> {
    let stripped = name.trim();
    if stripped.is_empty() {
        vec![segment_presence_user_error(["name"], "Name")]
    } else if stripped.chars().count() > 255 {
        vec![segment_length_user_error(
            ["name"],
            "Name",
            LengthUserErrorBound::TooLong { maximum: 255 },
        )]
    } else {
        Vec::new()
    }
}

fn segment_query_change_user_errors(query: &str) -> Vec<Value> {
    if query.trim().is_empty() {
        return vec![segment_presence_user_error(["query"], "Query")];
    }
    if query.chars().count() > 5000 {
        return vec![segment_length_user_error(
            ["query"],
            "Query",
            LengthUserErrorBound::TooLong { maximum: 5000 },
        )];
    }
    Vec::new()
}

/// A `CustomerSegmentMembersQueryUserError` (the CDP member-query surface),
/// which always carries a `code` and `__typename` unlike the default segment
/// mutation `UserError`.
fn member_query_user_error(field: Value, message: &str) -> Value {
    user_error_typed(
        "CustomerSegmentMembersQueryUserError",
        field,
        message,
        Some("INVALID"),
    )
}

/// Validate a `customerSegmentMembersQueryCreate(input: { query })` direct query
/// through the segment grammar. Returns `None` when the query parses (the job is
/// staged); otherwise a CDP-shaped error pointing at the first unexpected token.
fn member_query_direct_query_error(query: &str) -> Option<Value> {
    let trimmed = query.trim();
    if !trimmed.is_empty() && segment_query_grammar_accepts(trimmed) {
        return None;
    }
    let message = segment_query_unexpected_token_message(query)
        .unwrap_or_else(|| "Query is invalid.".to_string());
    Some(member_query_user_error(Value::Null, &message))
}

fn member_query_segment_id_top_level_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    segment_id: Option<&str>,
) -> Option<Vec<Value>> {
    let segment_id = segment_id?;
    let document = parsed_document(query, variables)?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "customerSegmentMembersQueryCreate")?;
    match shopify_gid_resource_type(segment_id) {
        Some("Segment") => None,
        Some(_) => segment_id_top_level_errors(segment_id, &field.response_key, field),
        None => Some(vec![member_query_segment_id_invalid_variable_error(
            &document, field, segment_id,
        )
        .unwrap_or_else(|| {
            member_query_segment_id_invalid_literal_error(&document, field, segment_id)
        })]),
    }
}

fn member_query_segment_id_invalid_variable_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    segment_id: &str,
) -> Option<Value> {
    let RawArgumentValue::Variable { name, value } = field.raw_arguments.get("input")? else {
        return None;
    };
    let value = value.as_ref()?;
    let variable_definition = document.variable_definitions.get(name)?;
    Some(invalid_variable_error(
        VariableValidationContext {
            variable_name: name,
            variable_type: &variable_definition.type_display,
            location: variable_definition.location,
        },
        value,
        vec![variable_problem_with_message_value_path(
            &[json!("segmentId")],
            &format!("Invalid global id '{segment_id}'"),
        )],
    ))
}

fn member_query_segment_id_invalid_literal_error(
    document: &ParsedDocument,
    field: &RootFieldSelection,
    segment_id: &str,
) -> Value {
    json!({
        "message": format!("Invalid global id '{segment_id}'"),
        "locations": [{"line": field.location.line, "column": field.location.column}],
        "path": [
            document.operation_path.as_str(),
            field.response_key.as_str(),
            "input",
            "segmentId"
        ],
        "extensions": {
            "code": "argumentLiteralsIncompatible",
            "typeName": "CoercionError"
        }
    })
}

/// Locate the first token that cannot continue a `[NOT] <filter> <operator>`
/// prefix and render Shopify's `Line 1 Column N: 'TOKEN' is unexpected.` lexer
/// message. The reported column is the position just past the previous token
/// (where the parser expected an operator / continuation).
fn segment_query_unexpected_token_message(query: &str) -> Option<String> {
    let tokens = segment_query_tokens(query);
    if tokens.is_empty() {
        return None;
    }
    let mut index = 0;
    // An optional leading boolean NOT prefix is consumed before the filter name.
    if tokens[index].text.eq_ignore_ascii_case("not") {
        index += 1;
    }
    if index >= tokens.len() {
        return None;
    }
    // Consume the filter identifier; an operator must follow.
    index += 1;
    if index < tokens.len() {
        let token = &tokens[index];
        if !segment_query_token_is_operator(&token.text) {
            let column = tokens[index - 1].end_column + 1;
            return Some(format!(
                "Line 1 Column {column}: '{}' is unexpected.",
                token.text
            ));
        }
    }
    None
}

#[derive(Debug)]
struct SegmentQueryToken {
    text: String,
    start_column: usize,
    end_column: usize,
}

fn segment_query_tokens(query: &str) -> Vec<SegmentQueryToken> {
    let chars: Vec<char> = query.chars().collect();
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (index, ch) in chars.iter().enumerate() {
        if ch.is_whitespace() {
            if let Some(begin) = start.take() {
                tokens.push(SegmentQueryToken {
                    text: chars[begin..index].iter().collect(),
                    start_column: begin + 1,
                    end_column: index,
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start.take() {
        tokens.push(SegmentQueryToken {
            text: chars[begin..].iter().collect(),
            start_column: begin + 1,
            end_column: chars.len(),
        });
    }
    tokens
}

/// Whether a token can begin the operator / continuation that follows a segment
/// filter name (comparison, set membership, null test, or boolean join).
fn segment_query_token_is_operator(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "=" | "!="
            | ">"
            | "<"
            | ">="
            | "<="
            | "BETWEEN"
            | "CONTAINS"
            | "IS"
            | "NOT"
            | "STARTS"
            | "AND"
            | "OR"
    )
}

fn segment_query_grammar_user_errors(query: &str) -> Vec<Value> {
    let stripped = query.trim();
    if segment_query_grammar_accepts(stripped) {
        Vec::new()
    } else {
        segment_query_grammar_error_messages(stripped)
            .into_iter()
            .map(|message| segment_user_error(json!(["query"]), &message))
            .collect()
    }
}

fn segment_query_grammar_error_messages(query: &str) -> Vec<String> {
    let mut messages = Vec::new();
    if let Some(message) = segment_query_unexpected_token_message(query) {
        messages.push(format!("Query {message}"));
    }
    if let Some(message) = segment_query_filter_not_found_message(query) {
        messages.push(message);
    }
    if messages.is_empty() {
        messages.push(segment_query_input_derived_invalid_message(query));
    }
    messages
}

fn segment_query_filter_not_found_message(query: &str) -> Option<String> {
    let tokens = segment_query_tokens(query);
    let mut index = 0;
    let mut column = tokens.first()?.start_column;
    if tokens[index].text.eq_ignore_ascii_case("not") {
        column = tokens[index].end_column + 1;
        index += 1;
    }
    let token = tokens.get(index)?;
    if segment_query_filter_name_is_known(&token.text) {
        return None;
    }
    Some(format!(
        "Query Line 1 Column {column}: '{}' filter cannot be found.",
        token.text
    ))
}

fn segment_query_input_derived_invalid_message(query: &str) -> String {
    segment_query_tokens(query)
        .last()
        .map(|token| {
            format!(
                "Query Line 1 Column {}: segment query is invalid near '{}'.",
                token.start_column, token.text
            )
        })
        .unwrap_or_else(|| "Invalid segment query".to_string())
}

fn segment_query_grammar_accepts(query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    if query.starts_with('(') && query.ends_with(')') {
        let mut depth = 0i32;
        let mut wraps = true;
        for (index, ch) in query.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && index != query.len() - 1 {
                        wraps = false;
                        break;
                    }
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if wraps && depth == 0 {
            return segment_query_grammar_accepts(&query[1..query.len() - 1]);
        }
    }
    if let Some((left, right)) = split_segment_query_boolean(query, " OR ") {
        return segment_query_grammar_accepts(left) && segment_query_grammar_accepts(right);
    }
    if segment_query_predicate_accepts(query) {
        return true;
    }
    if let Some((left, right)) = split_segment_query_boolean(query, " AND ") {
        return segment_query_grammar_accepts(left) && segment_query_grammar_accepts(right);
    }
    false
}

const SEGMENT_QUERY_FILTERS: &[&str] = &[
    "number_of_orders",
    "amount_spent",
    "customer_countries",
    "customer_tags",
    "email_subscription_status",
    "last_order_date",
    "companies",
];

fn segment_query_predicate_accepts(query: &str) -> bool {
    let Some((filter, rest)) = split_segment_query_filter(query) else {
        return false;
    };
    if !segment_query_filter_name_is_valid(filter) {
        return false;
    }
    if !segment_query_filter_name_is_known(filter) {
        return segment_query_unknown_filter_accepts(rest);
    }

    if filter == "companies" {
        return matches!(rest, "IS NULL" | "IS NOT NULL");
    }
    if let Some(value) = rest.strip_prefix("NOT CONTAINS ") {
        return matches!(filter, "customer_tags" | "customer_countries")
            && segment_query_value_is_quoted(value);
    }
    if let Some(value) = rest.strip_prefix("CONTAINS ") {
        return matches!(filter, "customer_tags" | "customer_countries")
            && segment_query_value_is_quoted(value);
    }
    if let Some((lower, upper)) = split_segment_query_between(rest) {
        return match filter {
            "number_of_orders" => {
                segment_query_value_is_integer(lower) && segment_query_value_is_integer(upper)
            }
            "amount_spent" => {
                segment_query_value_is_decimal(lower) && segment_query_value_is_decimal(upper)
            }
            "last_order_date" => {
                segment_query_value_is_date_like(lower) && segment_query_value_is_date_like(upper)
            }
            _ => false,
        };
    }
    if let Some((operator, value)) = split_segment_query_operator(rest) {
        return match filter {
            "number_of_orders" => segment_query_value_is_integer(value),
            "amount_spent" => segment_query_value_is_decimal(value),
            "email_subscription_status" => operator == "=" && segment_query_value_is_quoted(value),
            "last_order_date" => {
                matches!(operator, "=" | ">" | ">=" | "<" | "<=")
                    && segment_query_value_is_date_like(value)
            }
            _ => false,
        };
    }
    false
}

fn split_segment_query_filter(query: &str) -> Option<(&str, &str)> {
    let index = query.find(char::is_whitespace)?;
    let filter = &query[..index];
    let rest = query[index..].trim();
    if rest.is_empty() {
        return None;
    }
    Some((filter, rest))
}

fn segment_query_filter_name_is_valid(filter: &str) -> bool {
    let mut chars = filter.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn segment_query_filter_name_is_known(filter: &str) -> bool {
    SEGMENT_QUERY_FILTERS.contains(&filter)
}

fn segment_query_unknown_filter_accepts(rest: &str) -> bool {
    if matches!(rest, "IS NULL" | "IS NOT NULL") {
        return true;
    }
    if let Some(value) = rest.strip_prefix("NOT CONTAINS ") {
        return segment_query_value_is_quoted(value);
    }
    if let Some(value) = rest.strip_prefix("CONTAINS ") {
        return segment_query_value_is_quoted(value);
    }
    if let Some((lower, upper)) = split_segment_query_between(rest) {
        return segment_query_value_is_literal(lower) && segment_query_value_is_literal(upper);
    }
    if let Some((_, value)) = split_segment_query_operator(rest) {
        return segment_query_value_is_literal(value);
    }
    false
}

fn split_segment_query_boolean<'a>(query: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0i32;
    for (index, ch) in query.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && query[index..].starts_with(operator) {
            return Some((&query[..index], &query[index + operator.len()..]));
        }
    }
    None
}

fn split_segment_query_operator(rest: &str) -> Option<(&str, &str)> {
    for operator in [">=", "<=", "!=", ">", "<", "="] {
        if let Some(value) = rest.strip_prefix(operator) {
            return Some((operator, value.trim()));
        }
    }
    None
}

fn split_segment_query_between(rest: &str) -> Option<(&str, &str)> {
    let values = rest.strip_prefix("BETWEEN ")?;
    let (lower, upper) = values.split_once(" AND ")?;
    if upper.contains(" AND ") {
        return None;
    }
    let lower = lower.trim();
    let upper = upper.trim();
    if lower.is_empty() || upper.is_empty() {
        return None;
    }
    Some((lower, upper))
}

fn segment_query_value_is_quoted(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'')
}

fn segment_query_value_is_literal(value: &str) -> bool {
    segment_query_value_is_quoted(value)
        || segment_query_value_is_decimal(value)
        || segment_query_value_is_relative_date(value)
        || segment_query_value_is_bare(value)
}

fn segment_query_value_is_date_like(value: &str) -> bool {
    segment_query_value_is_relative_date(value) || segment_query_value_is_quoted(value)
}

fn segment_query_value_is_integer(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

fn segment_query_value_is_decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let Some((whole, fraction)) = value.split_once('.') else {
        return segment_query_value_is_integer(value);
    };
    !whole.is_empty()
        && !fraction.is_empty()
        && whole.chars().all(|ch| ch.is_ascii_digit())
        && fraction.chars().all(|ch| ch.is_ascii_digit())
}

fn segment_query_value_is_relative_date(value: &str) -> bool {
    let Some(value) = value.strip_prefix('-') else {
        return false;
    };
    let Some(days) = value.strip_suffix('d') else {
        return false;
    };
    !days.is_empty() && days.chars().all(|ch| ch.is_ascii_digit())
}

fn segment_query_value_is_bare(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.'))
}

fn segment_name_suffix_base(name: &str) -> (&str, u32) {
    let Some(prefix) = name.strip_suffix(')') else {
        return (name, 2);
    };
    let Some((base, suffix)) = prefix.rsplit_once(" (") else {
        return (name, 2);
    };
    let Some(number) = suffix.parse::<u32>().ok() else {
        return (name, 2);
    };
    (base, number + 1)
}

fn segment_update_attribute_present(
    arguments: &BTreeMap<String, ResolvedValue>,
    attribute: &str,
) -> bool {
    arguments
        .get(attribute)
        .is_some_and(|value| !matches!(value, ResolvedValue::Null))
}

fn segment_required_argument_error(
    root_field: &str,
    field: &RootFieldSelection,
    operation_path: &str,
) -> Option<Value> {
    let required: &[(&str, &str)] = match root_field {
        "segmentCreate" => &[("name", "String!"), ("query", "String!")],
        "segmentUpdate" | "segmentDelete" => &[("id", "ID!")],
        _ => &[],
    };
    let missing: Vec<&str> = required
        .iter()
        .filter_map(|(name, _)| (!field.raw_arguments.contains_key(*name)).then_some(*name))
        .collect();
    if !missing.is_empty() {
        let arguments = missing.join(", ");
        return Some(missing_required_arguments_error(
            root_field,
            &arguments,
            field.location,
            vec![json!(operation_path), json!(root_field)],
        ));
    }
    for (name, argument_type) in required {
        if field
            .raw_arguments
            .get(*name)
            .is_some_and(RawArgumentValue::is_literal_null)
        {
            return Some(required_argument_null_error(
                root_field,
                name,
                argument_type,
                field.location,
                vec![json!(operation_path), json!(root_field), json!(name)],
            ));
        }
    }
    None
}

fn segment_id_top_level_errors(
    id: &str,
    response_key: &str,
    field: &RootFieldSelection,
) -> Option<Vec<Value>> {
    match shopify_gid_resource_type(id) {
        Some("Segment") => None,
        Some(_) => Some(vec![json!({
            "message": "invalid id",
            "locations": [{"line": field.location.line, "column": field.location.column}],
            "extensions": {"code": "RESOURCE_NOT_FOUND"},
            "path": [response_key]
        })]),
        None => Some(vec![json!({
            "message": "Variable $id of type ID! was provided invalid value",
            "locations": [{"line": 2, "column": 38}],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": id,
                "problems": [{
                    "path": [],
                    "explanation": format!("Invalid global id '{id}'"),
                    "message": format!("Invalid global id '{id}'")
                }]
            }
        })]),
    }
}
