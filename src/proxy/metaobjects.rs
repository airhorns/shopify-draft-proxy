use super::*;
use std::sync::OnceLock;

pub(in crate::proxy) fn metaobject_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    [
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Metaobject",
            "field",
            metaobject_field_by_key,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "Metaobject",
            "definition",
            metaobject_definition_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "MetaobjectField",
            "reference",
            metaobject_field_reference,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "MetaobjectField",
            "references",
            metaobject_field_references,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "MetaobjectDefinition",
            "metaobjects",
            metaobject_definition_metaobjects,
        ),
    ]
    .into_iter()
    .collect()
}

pub(in crate::proxy) fn metaobject_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    [
        "Metaobject",
        "MetaobjectCapabilities",
        "MetaobjectDefinition",
        "MetaobjectField",
        "MetaobjectFieldDefinition",
    ]
    .into_iter()
    .map(|parent_type| {
        FieldResolverTypePolicy::property_backed_ordinary_fields(
            ApiSurface::Admin,
            parent_type,
            "argument-bearing metaobject field has no explicit canonical resolver",
        )
    })
    .collect()
}

fn metaobject_field_arguments(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> BTreeMap<String, ResolvedValue> {
    resolved_arguments_from_json(&invocation.arguments)
}

fn materialized_metaobject_field(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Option<Value> {
    invocation.parent.get(&invocation.field_name).cloned()
}

fn metaobject_field_by_key(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = metaobject_field_arguments(invocation);
    let key = resolved_string_field(&arguments, "key").unwrap_or_default();
    Ok(invocation
        .parent
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|field| field.get("key").and_then(Value::as_str) == Some(key.as_str()))
        .cloned()
        .unwrap_or(Value::Null))
}

fn metaobject_definition_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if let Some(value) = materialized_metaobject_field(invocation) {
        return Ok(value);
    }
    let meta_type = invocation
        .parent
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(proxy
        .metaobject_definition_by_type(meta_type)
        .map(|definition| proxy.metaobject_definition_canonical_value(&definition))
        .unwrap_or(Value::Null))
}

fn metaobject_field_reference(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if let Some(value) = materialized_metaobject_field(invocation) {
        return Ok(value);
    }
    Ok(proxy.canonical_metafield_reference_value(invocation.parent, Some(request)))
}

fn metaobject_field_references(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(proxy.canonical_metafield_references_connection_value(
        invocation.parent,
        &metaobject_field_arguments(invocation),
        Some(request),
    ))
}

fn metaobject_definition_metaobjects(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if let Some(value) = materialized_metaobject_field(invocation) {
        return Ok(value);
    }
    Ok(proxy.metaobject_definition_metaobjects_connection_value(
        invocation.parent,
        &metaobject_field_arguments(invocation),
    ))
}

/// Canonical input shared by metaobject roots after the GraphQL engine has
/// selected an operation and coerced its arguments. Domain behavior does not
/// receive or reconstruct an output selection tree.
struct MetaobjectRootInput {
    name: String,
    response_key: String,
    location: SourceLocation,
    arguments: BTreeMap<String, ResolvedValue>,
}

impl DraftProxy {
    pub(crate) fn metaobject_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            root_name,
            root_location,
            arguments,
            ..
        } = invocation;
        let field = MetaobjectRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            location: root_location,
            arguments: resolved_arguments_from_json(&arguments),
        };
        if self.config.read_mode != ReadMode::Snapshot {
            self.metaobject_live_hybrid_outcome(request, &field)
        } else {
            ResolverOutcome::value(self.metaobject_query_value(&field, request))
        }
    }

    pub(crate) fn metaobject_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            request,
            query,
            root_name,
            root_location,
            arguments,
            ..
        } = invocation;
        let field = MetaobjectRootInput {
            name: root_name.to_string(),
            response_key: response_key.to_string(),
            location: root_location,
            arguments: resolved_arguments_from_json(&arguments),
        };
        if self.metaobject_mutation_is_local(&field) {
            self.metaobject_mutation_outcome(&field, request, query)
        } else {
            // Transitional compatibility for cold upstream targets. The
            // guarded transport still prevents a registered local write from
            // escaping during normal runtime execution.
            self.cached_or_forward_upstream_root_outcome(request, response_key)
        }
    }
}

const STANDARD_METAOBJECT_TEMPLATES_FIXTURE: &str = include_str!(
    "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/standard-metaobject-templates.json"
);

static STANDARD_METAOBJECT_TEMPLATE_CATALOG: OnceLock<Value> = OnceLock::new();

fn standard_metaobject_template_catalog() -> &'static Value {
    STANDARD_METAOBJECT_TEMPLATE_CATALOG.get_or_init(|| {
        serde_json::from_str(STANDARD_METAOBJECT_TEMPLATES_FIXTURE)
            .expect("standard metaobject template catalog must be valid JSON")
    })
}

fn standard_metaobject_definition_template(meta_type: &str) -> Option<&'static Value> {
    standard_metaobject_template_catalog()
        .get("templates")
        .and_then(Value::as_array)?
        .iter()
        .find(|template| template.get("type").and_then(Value::as_str) == Some(meta_type))
}

fn standard_metaobject_definition_from_template(
    id: &str,
    template: &Value,
    timestamp: &str,
) -> Value {
    let mut definition = template.clone();
    if let Some(object) = definition.as_object_mut() {
        object.insert("id".to_string(), json!(id));
        object.insert("createdAt".to_string(), json!(timestamp));
        object.insert("updatedAt".to_string(), json!(timestamp));
    }
    definition
}

fn source_location_for_token_after(
    query: &str,
    start: SourceLocation,
    token: &str,
) -> Option<SourceLocation> {
    for (line_index, line) in query.lines().enumerate().skip(start.line.saturating_sub(1)) {
        let start_column = if line_index + 1 == start.line {
            start.column.saturating_sub(1)
        } else {
            0
        };
        let Some(search_slice) = line.get(start_column..) else {
            continue;
        };
        if let Some(offset) = search_slice.find(token) {
            return Some(SourceLocation {
                line: line_index + 1,
                column: start_column + offset + 1,
            });
        }
    }
    None
}

fn metaobject_bulk_delete_selector_error(
    field: &MetaobjectRootInput,
    query: &str,
) -> Option<Value> {
    let where_input = resolved_object_field(&field.arguments, "where");
    let ids_present = where_input
        .as_ref()
        .is_some_and(|input| input.contains_key("ids"))
        || field.arguments.contains_key("ids");
    let type_present = where_input
        .as_ref()
        .and_then(|input| resolved_string_field(input, "type"))
        .is_some_and(|value| !value.is_empty());

    if ids_present != type_present {
        return None;
    }

    let mut locations = vec![json!({
        "line": field.location.line,
        "column": field.location.column
    })];
    if ids_present {
        if let Some(location) = source_location_for_token_after(query, field.location, "ids") {
            locations.push(json!({
                "line": location.line,
                "column": location.column
            }));
        }
    }

    Some(json!({
        "message": "MetaobjectBulkDeleteWhereCondition requires exactly one of type, ids",
        "locations": locations,
        "extensions": {"code": "INVALID_FIELD_ARGUMENTS"},
        "path": [field.response_key.clone()]
    }))
}

fn metaobject_create_duplicate_field_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut errors = Vec::new();
    for (index, field) in resolved_object_list_field(input, "fields")
        .iter()
        .enumerate()
    {
        let Some(key) = resolved_string_field(field, "key") else {
            continue;
        };
        if seen.insert(key.clone()) {
            continue;
        }

        let field_index = index.to_string();
        let is_required_title = key == "title";
        errors.push(metaobject_indexed_user_error(
            vec![
                "metaobject".to_string(),
                "fields".to_string(),
                field_index.clone(),
            ],
            &format!("Field \"{key}\" duplicates other inputs"),
            Some("DUPLICATE_FIELD_INPUT"),
            json!(key.clone()),
            Value::Null,
        ));
        if is_required_title {
            errors.push(metaobject_indexed_user_error(
                vec!["metaobject".to_string(), "fields".to_string(), field_index],
                "Title can't be blank",
                Some("OBJECT_FIELD_REQUIRED"),
                json!(key),
                Value::Null,
            ));
        }
    }
    errors
}

fn metaobject_field_record_from_definition(
    field_definition: &Value,
    value: Option<&String>,
) -> Value {
    let field_type = field_definition["type"]["name"]
        .as_str()
        .unwrap_or("single_line_text_field");
    // A field defined on the type but never given a value for this entry reads back
    // as `null`/`null` (not empty string) — e.g. a field omitted at create time, or a
    // field the entry predates after a schema change adds it.
    let Some(raw_value) = value.map(String::as_str) else {
        return json!({
            "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
            "type": field_type,
            "value": Value::Null,
            "jsonValue": Value::Null,
            "definition": field_definition
        });
    };
    // Most fields echo their stored string verbatim; measurement and date_time fields
    // are normalized to Shopify's canonical form (uppercased units / decimal values /
    // explicit UTC offset). `json`/`rich_text_field` are free-form and never reshaped.
    let stored_value = if raw_value.is_empty() {
        raw_value.to_string()
    } else if field_type == "date_time" {
        metaobject_normalize_date_time_value(raw_value)
    } else if field_type == "rating" {
        serde_json::from_str::<Value>(raw_value)
            .ok()
            .as_ref()
            .and_then(metaobject_rating_value_string)
            .unwrap_or_else(|| raw_value.to_string())
    } else if matches!(field_type, "json" | "rich_text_field") {
        raw_value.to_string()
    } else if field_type.starts_with("list.") {
        serde_json::from_str::<Value>(raw_value)
            .ok()
            .as_ref()
            .and_then(|parsed| metaobject_list_value_string(field_type, parsed))
            .unwrap_or_else(|| raw_value.to_string())
    } else {
        serde_json::from_str::<Value>(raw_value)
            .ok()
            .as_ref()
            .and_then(metaobject_measurement_value_string)
            .unwrap_or_else(|| raw_value.to_string())
    };
    // jsonValue derives from the raw stored string so list-measurement units stay in
    // their verbatim (lowercase) form; only date_time reflects the normalized offset.
    let json_value_source = if field_type == "date_time" {
        stored_value.as_str()
    } else {
        raw_value
    };
    json!({
        "key": field_definition.get("key").cloned().unwrap_or(Value::Null),
        "type": field_type,
        "value": stored_value,
        "jsonValue": metaobject_field_json_value(field_type, Some(json_value_source)),
        "definition": field_definition
    })
}

fn metaobject_field_name(key: &str) -> String {
    key.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            format!("{}{}", first.to_uppercase(), chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn metaobject_definition_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
    timestamp: &str,
) -> Value {
    let name = resolved_string_field(input, "name").unwrap_or_default();
    let display_name_key = resolved_string_field(input, "displayNameKey");
    let field_definitions = resolved_object_list_field(input, "fieldDefinitions")
        .into_iter()
        .map(metaobject_field_definition_record)
        .collect::<Vec<_>>();
    json!({
        "id": id,
        "type": meta_type,
        "name": name,
        "description": input.get("description").and_then(resolved_value_string).map_or(Value::Null, |description| json!(description)),
        "displayNameKey": display_name_key,
        "access": metaobject_definition_access(input, meta_type),
        "capabilities": metaobject_definition_capabilities(input),
        "hasThumbnailField": metaobject_definition_has_thumbnail_field(&field_definitions),
        "fieldDefinitions": field_definitions,
        "metaobjectsCount": 0,
        "standardTemplate": Value::Null,
        "createdAt": timestamp,
        "updatedAt": timestamp
    })
}

fn metaobject_definition_from_record(record: &Value) -> Option<Value> {
    if let Some(definition) = record
        .get("definition")
        .filter(|definition| definition.is_object())
    {
        return Some(definition.clone());
    }
    let meta_type = record.get("type").and_then(Value::as_str)?;
    let field_definitions = record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| {
            field
                .get("definition")
                .filter(|definition| definition.is_object())
        })
        .cloned()
        .collect::<Vec<_>>();
    if field_definitions.is_empty() {
        return None;
    }
    let display_name_key = record["titleField"]["key"]
        .as_str()
        .or_else(|| {
            field_definitions
                .first()?
                .get("key")
                .and_then(Value::as_str)
        })
        .map_or(Value::Null, |key| json!(key));
    Some(json!({
        "id": Value::Null,
        "type": meta_type,
        "name": metaobject_field_name(meta_type),
        "description": Value::Null,
        "displayNameKey": display_name_key,
        "access": Value::Null,
        "capabilities": {
            "publishable": {"enabled": !record["capabilities"]["publishable"].is_null()},
            "onlineStore": {"enabled": !record["capabilities"]["onlineStore"].is_null(), "data": Value::Null},
            "renderable": {"enabled": false},
            "translatable": {"enabled": false}
        },
        "hasThumbnailField": metaobject_definition_has_thumbnail_field(&field_definitions),
        "fieldDefinitions": field_definitions,
        "metaobjectsCount": Value::Null,
        "standardTemplate": Value::Null,
        "createdAt": Value::Null,
        "updatedAt": Value::Null
    }))
}

fn metaobject_definition_access(input: &BTreeMap<String, ResolvedValue>, meta_type: &str) -> Value {
    let access = resolved_object_field(input, "access").unwrap_or_default();
    let admin = match resolved_string_field(&access, "admin").as_deref() {
        Some("MERCHANT_READ_WRITE") if metaobject_definition_is_app_reserved_type(meta_type) => {
            "MERCHANT_READ_WRITE"
        }
        Some("MERCHANT_READ_WRITE") | Some("PUBLIC_READ_WRITE") => "PUBLIC_READ_WRITE",
        Some("PUBLIC_READ") | Some("MERCHANT_READ") => "MERCHANT_READ",
        _ => "PUBLIC_READ_WRITE",
    };
    json!({
        "admin": admin,
        "storefront": resolved_string_field(&access, "storefront").unwrap_or_else(|| "NONE".to_string()),
        "customerAccount": resolved_string_field(&access, "customerAccount").unwrap_or_else(|| "NONE".to_string())
    })
}

fn metaobject_definition_has_thumbnail_field(field_definitions: &[Value]) -> bool {
    field_definitions
        .iter()
        .any(|field| field["type"]["name"].as_str() == Some("file_reference"))
}

fn metaobject_definition_capabilities(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let capabilities = resolved_object_field(input, "capabilities").unwrap_or_default();
    let publishable = resolved_object_field(&capabilities, "publishable")
        .and_then(|publishable| resolved_bool_field(&publishable, "enabled"))
        .unwrap_or(false);
    let online_store_input = resolved_object_field(&capabilities, "onlineStore");
    let online_store = online_store_input
        .as_ref()
        .and_then(|online_store| resolved_bool_field(online_store, "enabled"))
        .unwrap_or(false);
    let online_store_data = online_store_input
        .as_ref()
        .and_then(|online_store| resolved_object_field(online_store, "data"))
        .map_or(Value::Null, |data| {
            metaobject_online_store_capability_data(&data)
        });
    let renderable = resolved_object_field(&capabilities, "renderable")
        .and_then(|renderable| resolved_bool_field(&renderable, "enabled"))
        .unwrap_or(false);
    let translatable = resolved_object_field(&capabilities, "translatable")
        .and_then(|translatable| resolved_bool_field(&translatable, "enabled"))
        .unwrap_or(false);
    json!({
        "publishable": {"enabled": publishable},
        "onlineStore": {"enabled": online_store, "data": online_store_data},
        "renderable": {"enabled": renderable},
        "translatable": {"enabled": translatable}
    })
}

/// Normalises an onlineStore capability `data` input into its stored shape. The
/// `canCreateRedirects` describes current redirect capacity; it is not the
/// echoed value of the input-only `createRedirects` switch. The local model has
/// capacity when online-store data is present (the update path separately reads
/// `createRedirects` to decide whether to stage redirects).
fn metaobject_online_store_capability_data(data: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "urlHandle": resolved_string_field(data, "urlHandle")
            .map_or(Value::Null, |url_handle| json!(url_handle)),
        "canCreateRedirects": true
    })
}

fn metaobject_field_definition_record(input: BTreeMap<String, ResolvedValue>) -> Value {
    let key = resolved_string_field(&input, "key").unwrap_or_default();
    let name = resolved_string_field(&input, "name").unwrap_or_else(|| metaobject_field_name(&key));
    let field_type = metaobject_field_definition_type(&input);
    json!({
        "key": key,
        "name": name,
        "description": input.get("description").and_then(resolved_value_string).map_or(Value::Null, |description| json!(description)),
        "required": resolved_bool_field(&input, "required").unwrap_or(false),
        "type": {"name": field_type, "category": metaobject_field_type_category(&field_type)},
        "capabilities": metaobject_field_definition_capabilities(&input),
        "validations": resolved_object_list_field(&input, "validations")
            .into_iter()
            .map(|validation| {
                json!({
                    "name": resolved_string_field(&validation, "name").unwrap_or_default(),
                    "value": resolved_string_field(&validation, "value").unwrap_or_default()
                })
            })
            .collect::<Vec<_>>()
    })
}

fn metaobject_field_definition_capabilities(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let capabilities = resolved_object_field(input, "capabilities").unwrap_or_default();
    let admin_filterable = resolved_object_field(&capabilities, "adminFilterable")
        .and_then(|admin_filterable| resolved_bool_field(&admin_filterable, "enabled"))
        .unwrap_or(false);
    json!({
        "adminFilterable": {"enabled": admin_filterable}
    })
}

fn metaobject_field_definition_type(input: &BTreeMap<String, ResolvedValue>) -> String {
    resolved_string_field(input, "type")
        .or_else(|| {
            resolved_object_field(input, "type")
                .and_then(|value| resolved_string_field(&value, "name"))
        })
        .unwrap_or_else(|| "single_line_text_field".to_string())
}

fn metaobject_field_type_category(field_type: &str) -> &'static str {
    if let Some(inner_type) = field_type.strip_prefix("list.") {
        return metaobject_field_type_category(inner_type);
    }
    match field_type {
        "number_integer" | "number_decimal" => "NUMBER",
        "boolean" => "TRUE_FALSE",
        "date" | "date_time" => "DATE_TIME",
        "json" | "rich_text_field" => "JSON",
        value if value.ends_with("_reference") => "REFERENCE",
        _ => "TEXT",
    }
}

fn metaobject_definition_type_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    request: &Request,
) -> String {
    canonical_metaobject_definition_type(
        &resolved_string_field(input, "type").unwrap_or_default(),
        request,
    )
}

fn resolved_metaobject_definition_type_arg(
    value: Option<&ResolvedValue>,
    request: &Request,
) -> String {
    canonical_metaobject_definition_type(
        &value.and_then(resolved_value_string).unwrap_or_default(),
        request,
    )
}

fn canonical_metaobject_definition_type(raw: &str, request: &Request) -> String {
    let resolved = if let Some(suffix) = raw.strip_prefix("$app:") {
        request_app_namespace_api_client_id(request)
            .map(|api_client_id| format!("app--{api_client_id}--{suffix}"))
            .unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };
    resolved.to_lowercase()
}

fn metaobject_definition_type_identity_error(
    input: &BTreeMap<String, ResolvedValue>,
    request: &Request,
) -> Option<Value> {
    let raw_type = resolved_string_field(input, "type")?;
    (raw_type.starts_with("$app:") && request_app_namespace_api_client_id(request).is_none()).then(
        || {
            metaobject_field_error(
                vec!["definition", "type"],
                APP_NAMESPACE_IDENTITY_REQUIRED_MESSAGE,
                "NOT_AUTHORIZED",
            )
        },
    )
}

const MIN_FIELD_KEY_LENGTH: usize = 2;
const MAX_FIELD_KEY_LENGTH: usize = 64;
// Source: fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/definition-create-field-validations.json
// and fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-definition-limit-caps.json
const METAOBJECT_DEFINITION_FIELD_LIMIT: usize = 40;
// Source: fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-definition-limit-caps.json
const METAOBJECT_DEFINITION_ADMIN_FILTERABLE_FIELD_LIMIT: usize = 40;
// Source: fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/metaobject-definition-limit-caps.json
const METAOBJECT_DEFINITION_SHOP_LIMIT: usize = 128;
const FIELD_KEY_INVALID_MESSAGE: &str = "Key contains one or more invalid characters.";

fn metaobject_definition_is_reserved_type(meta_type: &str) -> bool {
    meta_type.starts_with("shopify--")
}

fn metaobject_definition_is_app_reserved_type(meta_type: &str) -> bool {
    meta_type.starts_with("app--")
}

fn metaobject_definition_field_limit() -> usize {
    METAOBJECT_DEFINITION_FIELD_LIMIT
}

fn metaobject_definition_max_fields_error(max_fields: usize) -> Value {
    metaobject_field_error(
        vec!["definition", "fieldDefinitions"],
        &format!("Maximum {max_fields} fields per metaobject definition"),
        "INVALID",
    )
}

fn metaobject_definition_field_key_is_reserved(key: &str) -> bool {
    matches!(key, "id" | "handle" | "system" | "metafields")
}

