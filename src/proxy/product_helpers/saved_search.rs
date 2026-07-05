use super::*;

pub(in crate::proxy) fn saved_search_connection_json(
    records: &[SavedSearchRecord],
    root_selection: &[SelectedField],
    api_client_id: &str,
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    selected_typed_connection(
        records,
        root_selection,
        |record, selections| {
            saved_search_json_with_query(
                record,
                selections,
                &saved_search_read_query_for_api_client(&record.query, api_client_id),
                api_client_id,
            )
        },
        saved_search_cursor,
        |page_info_selection| {
            saved_search_page_info_json(
                records,
                page_info_selection,
                has_next_page,
                has_previous_page,
            )
        },
    )
}

pub(in crate::proxy) fn saved_search_json_with_query(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
    query_display: &str,
    api_client_id: &str,
) -> Value {
    let filters = saved_search_filters_for_api_client(query_display, api_client_id);
    let legacy_id = saved_search_legacy_resource_id(&record.id);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SavedSearch")),
        "id" => Some(json!(record.id)),
        "legacyResourceId" => Some(json!(legacy_id)),
        "name" => Some(json!(record.name)),
        "query" => Some(json!(query_display)),
        "resourceType" => Some(json!(record.resource_type)),
        "searchTerms" => Some(json!(saved_search_search_terms(query_display))),
        "filters" => Some(Value::Array(
            filters
                .iter()
                .map(|(key, value)| saved_search_filter_json(key, value, &selection.selection))
                .collect(),
        )),
        _ => None,
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
    Some(SavedSearchRecord {
        id: value.get("id")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        query: value.get("query")?.as_str()?.to_string(),
        resource_type: value.get("resourceType")?.as_str()?.to_string(),
    })
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
    Some(SavedSearchRecord {
        id: node.get("id")?.as_str()?.to_string(),
        name: node.get("name")?.as_str()?.to_string(),
        query,
        resource_type: node
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or(fallback_resource_type)
            .to_string(),
    })
}

pub(in crate::proxy) fn saved_search_state_json(record: &SavedSearchRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "query": record.query,
        "resourceType": record.resource_type
    })
}

