use super::*;
use crate::graphql::ParsedDocument;

impl DraftProxy {
    pub(in crate::proxy) fn segment_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> (Value, Vec<Value>) {
        let mut errors = Vec::new();
        let data = root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "segment" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    match self.store.staged.segments.get(&id) {
                        Some(segment) => selected_json(segment, &field.selection),
                        None => {
                            errors.push(json!({
                                "message": "Segment does not exist",
                                "locations": [{
                                    "line": field.location.line,
                                    "column": field.location.column
                                }],
                                "extensions": { "code": "NOT_FOUND" },
                                "path": [field.response_key.clone()]
                            }));
                            Value::Null
                        }
                    }
                }
                "segments" => {
                    if let Some(connection) = self.store.staged.segment_catalog.get("segments") {
                        project_seeded_connection(connection, &field.arguments, &field.selection)
                    } else {
                        let records = self
                            .store
                            .staged
                            .segments
                            .values()
                            .cloned()
                            .collect::<Vec<_>>();
                        selected_connection_json_with_args(
                            records,
                            &field.arguments,
                            &field.selection,
                            value_id_cursor,
                        )
                    }
                }
                "segmentsCount" => match self.store.staged.segment_catalog.get("segmentsCount") {
                    Some(count) => selected_json(count, &field.selection),
                    None => selected_count_json(self.store.staged.segments.len(), &field.selection),
                },
                "segmentFilters"
                | "segmentFilterSuggestions"
                | "segmentValueSuggestions"
                | "segmentMigrations" => match self.store.staged.segment_catalog.get(&field.name) {
                    Some(connection) => {
                        project_seeded_connection(connection, &field.arguments, &field.selection)
                    }
                    None => return None,
                },
                _ => return None,
            })
        });
        (data, errors)
    }

    pub(in crate::proxy) fn segment_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let fields = document
            .root_fields
            .iter()
            .filter(|field| {
                matches!(
                    field.name.as_str(),
                    "segmentCreate" | "segmentUpdate" | "segmentDelete"
                )
            })
            .collect::<Vec<_>>();
        if fields.is_empty() {
            return json_error(400, "Operation has no root field");
        }
        let now = self.next_product_timestamp();
        let mut data = serde_json::Map::new();
        let mut staged_ids = Vec::new();
        for field in fields {
            if let Some(error) =
                segment_required_argument_error(&field.name, field, &document.operation_path)
            {
                return ok_json(json!({ "errors": [error] }));
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
                    if user_errors.is_empty() && self.store.staged.segments.len() >= 6000 {
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
                            .insert(id.clone(), segment.clone());
                        (segment, Value::Null, vec![], vec![id])
                    } else {
                        (Value::Null, Value::Null, user_errors, Vec::new())
                    }
                }
                "segmentUpdate" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(response) =
                        segment_id_top_level_error(&id, &field.response_key, field)
                    {
                        return response;
                    }
                    if !self.store.staged.segments.contains_key(&id) {
                        (
                            Value::Null,
                            Value::Null,
                            vec![segment_user_error(json!(["id"]), "Segment does not exist")],
                            Vec::new(),
                        )
                    } else if !segment_update_attribute_present(&arguments, "name")
                        && !segment_update_attribute_present(&arguments, "query")
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
                    } else {
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
                        let mut new_name = name_input.as_deref().map(str::trim).map(str::to_string);
                        if user_errors.is_empty() {
                            if let Some(name) = new_name.as_deref() {
                                match self.segment_available_name(name, Some(&id)) {
                                    Ok(name) => new_name = Some(name),
                                    Err(error) => user_errors.push(error),
                                }
                            }
                        }
                        if user_errors.is_empty() {
                            let mut segment = self.store.staged.segments.get(&id).cloned().unwrap();
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
                                .insert(id.clone(), segment.clone());
                            (segment, Value::Null, vec![], vec![id])
                        } else {
                            (Value::Null, Value::Null, user_errors, Vec::new())
                        }
                    }
                }
                "segmentDelete" => {
                    let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                    if let Some(response) =
                        segment_id_top_level_error(&id, &field.response_key, field)
                    {
                        return response;
                    }
                    if self.store.staged.segments.remove(&id).is_some() {
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
        if !staged_ids.is_empty() {
            self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        }
        ok_json(json!({ "data": data }))
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
        self.store.staged.segments.iter().any(|(id, segment)| {
            exclude_id != Some(id.as_str()) && segment["name"].as_str() == Some(name)
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

    pub(in crate::proxy) fn customer_segment_members_query_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) =
            primary_root_response_parts(query, variables, || {
                "customerSegmentMembersQueryCreate".to_string()
            });
        let query_selection =
            selected_child_selection(&payload_selection, "customerSegmentMembersQuery")
                .unwrap_or_default();
        let input = resolved_object_field(&arguments, "input").unwrap_or_default();
        let query_input = resolved_string_field(&input, "query");
        let segment_id_input = resolved_string_field(&input, "segmentId");
        if let Some(response) =
            member_query_segment_id_top_level_error(query, variables, segment_id_input.as_deref())
        {
            return response;
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
                if self.store.staged.segments.contains_key(segment_id) {
                    Vec::new()
                } else {
                    vec![member_query_user_error(Value::Null, "Invalid segment ID.")]
                }
            }
        };
        if !user_errors.is_empty() {
            return ok_json(json!({
                "data": {
                    response_key: customer_segment_members_query_payload_json(
                        Value::Null,
                        &payload_selection,
                        &query_selection,
                        user_errors,
                    )
                }
            }));
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
        self.record_mutation_log_entry(
            request,
            query,
            variables,
            "customerSegmentMembersQueryCreate",
            vec![id],
        );
        ok_json(json!({
            "data": {
                response_key: customer_segment_members_query_payload_json(
                    record,
                    &payload_selection,
                    &query_selection,
                    vec![],
                )
            }
        }))
    }
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
) -> Option<Response> {
    let segment_id = segment_id?;
    let document = parsed_document(query, variables)?;
    let field = document
        .root_fields
        .iter()
        .find(|field| field.name == "customerSegmentMembersQueryCreate")?;
    match shopify_gid_resource_type(segment_id) {
        Some("Segment") => None,
        Some(_) => segment_id_top_level_error(segment_id, &field.response_key, field),
        None => Some(ok_json(json!({
            "errors": [member_query_segment_id_invalid_variable_error(&document, field, segment_id)
                .unwrap_or_else(|| member_query_segment_id_invalid_literal_error(&document, field, segment_id))]
        }))),
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
        return Some(json!({
            "message": format!("Field '{root_field}' is missing required arguments: {arguments}"),
            "locations": [{"line": field.location.line, "column": field.location.column}],
            "path": [operation_path, root_field],
            "extensions": {
                "code": "missingRequiredArguments",
                "className": "Field",
                "name": root_field,
                "arguments": arguments
            }
        }));
    }
    for (name, argument_type) in required {
        if field
            .raw_arguments
            .get(*name)
            .is_some_and(RawArgumentValue::is_literal_null)
        {
            return Some(json!({
                "message": format!("Argument '{name}' on Field '{root_field}' has an invalid value (null). Expected type '{argument_type}'."),
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "path": [operation_path, root_field, *name],
                "extensions": {
                    "code": "argumentLiteralsIncompatible",
                    "typeName": "Field",
                    "argumentName": *name
                }
            }));
        }
    }
    None
}

fn segment_id_top_level_error(
    id: &str,
    response_key: &str,
    field: &RootFieldSelection,
) -> Option<Response> {
    match shopify_gid_resource_type(id) {
        Some("Segment") => None,
        Some(_) => Some(ok_json(json!({
            "errors": [{
                "message": "invalid id",
                "locations": [{"line": field.location.line, "column": field.location.column}],
                "extensions": {"code": "RESOURCE_NOT_FOUND"},
                "path": [response_key]
            }],
            "data": { response_key: null }
        }))),
        None => Some(ok_json(json!({
            "errors": [{
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
            }]
        }))),
    }
}