fn metaobject_definition_create_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
    existing_definitions: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if metaobject_definition_is_reserved_type(meta_type) {
        errors.push(metaobject_field_error(
            vec!["definition"],
            "Not authorized. This type is reserved for use by another application.",
            "NOT_AUTHORIZED",
        ));
        return errors;
    }

    let name = resolved_string_field(input, "name").unwrap_or_default();
    if name.trim().is_empty() {
        errors.push(metaobject_field_error(
            vec!["definition", "name"],
            &blank_message("Name"),
            BLANK_USER_ERROR_CODE,
        ));
    } else if name.chars().count() > 255 {
        errors.push(metaobject_field_error(
            vec!["definition", "name"],
            &too_long_message("Name", 255),
            TOO_LONG_USER_ERROR_CODE,
        ));
    }

    if meta_type.trim().is_empty() {
        errors.push(metaobject_field_error(
            vec!["definition", "type"],
            &blank_message("Type"),
            BLANK_USER_ERROR_CODE,
        ));
    } else if meta_type.chars().count() < 3 {
        errors.push(metaobject_field_error(
            vec!["definition", "type"],
            "Type is too short (minimum is 3 characters)",
            "TOO_SHORT",
        ));
    } else if meta_type.chars().count() > 255 {
        errors.push(metaobject_field_error(
            vec!["definition", "type"],
            &too_long_message("Type", 255),
            TOO_LONG_USER_ERROR_CODE,
        ));
    } else if !token_chars_valid(meta_type) {
        errors.push(metaobject_field_error(
            vec!["definition", "type"],
            "Type contains one or more invalid characters. Only alphanumeric characters, underscores, and dashes are allowed.",
            "INVALID",
        ));
    }

    if let Some(description) = resolved_string_field(input, "description") {
        if description.chars().count() > 255 {
            errors.push(metaobject_field_error(
                vec!["definition", "description"],
                &too_long_message("Description", 255),
                TOO_LONG_USER_ERROR_CODE,
            ));
        }
    }

    if let Some(access) = resolved_object_field(input, "access") {
        if resolved_string_field(&access, "admin").is_some()
            && !metaobject_definition_is_app_reserved_type(meta_type)
        {
            errors.push(metaobject_field_error(
                vec!["definition", "access", "admin"],
                "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
                "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
            ));
        }
    }

    let field_definitions = resolved_object_list_field(input, "fieldDefinitions");
    let max_fields = metaobject_definition_field_limit();
    if field_definitions.len() > max_fields {
        errors.push(metaobject_definition_max_fields_error(max_fields));
    }

    let admin_filterable_count = field_definitions
        .iter()
        .filter(|definition| {
            resolved_object_field(definition, "capabilities")
                .and_then(|capabilities| resolved_object_field(&capabilities, "adminFilterable"))
                .and_then(|admin_filterable| resolved_bool_field(&admin_filterable, "enabled"))
                .unwrap_or(false)
        })
        .count();
    if admin_filterable_count > METAOBJECT_DEFINITION_ADMIN_FILTERABLE_FIELD_LIMIT {
        errors.push(metaobject_field_error(
            vec!["definition", "fieldDefinitions"],
            &format!(
                "Maximum {METAOBJECT_DEFINITION_ADMIN_FILTERABLE_FIELD_LIMIT} admin filterable fields per metaobject definition"
            ),
            "INVALID",
        ));
    }

    let mut seen_keys = BTreeSet::new();
    for (index, field_definition) in field_definitions.iter().enumerate() {
        let key = resolved_string_field(field_definition, "key").unwrap_or_default();
        let index_string = index.to_string();
        let field_path = ["definition", "fieldDefinitions", &index_string];
        if push_metaobject_field_key_errors(&mut errors, &field_path, &field_path, &key) {
            continue;
        }
        if !seen_keys.insert(key.clone()) {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                &format!("Field \"{key}\" duplicates other inputs"),
                "DUPLICATE_FIELD_INPUT",
                json!(key),
                Value::Null,
            ));
            continue;
        }
        let field_type = metaobject_field_definition_type(field_definition);
        if !metafield_definition_type_allowed(&field_type) {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                &format!(
                    "Type name {field_type} is not a valid type. Valid types are: {}.",
                    metafield_definition_valid_type_message()
                ),
                "INCLUSION",
                json!(key),
                Value::Null,
            ));
        } else if metafield_definition_type_is_standard_definition_only(&field_type) {
            errors.push(metaobject_user_error(
                vec!["definition", "fieldDefinitions", &index_string],
                metafield_definition_standard_only_type_message(),
                "INVALID",
                json!(key),
                Value::Null,
            ));
        }
    }

    if let Some(display_name_key) = resolved_string_field(input, "displayNameKey") {
        if !field_definitions.iter().any(|definition| {
            resolved_string_field(definition, "key") == Some(display_name_key.clone())
        }) {
            errors.push(metaobject_field_error(
                vec!["definition", "displayNameKey"],
                &format!("Field definition \"{display_name_key}\" does not exist"),
                "UNDEFINED_OBJECT_FIELD",
            ));
        }
    }

    if existing_definitions >= METAOBJECT_DEFINITION_SHOP_LIMIT {
        errors.push(metaobject_field_error(
            vec!["definition"],
            &format!(
                "Total definition count exceeds the limit of {METAOBJECT_DEFINITION_SHOP_LIMIT}"
            ),
            "MAX_DEFINITIONS_EXCEEDED",
        ));
    }

    errors
}

fn push_metaobject_field_key_errors(
    errors: &mut Vec<Value>,
    index_path: &[&str],
    validation_path: &[&str],
    key: &str,
) -> bool {
    if metaobject_definition_field_key_is_reserved(key) {
        errors.push(metaobject_user_error(
            index_path.to_vec(),
            &format!("The name \"{key}\" is reserved for system use"),
            "RESERVED_NAME",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if key.trim().is_empty() {
        let path = validation_path.to_vec();
        // Blank keys surface Shopify's presence, length, and format errors in order.
        errors.push(metaobject_user_error(
            path.clone(),
            &blank_message("Key"),
            BLANK_USER_ERROR_CODE,
            json!(key),
            Value::Null,
        ));
        errors.push(metaobject_user_error(
            path.clone(),
            &format!("Key is too short (minimum is {MIN_FIELD_KEY_LENGTH} characters)"),
            "TOO_SHORT",
            json!(key),
            Value::Null,
        ));
        errors.push(metaobject_user_error(
            path,
            FIELD_KEY_INVALID_MESSAGE,
            "INVALID",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if key.chars().count() < MIN_FIELD_KEY_LENGTH {
        errors.push(metaobject_user_error(
            validation_path.to_vec(),
            &format!("Key is too short (minimum is {MIN_FIELD_KEY_LENGTH} characters)"),
            "TOO_SHORT",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if key.chars().count() > MAX_FIELD_KEY_LENGTH {
        errors.push(metaobject_user_error(
            validation_path.to_vec(),
            &too_long_message("Key", MAX_FIELD_KEY_LENGTH),
            TOO_LONG_USER_ERROR_CODE,
            json!(key),
            Value::Null,
        ));
        return true;
    }
    if !token_chars_valid(key) {
        errors.push(metaobject_user_error(
            validation_path.to_vec(),
            FIELD_KEY_INVALID_MESSAGE,
            "INVALID",
            json!(key),
            Value::Null,
        ));
        return true;
    }
    false
}

fn metaobject_renderable_capability_errors(
    input: &BTreeMap<String, ResolvedValue>,
    field_definitions: &[Value],
) -> Vec<Value> {
    let mut errors = Vec::new();
    let Some(capabilities) = resolved_object_field(input, "capabilities") else {
        return errors;
    };
    let Some(renderable) = resolved_object_field(&capabilities, "renderable") else {
        return errors;
    };
    if resolved_bool_field(&renderable, "enabled") != Some(true) {
        return errors;
    }
    let Some(data) = resolved_object_field(&renderable, "data") else {
        return errors;
    };
    // The renderable capability's meta-title/meta-description keys must reference
    // existing text-typed field definitions. Shopify validates them in a fixed
    // order and reports a single error anchored at ["definition","capabilities","renderable"].
    for (input_key, capability_key) in [
        ("metaTitleKey", "meta_title_key"),
        ("metaDescriptionKey", "meta_description_key"),
    ] {
        let Some(field_key) = resolved_string_field(&data, input_key) else {
            continue;
        };
        match field_definitions
            .iter()
            .find(|definition| definition["key"].as_str() == Some(field_key.as_str()))
        {
            None => errors.push(metaobject_field_error(
                vec!["definition", "capabilities", "renderable"],
                &format!("Field definition \"{field_key}\" does not exist"),
                "INVALID",
            )),
            Some(field_definition) => {
                let field_type = field_definition["type"]["name"]
                    .as_str()
                    .unwrap_or_default();
                if !matches!(
                    field_type,
                    "single_line_text_field" | "multi_line_text_field" | "rich_text_field"
                ) {
                    errors.push(metaobject_field_error(
                        vec!["definition", "capabilities", "renderable"],
                        &format!(
                            "Renderable Capability \"{capability_key}\" cannot reference the field definition \"{field_key}\" of type \"{field_type}\". Only single_line_text_field, multi_line_text_field, rich_text_field definitions are allowed."
                        ),
                        "FIELD_TYPE_INVALID",
                    ));
                }
            }
        }
    }
    errors
}

fn metaobject_definition_update_validation_errors(
    input: &BTreeMap<String, ResolvedValue>,
    meta_type: &str,
    field_definitions: &[Value],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(name) = resolved_string_field(input, "name") {
        if name.trim().is_empty() {
            errors.push(metaobject_field_error(
                vec!["definition", "name"],
                &blank_message("Name"),
                BLANK_USER_ERROR_CODE,
            ));
        } else if name.chars().count() > 255 {
            errors.push(metaobject_field_error(
                vec!["definition", "name"],
                &too_long_message("Name", 255),
                TOO_LONG_USER_ERROR_CODE,
            ));
        }
    }
    if let Some(description) = resolved_string_field(input, "description") {
        if description.chars().count() > 255 {
            errors.push(metaobject_field_error(
                vec!["definition", "description"],
                &too_long_message("Description", 255),
                TOO_LONG_USER_ERROR_CODE,
            ));
        }
    }
    if let Some(access) = resolved_object_field(input, "access") {
        if resolved_string_field(&access, "admin").is_some()
            && !metaobject_definition_is_app_reserved_type(meta_type)
        {
            errors.push(metaobject_field_error(
                vec!["definition", "access", "admin"],
                "Admin access can only be specified on metaobject definitions that have an app-reserved type.",
                "ADMIN_ACCESS_INPUT_NOT_ALLOWED",
            ));
        }
    }
    errors.extend(metaobject_renderable_capability_errors(
        input,
        field_definitions,
    ));
    let MetaobjectFieldOperationValidation {
        errors: field_operation_errors,
        resulting_keys,
    } = metaobject_field_operation_validation(input, field_definitions);
    errors.extend(field_operation_errors);
    if let Some(display_name_key) = resolved_string_field(input, "displayNameKey") {
        if !resulting_keys.contains(&display_name_key) {
            errors.push(metaobject_field_error(
                vec!["definition", "displayNameKey"],
                &format!("Field definition \"{display_name_key}\" does not exist"),
                "UNDEFINED_OBJECT_FIELD",
            ));
        }
    }
    errors
}

struct MetaobjectFieldOperationValidation {
    errors: Vec<Value>,
    resulting_keys: BTreeSet<String>,
}

/// Validates the `fieldDefinitions` operation list on a definition update. Each
/// entry is a one-of `{create|update|delete}` operation; Shopify reports errors
/// per operation index. Most operation-specific errors are nested under
/// `{create|update|delete}`, while reserved/duplicate create keys are anchored at
/// the operation index itself.
fn metaobject_field_operation_validation(
    input: &BTreeMap<String, ResolvedValue>,
    field_definitions: &[Value],
) -> MetaobjectFieldOperationValidation {
    let mut errors = Vec::new();
    let operations = resolved_object_list_field(input, "fieldDefinitions");
    let mut known_keys: BTreeSet<String> = field_definitions
        .iter()
        .filter_map(|definition| definition["key"].as_str().map(str::to_string))
        .collect();
    let mut seen_create_keys = BTreeSet::new();
    for (index, operation) in operations.iter().enumerate() {
        let index_string = index.to_string();
        if let Some(create) = resolved_object_field(operation, "create") {
            let key = resolved_string_field(&create, "key").unwrap_or_default();
            // Presence, length, and format validators anchor at the `create` object;
            // the already-taken validator anchors one level deeper at `create.key`.
            let index_path = ["definition", "fieldDefinitions", &index_string];
            let create_path = ["definition", "fieldDefinitions", &index_string, "create"];
            if push_metaobject_field_key_errors(&mut errors, &index_path, &create_path, &key) {
                continue;
            }
            if !seen_create_keys.insert(key.clone()) {
                errors.push(metaobject_user_error(
                    vec!["definition", "fieldDefinitions", &index_string],
                    &format!("Field \"{key}\" duplicates other inputs"),
                    "DUPLICATE_FIELD_INPUT",
                    json!(key),
                    Value::Null,
                ));
                continue;
            }
            if known_keys.contains(&key) {
                errors.push(metaobject_user_error(
                    vec![
                        "definition",
                        "fieldDefinitions",
                        &index_string,
                        "create",
                        "key",
                    ],
                    &format!("Field definition \"{key}\" is already taken"),
                    "OBJECT_FIELD_TAKEN",
                    json!(key),
                    Value::Null,
                ));
                continue;
            }
            known_keys.insert(key);
        } else if let Some(update) = resolved_object_field(operation, "update") {
            let key = resolved_string_field(&update, "key").unwrap_or_default();
            if !known_keys.contains(&key) {
                errors.push(metaobject_user_error(
                    vec![
                        "definition",
                        "fieldDefinitions",
                        &index_string,
                        "update",
                        "key",
                    ],
                    &format!("Field definition \"{key}\" does not exist"),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    Value::Null,
                ));
            }
        } else if let Some(delete) = resolved_object_field(operation, "delete") {
            let key = resolved_string_field(&delete, "key").unwrap_or_default();
            if !known_keys.contains(&key) {
                errors.push(metaobject_user_error(
                    vec![
                        "definition",
                        "fieldDefinitions",
                        &index_string,
                        "delete",
                        "key",
                    ],
                    &format!("Field definition \"{key}\" does not exist"),
                    "UNDEFINED_OBJECT_FIELD",
                    json!(key),
                    Value::Null,
                ));
            } else {
                known_keys.remove(&key);
            }
        }
    }
    let max_fields = metaobject_definition_field_limit();
    if known_keys.len() > max_fields {
        errors.push(metaobject_definition_max_fields_error(max_fields));
    }
    MetaobjectFieldOperationValidation {
        errors,
        resulting_keys: known_keys,
    }
}

fn update_metaobject_definition_record(
    mut definition: Value,
    input: &BTreeMap<String, ResolvedValue>,
    updated_at: &str,
) -> Value {
    if let Some(name) = resolved_string_field(input, "name") {
        definition["name"] = json!(name);
    }
    if input.contains_key("description") {
        definition["description"] = input
            .get("description")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |description| json!(description));
    }
    if input.contains_key("displayNameKey") {
        definition["displayNameKey"] = input
            .get("displayNameKey")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |display_name_key| json!(display_name_key));
    }
    if let Some(access_input) = resolved_object_field(input, "access") {
        let mut access = definition
            .get("access")
            .cloned()
            .unwrap_or_else(|| json!({"admin": "PUBLIC_READ_WRITE", "storefront": "NONE", "customerAccount": "NONE"}));
        if let Some(admin) = resolved_string_field(&access_input, "admin") {
            access["admin"] = json!(admin);
        }
        if let Some(storefront) = resolved_string_field(&access_input, "storefront") {
            access["storefront"] = json!(storefront);
        }
        if let Some(customer_account) = resolved_string_field(&access_input, "customerAccount") {
            access["customerAccount"] = json!(customer_account);
        }
        definition["access"] = access;
    }
    apply_metaobject_definition_capability_updates(&mut definition, input);
    apply_metaobject_definition_field_operations(&mut definition, input);
    definition["updatedAt"] = json!(updated_at);
    definition
}

/// Merges the capability changes from a definition-update input into the stored
/// capabilities, preserving capabilities the caller did not mention.
fn apply_metaobject_definition_capability_updates(
    definition: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    let Some(input_capabilities) = resolved_object_field(input, "capabilities") else {
        return;
    };
    for key in ["publishable", "onlineStore", "renderable", "translatable"] {
        let Some(capability) = resolved_object_field(&input_capabilities, key) else {
            continue;
        };
        if let Some(enabled) = resolved_bool_field(&capability, "enabled") {
            definition["capabilities"][key]["enabled"] = json!(enabled);
        }
        if key == "onlineStore" {
            if let Some(data) = resolved_object_field(&capability, "data") {
                definition["capabilities"]["onlineStore"]["data"] =
                    metaobject_online_store_capability_data(&data);
            }
        }
    }
}

/// Applies the `fieldDefinitions` create/update/delete operations from a
/// definition-update input to the stored field-definition list. Validation has
/// already run, so every operation here is known to be applicable.
fn apply_metaobject_definition_field_operations(
    definition: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    let operations = resolved_object_list_field(input, "fieldDefinitions");
    let reset_field_order = resolved_bool_field(input, "resetFieldOrder").unwrap_or(false);
    if operations.is_empty() {
        return;
    }
    let mut fields = definition["fieldDefinitions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // When `resetFieldOrder` is set, Shopify reorders the surviving field definitions
    // to follow the order the caller listed their create/update operations (deletes
    // drop out). We record that intended order as we apply each operation.
    let mut intended_order: Vec<String> = Vec::new();
    for operation in &operations {
        if let Some(create) = resolved_object_field(operation, "create") {
            if let Some(key) = resolved_string_field(&create, "key") {
                intended_order.push(key);
            }
            fields.push(metaobject_field_definition_record(create));
        } else if let Some(update) = resolved_object_field(operation, "update") {
            let key = resolved_string_field(&update, "key").unwrap_or_default();
            if !key.is_empty() {
                intended_order.push(key.clone());
            }
            if let Some(field) = fields
                .iter_mut()
                .find(|field| field["key"].as_str() == Some(key.as_str()))
            {
                apply_metaobject_field_definition_update(field, &update);
            }
        } else if let Some(delete) = resolved_object_field(operation, "delete") {
            let key = resolved_string_field(&delete, "key").unwrap_or_default();
            fields.retain(|field| field["key"].as_str() != Some(key.as_str()));
        }
    }
    if reset_field_order {
        // Stable sort by position in the intended order; any field the caller did not
        // mention keeps its relative position after the reordered ones.
        fields.sort_by_key(|field| {
            field["key"]
                .as_str()
                .and_then(|key| intended_order.iter().position(|ordered| ordered == key))
                .unwrap_or(usize::MAX)
        });
    }
    definition["hasThumbnailField"] = json!(metaobject_definition_has_thumbnail_field(&fields));
    definition["fieldDefinitions"] = json!(fields);
}

fn apply_metaobject_field_definition_update(
    field: &mut Value,
    update: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(name) = resolved_string_field(update, "name") {
        field["name"] = json!(name);
    }
    if update.contains_key("description") {
        field["description"] = update
            .get("description")
            .and_then(resolved_value_string)
            .map_or(Value::Null, |description| json!(description));
    }
    if let Some(required) = resolved_bool_field(update, "required") {
        field["required"] = json!(required);
    }
    if update.contains_key("validations") {
        field["validations"] = json!(resolved_object_list_field(update, "validations")
            .into_iter()
            .map(|validation| json!({
                "name": resolved_string_field(&validation, "name").unwrap_or_default(),
                "value": resolved_string_field(&validation, "value").unwrap_or_default()
            }))
            .collect::<Vec<_>>());
    }
    if update.contains_key("type") {
        let field_type = metaobject_field_definition_type(update);
        field["type"] = json!({
            "name": field_type,
            "category": metaobject_field_type_category(&field_type)
        });
    }
}

fn metaobject_field_json_value(field_type: &str, value: Option<&str>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match field_type {
        "number_integer" => value
            .parse::<i64>()
            .map_or(Value::Null, |number| json!(number)),
        // Shopify returns number_decimal jsonValue as the verbatim decimal string,
        // not a JSON number.
        "number_decimal" => json!(value),
        "boolean" => match value {
            "true" => json!(true),
            "false" => json!(false),
            _ => Value::Null,
        },
        "json" | "rich_text_field" => serde_json::from_str(value).unwrap_or_else(|_| json!(value)),
        value_type if value_type.starts_with("list.") => {
            let parsed = serde_json::from_str(value).unwrap_or_else(|_| json!([value]));
            metaobject_normalize_list_json_value(value_type, parsed)
        }
        // Structured scalar types (money, link, rating, and the measurement family)
        // serialize jsonValue as the parsed JSON object. Measurement objects also
        // uppercase their unit. Plain scalar strings (color, date, references, etc.)
        // fall through to the verbatim string.
        _ => match serde_json::from_str::<Value>(value) {
            Ok(parsed) if parsed.is_object() || parsed.is_array() => {
                metaobject_measurement_unit_uppercased(parsed)
            }
            _ => json!(value),
        },
    }
}

/// If `value` is a measurement-shaped object (`{"value": <number>, "unit": "<string>"}`)
/// returns the value and unit. Distinguishes measurements from money/link/rating which
/// have different key sets.
fn metaobject_measurement_parts(value: &Value) -> Option<(&Value, &str)> {
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    let number = object.get("value")?;
    if !number.is_number() {
        return None;
    }
    let unit = object.get("unit")?.as_str()?;
    Some((number, unit))
}

/// Uppercases the `unit` of a scalar measurement object, leaving every other shape
/// (money, link, rating, json, arrays) untouched. Scalar measurements normalize their
/// unit in jsonValue; list measurements echo the parsed input verbatim.
fn metaobject_measurement_unit_uppercased(value: Value) -> Value {
    if let Some((number, unit)) = metaobject_measurement_parts(&value) {
        return json!({"value": number, "unit": unit.to_uppercase()});
    }
    value
}

/// Shopify's classic `dimension`/`weight`/`volume` measurement families store their
/// unit as an abbreviation in jsonValue (e.g. `centimeters` -> `cm`). All newer
/// measurement families echo the verbatim lowercase unit. Returns the abbreviation when
/// one exists, otherwise the lowercase unit unchanged.
fn metaobject_measurement_storage_unit(field_type: &str, unit: &str) -> String {
    let lower = unit.to_lowercase();
    let abbreviation = match field_type {
        "dimension" | "list.dimension" => match lower.as_str() {
            "millimeters" => Some("mm"),
            "centimeters" => Some("cm"),
            "meters" => Some("m"),
            "inches" => Some("in"),
            "feet" => Some("ft"),
            "yards" => Some("yd"),
            _ => None,
        },
        "weight" | "list.weight" => match lower.as_str() {
            "grams" => Some("g"),
            "kilograms" => Some("kg"),
            "ounces" => Some("oz"),
            "pounds" => Some("lb"),
            _ => None,
        },
        "volume" | "list.volume" => match lower.as_str() {
            "milliliters" => Some("ml"),
            "centiliters" => Some("cl"),
            "liters" => Some("l"),
            "cubic_meters" => Some("m3"),
            "fluid_ounces" => Some("fl oz"),
            "imperial_fluid_ounces" => Some("imp fl oz"),
            "pints" => Some("pt"),
            "imperial_pints" => Some("imp pt"),
            "quarts" => Some("qt"),
            "imperial_quarts" => Some("imp qt"),
            "gallons" => Some("gal"),
            "imperial_gallons" => Some("imp gal"),
            _ => None,
        },
        _ => None,
    };
    abbreviation.map(str::to_string).unwrap_or(lower)
}

/// Normalizes a list field's jsonValue array per element: date_time strings gain an
/// explicit UTC offset, and dimension/weight/volume measurements use abbreviated units.
/// Every other list element is echoed verbatim from the parsed input.
fn metaobject_normalize_list_json_value(field_type: &str, parsed: Value) -> Value {
    let Some(items) = parsed.as_array() else {
        return parsed;
    };
    let normalized = items
        .iter()
        .map(|item| {
            if field_type == "list.date_time" {
                if let Some(text) = item.as_str() {
                    return json!(metaobject_normalize_date_time_value(text));
                }
            }
            if matches!(field_type, "list.dimension" | "list.weight" | "list.volume") {
                if let Some((number, unit)) = metaobject_measurement_parts(item) {
                    return json!({
                        "value": number,
                        "unit": metaobject_measurement_storage_unit(field_type, unit),
                    });
                }
            }
            // number_decimal elements echo as decimal strings, not JSON numbers.
            if field_type == "list.number_decimal" && item.is_number() {
                return json!(item.to_string());
            }
            item.clone()
        })
        .collect::<Vec<_>>();
    Value::Array(normalized)
}

/// Renders one element of a list field's `value` string with Shopify's canonical
/// formatting (measurement floats + uppercased units, date_time offsets, decimal
/// stringification, rating key order). Other elements serialize verbatim.
fn metaobject_list_value_token(field_type: &str, item: &Value) -> String {
    if let Some((number, unit)) = metaobject_measurement_parts(item) {
        return metaobject_measurement_value_token(number, unit);
    }
    match field_type {
        "list.date_time" => {
            if let Some(text) = item.as_str() {
                return Value::String(metaobject_normalize_date_time_value(text)).to_string();
            }
        }
        // number_decimal elements are stored as decimal strings ([10.4] -> ["10.4"]).
        "list.number_decimal" => {
            if item.is_number() {
                return Value::String(item.to_string()).to_string();
            }
        }
        "list.rating" => {
            if let Some(rendered) = metaobject_rating_value_string(item) {
                return rendered;
            }
        }
        _ => {}
    }
    serde_json::to_string(item).unwrap_or_else(|_| item.to_string())
}

/// Renders the full `value` string of any `list.*` field. Returns `None` when the parsed
/// JSON is not an array.
fn metaobject_list_value_string(field_type: &str, parsed: &Value) -> Option<String> {
    let items = parsed.as_array()?;
    let rendered = items
        .iter()
        .map(|item| metaobject_list_value_token(field_type, item))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("[{rendered}]"))
}

fn metaobject_format_measurement_number(number: &Value) -> String {
    match number.as_f64() {
        Some(number) if number.fract() == 0.0 => format!("{number:.1}"),
        Some(number) => format!("{number}"),
        None => number.to_string(),
    }
}

fn metaobject_measurement_value_token(number: &Value, unit: &str) -> String {
    format!(
        "{{\"value\":{},\"unit\":\"{}\"}}",
        metaobject_format_measurement_number(number),
        unit.to_uppercase()
    )
}

/// Renders a measurement object's `value` field with Shopify's canonical formatting:
/// the numeric value is always emitted as a decimal (`5` -> `5.0`) and the unit is
/// uppercased. Returns `None` when the parsed JSON is not measurement-shaped.
fn metaobject_measurement_value_string(parsed: &Value) -> Option<String> {
    if let Some((number, unit)) = metaobject_measurement_parts(parsed) {
        return Some(metaobject_measurement_value_token(number, unit));
    }
    let items = parsed.as_array()?;
    if items.is_empty()
        || !items
            .iter()
            .all(|item| metaobject_measurement_parts(item).is_some())
    {
        return None;
    }
    let rendered = items
        .iter()
        .map(|item| {
            let (number, unit) = metaobject_measurement_parts(item).expect("checked above");
            metaobject_measurement_value_token(number, unit)
        })
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("[{rendered}]"))
}

/// Shopify re-emits a `rating` field's value with its keys in canonical order
/// (`scale_min`, `scale_max`, `value`) regardless of the order they were submitted in.
fn metaobject_rating_value_string(parsed: &Value) -> Option<String> {
    let object = parsed.as_object()?;
    if object.len() != 3 {
        return None;
    }
    let scale_min = object.get("scale_min")?;
    let scale_max = object.get("scale_max")?;
    let value = object.get("value")?;
    Some(format!(
        "{{\"scale_min\":{scale_min},\"scale_max\":{scale_max},\"value\":{value}}}"
    ))
}

/// Shopify normalizes a date_time without an explicit offset to UTC (`+00:00`).
fn metaobject_normalize_date_time_value(value: &str) -> String {
    let Some((_, time)) = value.split_once('T') else {
        return value.to_string();
    };
    let has_offset =
        time.contains('+') || time.contains('-') || time.ends_with('Z') || time.ends_with('z');
    if has_offset {
        value.to_string()
    } else {
        format!("{value}+00:00")
    }
}

fn metaobject_value_matches_type(field_type: &str, value: &str) -> bool {
    match field_type {
        "number_integer" => value.parse::<i64>().is_ok(),
        "number_decimal" => value.parse::<f64>().is_ok(),
        "boolean" => matches!(value, "true" | "false"),
        "json" | "rich_text_field" => serde_json::from_str::<Value>(value).is_ok(),
        value_type if value_type.starts_with("list.") => serde_json::from_str::<Value>(value)
            .ok()
            .is_some_and(|value| value.is_array()),
        _ => true,
    }
}

fn metaobject_field_validation_value(validations: &[Value], name: &str) -> Option<String> {
    validations
        .iter()
        .find(|validation| validation["name"].as_str() == Some(name))
        .and_then(|validation| validation["value"].as_str())
        .map(str::to_string)
}

fn metaobject_value_is_valid_date(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) else {
        return false;
    };
    parts[0].len() == 4
        && (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
}

fn metaobject_value_is_valid_date_time(value: &str) -> bool {
    let Some((date_part, time_part)) = value.split_once(['T', ' ']) else {
        return false;
    };
    if !metaobject_value_is_valid_date(date_part) {
        return false;
    }
    let time_core = time_part.split(['+', 'Z', '.']).next().unwrap_or(time_part);
    let segments: Vec<&str> = time_core.split(':').collect();
    if !(2..=3).contains(&segments.len()) {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty() && segment.chars().all(|character| character.is_ascii_digit())
    })
}

const METAOBJECT_MONEY_INVALID_MESSAGE: &str = "Value must be a stringified JSON object with amount (numeric) and currency_code (string matching the shop's currency) fields.";
const METAOBJECT_LINK_SCHEME_INVALID_MESSAGE: &str =
    "Value must be one of the following URL schemes: http, https, mailto, sms, tel.";
const METAOBJECT_LINK_DOMAIN_INVALID_MESSAGE: &str =
    "Value must conform to the domain restriction you set.";
const METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE: &str =
    "Value must be a stringified JSON object with a value (numeric) and unit (string from one the supported measurement units) fields.";

struct MetaobjectFieldValueValidationContext<'a> {
    proxy: &'a DraftProxy,
    existing_id: Option<&'a str>,
    validate_existing_values: bool,
}

struct MetaobjectFieldValueError {
    message: String,
    code: &'static str,
    element_index: Value,
}

impl MetaobjectFieldValueError {
    fn invalid_value(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "INVALID_VALUE",
            element_index: Value::Null,
        }
    }

    fn with_element_index(mut self, element_index: Value) -> Self {
        self.element_index = element_index;
        self
    }

    fn taken(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "TAKEN",
            element_index: Value::Null,
        }
    }
}

fn metaobject_classic_measurement_value_error(field_type: &str, value: &str) -> Option<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return Some("Value must contain unit and value.".to_string());
    };
    let Some(object) = parsed.as_object() else {
        return Some("Value must contain unit and value.".to_string());
    };
    let has_numeric_value = object.get("value").is_some_and(|value| {
        value.is_number()
            || value
                .as_str()
                .is_some_and(|value| value.parse::<f64>().is_ok())
    });
    if !has_numeric_value {
        return Some("Value must contain unit and value.".to_string());
    }
    let Some(unit) = object.get("unit").and_then(Value::as_str) else {
        return Some("Value must contain unit and value.".to_string());
    };
    if unit.is_empty() {
        return Some("Value must contain unit and value.".to_string());
    }
    (!measurement_unit_is_supported(field_type, unit))
        .then(|| "Value must be a supported unit.".to_string())
}