pub(in crate::proxy) fn saved_search_filter_json(
    key: &str,
    value: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SearchFilter")),
        "key" => Some(json!(key)),
        "value" => Some(json!(value)),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_page_info_json(
    records: &[SavedSearchRecord],
    selections: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    selected_json(
        &connection_page_info(
            has_next_page,
            has_previous_page,
            records.first().map(saved_search_cursor),
            records.last().map(saved_search_cursor),
        ),
        selections,
    )
}

pub(in crate::proxy) fn saved_search_mutation_payload_json(
    record: Option<&SavedSearchRecord>,
    payload_selections: &[SelectedField],
    saved_search_selections: &[SelectedField],
    api_client_id: &str,
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "savedSearch" => Some(match record {
                Some(record) => saved_search_json_with_query(
                    record,
                    saved_search_selections,
                    &record.query,
                    api_client_id,
                ),
                None => Value::Null,
            }),
            "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn saved_search_required_input_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let document = parsed_document(query, variables)?;
    let operation_name = document
        .operation_name
        .as_deref()
        .unwrap_or("AnonymousOperation");
    let field = document.root_fields.iter().find(|field| {
        matches!(
            field.name.as_str(),
            "savedSearchCreate" | "savedSearchUpdate" | "savedSearchDelete"
        )
    })?;
    let input_type = match field.name.as_str() {
        "savedSearchCreate" => "SavedSearchCreateInput",
        "savedSearchUpdate" => "SavedSearchUpdateInput",
        "savedSearchDelete" => "SavedSearchDeleteInput",
        _ => return None,
    };
    let variable_input = variables.get("input");
    let input = match field.arguments.get("input") {
        Some(ResolvedValue::Object(input)) => input,
        _ => return None,
    };

    if variable_input.is_some() {
        let value = variable_input
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        let mut errors = Vec::new();
        if field.name == "savedSearchCreate" && !input.contains_key("resourceType") {
            errors.push(invalid_variable_required_field_error(
                "resourceType",
                input_type,
                value.clone(),
                55,
            ));
        }
        if field.name == "savedSearchCreate" && !input.contains_key("name") {
            errors.push(invalid_variable_required_field_error(
                "name",
                input_type,
                value.clone(),
                47,
            ));
        }
        if field.name == "savedSearchUpdate" && !input.contains_key("id") {
            errors.push(invalid_variable_required_field_error(
                "id",
                input_type,
                value.clone(),
                47,
            ));
        }
        if field.name == "savedSearchDelete"
            && input
                .get("id")
                .is_none_or(|value| matches!(value, ResolvedValue::Null))
        {
            errors.push(invalid_variable_required_field_error(
                "id", input_type, value, 45,
            ));
        }
        return (!errors.is_empty()).then(|| ok_json(json!({ "errors": errors })));
    }

    let required_fields: &[(&str, &str)] = match field.name.as_str() {
        "savedSearchCreate" => &[
            ("name", "String!"),
            ("query", "String!"),
            ("resourceType", "SearchResultType!"),
        ],
        "savedSearchUpdate" => &[("id", "ID!")],
        "savedSearchDelete" => &[("id", "ID!")],
        _ => &[],
    };
    let errors = required_fields
        .iter()
        .filter(|(name, _)| !input.contains_key(*name))
        .map(|(name, ty)| {
            missing_required_input_attribute_error(
                operation_name,
                &field.name,
                input_type,
                name,
                ty,
            )
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Some(ok_json(json!({ "errors": errors })));
    }
    None
}

pub(in crate::proxy) fn missing_required_input_attribute_error(
    operation_name: &str,
    root_field: &str,
    input_object_type: &str,
    argument_name: &str,
    argument_type: &str,
) -> Value {
    json!({
        "message": format!("Argument '{}' on InputObject '{}' is required. Expected type {}", argument_name, input_object_type, argument_type),
        "locations": [{ "line": 2, "column": 28 }],
        "path": [format!("mutation {}", operation_name), root_field, "input", argument_name],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": argument_type,
            "inputObjectType": input_object_type
        }
    })
}

pub(in crate::proxy) fn invalid_variable_required_field_error(
    field: &str,
    input_object_type: &str,
    value: Value,
    column: u64,
) -> Value {
    invalid_variable_error_envelope(
        format!(
            "Variable $input of type {input_object_type}! was provided invalid value for {field} (Expected value to not be null)"
        ),
        SourceLocation {
            line: 1,
            column: column as usize,
        },
        value,
        json!([{ "path": [field], "explanation": "Expected value to not be null" }]),
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

pub(in crate::proxy) fn saved_search_delete_payload_json(
    deleted_id: Option<&str>,
    shop: &Value,
    payload_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedSavedSearchId" => Some(match deleted_id {
                Some(id) => json!(id),
                None => Value::Null,
            }),
            "shop" => Some(selected_json(shop, &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn saved_search_input_from_field(
    field: &RootFieldSelection,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match field.arguments.get("input") {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
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
    request_header(request, "x-shopify-draft-proxy-api-client-id")
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

pub(in crate::proxy) fn is_saved_search_root(root: &str) -> bool {
    matches!(
        root,
        "automaticDiscountSavedSearches"
            | "codeDiscountSavedSearches"
            | "collectionSavedSearches"
            | "customerSavedSearches"
            | "discountRedeemCodeSavedSearches"
            | "draftOrderSavedSearches"
            | "fileSavedSearches"
            | "orderSavedSearches"
            | "productSavedSearches"
    )
}

pub(in crate::proxy) fn saved_search_resource_type(root: &str) -> &'static str {
    match root {
        "automaticDiscountSavedSearches" => "DISCOUNT",
        "codeDiscountSavedSearches" => "DISCOUNT",
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
    SavedSearchRecord {
        id: id.to_string(),
        name: name.to_string(),
        query: query.to_string(),
        resource_type: resource_type.to_string(),
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
    format!("cursor:{}", record.id)
}

fn saved_search_connection_nodes(connection: &Value) -> Vec<Value> {
    let mut nodes = connection
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    nodes.extend(
        connection
            .get("edges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|edge| edge.get("node").cloned()),
    );
    nodes
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
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in query.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

impl DraftProxy {
    pub(in crate::proxy) fn saved_search_overlay_read_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let fields = root_fields(query, variables).unwrap_or_default();
        if self.config.read_mode == ReadMode::LiveHybrid
            && fields.iter().any(|field| field.name == "fileSavedSearches")
        {
            let mut response = (self.upstream_transport)(request.clone());
            if (200..300).contains(&response.status) {
                self.hydrate_file_saved_searches_from_response_data(
                    request,
                    &fields,
                    &response.body["data"],
                );
                response.body["data"] = self.saved_search_overlay_read_fields(request, &fields);
            }
            return response;
        }

        ok_json(json!({
            "data": self.saved_search_overlay_read_fields(request, &fields)
        }))
    }

    pub(in crate::proxy) fn saved_search_overlay_read_fields(
        &self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Value {
        let api_client_id = saved_search_request_api_client_id(request);
        let mut data = serde_json::Map::new();
        for field in fields {
            if !is_saved_search_root(&field.name) {
                continue;
            }
            data.insert(
                field.response_key.clone(),
                self.saved_search_connection_field(field, &api_client_id),
            );
        }
        Value::Object(data)
    }

    fn hydrate_file_saved_searches_from_response_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
        data: &Value,
    ) {
        let api_client_id = saved_search_request_api_client_id(request);
        for field in fields {
            if field.name != "fileSavedSearches" {
                continue;
            }
            let Some(connection) = data.get(&field.response_key) else {
                continue;
            };
            for node in saved_search_connection_nodes(connection) {
                let Some(record) = saved_search_record_from_node(&node, "FILE", &api_client_id)
                else {
                    continue;
                };
                if !self.store.staged.saved_searches.is_tombstoned(&record.id) {
                    self.store
                        .base
                        .saved_searches
                        .insert(record.id.clone(), record);
                }
            }
        }
    }

    pub(in crate::proxy) fn saved_search_connection_field(
        &self,
        field: &RootFieldSelection,
        api_client_id: &str,
    ) -> Value {
        let resource_type = saved_search_resource_type(&field.name);
        let mut records = self.saved_search_records_for_resource(resource_type);
        if let Some(ResolvedValue::String(query)) = field.arguments.get("query") {
            let needle = query.to_lowercase();
            records.retain(|record| {
                record.name.to_lowercase().contains(&needle)
                    || record.query.to_lowercase().contains(&needle)
            });
        }
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            records.reverse();
        }
        let mut has_previous_page = false;
        if let Some(ResolvedValue::String(after)) = field.arguments.get("after") {
            if let Some(index) = records
                .iter()
                .position(|record| saved_search_cursor(record) == *after)
            {
                records = records.into_iter().skip(index + 1).collect();
                has_previous_page = true;
            }
        }
        let total_after_cursor = records.len();
        let limit = match field.arguments.get("first") {
            Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as usize),
            _ => None,
        };
        let mut has_next_page = false;
        if let Some(limit) = limit {
            has_next_page = total_after_cursor > limit;
            records.truncate(limit);
        }
        saved_search_connection_json(
            &records,
            &field.selection,
            api_client_id,
            has_next_page,
            has_previous_page,
        )
    }

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

    pub(in crate::proxy) fn saved_search_mutation_fields(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let api_client_id = saved_search_request_api_client_id(request);
        let mut log_drafts = Vec::new();
        let fields = root_fields(query, variables).unwrap_or_default();
        let data = root_payload_json(&fields, |field| {
            let outcome = match field.name.as_str() {
                "savedSearchCreate" => self.saved_search_create_field(field, &api_client_id),
                "savedSearchUpdate" => self.saved_search_update_field(field, &api_client_id),
                "savedSearchDelete" => self.saved_search_delete_field(field),
                _ => return None,
            };
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            Some(outcome.value)
        });
        MutationOutcome::with_log_drafts(ok_json(json!({ "data": data })), log_drafts)
    }

    pub(in crate::proxy) fn saved_search_create_field(
        &mut self,
        field: &RootFieldSelection,
        api_client_id: &str,
    ) -> MutationFieldOutcome {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                api_client_id,
                vec![saved_search_input_required_user_error()],
            ));
        };
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        let search_query = resolved_string_field(&input, "query").unwrap_or_default();
        let resource_type =
            resolved_string_field(&input, "resourceType").unwrap_or_else(|| "PRODUCT".to_string());
        let mut user_errors = self.saved_search_field_user_errors(
            SavedSearchQueryValidationOperation::Create,
            &resource_type,
            &name,
            None,
        );
        user_errors.extend(saved_search_query_user_errors(
            SavedSearchQueryValidationOperation::Create,
            &resource_type,
            &search_query,
        ));
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                api_client_id,
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("SavedSearch");
        let record = SavedSearchRecord {
            id: id.clone(),
            name,
            query: normalize_saved_search_query_for_api_client(&search_query, api_client_id),
            resource_type,
        };
        self.store.stage_saved_search(record.clone());
        MutationFieldOutcome::staged(
            saved_search_mutation_payload_json(
                Some(&record),
                payload_selection,
                &saved_search_selection,
                api_client_id,
                Vec::new(),
            ),
            LogDraft::staged("savedSearchCreate", "saved_searches", vec![id]),
        )
    }

    pub(in crate::proxy) fn saved_search_update_field(
        &mut self,
        field: &RootFieldSelection,
        api_client_id: &str,
    ) -> MutationFieldOutcome {
        let payload_selection = &field.selection;
        let saved_search_selection = nested_selected_fields(payload_selection, &["savedSearch"]);
        let Some(input) = saved_search_input_from_field(field) else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                api_client_id,
                vec![saved_search_input_required_user_error()],
            ));
        };
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let existing = self.store.saved_search_by_id(&id);
        let Some(existing) = existing else {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                None,
                payload_selection,
                &saved_search_selection,
                api_client_id,
                vec![saved_search_missing_user_error()],
            ));
        };
        let requested_name =
            resolved_string_field(&input, "name").unwrap_or_else(|| existing.name.clone());
        let requested_query =
            resolved_string_field(&input, "query").unwrap_or_else(|| existing.query.clone());
        let mut updated = existing.clone();
        updated.query =
            normalize_saved_search_query_for_api_client(&requested_query, api_client_id);
        let mut user_errors = self.saved_search_field_user_errors(
            SavedSearchQueryValidationOperation::Update,
            &existing.resource_type,
            &requested_name,
            Some(&id),
        );
        user_errors.extend(saved_search_query_user_errors(
            SavedSearchQueryValidationOperation::Update,
            &existing.resource_type,
            &requested_query,
        ));
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                api_client_id,
                user_errors,
            ));
        }
        updated.name = requested_name;
        self.store.stage_saved_search(updated.clone());
        MutationFieldOutcome::staged(
            saved_search_mutation_payload_json(
                Some(&updated),
                payload_selection,
                &saved_search_selection,
                api_client_id,
                Vec::new(),
            ),
            LogDraft::staged(
                "savedSearchUpdate",
                "saved_searches",
                vec![updated.id.clone()],
            ),
        )
    }

    pub(in crate::proxy) fn saved_search_delete_field(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = saved_search_input_from_field(field);
        let id = input
            .as_ref()
            .and_then(|input| resolved_string_field(input, "id"))
            .unwrap_or_default();
        let deleted = self.store.delete_saved_search(&id);
        let shop = self.store.effective_shop();
        let value = saved_search_delete_payload_json(
            if deleted { Some(&id) } else { None },
            &shop,
            &field.selection,
            if deleted {
                Vec::new()
            } else {
                vec![saved_search_missing_user_error()]
            },
        );
        if deleted {
            MutationFieldOutcome::staged(
                value,
                LogDraft::staged("savedSearchDelete", "saved_searches", vec![id.clone()]),
            )
        } else {
            MutationFieldOutcome::unlogged(value)
        }
    }
}