fn metaobject_value_is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|character| character.is_ascii_hexdigit())
}

fn metaobject_value_is_valid_url(value: &str) -> bool {
    let lowercased = value.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "sms:", "tel:"]
        .iter()
        .any(|scheme| lowercased.starts_with(scheme))
}

fn metaobject_reference_value_error(
    value: &str,
    gid_types: &[&str],
    message: &str,
) -> Option<String> {
    if shopify_gid_resource_type(value).is_some_and(|resource_type| {
        gid_types
            .iter()
            .any(|gid_type| resource_type.eq_ignore_ascii_case(gid_type))
    }) {
        None
    } else {
        Some(message.to_string())
    }
}

fn metaobject_validation_string_list(validations: &[Value], name: &str) -> Vec<String> {
    let Some(value) = metaobject_field_validation_value(validations, name) else {
        return Vec::new();
    };
    match serde_json::from_str::<Value>(&value) {
        Ok(Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Ok(Value::String(value)) => vec![value],
        _ => vec![value],
    }
}

fn metaobject_link_url(value: &Value) -> Option<&str> {
    value.as_object()?.get("url")?.as_str()
}

fn metaobject_link_allowed_domains_match(url: &str, validations: &[Value]) -> bool {
    let allowed_domains = metaobject_validation_string_list(validations, "allowed_domains")
        .into_iter()
        .filter_map(|domain| {
            let trimmed = domain.trim().trim_start_matches("*.").to_ascii_lowercase();
            if trimmed.is_empty() {
                None
            } else if let Ok(parsed) = url::Url::parse(&trimmed) {
                parsed.host_str().map(|host| host.to_ascii_lowercase())
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<_>>();
    if allowed_domains.is_empty() {
        return true;
    }
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
    else {
        return false;
    };
    allowed_domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

fn metaobject_link_value_error(value: &str, validations: &[Value]) -> Option<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return Some("Value must be a valid link.".to_string());
    };
    if !is_shopify_link_value(&parsed) {
        if let Some(url) = metaobject_link_url(&parsed) {
            if !is_shopify_metafield_url(url) {
                return Some(METAOBJECT_LINK_SCHEME_INVALID_MESSAGE.to_string());
            }
        }
        return Some("Value must be a valid link.".to_string());
    }
    let Some(url) = metaobject_link_url(&parsed) else {
        return Some("Value must be a valid link.".to_string());
    };
    if metaobject_link_allowed_domains_match(url, validations) {
        None
    } else {
        Some(METAOBJECT_LINK_DOMAIN_INVALID_MESSAGE.to_string())
    }
}

fn metaobject_structured_measurement_value_error(field_type: &str, value: &str) -> Option<String> {
    let Ok(parsed) = serde_json::from_str::<Value>(value) else {
        return Some(METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE.to_string());
    };
    if !parsed.is_object() {
        return Some(METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE.to_string());
    }
    shopify_measurement_value_error(field_type, &parsed).map(|message| {
        if matches!(
            message.as_str(),
            "Value must contain unit and value." | "Value must be a non-negative number."
        ) {
            METAOBJECT_MEASUREMENT_OBJECT_INVALID_MESSAGE.to_string()
        } else {
            message
        }
    })
}

fn metaobject_language_value_error(value: &str) -> Option<String> {
    if default_available_locale_is_supported(value) {
        None
    } else {
        Some("Value must be in ISO 639-1 format.".to_string())
    }
}

fn metaobject_id_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    field_key: &str,
    value: &str,
) -> Option<MetaobjectFieldValueError> {
    if value.is_empty() {
        return None;
    }
    let context = context?;
    let taken = context
        .proxy
        .store
        .staged
        .metaobjects
        .values()
        .any(|record| {
            record.get("id").and_then(Value::as_str) != context.existing_id
                && record["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|field| {
                        field.get("key").and_then(Value::as_str) == Some(field_key)
                            && field.get("type").and_then(Value::as_str) == Some("id")
                            && field.get("value").and_then(Value::as_str) == Some(value)
                    })
        });
    taken.then(|| {
        MetaobjectFieldValueError::taken(
            "Value is already assigned to another metafield. Choose a different value to ensure it remains unique.",
        )
    })
}

fn metaobject_mixed_reference_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    value: &str,
    validations: &[Value],
) -> Option<String> {
    if shopify_gid_resource_type(value) != Some("Metaobject") {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    }
    let allowed_definition_ids = {
        let mut values =
            metaobject_validation_string_list(validations, "metaobject_definition_ids");
        values.extend(metaobject_validation_string_list(
            validations,
            "metaobject_definition_id",
        ));
        values
    };
    if allowed_definition_ids.is_empty() {
        return None;
    }
    let context = context?;
    let record = context.proxy.metaobject_by_id(value)?;
    let Some(meta_type) = record.get("type").and_then(Value::as_str) else {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    };
    let Some(definition) = context
        .proxy
        .metaobject_definition_by_type(meta_type)
        .or_else(|| metaobject_definition_from_record(&record))
    else {
        return Some(
            "Value must belong to one of the specified metaobject definitions.".to_string(),
        );
    };
    let target_definition_id = definition.get("id").and_then(Value::as_str);
    if target_definition_id
        .is_some_and(|id| allowed_definition_ids.iter().any(|allowed| allowed == id))
    {
        None
    } else {
        Some("Value must belong to one of the specified metaobject definitions.".to_string())
    }
}

/// Validates a single (non-list) metaobject field value against its type and
/// declared validations, returning Shopify's specific error message when the
/// value is unacceptable. `is_update` captures create/update asymmetry (a
/// malformed boolean is tolerated on create but rejected on update).
fn metaobject_scalar_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    field_key: &str,
    field_type: &str,
    value: &str,
    validations: &[Value],
    is_update: bool,
) -> Option<MetaobjectFieldValueError> {
    if field_type == "id" {
        return metaobject_id_value_error(context, field_key, value);
    }

    let message = match field_type {
        "number_integer" => {
            let Ok(parsed) = value.parse::<i64>() else {
                return Some(MetaobjectFieldValueError::invalid_value(
                    "Value must be an integer.",
                ));
            };
            if let Some(max) = metaobject_field_validation_value(validations, "max")
                .and_then(|max| max.parse::<i64>().ok())
            {
                if parsed > max {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a maximum of {max}."
                    )));
                }
            }
            if let Some(min) = metaobject_field_validation_value(validations, "min")
                .and_then(|min| min.parse::<i64>().ok())
            {
                if parsed < min {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a minimum of {min}."
                    )));
                }
            }
            None
        }
        "number_decimal" => {
            if value.parse::<f64>().is_err() {
                Some("Value must be a decimal.".to_string())
            } else {
                None
            }
        }
        "boolean" => {
            if matches!(value, "true" | "false") || !is_update {
                None
            } else {
                Some("Value must be true or false.".to_string())
            }
        }
        "date" => {
            if metaobject_value_is_valid_date(value) {
                None
            } else {
                Some("Value must be in YYYY-MM-DD format.".to_string())
            }
        }
        "date_time" => {
            if metaobject_value_is_valid_date_time(value) {
                None
            } else {
                Some("Value must be in “YYYY-MM-DDTHH:MM:SS” format. For example: 2022-06-01T15:30:00".to_string())
            }
        }
        "money" => serde_json::from_str::<Value>(value)
            .ok()
            .as_ref()
            .filter(|parsed| is_shopify_money_value(parsed))
            .map(|_| ())
            .is_none()
            .then(|| METAOBJECT_MONEY_INVALID_MESSAGE.to_string()),
        "link" => metaobject_link_value_error(value, validations),
        "language" => metaobject_language_value_error(value),
        "dimension" | "volume" | "weight" => {
            metaobject_classic_measurement_value_error(field_type, value)
        }
        "rating" => {
            let parsed = serde_json::from_str::<Value>(value).ok()?;
            let rating = parsed.get("value").and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.as_str()?.parse::<f64>().ok())
            })?;
            if let Some(scale_max) = metaobject_field_validation_value(validations, "scale_max") {
                if scale_max
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|max| rating > max)
                {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a maximum of {scale_max}."
                    )));
                }
            }
            if let Some(scale_min) = metaobject_field_validation_value(validations, "scale_min") {
                if scale_min
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|min| rating < min)
                {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a minimum of {scale_min}."
                    )));
                }
            }
            None
        }
        "color" => {
            if metaobject_value_is_hex_color(value) {
                None
            } else {
                Some("Value must be a hex color code.".to_string())
            }
        }
        "url" => {
            if metaobject_value_is_valid_url(value) {
                None
            } else {
                Some("Value cannot have an empty scheme (protocol), must include one of the following URL schemes: [\"http\", \"https\", \"mailto\", \"sms\", \"tel\"].'".to_string())
            }
        }
        "product_reference" => metaobject_reference_value_error(
            value,
            &["Product"],
            "Value must be a valid product reference.",
        ),
        "variant_reference" => metaobject_reference_value_error(
            value,
            &["ProductVariant"],
            "Value must be a valid product variant reference.",
        ),
        "collection_reference" => metaobject_reference_value_error(
            value,
            &["Collection"],
            "Value must be a valid collection reference.",
        ),
        "customer_reference" => metaobject_reference_value_error(
            value,
            &["Customer"],
            "Value must be a valid customer reference.",
        ),
        "company_reference" => metaobject_reference_value_error(
            value,
            &["Company"],
            "Value must be a valid company reference.",
        ),
        "metaobject_reference" => metaobject_reference_value_error(
            value,
            &["Metaobject"],
            "Value require that you select a metaobject.",
        ),
        "file_reference" => metaobject_reference_value_error(
            value,
            &[
                "MediaImage",
                "GenericFile",
                "Video",
                "ExternalVideo",
                "Model3d",
                "File",
            ],
            "Value must be a file reference string.",
        ),
        "page_reference" => metaobject_reference_value_error(
            value,
            &["Page"],
            "Value must be a valid page reference.",
        ),
        "order_reference" => metaobject_reference_value_error(
            value,
            &["Order"],
            "Value must be a valid order reference.",
        ),
        "article_reference" => metaobject_reference_value_error(
            value,
            &["Article"],
            "Value must be a valid article reference.",
        ),
        "product_taxonomy_value_reference" => metaobject_reference_value_error(
            value,
            &["ProductTaxonomyValue", "TaxonomyValue"],
            "Value require that you select a product taxonomy value.",
        ),
        "mixed_reference" => metaobject_mixed_reference_value_error(context, value, validations),
        "single_line_text_field" | "multi_line_text_field" => {
            if let Some(max) = metaobject_field_validation_value(validations, "max")
                .and_then(|max| max.parse::<usize>().ok())
            {
                if value.chars().count() > max {
                    return Some(MetaobjectFieldValueError::invalid_value(format!(
                        "Value has a maximum length of {max}."
                    )));
                }
            }
            None
        }
        _ if is_measurement_metafield_type_name(field_type) => {
            metaobject_structured_measurement_value_error(field_type, value)
        }
        _ => None,
    };
    message.map(MetaobjectFieldValueError::invalid_value)
}

/// Validates a metaobject field value (scalar or `list.<type>`), returning the
/// error message and the `elementIndex` Shopify reports (null for scalars, the
/// offending element's index for list values).
/// Types whose structural validity is checked by `metaobject_value_matches_type`
/// rather than the typed value validator (which returns no opinion for them).
fn metaobject_value_uses_generic_fallback(field_type: &str) -> bool {
    field_type.starts_with("list.") || matches!(field_type, "json" | "rich_text_field")
}

fn metaobject_field_value_error(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    field_key: &str,
    field_type: &str,
    value: &str,
    validations: &[Value],
    is_update: bool,
) -> Option<MetaobjectFieldValueError> {
    if let Some(base_type) = field_type.strip_prefix("list.") {
        let parsed = serde_json::from_str::<Value>(value).ok()?;
        let elements = parsed.as_array()?;
        for (index, element) in elements.iter().enumerate() {
            let element_value = match element {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            if let Some(error) = metaobject_scalar_value_error(
                context,
                field_key,
                base_type,
                &element_value,
                validations,
                is_update,
            ) {
                return Some(error.with_element_index(json!(index)));
            }
        }
        None
    } else {
        metaobject_scalar_value_error(
            context,
            field_key,
            field_type,
            value,
            validations,
            is_update,
        )
    }
}

fn metaobject_create_input_values(
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for field in resolved_object_list_field(input, "fields") {
        if let (Some(key), Some(value)) = (
            resolved_string_field(&field, "key"),
            resolved_string_field(&field, "value"),
        ) {
            values.insert(key, value);
        }
    }
    if let Some(object) = resolved_object_field(input, "values") {
        for (key, value) in &object {
            match value {
                ResolvedValue::String(value) => {
                    values.insert(key.clone(), value.clone());
                }
                ResolvedValue::Null => {
                    values.insert(key.clone(), String::new());
                }
                _ => {
                    values.insert(key.clone(), resolved_value_json(value).to_string());
                }
            }
        }
    }
    values
}

fn metaobject_input_field_by_key(
    input: &BTreeMap<String, ResolvedValue>,
    target_key: &str,
) -> Option<(usize, BTreeMap<String, ResolvedValue>)> {
    resolved_object_list_field(input, "fields")
        .into_iter()
        .enumerate()
        .find_map(|(index, field)| {
            (resolved_string_field(&field, "key").as_deref() == Some(target_key))
                .then_some((index, field))
        })
}

fn metaobject_existing_field_values(record: &Value) -> BTreeMap<String, String> {
    record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| {
            Some((
                field.get("key")?.as_str()?.to_string(),
                field
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect()
}

fn metaobject_merged_input_values(
    record: &Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, String> {
    let mut values = metaobject_existing_field_values(record);
    values.extend(metaobject_create_input_values(input));
    values
}

fn metaobject_handle_validation_errors(handle: &str, field: Vec<&str>) -> Vec<Value> {
    let mut errors = Vec::new();
    if handle.is_empty() {
        errors.push(metaobject_field_error(
            field.clone(),
            &blank_message("Handle"),
            BLANK_USER_ERROR_CODE,
        ));
    }
    if handle.len() > 255 {
        errors.push(metaobject_field_error(
            field.clone(),
            &too_long_message("Handle", 255),
            TOO_LONG_USER_ERROR_CODE,
        ));
    }
    if handle.is_empty()
        || handle.starts_with('-')
        || handle.ends_with('-')
        || !handle
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        errors.push(metaobject_field_error(
            field,
            "Handle is invalid",
            "INVALID",
        ));
    }
    errors
}

fn metaobject_create_validation_errors(
    context: Option<&MetaobjectFieldValueValidationContext<'_>>,
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    is_update: bool,
) -> Vec<Value> {
    let mut errors = metaobject_create_duplicate_field_errors(input);
    let definition_keys = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field.get("key").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut provided_keys = BTreeSet::new();

    for (index, field) in resolved_object_list_field(input, "fields")
        .iter()
        .enumerate()
    {
        let key = resolved_string_field(field, "key").unwrap_or_default();
        provided_keys.insert(key.clone());
        if !definition_keys.contains(key.as_str()) {
            errors.push(metaobject_user_error(
                vec!["metaobject", "fields", &index.to_string()],
                &format!("Field definition \"{key}\" does not exist"),
                "UNDEFINED_OBJECT_FIELD",
                json!(key),
                Value::Null,
            ));
        } else if let Some(field_definition) = definition["fieldDefinitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|definition| definition.get("key").and_then(Value::as_str) == Some(key.as_str()))
        {
            let value = resolved_string_field(field, "value").unwrap_or_default();
            let field_type = field_definition["type"]["name"]
                .as_str()
                .unwrap_or_default();
            let validations = field_definition["validations"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if let Some(error) = metaobject_field_value_error(
                context,
                &key,
                field_type,
                &value,
                validations,
                is_update,
            ) {
                errors.push(metaobject_user_error(
                    vec!["metaobject", "fields", &index.to_string()],
                    &error.message,
                    error.code,
                    json!(key),
                    error.element_index,
                ));
            } else if metaobject_value_uses_generic_fallback(field_type)
                && !metaobject_value_matches_type(field_type, &value)
            {
                // json/rich-text/list-shape validation that the typed
                // validator intentionally defers to the structural check.
                errors.push(metaobject_user_error(
                    vec!["metaobject", "fields", &index.to_string()],
                    &format!("Value is invalid for field \"{key}\"."),
                    "INVALID_VALUE",
                    json!(key),
                    json!(index),
                ));
            }
        }
    }

    // Undefined keys are flagged only for fields the caller explicitly supplied in
    // this request (handled in the `fields` loop above). Stale values merged from a
    // pre-existing entry whose definition later dropped the field are NOT re-flagged
    // here — Shopify only errors on keys present in the current input.

    for field_definition in definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let key = field_definition
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if context.is_some_and(|context| context.validate_existing_values)
            && !provided_keys.contains(key)
        {
            if let Some(value) = input_values.get(key).filter(|value| !value.is_empty()) {
                let field_type = field_definition["type"]["name"]
                    .as_str()
                    .unwrap_or_default();
                let validations = field_definition["validations"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                if let Some(error) = metaobject_field_value_error(
                    context,
                    key,
                    field_type,
                    value,
                    validations,
                    is_update,
                ) {
                    errors.push(metaobject_user_error(
                        vec!["metaobject"],
                        &error.message,
                        error.code,
                        json!(key),
                        error.element_index,
                    ));
                } else if metaobject_value_uses_generic_fallback(field_type)
                    && !metaobject_value_matches_type(field_type, value)
                {
                    errors.push(metaobject_user_error(
                        vec!["metaobject"],
                        &format!("Value is invalid for field \"{key}\"."),
                        "INVALID_VALUE",
                        json!(key),
                        Value::Null,
                    ));
                }
            }
        }
        if field_definition
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && input_values
                .get(key)
                .is_none_or(|value| value.trim().is_empty())
        {
            // A required field reads as blank either because the caller supplied it
            // empty (anchor at that input field's index) or omitted it entirely
            // (anchor at the metaobject root). The message uses the field's display
            // name: "Summary can't be blank".
            let provided_index = resolved_object_list_field(input, "fields")
                .iter()
                .rposition(|field| resolved_string_field(field, "key").as_deref() == Some(key));
            let field_path = match provided_index {
                Some(index) => json!(["metaobject", "fields", index.to_string()]),
                None => json!(["metaobject"]),
            };
            let field_name = field_definition
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| metaobject_field_name(key));
            errors.push(metaobject_indexed_user_error(
                field_path,
                &format!("{field_name} can't be blank"),
                Some("OBJECT_FIELD_REQUIRED"),
                json!(key),
                Value::Null,
            ));
        }
    }

    if let Some(capabilities) = resolved_object_field(input, "capabilities") {
        // Shopify reports capability guard errors in a fixed capability order
        // (publishable, onlineStore, renderable, translatable) regardless of the
        // order the caller supplied them, anchored at
        // ["metaobject", "capabilities", <name>] with a null elementKey.
        for key in ["publishable", "onlineStore", "renderable", "translatable"] {
            if !capabilities.contains_key(key) {
                continue;
            }
            let enabled = definition["capabilities"][key]["enabled"]
                .as_bool()
                .unwrap_or(false);
            if !enabled {
                let message_key = if key == "onlineStore" {
                    "online_store"
                } else {
                    key
                };
                errors.push(metaobject_user_error(
                    vec!["metaobject", "capabilities", key],
                    &format!("Capability is not enabled: {message_key}"),
                    "CAPABILITY_NOT_ENABLED",
                    Value::Null,
                    Value::Null,
                ));
            }
        }
    }

    errors
}

fn metaobject_user_error(
    field: Vec<&str>,
    message: &str,
    code: &str,
    element_key: Value,
    element_index: Value,
) -> Value {
    metaobject_indexed_user_error(field, message, Some(code), element_key, element_index)
}

fn metaobject_field_error(field: impl Into<UserErrorField>, message: &str, code: &str) -> Value {
    metaobject_indexed_user_error(field, message, Some(code), Value::Null, Value::Null)
}

fn metaobject_no_definition_error(path_root: &str, meta_type: &str, code: &str) -> Value {
    metaobject_field_error(
        vec![path_root, "type"],
        &format!("No metaobject definition exists for type \"{meta_type}\""),
        code,
    )
}

fn metaobject_keyed_display_name(
    definition: &Value,
    input_values: &BTreeMap<String, String>,
) -> Option<String> {
    definition
        .get("displayNameKey")
        .and_then(Value::as_str)
        .and_then(|key| input_values.get(key))
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn metaobject_display_name(
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    handle_display_source: &str,
) -> String {
    metaobject_keyed_display_name(definition, input_values)
        .unwrap_or_else(|| metaobject_display_name_from_handle(handle_display_source))
}

fn metaobject_display_name_from_handle(handle: &str) -> String {
    let handle = handle.trim();
    if let Some((base, code)) = metaobject_random_handle_parts(handle) {
        return format!(
            "{} #{}",
            titleize_metaobject_handle(base),
            code.to_ascii_uppercase()
        );
    }
    titleize_metaobject_handle(handle)
}

fn metaobject_random_handle_parts(handle: &str) -> Option<(&str, &str)> {
    let (base, code) = handle.rsplit_once('-')?;
    if base.is_empty()
        || code.len() != 8
        || !code
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((base, code))
}

fn titleize_metaobject_handle(handle: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_was_lower_or_digit = false;
    for character in handle.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                words.push(current);
                current = String::new();
            }
            previous_was_lower_or_digit = false;
            continue;
        }
        if character.is_ascii_uppercase() && previous_was_lower_or_digit && !current.is_empty() {
            words.push(current);
            current = String::new();
        }
        previous_was_lower_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
        current.push(character);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .into_iter()
        .map(|word| {
            let mut lowercase = word.to_ascii_lowercase();
            if let Some(first) = lowercase.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            lowercase
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct MetaobjectHandleChoice {
    handle: String,
    display_source: String,
}

fn metaobject_random_handle_suffix(meta_type: &str, id: &str, attempt: u64) -> String {
    let seed = format!("{meta_type}:{id}:{attempt}");
    let digest = md5::compute(seed.as_bytes());
    format!("{digest:x}").chars().take(8).collect()
}

fn metaobject_publishable_status(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
) -> String {
    let publishable_enabled = definition["capabilities"]["publishable"]["enabled"]
        .as_bool()
        .unwrap_or(false);
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, "publishable"))
        .and_then(|publishable| resolved_string_field(&publishable, "status"))
        .unwrap_or_else(|| {
            if publishable_enabled {
                "DRAFT".to_string()
            } else {
                "ACTIVE".to_string()
            }
        })
}

fn metaobject_updated_publishable_status(
    input: &BTreeMap<String, ResolvedValue>,
    definition: &Value,
    existing: &Value,
) -> String {
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, "publishable"))
        .and_then(|publishable| resolved_string_field(&publishable, "status"))
        .or_else(|| {
            existing["capabilities"]["publishable"]["status"]
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_else(|| metaobject_publishable_status(input, definition))
}

fn metaobject_online_store_template_suffix_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let capabilities = resolved_object_field(input, "capabilities")?;
    let online_store = resolved_object_field(&capabilities, "onlineStore")?;
    match online_store.get("templateSuffix") {
        Some(ResolvedValue::String(template_suffix)) => Some(json!(template_suffix)),
        Some(ResolvedValue::Null) => Some(Value::Null),
        _ => None,
    }
}

fn metaobject_existing_online_store_template_suffix(existing: &Value) -> Option<Value> {
    let online_store = existing
        .get("capabilities")?
        .get("onlineStore")?
        .as_object()?;
    Some(
        online_store
            .get("templateSuffix")
            .cloned()
            .unwrap_or(Value::Null),
    )
}

fn metaobject_definition_online_store_url_handle(definition: &Value) -> Option<&str> {
    definition
        .get("capabilities")?
        .get("onlineStore")?
        .get("data")?
        .get("urlHandle")?
        .as_str()
        .filter(|url_handle| !url_handle.is_empty())
}

fn metaobject_definition_online_store_can_create_redirects(definition: &Value) -> bool {
    definition["capabilities"]["onlineStore"]["data"]["canCreateRedirects"]
        .as_bool()
        .unwrap_or(false)
}

fn metaobject_definition_input_create_redirects(input: &BTreeMap<String, ResolvedValue>) -> bool {
    resolved_object_field(input, "capabilities")
        .and_then(|capabilities| resolved_object_field(&capabilities, "onlineStore"))
        .and_then(|online_store| resolved_object_field(&online_store, "data"))
        .and_then(|data| {
            resolved_bool_field(&data, "createRedirects")
                .or_else(|| resolved_bool_field(&data, "canCreateRedirects"))
        })
        .unwrap_or(false)
}

fn metaobject_definition_has_renderable_online_store(definition: &Value) -> bool {
    definition["capabilities"]["renderable"]["enabled"]
        .as_bool()
        .unwrap_or(false)
        && definition["capabilities"]["onlineStore"]["enabled"]
            .as_bool()
            .unwrap_or(false)
        && metaobject_definition_online_store_url_handle(definition).is_some()
}

fn metaobject_record_has_active_online_store(record: &Value) -> bool {
    record["capabilities"]["publishable"]["status"].as_str() == Some("ACTIVE")
        && record
            .get("capabilities")
            .and_then(|capabilities| capabilities.get("onlineStore"))
            .is_some_and(|online_store| !online_store.is_null())
}

fn metaobject_page_path(definition: &Value, handle: &str) -> Option<String> {
    let url_handle = metaobject_definition_online_store_url_handle(definition)?;
    (!handle.is_empty()).then(|| format!("/pages/{url_handle}/{handle}"))
}

fn metaobject_required_field_errors_for_upsert(
    errors: Vec<Value>,
    definition: &Value,
) -> Vec<Value> {
    errors
        .into_iter()
        .map(|mut error| {
            if error.get("code").and_then(Value::as_str) == Some("OBJECT_FIELD_REQUIRED") {
                let key = error
                    .get("elementKey")
                    .and_then(Value::as_str)
                    .or_else(|| definition.get("displayNameKey").and_then(Value::as_str))
                    .unwrap_or("field")
                    .to_string();
                error["field"] = json!([]);
                error["message"] = json!(format!("{} can't be blank", metaobject_field_name(&key)));
            }
            error
        })
        .collect()
}

struct MetaobjectRecordOptions<'a> {
    created_at: Option<&'a str>,
    display_name: &'a str,
    publishable_status: &'a str,
    online_store_template_suffix: Value,
    updated_at: &'a str,
}

struct MetaobjectUpdateApplyOptions {
    definition_error_path_root: &'static str,
    handle_error_path: Vec<&'static str>,
    default_handle_display_source: Option<String>,
    rewrite_required_field_errors: bool,
    hydrate_requested_handle: bool,
    stage_redirect_new_handle: bool,
}

struct MetaobjectUpdateApplyContext<'a> {
    existing: Value,
    meta_type: &'a str,
    input: BTreeMap<String, ResolvedValue>,
    options: MetaobjectUpdateApplyOptions,
}

fn metaobject_record_from_definition_with_options(
    id: &str,
    handle: &str,
    definition: &Value,
    input_values: &BTreeMap<String, String>,
    options: MetaobjectRecordOptions<'_>,
) -> Value {
    let meta_type = definition["type"].as_str().unwrap_or_default();
    let fields = definition["fieldDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|field_definition| {
            let key = field_definition
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            metaobject_field_record_from_definition(field_definition, input_values.get(key))
        })
        .collect::<Vec<_>>();
    let title_field = definition["displayNameKey"]
        .as_str()
        .and_then(|key| {
            fields
                .iter()
                .find(|field| field.get("key").and_then(Value::as_str) == Some(key))
                .cloned()
        })
        .or_else(|| fields.first().cloned());
    let mut record = json!({
        "id": id,
        "handle": handle,
        "type": meta_type,
        "displayName": options.display_name,
        "updatedAt": options.updated_at,
        "capabilities": {
            "publishable": if definition["capabilities"]["publishable"]["enabled"].as_bool().unwrap_or(false) {
                json!({"status": options.publishable_status})
            } else {
                Value::Null
            },
            "onlineStore": if definition["capabilities"]["onlineStore"]["enabled"].as_bool().unwrap_or(false) {
                json!({"templateSuffix": options.online_store_template_suffix})
            } else {
                Value::Null
            }
        },
        "fields": fields,
        "titleField": title_field
    });
    if let Some(created_at) = options.created_at {
        record["createdAt"] = json!(created_at);
    }
    record
}

fn metaobject_nodes_from_upstream_data(data: &serde_json::Map<String, Value>) -> Vec<Value> {
    let mut nodes = Vec::new();
    for value in data.values() {
        if let Some(connection_nodes) = value.get("nodes").and_then(Value::as_array) {
            nodes.extend(
                connection_nodes
                    .iter()
                    .filter(|node| node.is_object())
                    .cloned(),
            );
        }
        if let Some(edges) = value.get("edges").and_then(Value::as_array) {
            nodes.extend(
                edges
                    .iter()
                    .filter_map(|edge| edge.get("node").filter(|node| node.is_object()).cloned()),
            );
        }
    }
    nodes
}

pub(in crate::proxy) fn metaobject_cursor(record: &Value) -> String {
    format!(
        "cursor:{}",
        record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("metaobject")
    )
}

fn metaobject_string_value(record: &Value, field: &str) -> String {
    record
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn metaobject_normalized_sort_value(record: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(metaobject_string_value(record, field).to_ascii_lowercase())
}

fn metaobject_id_sort_value(record: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(record.get("id").and_then(Value::as_str))
}

fn metaobject_staged_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let sort_key = sort_key
        .unwrap_or("id")
        .replace('-', "_")
        .to_ascii_lowercase();
    let primary = match sort_key.as_str() {
        "display_name" | "displayname" => metaobject_normalized_sort_value(record, "displayName"),
        "type" => metaobject_normalized_sort_value(record, "type"),
        "updated_at" | "updatedat" => StagedSortValue::String(metaobject_sortable_datetime(
            &metaobject_string_value(record, "updatedAt"),
        )),
        "id" => metaobject_id_sort_value(record),
        _ => metaobject_id_sort_value(record),
    };
    vec![primary, metaobject_id_sort_value(record)]
}

fn metaobject_connection_node_key(node: &Value) -> Option<String> {
    let meta_type = node.get("type").and_then(Value::as_str).unwrap_or_default();
    let handle = node
        .get("handle")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !meta_type.is_empty() && !handle.is_empty() {
        return Some(format!("type:{meta_type}:handle:{handle}"));
    }
    node.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(|id| format!("id:{id}"))
}

fn metaobject_definition_connection_node_key(node: &Value) -> Option<String> {
    node.get("type")
        .and_then(Value::as_str)
        .filter(|meta_type| !meta_type.is_empty())
        .map(|meta_type| format!("type:{meta_type}"))
        .or_else(|| {
            node.get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(|id| format!("id:{id}"))
        })
}

fn normalize_connection_page_info(connection: &mut Value) {
    let edge_cursors = connection
        .get("edges")
        .and_then(Value::as_array)
        .map(|edges| {
            edges
                .iter()
                .filter_map(|edge| edge.get("cursor").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let node_count = connection
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(edge_cursors.len());
    let Some(page_info) = connection
        .get_mut("pageInfo")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if node_count == 0 && edge_cursors.is_empty() {
        page_info.insert("startCursor".to_string(), Value::Null);
        page_info.insert("endCursor".to_string(), Value::Null);
        return;
    }
    if let Some(first) = edge_cursors.first() {
        page_info.insert("startCursor".to_string(), json!(first));
    }
    if let Some(last) = edge_cursors.last() {
        page_info.insert("endCursor".to_string(), json!(last));
    }
}

fn upstream_response_id<'a>(
    upstream_data: &'a serde_json::Map<String, Value>,
    response_key: &str,
) -> Option<&'a str> {
    upstream_data
        .get(response_key)?
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetaobjectSearchOperator {
    Eq,
    Gt,
    Gte,
    Lt,
    Lte,
}

fn metaobject_search_value(raw_value: &str) -> (MetaobjectSearchOperator, String) {
    let value = raw_value.trim().trim_matches('"').trim_matches('\'');
    if let Some(value) = value.strip_prefix(">=") {
        (MetaobjectSearchOperator::Gte, value.trim().to_string())
    } else if let Some(value) = value.strip_prefix('>') {
        (MetaobjectSearchOperator::Gt, value.trim().to_string())
    } else if let Some(value) = value.strip_prefix("<=") {
        (MetaobjectSearchOperator::Lte, value.trim().to_string())
    } else if let Some(value) = value.strip_prefix('<') {
        (MetaobjectSearchOperator::Lt, value.trim().to_string())
    } else {
        (MetaobjectSearchOperator::Eq, value.to_string())
    }
}

fn metaobject_compare_order<T: Ord>(
    actual: T,
    expected: T,
    operator: MetaobjectSearchOperator,
) -> bool {
    match operator {
        MetaobjectSearchOperator::Eq => actual == expected,
        MetaobjectSearchOperator::Gt => actual > expected,
        MetaobjectSearchOperator::Gte => actual >= expected,
        MetaobjectSearchOperator::Lt => actual < expected,
        MetaobjectSearchOperator::Lte => actual <= expected,
    }
}

fn metaobject_text_matches(actual: Option<&str>, raw_value: &str) -> bool {
    let (_, value) = metaobject_search_value(raw_value);
    let needle = value.to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    let actual = actual.unwrap_or_default().to_ascii_lowercase();
    if let Some(prefix) = needle.strip_suffix('*') {
        return actual
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&needle)
}

fn metaobject_id_matches(record: &Value, raw_value: &str) -> bool {
    let (operator, value) = metaobject_search_value(raw_value);
    let actual = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let actual_tail = resource_id_tail(actual);
    let expected_tail = resource_id_tail(&value);
    if operator == MetaobjectSearchOperator::Eq {
        return actual == value || actual_tail == expected_tail;
    }
    let Ok(actual_id) = actual_tail.parse::<i64>() else {
        return false;
    };
    let Ok(expected_id) = expected_tail.parse::<i64>() else {
        return false;
    };
    metaobject_compare_order(actual_id, expected_id, operator)
}

fn metaobject_updated_at_matches(record: &Value, raw_value: &str) -> bool {
    let value = raw_value.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return false;
    }
    let (operator, expected) = search_comparator(value);
    if expected.is_empty() {
        return false;
    }
    let Some(actual) = record.get("updatedAt").and_then(Value::as_str) else {
        return false;
    };
    let (actual, expected) = if expected.contains('T') {
        (
            metaobject_sortable_datetime(actual),
            metaobject_sortable_datetime(expected),
        )
    } else {
        (
            search_datetime_value(actual, expected).to_string(),
            expected.to_string(),
        )
    };
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(&expected),
    }
}

fn metaobject_sortable_datetime(value: &str) -> String {
    let value = value.trim();
    let Some(without_z) = value.strip_suffix('Z').or_else(|| value.strip_suffix('z')) else {
        return value.to_string();
    };
    let Some(time_start) = without_z.find('T').or_else(|| without_z.find('t')) else {
        return format!("{without_z}.000000000Z");
    };
    let time_part = &without_z[time_start + 1..];
    if let Some((base, fraction)) = without_z.rsplit_once('.') {
        if time_part.contains('.') && fraction.chars().all(|character| character.is_ascii_digit()) {
            let mut normalized = fraction.chars().take(9).collect::<String>();
            while normalized.len() < 9 {
                normalized.push('0');
            }
            return format!("{base}.{normalized}Z");
        }
    }
    format!("{without_z}.000000000Z")
}

fn metaobject_field_search_text(field: &Value) -> Option<String> {
    field
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            field.get("jsonValue").map(|value| match value {
                Value::String(value) => value.clone(),
                Value::Null => String::new(),
                value => value.to_string(),
            })
        })
}

fn metaobject_field_matches(record: &Value, key: &str, raw_value: &str) -> bool {
    record
        .get("fields")
        .and_then(Value::as_array)
        .map(|fields| {
            fields.iter().any(|field| {
                field.get("key").and_then(Value::as_str) == Some(key)
                    && metaobject_text_matches(
                        metaobject_field_search_text(field).as_deref(),
                        raw_value,
                    )
            })
        })
        .unwrap_or(false)
}

fn metaobject_free_text_matches(record: &Value, raw_value: &str) -> bool {
    metaobject_text_matches(record.get("displayName").and_then(Value::as_str), raw_value)
        || metaobject_text_matches(record.get("handle").and_then(Value::as_str), raw_value)
        || metaobject_text_matches(record.get("type").and_then(Value::as_str), raw_value)
        || record
            .get("fields")
            .and_then(Value::as_array)
            .map(|fields| {
                fields.iter().any(|field| {
                    metaobject_text_matches(
                        metaobject_field_search_text(field).as_deref(),
                        raw_value,
                    )
                })
            })
            .unwrap_or(false)
}

fn metaobject_search_term_decision(record: &Value, term: &str) -> StagedSearchDecision {
    let term = term.trim().trim_matches('\'').trim_matches('"');
    if term.is_empty() {
        return StagedSearchDecision::Match;
    }
    let Some((raw_key, raw_value)) = term.split_once(':') else {
        return StagedSearchDecision::from_bool(metaobject_free_text_matches(record, term));
    };
    let key = raw_key.trim();
    if key.is_empty() || raw_value.trim().is_empty() {
        return StagedSearchDecision::Unsupported;
    }
    let key_normalized = key.replace('-', "_").to_ascii_lowercase();
    let matches = match key_normalized.as_str() {
        "display_name" | "displayname" => {
            metaobject_text_matches(record.get("displayName").and_then(Value::as_str), raw_value)
        }
        "handle" => {
            metaobject_text_matches(record.get("handle").and_then(Value::as_str), raw_value)
        }
        "id" => metaobject_id_matches(record, raw_value),
        "updated_at" | "updatedat" => metaobject_updated_at_matches(record, raw_value),
        field_key if field_key.starts_with("fields.") => {
            let field_key = key.trim_start_matches("fields.");
            !field_key.is_empty() && metaobject_field_matches(record, field_key, raw_value)
        }
        _ => return StagedSearchDecision::Unsupported,
    };
    StagedSearchDecision::from_bool(matches)
}

fn metaobject_search_decision(record: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    for term in saved_search_query_tokens(query) {
        if term.eq_ignore_ascii_case("AND") {
            continue;
        }
        match metaobject_search_term_decision(record, &term) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::Match
}

impl DraftProxy {
    pub(in crate::proxy) fn has_local_metaobject_state(&self) -> bool {
        !self.store.staged.metaobject_definitions.is_empty()
            || !self.store.staged.metaobjects.is_empty()
    }

    /// Decides whether a metaobject mutation request should be staged locally or
    /// forwarded upstream. Create/Delete and definition Create/Delete are always
    /// emulated locally. Update/Upsert/DefinitionUpdate are emulated locally only
    /// when their target already exists in local staged state (i.e. it was created
    /// in this scenario): a backend that staged the resource locally also expects
    /// the proxy to mutate it locally. When the target lives upstream (seeded or
    /// live-captured records the proxy never created), the request is forwarded so
    /// the real backend response is used instead of a synthetic one.
    fn metaobject_mutation_is_local(&self, field: &MetaobjectRootInput) -> bool {
        match field.name.as_str() {
            "metaobjectUpdate" => resolved_string_field(&field.arguments, "id")
                .map(|id| self.metaobject_staged_key_by_id(&id).is_some())
                .unwrap_or(false),
            "metaobjectUpsert" => match resolved_object_field(&field.arguments, "handle") {
                Some(handle) => resolved_string_field(&handle, "type")
                    .map(|meta_type| self.metaobject_definition_by_type(&meta_type).is_some())
                    .unwrap_or(false),
                _ => false,
            },
            "metaobjectDefinitionUpdate" => resolved_string_field(&field.arguments, "id")
                .map(|id| self.metaobject_definition_staged_key_by_id(&id).is_some())
                .unwrap_or(false),
            // Creates and deletes are always emulated locally.
            _ => true,
        }
    }

    fn metaobject_query_value(&self, field: &MetaobjectRootInput, request: &Request) -> Value {
        match field.name.as_str() {
            "metaobjects" => self.metaobject_connection(field),
            "metaobject" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.metaobject_by_id(&id)
                    .map(|record| self.metaobject_canonical_value(&record))
                    .unwrap_or(Value::Null)
            }
            "metaobjectByHandle" => self.metaobject_by_handle_arg(field).unwrap_or(Value::Null),
            "metaobjectDefinition" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.metaobject_definition_by_id(&id)
                    .map(|definition| self.metaobject_definition_canonical_value(&definition))
                    .unwrap_or(Value::Null)
            }
            "metaobjectDefinitionByType" => {
                let meta_type =
                    resolved_metaobject_definition_type_arg(field.arguments.get("type"), request);
                self.metaobject_definition_by_type(&meta_type)
                    .map(|definition| self.metaobject_definition_canonical_value(&definition))
                    .unwrap_or(Value::Null)
            }
            "metaobjectDefinitions" => self.metaobject_definition_connection(field),
            _ => Value::Null,
        }
    }

    fn metaobject_live_hybrid_outcome(
        &mut self,
        request: &Request,
        field: &MetaobjectRootInput,
    ) -> ResolverOutcome<Value> {
        let mut result =
            self.cached_or_forward_upstream_graphql_result(request, &field.response_key);
        let Some(data) = result.data.as_object_mut() else {
            return result.outcome;
        };
        let mut canonical_fallback = None;
        if !data.contains_key(&field.response_key) {
            if let Some(value) = data.get(&field.name).cloned() {
                data.insert(field.response_key.clone(), value);
                canonical_fallback = data.get(&field.response_key).cloned();
            }
        }
        let upstream_nodes = metaobject_nodes_from_upstream_data(data);
        if !data.contains_key(&field.response_key) {
            let fallback = match field.name.as_str() {
                "metaobject" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    upstream_nodes
                        .iter()
                        .find(|node| node.get("id").and_then(Value::as_str) == Some(&id))
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                "metaobjectByHandle" => {
                    let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
                        return result.outcome;
                    };
                    let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
                    let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
                    upstream_nodes
                        .iter()
                        .find(|node| {
                            node.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                                && node.get("handle").and_then(Value::as_str)
                                    == Some(meta_handle.as_str())
                        })
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), fallback);
            canonical_fallback = data.get(&field.response_key).cloned();
        }
        if let Some(value) = self.metaobject_live_hybrid_overlay_value(field, request, data) {
            data.insert(field.response_key.clone(), value.clone());
            // The overlay is a canonical domain value, even when it was merged
            // with an upstream connection. Nested argument-bearing fields must
            // therefore continue through the local field resolver registry.
            result.outcome.value_source = crate::admin_graphql::ResolverValueSource::Local;
            result.outcome.value = value;
        } else if let Some(value) = canonical_fallback {
            // Some hydration transports return a canonical root key or only a
            // sibling `nodes` payload. That derived value is local resolver
            // input, not an alias-shaped Shopify transport result.
            result.outcome.value_source = crate::admin_graphql::ResolverValueSource::Local;
            result.outcome.value = value;
        }
        result.outcome
    }

    fn metaobject_live_hybrid_overlay_value(
        &self,
        field: &MetaobjectRootInput,
        request: &Request,
        upstream_data: &serde_json::Map<String, Value>,
    ) -> Option<Value> {
        match field.name.as_str() {
            "metaobject" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                if self.store.staged.metaobjects.is_tombstoned(&id) {
                    return Some(Value::Null);
                }
                self.metaobject_by_id(&id)
                    .map(|record| self.metaobject_canonical_value(&record))
            }
            "metaobjectByHandle" => {
                let Some(ResolvedValue::Object(handle)) = field.arguments.get("handle") else {
                    return None;
                };
                let meta_type = resolved_string_field(handle, "type").unwrap_or_default();
                let meta_handle = resolved_string_field(handle, "handle").unwrap_or_default();
                if let Some(record) = self.metaobject_by_type_and_handle(&meta_type, &meta_handle) {
                    return Some(self.metaobject_canonical_value(&record));
                }
                if upstream_response_id(upstream_data, &field.response_key)
                    .is_some_and(|id| self.store.staged.metaobjects.is_tombstoned(id))
                {
                    return Some(Value::Null);
                }
                None
            }
            "metaobjectDefinition" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                if self.store.staged.metaobject_definitions.is_tombstoned(&id) {
                    return Some(Value::Null);
                }
                self.metaobject_definition_by_id(&id)
                    .map(|definition| self.metaobject_definition_canonical_value(&definition))
            }
            "metaobjectDefinitionByType" => {
                let meta_type =
                    resolved_metaobject_definition_type_arg(field.arguments.get("type"), request);
                if let Some(definition) = self.metaobject_definition_by_type(&meta_type) {
                    return Some(self.metaobject_definition_canonical_value(&definition));
                }
                if upstream_response_id(upstream_data, &field.response_key)
                    .is_some_and(|id| self.store.staged.metaobject_definitions.is_tombstoned(id))
                {
                    return Some(Value::Null);
                }
                None
            }
            "metaobjects" => {
                if !self.has_local_metaobject_state() {
                    return None;
                }
                let local = self.metaobject_connection(field);
                Some(self.merge_metaobject_connection_overlay(
                    upstream_data.get(&field.response_key),
                    local,
                    field,
                    metaobject_connection_node_key,
                    |id| self.store.staged.metaobjects.is_tombstoned(id),
                ))
            }
            "metaobjectDefinitions" => {
                if !self.has_local_metaobject_state() {
                    return None;
                }
                let local = self.metaobject_definition_connection(field);
                Some(self.merge_metaobject_connection_overlay(
                    upstream_data.get(&field.response_key),
                    local,
                    field,
                    metaobject_definition_connection_node_key,
                    |id| self.store.staged.metaobject_definitions.is_tombstoned(id),
                ))
            }
            _ => None,
        }
    }

    fn merge_metaobject_connection_overlay<F, T>(
        &self,
        upstream: Option<&Value>,
        local: Value,
        field: &MetaobjectRootInput,
        node_key: F,
        is_tombstoned: T,
    ) -> Value
    where
        F: Fn(&Value) -> Option<String>,
        T: Fn(&str) -> bool,
    {
        let Some(upstream) = upstream.filter(|value| value.is_object()) else {
            return local;
        };
        let mut merged = upstream.clone();
        let local_nodes = local
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let local_edges = local
            .get("edges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut local_by_key = BTreeMap::new();
        for node in &local_nodes {
            if let Some(key) = node_key(node) {
                local_by_key.insert(key, node.clone());
            }
        }
        for edge in &local_edges {
            if let Some(key) = edge.get("node").and_then(&node_key) {
                local_by_key
                    .entry(key)
                    .or_insert_with(|| edge.get("node").cloned().unwrap_or(Value::Null));
            }
        }

        let mut tombstoned_keys = BTreeSet::new();
        let mut represented_node_keys = BTreeSet::new();
        if let Some(nodes) = merged.get_mut("nodes").and_then(Value::as_array_mut) {
            let mut filtered = Vec::new();
            for node in nodes.drain(..) {
                let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
                if !id.is_empty() && is_tombstoned(id) {
                    if let Some(key) = node_key(&node) {
                        tombstoned_keys.insert(key);
                    }
                    continue;
                }
                if let Some(key) = node_key(&node) {
                    represented_node_keys.insert(key.clone());
                    filtered.push(local_by_key.get(&key).cloned().unwrap_or(node));
                } else {
                    filtered.push(node);
                }
            }
            *nodes = filtered;
        }
        let mut represented_edge_keys = BTreeSet::new();
        if let Some(edges) = merged.get_mut("edges").and_then(Value::as_array_mut) {
            let mut filtered = Vec::new();
            for mut edge in edges.drain(..) {
                let node = edge.get("node").cloned().unwrap_or(Value::Null);
                let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
                if !id.is_empty() && is_tombstoned(id) {
                    if let Some(key) = node_key(&node) {
                        tombstoned_keys.insert(key);
                    }
                    continue;
                }
                if let Some(key) = node_key(&node) {
                    represented_edge_keys.insert(key.clone());
                    if let Some(local_node) = local_by_key.get(&key) {
                        if let Some(edge_object) = edge.as_object_mut() {
                            edge_object.insert("node".to_string(), local_node.clone());
                        }
                    }
                }
                filtered.push(edge);
            }
            *edges = filtered;
        }

        for key in tombstoned_keys {
            represented_node_keys.insert(key.clone());
            represented_edge_keys.insert(key);
        }

        let first_limit = resolved_int_field(&field.arguments, "first")
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(usize::MAX);
        if let Some(nodes) = merged.get_mut("nodes").and_then(Value::as_array_mut) {
            let mut remaining = first_limit.saturating_sub(nodes.len());
            if remaining > 0 {
                for node in &local_nodes {
                    let Some(key) = node_key(node) else {
                        continue;
                    };
                    if represented_node_keys.contains(&key) {
                        continue;
                    }
                    nodes.push(node.clone());
                    represented_node_keys.insert(key);
                    remaining = remaining.saturating_sub(1);
                    if remaining == 0 {
                        break;
                    }
                }
            }
        }
        if let Some(edges) = merged.get_mut("edges").and_then(Value::as_array_mut) {
            let mut remaining = first_limit.saturating_sub(edges.len());
            if remaining > 0 {
                for edge in &local_edges {
                    let Some(key) = edge.get("node").and_then(&node_key) else {
                        continue;
                    };
                    if represented_edge_keys.contains(&key) {
                        continue;
                    }
                    edges.push(edge.clone());
                    represented_edge_keys.insert(key);
                    remaining = remaining.saturating_sub(1);
                    if remaining == 0 {
                        break;
                    }
                }
            }
        }
        normalize_connection_page_info(&mut merged);
        merged
    }

    fn metaobject_mutation_outcome(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        query: &str,
    ) -> ResolverOutcome<Value> {
        let mut staged_ids = Vec::new();
        let mut log_successful_noop = false;
        let mut errors = Vec::new();
        let value = if field.name == "metaobjectBulkDelete" {
            if let Some(error) = metaobject_bulk_delete_selector_error(field, query) {
                errors.push(error);
                Value::Null
            } else {
                self.metaobject_mutation_value(
                    field,
                    request,
                    &mut staged_ids,
                    &mut log_successful_noop,
                )
            }
        } else {
            self.metaobject_mutation_value(
                field,
                request,
                &mut staged_ids,
                &mut log_successful_noop,
            )
        };
        let should_log = !staged_ids.is_empty() || log_successful_noop;
        if should_log {
            // Each successful metaobject mutation reserves one synthetic id for its
            // mutation-log entry after allocating the resources it creates, matching
            // the current synthetic-id bookkeeping (e.g. a definition lands on /1 and
            // the next entry on /3 because the definition's log entry consumed /2).
            self.reserve_synthetic_log_id();
        }
        let mut outcome = ResolverOutcome::value(value);
        if !errors.is_empty() {
            outcome.errors = root_field_errors_from_json(&errors, &field.response_key);
        }
        if should_log {
            outcome
                .log_drafts
                .push(LogDraft::staged(&field.name, "metaobjects", staged_ids));
        }
        outcome
    }

    fn metaobject_mutation_value(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
        log_successful_noop: &mut bool,
    ) -> Value {
        match field.name.as_str() {
            "metaobjectCreate" => self.metaobject_create(field, request, staged_ids),
            "metaobjectUpdate" => self.metaobject_update(field, request, staged_ids),
            "metaobjectUpsert" => self.metaobject_upsert(field, request, staged_ids),
            "metaobjectDelete" => self.metaobject_delete(field, request, staged_ids),
            "metaobjectBulkDelete" => {
                self.metaobject_bulk_delete(field, request, staged_ids, log_successful_noop)
            }
            "metaobjectDefinitionCreate" => {
                self.metaobject_definition_create(field, request, staged_ids)
            }
            "metaobjectDefinitionUpdate" => self.metaobject_definition_update(field, staged_ids),
            "metaobjectDefinitionDelete" => self.metaobject_definition_delete(field, staged_ids),
            "standardMetaobjectDefinitionEnable" => {
                self.standard_metaobject_definition_enable(field, staged_ids, log_successful_noop)
            }
            _ => Value::Null,
        }
    }

    pub(in crate::proxy) fn metaobject_node_value_by_id(&self, id: &str) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("Metaobject") => {
                let key = self.metaobject_staged_key_by_id(id)?;
                if self.store.staged.metaobjects.is_tombstoned(&key) {
                    return Some(Value::Null);
                }
                self.store
                    .staged
                    .metaobjects
                    .get(&key)
                    .cloned()
                    .map(|record| self.metaobject_canonical_value(&record))
            }
            Some("MetaobjectDefinition") => {
                let key = self.metaobject_definition_staged_key_by_id(id)?;
                if self.store.staged.metaobject_definitions.is_tombstoned(&key) {
                    return Some(Value::Null);
                }
                self.store
                    .staged
                    .metaobject_definitions
                    .get(&key)
                    .cloned()
                    .map(|definition| self.metaobject_definition_canonical_value(&definition))
            }
            _ => None,
        }
    }

    pub(in crate::proxy) fn metaobject_canonical_value(&self, record: &Value) -> Value {
        let mut record = self.project_metaobject_against_definition(record);
        record["__typename"] = json!("Metaobject");
        record
    }

    fn stored_metaobject_canonical_value(&self, record: &Value) -> Value {
        let mut record = record.clone();
        record["__typename"] = json!("Metaobject");
        record
    }

    fn metaobject_staged_key_by_id(&self, id: &str) -> Option<String> {
        staged_record_key_for_shopify_gid(&self.store.staged.metaobjects, id, "Metaobject")
    }

    pub(in crate::proxy) fn metaobject_by_id(&self, id: &str) -> Option<Value> {
        let key = self.metaobject_staged_key_by_id(id)?;
        self.store.staged.metaobjects.get(&key).cloned()
    }

    /// Resolve a linked metaobject reference (the gid stored in a product option's
    /// `linkedMetafieldValue`) to its display name, projected against the current
    /// definition. Used to render product option values whose names mirror the linked
    /// metaobject entry (e.g. "One"/"Two") rather than echoing the raw gid.
    pub(in crate::proxy) fn linked_metaobject_display_name(&self, id: &str) -> Option<String> {
        let record = self.metaobject_by_id(id)?;
        self.project_metaobject_against_definition(&record)
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn hydrate_metaobject_by_id(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query MetaobjectHydrateById($id: ID!) { node(id: $id) { __typename } metaobject(id: $id) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } titleField: field(key: \"title\") { key type value jsonValue definition { key name required type { name category } } } } }",
                "variables": {"id": id}
            }),
        );
        let record = response.body["data"]["metaobject"].clone();
        if !record.is_object() {
            return None;
        }
        self.store
            .staged
            .metaobjects
            .insert(id.to_string(), record.clone());
        Some(record)
    }

    fn hydrate_metaobjects_by_ids(&mut self, request: &Request, ids: &[String]) {
        if self.config.read_mode == ReadMode::Snapshot || ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "#graphql\n  query MetaobjectBulkDeleteHydrateByIds($ids: [ID!]!) {\n    nodes(ids: $ids) {\n      __typename\n      ... on Metaobject {\n        id\n        handle\n        type\n        displayName\n        createdAt\n        updatedAt\n        capabilities {\n          publishable {\n            status\n          }\n          onlineStore {\n            templateSuffix\n          }\n        }\n        fields {\n          key\n          type\n          value\n          jsonValue\n          definition {\n            key\n            name\n            required\n            type {\n              name\n              category\n            }\n          }\n        }\n        titleField: field(key: \"title\") {\n          key\n          type\n          value\n          jsonValue\n          definition {\n            key\n            name\n            required\n            type {\n              name\n              category\n            }\n          }\n        }\n      }\n    }\n  }\n",
                "variables": {"ids": ids}
            }),
        );
        let nodes = response.body["data"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for node in nodes {
            if node.get("__typename").and_then(Value::as_str) != Some("Metaobject") {
                continue;
            }
            let mut record = node;
            if let Some(record_object) = record.as_object_mut() {
                record_object.remove("__typename");
            }
            let Some(id) = record.get("id").and_then(Value::as_str) else {
                continue;
            };
            self.store.staged.metaobjects.insert(id.to_string(), record);
        }
    }

    fn hydrate_metaobject_by_handle(
        &mut self,
        request: &Request,
        meta_type: &str,
        meta_handle: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || meta_type.is_empty()
            || meta_handle.is_empty()
        {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query MetaobjectHydrateByHandle($type: String!, $handle: String!) { metaobjectByHandle(handle: { type: $type, handle: $handle }) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }",
                "variables": {"type": meta_type, "handle": meta_handle}
            }),
        );
        let mut record = response.body["data"]["metaobjectByHandle"].clone();
        if !record.is_object() {
            return None;
        }
        if let Some(definition) = record
            .get("definition")
            .filter(|definition| definition.is_object())
        {
            if let Some(definition_id) = definition.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .metaobject_definitions
                    .tombstones
                    .remove(definition_id);
                self.store
                    .staged
                    .metaobject_definitions
                    .insert(definition_id.to_string(), definition.clone());
            }
        }
        if let Some(record_object) = record.as_object_mut() {
            record_object.remove("definition");
        }
        let id = record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return Some(record);
        }
        self.store.staged.metaobjects.insert(id, record.clone());
        Some(record)
    }

    fn hydrate_metaobjects_by_type(&mut self, request: &Request, meta_type: &str) -> Vec<Value> {
        if self.config.read_mode == ReadMode::Snapshot || meta_type.is_empty() {
            return Vec::new();
        }
        let query = "#graphql
  query MetaobjectBulkDeleteHydrateByType($type: String!) {
    catalog: metaobjects(type: $type, first: 250) {
      nodes {
        id
        handle
        type
        displayName
        createdAt
        updatedAt
        capabilities {
          publishable {
            status
          }
          onlineStore {
            templateSuffix
          }
        }
        fields {
          key
          type
          value
          jsonValue
          definition {
            key
            name
            required
            type {
              name
              category
            }
          }
        }
      }
    }
    definition: metaobjectDefinitionByType(type: $type) {
      id
      type
      name
      description
      displayNameKey
      access {
        admin
        storefront
      }
      capabilities {
        publishable {
          enabled
        }
        translatable {
          enabled
        }
        renderable {
          enabled
        }
        onlineStore {
          enabled
        }
      }
      fieldDefinitions {
        key
        name
        description
        required
        type {
          name
          category
        }
        validations {
          name
          value
        }
      }
      hasThumbnailField
      metaobjectsCount
      standardTemplate {
        type
        name
      }
      createdAt
      updatedAt
    }
  }
";
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "variables": {"type": meta_type}
            }),
        );
        if let Some(definition) = response.body["data"]
            .get("definition")
            .or_else(|| response.body["data"].get("metaobjectDefinitionByType"))
            .filter(|value| value.is_object())
            .cloned()
        {
            if let Some(id) = definition.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .metaobject_definitions
                    .tombstones
                    .remove(id);
                self.store
                    .staged
                    .metaobject_definitions
                    .insert(id.to_string(), definition);
            }
        }
        let nodes = response.body["data"]
            .get("catalog")
            .or_else(|| response.body["data"].get("metaobjects"))
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for record in &nodes {
            if let Some(id) = record.get("id").and_then(Value::as_str) {
                self.store
                    .staged
                    .metaobjects
                    .insert(id.to_string(), record.clone());
            }
        }
        nodes
    }

    fn metaobject_by_handle_arg(&self, field: &MetaobjectRootInput) -> Option<Value> {
        let handle = resolved_object_field(&field.arguments, "handle")?;
        let meta_type = resolved_string_field(&handle, "type").unwrap_or_default();
        let meta_handle = resolved_string_field(&handle, "handle").unwrap_or_default();
        self.metaobject_by_type_and_handle(&meta_type, &meta_handle)
            .map(|record| self.metaobject_canonical_value(&record))
    }

    pub(in crate::proxy) fn metaobject_by_type_and_handle(
        &self,
        meta_type: &str,
        meta_handle: &str,
    ) -> Option<Value> {
        self.store
            .staged
            .metaobjects
            .values()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && record.get("handle").and_then(Value::as_str) == Some(meta_handle)
                    && !self
                        .store
                        .staged
                        .metaobjects
                        .is_tombstoned(record.get("id").and_then(Value::as_str).unwrap_or_default())
            })
            .cloned()
    }

    fn metaobject_connection(&self, field: &MetaobjectRootInput) -> Value {
        let meta_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
        let records: Vec<Value> =
            self.store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .map(|record| self.project_metaobject_against_definition(record))
                // A row whose required display field has no value is not yet surfaced
                // by the Admin search index that backs `metaobjects(type:)`.
                .filter(|record| self.metaobject_visible_in_catalog(record))
                .collect();
        staged_connection_value_with_args(
            records,
            &field.arguments,
            metaobject_search_decision,
            metaobject_staged_sort_key,
            |record| self.metaobject_canonical_value(record),
            metaobject_cursor,
        )
    }

    fn metaobject_create(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(input) = resolved_object_field(&field.arguments, "metaobject") else {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": []
            }));
        };
        self.stage_metaobject_create_from_input(&input, request, staged_ids, false)
    }

    fn stage_metaobject_create_from_input(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        staged_ids: &mut Vec<String>,
        upsert_required_errors: bool,
    ) -> Value {
        let meta_type = resolved_string_field(input, "type").unwrap_or_default();
        let definition = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type));
        let Some(definition) = definition else {
            let user_errors = metaobject_create_duplicate_field_errors(input);
            if !user_errors.is_empty() {
                return self.metaobject_payload_canonical_value(json!({
                    "metaobject": null,
                    "userErrors": user_errors
                }));
            }
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": [metaobject_no_definition_error(
                    "metaobject",
                    &meta_type,
                    "UNDEFINED_OBJECT_TYPE",
                )]
            }));
        };
        if definition["access"]["admin"].as_str() == Some("MERCHANT_READ") {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": [metaobject_user_error(
                    vec!["metaobject", "type"],
                    "Not authorized to create metaobjects for this type.",
                    "NOT_AUTHORIZED",
                    Value::Null,
                    Value::Null
                )]
            }));
        }
        let input_values = metaobject_create_input_values(input);
        let validation_context = MetaobjectFieldValueValidationContext {
            proxy: self,
            existing_id: None,
            validate_existing_values: false,
        };
        let mut validation_errors = metaobject_create_validation_errors(
            Some(&validation_context),
            input,
            &definition,
            &input_values,
            false,
        );
        if upsert_required_errors {
            validation_errors =
                metaobject_required_field_errors_for_upsert(validation_errors, &definition);
        }
        if let Some(handle) = resolved_string_field(input, "handle") {
            if !handle.is_empty() {
                validation_errors.extend(metaobject_handle_validation_errors(
                    &handle,
                    vec!["metaobject", "handle"],
                ));
            }
        }
        if !validation_errors.is_empty() {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": validation_errors
            }));
        }

        let id = self.next_proxy_synthetic_gid("Metaobject");
        let handle_choice = if let Some(requested_handle) = resolved_string_field(input, "handle") {
            if requested_handle.trim().is_empty() {
                self.available_blank_metaobject_handle(&definition, &input_values, &meta_type, &id)
            } else {
                self.available_metaobject_handle(&meta_type, &requested_handle)
            }
        } else {
            self.available_generated_metaobject_handle(&meta_type, &id)
        };
        let display_name =
            metaobject_display_name(&definition, &input_values, &handle_choice.display_source);
        let publishable_status = metaobject_publishable_status(input, &definition);
        let timestamp = self.next_mutation_timestamp();
        let record = metaobject_record_from_definition_with_options(
            &id,
            &handle_choice.handle,
            &definition,
            &input_values,
            MetaobjectRecordOptions {
                created_at: Some(&timestamp),
                display_name: &display_name,
                publishable_status: &publishable_status,
                online_store_template_suffix: metaobject_online_store_template_suffix_input(input)
                    .unwrap_or(Value::Null),
                updated_at: &timestamp,
            },
        );
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        self.increment_metaobject_definition_count(&meta_type, 1);
        staged_ids.push(id);
        self.metaobject_payload_canonical_value(json!({
            "metaobject": record,
            "userErrors": []
        }))
    }

    fn metaobject_display_name_conflict_errors(
        &self,
        existing_id: &str,
        definition: &Value,
        input: &BTreeMap<String, ResolvedValue>,
        input_values: &BTreeMap<String, String>,
        handle_display_source: &str,
    ) -> Vec<Value> {
        let display_name_key = definition
            .get("displayNameKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if display_name_key.is_empty() {
            return Vec::new();
        }
        let Some((field_index, _)) = metaobject_input_field_by_key(input, display_name_key) else {
            return Vec::new();
        };
        let display_name = metaobject_display_name(definition, input_values, handle_display_source);
        let meta_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let conflicts_linked_option_value = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| {
                record.get("id").and_then(Value::as_str) != Some(existing_id)
                    && record.get("type").and_then(Value::as_str) == Some(meta_type)
                    && record.get("displayName").and_then(Value::as_str)
                        == Some(display_name.as_str())
            })
            .filter_map(|record| record.get("id").and_then(Value::as_str))
            .any(|other_id| {
                self.store
                    .staged
                    .linked_product_option_metaobject_sets
                    .iter()
                    .any(|ids| ids.contains(existing_id) && ids.contains(other_id))
            });
        if !conflicts_linked_option_value {
            return Vec::new();
        }
        let index = field_index.to_string();
        vec![metaobject_user_error(
            vec!["metaobject", "fields", &index],
            "The display name you have chosen is already in use as an option value. Choose a different name to avoid conflicts.",
            "DISPLAY_NAME_CONFLICT",
            Value::Null,
            Value::Null,
        )]
    }

    fn metaobject_update(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self
            .metaobject_by_id(&id)
            .or_else(|| self.hydrate_metaobject_by_id(request, &id))
        else {
            return self.metaobject_payload_canonical_value(json!({
                    "metaobject": null,
                    "userErrors": [metaobject_field_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND")]
                }));
        };
        let Some(input) = resolved_object_field(&field.arguments, "metaobject") else {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": existing,
                "userErrors": []
            }));
        };
        let meta_type = existing
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.apply_metaobject_update_to_existing(
            request,
            staged_ids,
            MetaobjectUpdateApplyContext {
                existing,
                meta_type: &meta_type,
                input,
                options: MetaobjectUpdateApplyOptions {
                    definition_error_path_root: "metaobject",
                    handle_error_path: vec!["metaobject", "handle"],
                    default_handle_display_source: None,
                    rewrite_required_field_errors: false,
                    hydrate_requested_handle: false,
                    stage_redirect_new_handle: true,
                },
            },
        )
    }

    fn metaobject_upsert(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(handle) = resolved_object_field(&field.arguments, "handle") else {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": []
            }));
        };
        let meta_type = resolved_string_field(&handle, "type").unwrap_or_default();
        let meta_handle_input = resolved_string_field(&handle, "handle").unwrap_or_default();
        let meta_handle = slugify_handle(&meta_handle_input);
        let Some(input) = resolved_object_field(&field.arguments, "metaobject") else {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": []
            }));
        };
        if !meta_handle_input.is_empty() {
            let locator_errors =
                metaobject_handle_validation_errors(&meta_handle_input, vec!["handle", "handle"]);
            if !locator_errors.is_empty() {
                return self.metaobject_payload_canonical_value(json!({
                    "metaobject": null,
                    "userErrors": locator_errors
                }));
            }
        }
        if let Some(existing) = self
            .metaobject_by_type_and_handle(&meta_type, &meta_handle)
            .or_else(|| self.hydrate_metaobject_by_handle(request, &meta_type, &meta_handle))
        {
            let mut update_input = input.clone();
            if let Some(handle) = resolved_string_field(&input, "handle") {
                update_input.insert("handle".to_string(), ResolvedValue::String(handle));
            }
            return self.apply_metaobject_update_to_existing(
                request,
                staged_ids,
                MetaobjectUpdateApplyContext {
                    existing,
                    meta_type: &meta_type,
                    input: update_input,
                    options: MetaobjectUpdateApplyOptions {
                        definition_error_path_root: "handle",
                        handle_error_path: vec!["handle", "handle"],
                        default_handle_display_source: Some(meta_handle_input.clone()),
                        rewrite_required_field_errors: true,
                        hydrate_requested_handle: true,
                        stage_redirect_new_handle: false,
                    },
                },
            );
        }

        let Some(_) = self
            .metaobject_definition_by_type(&meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, &meta_type))
        else {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": [metaobject_no_definition_error(
                    "handle",
                    &meta_type,
                    "UNDEFINED_OBJECT_TYPE",
                )]
            }));
        };
        let mut create_input = input.clone();
        create_input.insert("type".to_string(), ResolvedValue::String(meta_type));
        create_input.insert(
            "handle".to_string(),
            ResolvedValue::String(meta_handle_input),
        );
        self.stage_metaobject_create_from_input(&create_input, request, staged_ids, true)
    }

    fn apply_metaobject_update_to_existing(
        &mut self,
        request: &Request,
        staged_ids: &mut Vec<String>,
        context: MetaobjectUpdateApplyContext<'_>,
    ) -> Value {
        let MetaobjectUpdateApplyContext {
            existing,
            meta_type,
            input,
            options,
        } = context;
        let Some(definition) = self
            .metaobject_definition_by_type(meta_type)
            .or_else(|| self.hydrate_metaobject_definition_by_type(request, meta_type))
            .or_else(|| metaobject_definition_from_record(&existing))
        else {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": [metaobject_no_definition_error(
                    options.definition_error_path_root,
                    meta_type,
                    "UNDEFINED_OBJECT_TYPE",
                )]
            }));
        };

        let id = existing
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let existing_handle = existing
            .get("handle")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let input_values = metaobject_merged_input_values(&existing, &input);
        let validation_context = MetaobjectFieldValueValidationContext {
            proxy: self,
            existing_id: Some(&id),
            validate_existing_values: true,
        };
        let validation_errors = metaobject_create_validation_errors(
            Some(&validation_context),
            &input,
            &definition,
            &input_values,
            true,
        );
        let mut validation_errors = if options.rewrite_required_field_errors {
            metaobject_required_field_errors_for_upsert(validation_errors, &definition)
        } else {
            validation_errors
        };
        let (next_handle, handle_display_source) = if let Some(handle) =
            resolved_string_field(&input, "handle")
        {
            validation_errors.extend(metaobject_handle_validation_errors(
                &handle,
                options.handle_error_path.clone(),
            ));
            let normalized = slugify_handle(&handle);
            if options.hydrate_requested_handle
                && !self.metaobject_handle_exists_case_insensitive(meta_type, &normalized)
            {
                self.hydrate_metaobject_by_handle(request, meta_type, &normalized);
            }
            if self.metaobject_handle_belongs_to_other_case_insensitive(meta_type, &normalized, &id)
            {
                validation_errors.push(metaobject_user_error(
                    options.handle_error_path.clone(),
                    "Handle has already been taken",
                    "TAKEN",
                    Value::Null,
                    Value::Null,
                ));
            }
            (normalized, handle)
        } else {
            (
                existing_handle.clone(),
                options
                    .default_handle_display_source
                    .unwrap_or_else(|| existing_handle.clone()),
            )
        };
        validation_errors.extend(self.metaobject_display_name_conflict_errors(
            &id,
            &definition,
            &input,
            &input_values,
            &handle_display_source,
        ));
        if !validation_errors.is_empty() {
            return self.metaobject_payload_canonical_value(json!({
                "metaobject": null,
                "userErrors": validation_errors
            }));
        }

        let display_name =
            metaobject_display_name(&definition, &input_values, &handle_display_source);
        let publishable_status =
            metaobject_updated_publishable_status(&input, &definition, &existing);
        // `_with_options` nulls the publishable capability when the definition has it
        // disabled (e.g. after a schema change turned it off), matching how Shopify
        // reads back entries whose definition no longer exposes the capability.
        let created_at = existing.get("createdAt").and_then(Value::as_str);
        let updated_at = self.next_mutation_timestamp();
        let record = metaobject_record_from_definition_with_options(
            &id,
            &next_handle,
            &definition,
            &input_values,
            MetaobjectRecordOptions {
                created_at,
                display_name: &display_name,
                publishable_status: &publishable_status,
                online_store_template_suffix: metaobject_online_store_template_suffix_input(&input)
                    .or_else(|| metaobject_existing_online_store_template_suffix(&existing))
                    .unwrap_or(Value::Null),
                updated_at: &updated_at,
            },
        );
        self.store
            .staged
            .metaobjects
            .insert(id.clone(), record.clone());
        if options.stage_redirect_new_handle
            && resolved_bool_field(&input, "redirectNewHandle").unwrap_or(false)
            && existing_handle != next_handle
            && metaobject_definition_has_renderable_online_store(&definition)
            && metaobject_record_has_active_online_store(&record)
        {
            if let (Some(path), Some(target)) = (
                metaobject_page_path(&definition, &existing_handle),
                metaobject_page_path(&definition, &next_handle),
            ) {
                self.stage_url_redirect(path, target);
            }
        }
        staged_ids.push(id);
        self.metaobject_payload_canonical_value(json!({
            "metaobject": record,
            "userErrors": []
        }))
    }

    fn metaobject_delete(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let submitted_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if self.metaobject_by_id(&submitted_id).is_none()
            && self
                .hydrate_metaobject_by_id(request, &submitted_id)
                .is_none()
        {
            return json!({
                "deletedId": null,
                "userErrors": [metaobject_indexed_user_error(
                    ["id"],
                    "Record not found",
                    Some("RECORD_NOT_FOUND"),
                    Value::Null,
                    Value::Null
                )]
            });
        }
        let storage_id = self
            .metaobject_staged_key_by_id(&submitted_id)
            .unwrap_or_else(|| submitted_id.clone());
        let record = self
            .store
            .staged
            .metaobjects
            .get(&storage_id)
            .cloned()
            .unwrap_or(Value::Null);
        self.store.staged.metaobjects.remove(&storage_id);
        self.store.staged.metaobjects.tombstone(storage_id.clone());
        if let Some(meta_type) = record.get("type").and_then(Value::as_str) {
            self.increment_metaobject_definition_count(meta_type, -1);
        }
        staged_ids.push(storage_id);
        json!({"deletedId": submitted_id, "userErrors": []})
    }

    fn metaobject_bulk_delete(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
        log_successful_noop: &mut bool,
    ) -> Value {
        let where_input = resolved_object_field(&field.arguments, "where");
        let ids = where_input
            .as_ref()
            .and_then(|input| input.get("ids"))
            .or_else(|| field.arguments.get("ids"))
            .and_then(|value| {
                resolved_value_list(value).map(|values| {
                    values
                        .iter()
                        .filter_map(resolved_value_string)
                        .take(250)
                        .collect::<Vec<_>>()
                })
            });
        let meta_type = where_input
            .as_ref()
            .and_then(|input| resolved_string_field(input, "type"))
            .filter(|value| !value.is_empty());
        if ids.is_some() == meta_type.is_some() {
            return Value::Null;
        }

        let user_errors: Vec<Value> = Vec::new();
        let mut touched_ids = Vec::new();
        if let Some(ids) = ids {
            let mut ids_to_hydrate = Vec::new();
            for id in &ids {
                if !id.is_empty()
                    && self.metaobject_staged_key_by_id(id).is_none()
                    && !ids_to_hydrate
                        .iter()
                        .any(|existing_id: &String| existing_id == id)
                {
                    ids_to_hydrate.push(id.clone());
                }
            }
            self.hydrate_metaobjects_by_ids(request, &ids_to_hydrate);
            for id in ids {
                let Some(storage_id) = self.metaobject_staged_key_by_id(&id) else {
                    continue;
                };
                let record = self.store.staged.metaobjects.get(&storage_id).cloned();
                if let Some(record) = record {
                    self.store.staged.metaobjects.remove(&storage_id);
                    self.store.staged.metaobjects.tombstone(storage_id.clone());
                    if let Some(meta_type) = record.get("type").and_then(Value::as_str) {
                        self.increment_metaobject_definition_count(meta_type, -1);
                    }
                    touched_ids.push(storage_id);
                }
            }
        } else if let Some(meta_type) = meta_type {
            let has_local_rows_for_type = self.store.staged.metaobjects.values().any(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
            });
            if !has_local_rows_for_type && self.metaobject_definition_by_type(&meta_type).is_none()
            {
                self.hydrate_metaobjects_by_type(request, &meta_type);
            }
            let has_rows_for_type = self.store.staged.metaobjects.values().any(|record| {
                record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
            });
            if self.metaobject_definition_by_type(&meta_type).is_none() && !has_rows_for_type {
                return json!({
                    "job": null,
                    "userErrors": [metaobject_no_definition_error(
                        "where",
                        &meta_type,
                        "RECORD_NOT_FOUND",
                    )]
                });
            }
            let ids_to_delete = self
                .store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
                .collect::<Vec<_>>();
            for id in ids_to_delete {
                self.store.staged.metaobjects.remove(&id);
                self.store.staged.metaobjects.tombstone(id.clone());
                touched_ids.push(id);
            }
            self.increment_metaobject_definition_count(&meta_type, -(touched_ids.len() as i64));
        }

        if touched_ids.is_empty() && user_errors.is_empty() {
            *log_successful_noop = true;
        }
        staged_ids.extend(touched_ids);
        json!({
            "job": {"id": self.next_proxy_synthetic_gid("Job"), "done": false},
            "userErrors": user_errors
        })
    }

    fn metaobject_payload_canonical_value(&self, mut payload: Value) -> Value {
        if let Some(metaobject) = payload
            .get("metaobject")
            .filter(|metaobject| metaobject.is_object())
            .cloned()
        {
            payload["metaobject"] = self.stored_metaobject_canonical_value(&metaobject);
        }
        payload
    }

    fn metaobject_definition_payload_canonical_value(&self, mut payload: Value) -> Value {
        if let Some(definition) = payload
            .get("metaobjectDefinition")
            .filter(|definition| definition.is_object())
            .cloned()
        {
            payload["metaobjectDefinition"] =
                self.metaobject_definition_canonical_value(&definition);
        }
        payload
    }

    /// Re-projects a stored metaobject entry against the current local definition for
    /// its type, so reads reflect schema changes applied after the entry was created:
    /// field definitions (key/name/required/type), field order, newly-added fields
    /// (read as `null`), dropped fields, the recomputed `displayName`, and the
    /// `publishable` capability all follow the live definition. Stored field VALUES
    /// are preserved verbatim. When no local definition is staged for the type (e.g.
    /// an upstream-hydrated entry), the record is returned unchanged.
    pub(in crate::proxy) fn project_metaobject_against_definition(&self, record: &Value) -> Value {
        let Some(meta_type) = record.get("type").and_then(Value::as_str) else {
            return record.clone();
        };
        let Some(definition) = self.metaobject_definition_by_type(meta_type) else {
            return record.clone();
        };
        let mut stored: BTreeMap<String, (Value, Value)> = BTreeMap::new();
        if let Some(fields) = record["fields"].as_array() {
            for entry in fields {
                if let Some(key) = entry.get("key").and_then(Value::as_str) {
                    stored.insert(
                        key.to_string(),
                        (
                            entry.get("value").cloned().unwrap_or(Value::Null),
                            entry.get("jsonValue").cloned().unwrap_or(Value::Null),
                        ),
                    );
                }
            }
        }
        let fields = definition["fieldDefinitions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|field_definition| {
                let key = field_definition
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let field_type = field_definition["type"]["name"]
                    .as_str()
                    .unwrap_or("single_line_text_field");
                let (value, json_value) = stored
                    .get(key)
                    .cloned()
                    .unwrap_or((Value::Null, Value::Null));
                json!({
                    "key": key,
                    "type": field_type,
                    "value": value,
                    "jsonValue": json_value,
                    "definition": field_definition
                })
            })
            .collect::<Vec<_>>();

        let display_field_present = definition
            .get("displayNameKey")
            .and_then(Value::as_str)
            .and_then(|key| stored.get(key))
            .and_then(|(value, _)| value.as_str())
            .is_some_and(|value| !value.trim().is_empty());
        let display_name = if display_field_present {
            // Keep the displayName the write path already computed for this field.
            // Shopify renders displayName from the raw input, which can differ from
            // the normalized stored field value (e.g. a measurement `60` vs `60.0`,
            // `kilometers_per_hour` vs `KILOMETERS_PER_HOUR`), so re-deriving it from
            // the stored field value here would corrupt it.
            record.get("displayName").cloned().unwrap_or(Value::Null)
        } else {
            // A blank display field (e.g. a schema change moved displayNameKey onto a
            // field this row never set) falls back to the entry's handle, title-cased
            // ("codex-har-245-pre-..." -> "Codex Har 245 Pre ...").
            json!(metaobject_field_name(
                record
                    .get("handle")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ))
        };

        let publishable_enabled = definition["capabilities"]["publishable"]["enabled"]
            .as_bool()
            .unwrap_or(false);
        let publishable = if publishable_enabled {
            record["capabilities"]["publishable"].clone()
        } else {
            Value::Null
        };

        let mut projected = record.clone();
        projected["fields"] = json!(fields);
        projected["displayName"] = display_name;
        if let Some(capabilities) = projected
            .get_mut("capabilities")
            .and_then(Value::as_object_mut)
        {
            capabilities.insert("publishable".to_string(), publishable);
        }
        projected
    }

    /// Whether a (already definition-projected) entry is visible in an immediate
    /// `metaobjects(type:)` catalog read. Rows missing a value for a required display
    /// field are omitted, matching Shopify's behaviour where such rows are not yet
    /// surfaced by the Admin search index.
    pub(in crate::proxy) fn metaobject_visible_in_catalog(&self, projected: &Value) -> bool {
        let Some(meta_type) = projected.get("type").and_then(Value::as_str) else {
            return true;
        };
        let Some(definition) = self.metaobject_definition_by_type(meta_type) else {
            return true;
        };
        let Some(display_key) = definition.get("displayNameKey").and_then(Value::as_str) else {
            return true;
        };
        let display_required = definition["fieldDefinitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|field| field.get("key").and_then(Value::as_str) == Some(display_key))
            .and_then(|field| field.get("required").and_then(Value::as_bool))
            .unwrap_or(false);
        if !display_required {
            return true;
        }
        projected["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|field| field.get("key").and_then(Value::as_str) == Some(display_key))
            .and_then(|field| field.get("value").and_then(Value::as_str))
            .is_some_and(|value| !value.trim().is_empty())
    }

    fn metaobject_definition_create(
        &mut self,
        field: &MetaobjectRootInput,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(definition_input) = resolved_object_field(&field.arguments, "definition") else {
            return json!({"metaobjectDefinition": null, "userErrors": []});
        };
        let meta_type = metaobject_definition_type_from_input(&definition_input, request);
        let existing_definitions = self
            .store
            .staged
            .metaobject_definitions
            .iter()
            .filter(|(id, _)| !self.store.staged.metaobject_definitions.is_tombstoned(id))
            .count();
        let validation_errors = if let Some(error) =
            metaobject_definition_type_identity_error(&definition_input, request)
        {
            vec![error]
        } else {
            metaobject_definition_create_validation_errors(
                &definition_input,
                &meta_type,
                existing_definitions,
            )
        };
        if !validation_errors.is_empty() {
            return json!({
                "metaobjectDefinition": null,
                "userErrors": validation_errors
            });
        }
        if self.metaobject_definition_by_type(&meta_type).is_some() {
            return json!({
                "metaobjectDefinition": null,
                "userErrors": [metaobject_field_error(vec!["definition", "type"], "Type has already been taken", "TAKEN")]
            });
        }
        let id = self.next_proxy_synthetic_gid("MetaobjectDefinition");
        let timestamp = self.next_mutation_timestamp();
        let definition =
            metaobject_definition_record(&id, &definition_input, &meta_type, &timestamp);
        self.store
            .staged
            .metaobject_definitions
            .insert(id.clone(), definition.clone());
        self.store
            .staged
            .metaobject_definitions
            .tombstones
            .remove(&id);
        staged_ids.push(id);
        self.metaobject_definition_payload_canonical_value(json!({
            "metaobjectDefinition": definition,
            "userErrors": []
        }))
    }

    fn metaobject_definition_update(
        &mut self,
        field: &MetaobjectRootInput,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let submitted_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(storage_id) = self.metaobject_definition_staged_key_by_id(&submitted_id) else {
            return json!({
                "metaobjectDefinition": null,
                "userErrors": [metaobject_field_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND")]
            });
        };
        let Some(definition) = self
            .store
            .staged
            .metaobject_definitions
            .get(&storage_id)
            .cloned()
        else {
            return json!({
                "metaobjectDefinition": null,
                "userErrors": [metaobject_field_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND")]
            });
        };
        let Some(definition_input) = resolved_object_field(&field.arguments, "definition") else {
            return self.metaobject_definition_payload_canonical_value(json!({
                "metaobjectDefinition": definition,
                "userErrors": []
            }));
        };
        let meta_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // A metaobject definition that backs a product option's linked metafield has
        // an immutable display name field: Shopify forbids re-pointing displayNameKey
        // while the definition is linked, because the option values are surfaced by
        // their resolved display name. The link set is populated by
        // `record_product_option_linked_metaobject_definitions` during
        // productOptionsCreate.
        if self
            .store
            .staged
            .product_option_linked_metaobject_definition_ids
            .contains(&storage_id)
        {
            let current_display_name_key = definition.get("displayNameKey").and_then(Value::as_str);
            let changes_display_name_key =
                resolved_string_field(&definition_input, "displayNameKey")
                    .is_some_and(|next| Some(next.as_str()) != current_display_name_key);
            if changes_display_name_key {
                return json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_field_error(
                        vec!["definition", "displayNameKey"],
                        "Cannot change display name field when metaobject is used in product options",
                        "IMMUTABLE",
                    )]
                });
            }
        }
        let existing_field_definitions = definition["fieldDefinitions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let validation_errors = metaobject_definition_update_validation_errors(
            &definition_input,
            &meta_type,
            &existing_field_definitions,
        );
        if !validation_errors.is_empty() {
            return json!({
                "metaobjectDefinition": null,
                "userErrors": validation_errors
            });
        }
        let old_url_handle =
            metaobject_definition_online_store_url_handle(&definition).map(str::to_string);
        let updated_at = self.next_mutation_timestamp();
        let updated =
            update_metaobject_definition_record(definition, &definition_input, &updated_at);
        let new_url_handle =
            metaobject_definition_online_store_url_handle(&updated).map(str::to_string);
        let redirect_paths = if metaobject_definition_input_create_redirects(&definition_input)
            && metaobject_definition_online_store_can_create_redirects(&updated)
            && old_url_handle.is_some()
            && new_url_handle.is_some()
            && old_url_handle != new_url_handle
        {
            let old_url_handle = old_url_handle.unwrap_or_default();
            let new_url_handle = new_url_handle.unwrap_or_default();
            self.store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                        && metaobject_record_has_active_online_store(record)
                })
                .filter_map(|record| {
                    let handle = record.get("handle").and_then(Value::as_str)?;
                    Some((
                        format!("/pages/{old_url_handle}/{handle}"),
                        format!("/pages/{new_url_handle}/{handle}"),
                    ))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        self.store
            .staged
            .metaobject_definitions
            .insert(storage_id.clone(), updated.clone());
        for (path, target) in redirect_paths {
            self.stage_url_redirect(path, target);
        }
        staged_ids.push(storage_id);
        self.metaobject_definition_payload_canonical_value(json!({
            "metaobjectDefinition": updated,
            "userErrors": []
        }))
    }

    fn metaobject_definition_delete(
        &mut self,
        field: &MetaobjectRootInput,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let submitted_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(storage_id) = self.metaobject_definition_staged_key_by_id(&submitted_id) else {
            return json!({
                "deletedId": null,
                "userErrors": [metaobject_field_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND")]
            });
        };
        let Some(definition) = self
            .store
            .staged
            .metaobject_definitions
            .get(&storage_id)
            .cloned()
        else {
            return json!({
                "deletedId": null,
                "userErrors": [metaobject_field_error(vec!["id"], "Record not found", "RECORD_NOT_FOUND")]
            });
        };
        let meta_type = definition["type"].as_str().unwrap_or_default().to_string();
        let ids_to_delete = self
            .store
            .staged
            .metaobjects
            .values()
            .filter(|record| record.get("type").and_then(Value::as_str) == Some(meta_type.as_str()))
            .filter_map(|record| record.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        for metaobject_id in ids_to_delete {
            self.store.staged.metaobjects.remove(&metaobject_id);
            self.store.staged.metaobjects.tombstone(metaobject_id);
        }
        self.store.staged.metaobject_definitions.remove(&storage_id);
        self.store
            .staged
            .metaobject_definitions
            .tombstone(storage_id.clone());
        staged_ids.push(storage_id);
        json!({"deletedId": submitted_id, "userErrors": []})
    }

    fn standard_metaobject_definition_enable(
        &mut self,
        field: &MetaobjectRootInput,
        staged_ids: &mut Vec<String>,
        log_successful_noop: &mut bool,
    ) -> Value {
        let meta_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
        if let Some(definition) = self.metaobject_definition_by_type(&meta_type) {
            *log_successful_noop = true;
            return self.metaobject_definition_payload_canonical_value(json!({
                "metaobjectDefinition": definition,
                "userErrors": []
            }));
        }

        let Some(template) = standard_metaobject_definition_template(&meta_type) else {
            return self.metaobject_definition_payload_canonical_value(json!({
                    "metaobjectDefinition": null,
                    "userErrors": [metaobject_field_error(vec!["type"], "Record not found", "RECORD_NOT_FOUND")]
                }));
        };

        let id = self.next_proxy_synthetic_gid("MetaobjectDefinition");
        let timestamp = self.next_mutation_timestamp();
        let definition = standard_metaobject_definition_from_template(&id, template, &timestamp);
        self.store
            .staged
            .metaobject_definitions
            .insert(id.clone(), definition.clone());
        self.store
            .staged
            .metaobject_definitions
            .tombstones
            .remove(&id);
        staged_ids.push(id);
        self.metaobject_definition_payload_canonical_value(json!({
            "metaobjectDefinition": definition,
            "userErrors": []
        }))
    }

    fn metaobject_definition_staged_key_by_id(&self, id: &str) -> Option<String> {
        staged_record_key_for_shopify_gid(
            &self.store.staged.metaobject_definitions,
            id,
            "MetaobjectDefinition",
        )
    }

    pub(in crate::proxy) fn metaobject_definition_by_id(&self, id: &str) -> Option<Value> {
        let key = self.metaobject_definition_staged_key_by_id(id)?;
        self.store.staged.metaobject_definitions.get(&key).cloned()
    }

    pub(in crate::proxy) fn metaobject_definition_by_type(&self, meta_type: &str) -> Option<Value> {
        self.store
            .staged
            .metaobject_definitions
            .values()
            .find(|definition| definition.get("type").and_then(Value::as_str) == Some(meta_type))
            .cloned()
    }

    fn metaobject_definition_with_derived_fields(&self, definition: &Value) -> Value {
        let mut definition = definition.clone();
        definition["metaobjectsCount"] = json!(self
            .metaobject_definition_child_metaobjects(&definition)
            .len());
        definition
    }

    pub(in crate::proxy) fn metaobject_definition_canonical_value(
        &self,
        definition: &Value,
    ) -> Value {
        let mut definition = self.metaobject_definition_with_derived_fields(definition);
        definition["__typename"] = json!("MetaobjectDefinition");
        definition
    }

    fn metaobject_definition_child_metaobjects(&self, definition: &Value) -> Vec<Value> {
        let meta_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut records =
            self.store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type)
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .map(|record| self.project_metaobject_against_definition(record))
                .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            metaobject_staged_sort_key(left, None)
                .cmp(&metaobject_staged_sort_key(right, None))
                .then_with(|| metaobject_cursor(left).cmp(&metaobject_cursor(right)))
        });
        records
    }

    fn metaobject_definition_metaobjects_connection_value(
        &self,
        definition: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        connection_value_with_args(
            self.metaobject_definition_child_metaobjects(definition)
                .into_iter()
                .map(|record| self.metaobject_canonical_value(&record))
                .collect(),
            arguments,
            metaobject_cursor,
        )
    }

    fn hydrate_metaobject_definition_by_type(
        &mut self,
        request: &Request,
        meta_type: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot || meta_type.trim().is_empty() {
            return None;
        }
        let query = "query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }";
        let body = json!({
            "query": query,
            "variables": {"type": meta_type}
        });
        let response = self.upstream_post(request, body);
        if response.status < 200 || response.status >= 300 {
            return None;
        }
        let definition = response
            .body
            .get("data")
            .and_then(|data| data.get("metaobjectDefinitionByType"))
            .filter(|definition| definition.is_object())?
            .clone();
        let id = definition
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return Some(definition);
        }
        self.store
            .staged
            .metaobject_definitions
            .tombstones
            .remove(&id);
        self.store
            .staged
            .metaobject_definitions
            .insert(id, definition.clone());
        Some(definition)
    }

    fn metaobject_definition_connection(&self, field: &MetaobjectRootInput) -> Value {
        let mut records = self
            .store
            .staged
            .metaobject_definitions
            .values()
            .filter(|definition| {
                !self.store.staged.metaobject_definitions.is_tombstoned(
                    definition
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.get("type")
                .and_then(Value::as_str)
                .cmp(&right.get("type").and_then(Value::as_str))
        });
        connection_value_with_args(
            records
                .into_iter()
                .map(|definition| self.metaobject_definition_canonical_value(&definition))
                .collect(),
            &field.arguments,
            |definition| {
                format!(
                    "cursor:{}",
                    definition
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("metaobject-definition")
                )
            },
        )
    }

    fn increment_metaobject_definition_count(&mut self, meta_type: &str, delta: i64) {
        let Some((id, mut definition)) = self
            .store
            .staged
            .metaobject_definitions
            .iter()
            .find(|(_, definition)| {
                definition.get("type").and_then(Value::as_str) == Some(meta_type)
            })
            .map(|(id, definition)| (id.clone(), definition.clone()))
        else {
            return;
        };
        let current = definition
            .get("metaobjectsCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        definition["metaobjectsCount"] = json!((current + delta).max(0));
        self.store
            .staged
            .metaobject_definitions
            .insert(id, definition);
    }

    fn available_generated_metaobject_handle(
        &self,
        meta_type: &str,
        id: &str,
    ) -> MetaobjectHandleChoice {
        let base = slugify_handle(meta_type);
        let base = if base.is_empty() {
            "metaobject".to_string()
        } else {
            base
        };
        for attempt in 0.. {
            let suffix = metaobject_random_handle_suffix(meta_type, id, attempt);
            let candidate = format!("{base}-{suffix}");
            if !self.metaobject_handle_exists_case_insensitive(meta_type, &candidate) {
                return MetaobjectHandleChoice {
                    handle: candidate.clone(),
                    display_source: candidate,
                };
            }
        }
        unreachable!("infinite random handle search must return")
    }

    fn available_blank_metaobject_handle(
        &self,
        definition: &Value,
        input_values: &BTreeMap<String, String>,
        meta_type: &str,
        id: &str,
    ) -> MetaobjectHandleChoice {
        metaobject_keyed_display_name(definition, input_values)
            .map(|display_name| self.available_metaobject_handle(meta_type, &display_name))
            .unwrap_or_else(|| self.available_generated_metaobject_handle(meta_type, id))
    }

    fn available_metaobject_handle(
        &self,
        meta_type: &str,
        requested: &str,
    ) -> MetaobjectHandleChoice {
        let base = slugify_handle(requested);
        let base = if base.is_empty() {
            format!("{meta_type}-{}", self.next_synthetic_id)
        } else {
            base
        };
        let display_base = if requested.trim().is_empty() {
            base.clone()
        } else {
            requested.trim().to_string()
        };
        if !self.metaobject_handle_exists_case_insensitive(meta_type, &base) {
            return MetaobjectHandleChoice {
                handle: base,
                display_source: display_base,
            };
        }
        for suffix in 1.. {
            let candidate = format!("{base}-{suffix}");
            if !self.metaobject_handle_exists_case_insensitive(meta_type, &candidate) {
                return MetaobjectHandleChoice {
                    handle: candidate,
                    display_source: format!("{display_base}-{suffix}"),
                };
            }
        }
        unreachable!("infinite suffix search must return")
    }

    fn metaobject_handle_exists_case_insensitive(&self, meta_type: &str, handle: &str) -> bool {
        self.metaobject_handle_belongs_to_other_case_insensitive(meta_type, handle, "")
    }

    fn metaobject_handle_belongs_to_other_case_insensitive(
        &self,
        meta_type: &str,
        handle: &str,
        current_id: &str,
    ) -> bool {
        self.store.staged.metaobjects.values().any(|record| {
            record.get("type").and_then(Value::as_str) == Some(meta_type)
                && record
                    .get("handle")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(handle))
                && record.get("id").and_then(Value::as_str) != Some(current_id)
                && !self
                    .store
                    .staged
                    .metaobjects
                    .is_tombstoned(record.get("id").and_then(Value::as_str).unwrap_or_default())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_proxy() -> DraftProxy {
        DraftProxy::new(Config {
            read_mode: ReadMode::Snapshot,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_upstream_transport(|_| panic!("metaobject definition tests should stay local"))
    }

    fn graphql_request(query: &str, variables: Value) -> Request {
        Request {
            method: "POST".to_string(),
            path: "/admin/api/2026-04/graphql.json".to_string(),
            headers: BTreeMap::new(),
            body: json!({ "query": query, "variables": variables }).to_string(),
        }
    }

    fn create_metaobject_definition(proxy: &mut DraftProxy, meta_type: &str) -> Value {
        let response = proxy.process_request(graphql_request(
            r#"
            mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
              metaobjectDefinitionCreate(definition: $definition) {
                metaobjectDefinition {
                  id
                  type
                  metaobjectsCount
                  metaobjects(first: 1) { nodes { id } }
                }
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({"definition": {
                "type": meta_type,
                "name": "Definition Child Article",
                "displayNameKey": "title",
                "fieldDefinitions": [
                    {"key": "title", "name": "Title", "type": "single_line_text_field", "required": true},
                    {"key": "body", "name": "Body", "type": "multi_line_text_field", "required": false}
                ]
            }}),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
            json!([])
        );
        assert_eq!(
            response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]
                ["metaobjects"]["nodes"],
            json!([])
        );
        response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"].clone()
    }

    fn create_metaobject(
        proxy: &mut DraftProxy,
        meta_type: &str,
        handle: &str,
        title: &str,
    ) -> Value {
        let response = proxy.process_request(graphql_request(
            r#"
            mutation CreateEntry($metaobject: MetaobjectCreateInput!) {
              metaobjectCreate(metaobject: $metaobject) {
                metaobject { id handle type displayName }
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({"metaobject": {
                "type": meta_type,
                "handle": handle,
                "fields": [
                    {"key": "title", "value": title},
                    {"key": "body", "value": "Body"}
                ]
            }}),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["metaobjectCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["metaobjectCreate"]["metaobject"].clone()
    }

    fn canonical_gid(id: &Value) -> String {
        id.as_str().unwrap().split('?').next().unwrap().to_string()
    }

    #[test]
    fn live_hybrid_metaobject_preserves_repeated_upstream_field_aliases() {
        let calls = Arc::new(Mutex::new(0));
        let observed_calls = Arc::clone(&calls);
        let mut proxy = DraftProxy::new(Config {
            read_mode: ReadMode::LiveHybrid,
            unsupported_mutation_mode: None,
            bulk_operation_run_mutation_max_input_file_size_bytes: None,
            port: 0,
            shopify_admin_origin: "https://shopify.com".to_string(),
            snapshot_path: None,
        })
        .with_upstream_transport(move |_| {
            *observed_calls.lock().unwrap() += 1;
            Response {
                status: 200,
                headers: BTreeMap::new(),
                body: json!({
                    "data": {
                        "parent": {
                            "fields": [
                                {"key": "single_ref", "value": "single"},
                                {"key": "list_ref", "value": "list"}
                            ],
                            "singleRef": {"key": "single_ref", "value": "single"},
                            "listRef": {"key": "list_ref", "value": "list"}
                        }
                    }
                }),
            }
        });

        let response = proxy.process_request(graphql_request(
            r#"
            query MetaobjectAliases($id: ID!) {
              parent: metaobject(id: $id) {
                fields { key value }
                singleRef: field(key: "single_ref") { key value }
                listRef: field(key: "list_ref") { key value }
              }
            }
            "#,
            json!({"id": "gid://shopify/Metaobject/1"}),
        ));

        assert_eq!(response.status, 200);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(
            response.body["data"]["parent"]["singleRef"],
            json!({"key": "single_ref", "value": "single"})
        );
        assert_eq!(
            response.body["data"]["parent"]["listRef"],
            json!({"key": "list_ref", "value": "list"})
        );
    }

    #[test]
    fn canonical_metaobject_id_resolves_staged_synthetic_lifecycle() {
        let mut proxy = test_proxy();
        let meta_type = "canonical_gid_article";
        create_metaobject_definition(&mut proxy, meta_type);
        let entry = create_metaobject(&mut proxy, meta_type, "canonical-entry", "Canonical Entry");
        let synthetic_id = entry["id"].as_str().unwrap().to_string();
        let canonical_id = canonical_gid(&entry["id"]);
        assert_ne!(synthetic_id, canonical_id);

        let read_query = r#"
            query ReadCanonicalMetaobject($id: ID!) {
              direct: metaobject(id: $id) { id handle type displayName }
              relay: node(id: $id) {
                __typename
                ... on Metaobject { id handle type displayName }
              }
            }
            "#;
        let canonical_read =
            proxy.process_request(graphql_request(read_query, json!({"id": canonical_id})));
        assert_eq!(canonical_read.status, 200);
        assert_eq!(
            canonical_read.body["data"]["direct"]["id"],
            json!(synthetic_id)
        );
        assert_eq!(
            canonical_read.body["data"]["relay"]["__typename"],
            json!("Metaobject")
        );

        let synthetic_read =
            proxy.process_request(graphql_request(read_query, json!({"id": synthetic_id})));
        assert_eq!(synthetic_read.status, 200);
        assert_eq!(
            synthetic_read.body["data"]["direct"]["handle"],
            json!("canonical-entry")
        );

        let metafields_set = proxy.process_request(graphql_request(
            r#"
            mutation SetCanonicalMetaobjectReference($metafields: [MetafieldsSetInput!]!) {
              metafieldsSet(metafields: $metafields) {
                metafields { namespace key type value }
                userErrors { field message code elementIndex }
              }
            }
            "#,
            json!({"metafields": [{
                "ownerId": "gid://shopify/Product/10173064872245",
                "namespace": "canonical_reference",
                "key": "linked",
                "type": "metaobject_reference",
                "value": canonical_id
            }]}),
        ));
        assert_eq!(
            metafields_set.body["data"]["metafieldsSet"]["userErrors"],
            json!([])
        );
        assert_eq!(
            metafields_set.body["data"]["metafieldsSet"]["metafields"][0]["value"],
            json!(canonical_id)
        );

        let update = proxy.process_request(graphql_request(
            r#"
            mutation UpdateCanonicalMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
              metaobjectUpdate(id: $id, metaobject: $metaobject) {
                metaobject { id handle displayName fields { key value } }
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({
                "id": canonical_id,
                "metaobject": {
                    "handle": "canonical-entry-updated",
                    "fields": [{"key": "title", "value": "Canonical Entry Updated"}]
                }
            }),
        ));
        assert_eq!(
            update.body["data"]["metaobjectUpdate"]["userErrors"],
            json!([])
        );
        assert_eq!(
            update.body["data"]["metaobjectUpdate"]["metaobject"]["id"],
            json!(synthetic_id)
        );
        assert_eq!(proxy.store.staged.metaobjects.len(), 1);
        assert!(proxy
            .store
            .staged
            .metaobjects
            .contains_staged(&synthetic_id));
        assert!(!proxy
            .store
            .staged
            .metaobjects
            .contains_staged(&canonical_id));

        let bulk_entry =
            create_metaobject(&mut proxy, meta_type, "canonical-bulk-entry", "Bulk Entry");
        let bulk_synthetic_id = bulk_entry["id"].as_str().unwrap().to_string();
        let bulk_canonical_id = canonical_gid(&bulk_entry["id"]);
        let bulk_delete = proxy.process_request(graphql_request(
            r#"
            mutation BulkDeleteCanonicalMetaobject($ids: [ID!]!) {
              metaobjectBulkDelete(where: {ids: $ids}) {
                job { id done }
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({"ids": [bulk_canonical_id]}),
        ));
        assert_eq!(
            bulk_delete.body["data"]["metaobjectBulkDelete"]["userErrors"],
            json!([])
        );
        assert!(bulk_delete.body["data"]["metaobjectBulkDelete"]["job"].is_object());
        assert_eq!(proxy.store.staged.metaobjects.len(), 1);
        assert!(proxy
            .store
            .staged
            .metaobjects
            .is_tombstoned(&bulk_synthetic_id));

        let delete = proxy.process_request(graphql_request(
            r#"
            mutation DeleteCanonicalMetaobject($id: ID!) {
              metaobjectDelete(id: $id) {
                deletedId
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({"id": canonical_id}),
        ));
        assert_eq!(
            delete.body["data"]["metaobjectDelete"]["userErrors"],
            json!([])
        );
        assert_eq!(
            delete.body["data"]["metaobjectDelete"]["deletedId"],
            json!(canonical_id)
        );
        assert!(proxy.store.staged.metaobjects.is_tombstoned(&synthetic_id));

        let post_delete = proxy.process_request(graphql_request(
            r#"
            query ReadDeletedCanonicalMetaobject($canonicalId: ID!, $syntheticId: ID!) {
              canonical: metaobject(id: $canonicalId) { id }
              synthetic: metaobject(id: $syntheticId) { id }
              canonicalNode: node(id: $canonicalId) { __typename id }
              syntheticNode: node(id: $syntheticId) { __typename id }
            }
            "#,
            json!({"canonicalId": canonical_id, "syntheticId": synthetic_id}),
        ));
        assert_eq!(post_delete.status, 200);
        assert_eq!(post_delete.body["data"]["canonical"], Value::Null);
        assert_eq!(post_delete.body["data"]["synthetic"], Value::Null);
        assert_eq!(post_delete.body["data"]["canonicalNode"], Value::Null);
        assert_eq!(post_delete.body["data"]["syntheticNode"], Value::Null);
    }

    #[test]
    fn canonical_metaobject_definition_id_resolves_staged_synthetic_lifecycle() {
        let mut proxy = test_proxy();
        let meta_type = "canonical_definition_article";
        let definition = create_metaobject_definition(&mut proxy, meta_type);
        let synthetic_id = definition["id"].as_str().unwrap().to_string();
        let canonical_id = canonical_gid(&definition["id"]);
        assert_ne!(synthetic_id, canonical_id);

        let read_query = r#"
            query ReadCanonicalDefinition($id: ID!) {
              direct: metaobjectDefinition(id: $id) { id type name }
              relay: node(id: $id) {
                __typename
                ... on MetaobjectDefinition { id type name }
              }
            }
            "#;
        let canonical_read =
            proxy.process_request(graphql_request(read_query, json!({"id": canonical_id})));
        assert_eq!(canonical_read.status, 200);
        assert_eq!(
            canonical_read.body["data"]["direct"]["id"],
            json!(synthetic_id)
        );
        assert_eq!(
            canonical_read.body["data"]["relay"]["__typename"],
            json!("MetaobjectDefinition")
        );

        let update = proxy.process_request(graphql_request(
            r#"
            mutation UpdateCanonicalDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
              metaobjectDefinitionUpdate(id: $id, definition: $definition) {
                metaobjectDefinition { id type name description }
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({
                "id": canonical_id,
                "definition": {
                    "name": "Canonical Definition Updated",
                    "description": "Updated by canonical id"
                }
            }),
        ));
        assert_eq!(
            update.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
            json!([])
        );
        assert_eq!(
            update.body["data"]["metaobjectDefinitionUpdate"]["metaobjectDefinition"]["id"],
            json!(synthetic_id)
        );
        assert_eq!(proxy.store.staged.metaobject_definitions.len(), 1);
        assert!(proxy
            .store
            .staged
            .metaobject_definitions
            .contains_staged(&synthetic_id));
        assert!(!proxy
            .store
            .staged
            .metaobject_definitions
            .contains_staged(&canonical_id));

        let delete = proxy.process_request(graphql_request(
            r#"
            mutation DeleteCanonicalDefinition($id: ID!) {
              metaobjectDefinitionDelete(id: $id) {
                deletedId
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
            json!({"id": canonical_id}),
        ));
        assert_eq!(
            delete.body["data"]["metaobjectDefinitionDelete"]["userErrors"],
            json!([])
        );
        assert_eq!(
            delete.body["data"]["metaobjectDefinitionDelete"]["deletedId"],
            json!(canonical_id)
        );
        assert_eq!(proxy.store.staged.metaobject_definitions.len(), 0);
        assert!(proxy
            .store
            .staged
            .metaobject_definitions
            .is_tombstoned(&synthetic_id));

        let post_delete = proxy.process_request(graphql_request(
            r#"
            query ReadDeletedCanonicalDefinition($canonicalId: ID!, $syntheticId: ID!) {
              canonical: metaobjectDefinition(id: $canonicalId) { id }
              synthetic: metaobjectDefinition(id: $syntheticId) { id }
              canonicalNode: node(id: $canonicalId) { __typename id }
              syntheticNode: node(id: $syntheticId) { __typename id }
            }
            "#,
            json!({"canonicalId": canonical_id, "syntheticId": synthetic_id}),
        ));
        assert_eq!(post_delete.status, 200);
        assert_eq!(post_delete.body["data"]["canonical"], Value::Null);
        assert_eq!(post_delete.body["data"]["synthetic"], Value::Null);
        assert_eq!(post_delete.body["data"]["canonicalNode"], Value::Null);
        assert_eq!(post_delete.body["data"]["syntheticNode"], Value::Null);
    }

    #[test]
    fn metaobject_definition_children_connection_windows_staged_entries() {
        let mut proxy = test_proxy();
        let meta_type = "definition_children_article";
        let definition = create_metaobject_definition(&mut proxy, meta_type);
        let first = create_metaobject(&mut proxy, meta_type, "alpha-entry", "Alpha Entry");
        let second = create_metaobject(&mut proxy, meta_type, "bravo-entry", "Bravo Entry");

        let first_page = proxy.process_request(graphql_request(
            r#"
            query ReadDefinitionChildren($id: ID!, $type: String!) {
              byId: metaobjectDefinition(id: $id) {
                metaobjectsCount
                metaobjects(first: 1) {
                  nodes {
                    id
                    handle
                    type
                    displayName
                    fields { key value definition { key name } }
                  }
                  edges { cursor node { id handle } }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
              byType: metaobjectDefinitionByType(type: $type) {
                metaobjectsCount
                metaobjects(first: 2) {
                  nodes { id handle }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
            "#,
            json!({"id": definition["id"], "type": meta_type}),
        ));

        assert_eq!(first_page.status, 200);
        let by_id = &first_page.body["data"]["byId"];
        let first_connection = &by_id["metaobjects"];
        assert_eq!(by_id["metaobjectsCount"], json!(2));
        assert_eq!(first_connection["nodes"][0]["id"], first["id"]);
        assert_eq!(first_connection["nodes"][0]["handle"], json!("alpha-entry"));
        assert_eq!(
            first_connection["nodes"][0]["fields"][0]["definition"]["name"],
            json!("Title")
        );
        assert_eq!(
            first_connection["edges"][0]["cursor"],
            metaobject_cursor(&first)
        );
        assert_eq!(
            first_connection["pageInfo"],
            json!({
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": metaobject_cursor(&first),
                "endCursor": metaobject_cursor(&first)
            })
        );

        let by_type_connection = &first_page.body["data"]["byType"]["metaobjects"];
        assert_eq!(
            first_page.body["data"]["byType"]["metaobjectsCount"],
            json!(2)
        );
        assert_eq!(by_type_connection["nodes"][0]["id"], first["id"]);
        assert_eq!(by_type_connection["nodes"][1]["id"], second["id"]);
        assert_eq!(by_type_connection["pageInfo"]["hasNextPage"], json!(false));

        let second_page = proxy.process_request(graphql_request(
            r#"
            query ReadDefinitionChildrenAfter($id: ID!, $after: String!) {
              metaobjectDefinition(id: $id) {
                metaobjects(first: 1, after: $after) {
                  nodes { id handle }
                  pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                }
              }
            }
            "#,
            json!({"id": definition["id"], "after": metaobject_cursor(&first)}),
        ));

        assert_eq!(second_page.status, 200);
        let second_connection = &second_page.body["data"]["metaobjectDefinition"]["metaobjects"];
        assert_eq!(second_connection["nodes"][0]["id"], second["id"]);
        assert_eq!(
            second_connection["pageInfo"],
            json!({
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": metaobject_cursor(&second),
                "endCursor": metaobject_cursor(&second)
            })
        );
    }

    #[test]
    fn metaobject_definition_from_record_preserves_real_or_unknown_access() {
        let hydrated_record = json!({
            "type": "restricted_article",
            "definition": {
                "id": "gid://shopify/MetaobjectDefinition/1",
                "type": "restricted_article",
                "access": {
                    "admin": "MERCHANT_READ",
                    "storefront": "NONE",
                    "customerAccount": "NONE"
                },
                "fieldDefinitions": [{
                    "key": "title",
                    "name": "Title",
                    "required": true,
                    "type": { "name": "single_line_text_field", "category": "TEXT" }
                }]
            },
            "fields": [{
                "key": "title",
                "value": "Hidden",
                "definition": {
                    "key": "title",
                    "name": "Title",
                    "required": true,
                    "type": { "name": "single_line_text_field", "category": "TEXT" }
                }
            }]
        });
        let definition = metaobject_definition_from_record(&hydrated_record)
            .expect("hydrated definition should be reused");
        assert_eq!(definition["access"]["admin"], json!("MERCHANT_READ"));

        let inferred_record = json!({
            "type": "observed_article",
            "titleField": { "key": "title" },
            "capabilities": {},
            "fields": [{
                "key": "title",
                "value": "Observed",
                "definition": {
                    "key": "title",
                    "name": "Title",
                    "required": true,
                    "type": { "name": "single_line_text_field", "category": "TEXT" }
                }
            }]
        });
        let inferred = metaobject_definition_from_record(&inferred_record)
            .expect("field definitions should still infer a definition shell");
        assert_eq!(inferred["access"], Value::Null);
    }
}
