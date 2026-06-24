use super::*;
use crate::graphql::{parsed_document, ParsedDocument, RawArgumentValue};
use std::collections::{BTreeMap, BTreeSet};

mod inventory_helpers;
mod marketing_helpers;
mod webhook_helpers;

pub(in crate::proxy) use self::inventory_helpers::*;
pub(in crate::proxy) use self::marketing_helpers::*;
pub(in crate::proxy) use self::webhook_helpers::*;

const INVENTORY_VALID_REASONS: &[&str] = &[
    "correction",
    "cycle_count_available",
    "damaged",
    "movement_canceled",
    "movement_created",
    "movement_received",
    "movement_updated",
    "other",
    "promotion",
    "quality_control",
    "received",
    "reservation_created",
    "reservation_deleted",
    "reservation_updated",
    "restock",
    "safety_stock",
    "shrinkage",
];
const INVENTORY_PUBLIC_ADJUST_QUANTITY_NAMES: &[&str] = &[
    "available",
    "damaged",
    "incoming",
    "quality_control",
    "reserved",
    "safety_stock",
];
const INVENTORY_SET_QUANTITY_NAMES: &[&str] = &["available", "on_hand"];
const INVENTORY_INVALID_PUBLIC_QUANTITY_NAME_MESSAGE: &str = "The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.";
const INVENTORY_INVALID_SET_QUANTITY_NAME_MESSAGE: &str =
    "The quantity name must be either 'available' or 'on_hand'.";
const INVENTORY_SET_QUANTITY_MAX: i64 = 1_000_000_000;
const DEFAULT_INVENTORY_LOCATION_ID: &str = "gid://shopify/Location/106318430514";
const FALLBACK_INVENTORY_LOCATION_ID: &str = "gid://shopify/Location/68509171945";
const INVENTORY_MAX_ACTIVE_LEVELS: usize = 200;
const INVENTORY_ITEM_WEIGHT_UNITS: &[&str] = &["KILOGRAMS", "GRAMS", "POUNDS", "OUNCES"];
const INVENTORY_VALID_COUNTRY_CODES: &[&str] = &[
    "AC", "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AN", "AO", "AR", "AT", "AU", "AW", "AX", "AZ",
    "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS",
    "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN",
    "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE",
    "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FO", "FR", "GA", "GB", "GD", "GE", "GF", "GG",
    "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GW", "GY", "HK", "HM", "HN", "HR",
    "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM", "JO", "JP",
    "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK",
    "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MK", "ML", "MM", "MN",
    "MO", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NC", "NE", "NF", "NG",
    "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM",
    "PN", "PS", "PT", "PY", "QA", "RE", "RO", "RS", "RU", "RW", "SA", "SB", "SC", "SD", "SE", "SG",
    "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS", "ST", "SV", "SX", "SY", "SZ", "TA",
    "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO", "TR", "TT", "TV", "TW", "TZ",
    "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VN", "VU", "WF", "WS", "XK", "YE",
    "YT", "ZA", "ZM", "ZW",
];
const INVENTORY_TRANSFER_HYDRATE_NODES_QUERY: &str = r#"#graphql
  query ProductsHydrateNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on InventoryItem {
        tracked
        requiresShipping
        measurement { weight { unit value } }
        variant {
          id
          title
          inventoryQuantity
          selectedOptions { name value }
          product {
            id
            title
            handle
            status
            totalInventory
            tracksInventory
          }
        }
        inventoryLevels(first: 50) {
          nodes {
            id
            location { id name }
            quantities(names: ["available", "on_hand", "committed", "incoming", "reserved", "damaged", "quality_control", "safety_stock"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
      ... on Location {
        id
        name
        isActive
      }
    }
  }
"#;

impl DraftProxy {
    fn inventory_level_view_state(&self) -> InventoryLevelViewState<'_> {
        InventoryLevelViewState {
            inventory_level_ids: &self.store.staged.inventory_level_ids,
            inactive_levels: &self.store.staged.inactive_inventory_levels,
            quantity_updated_at: &self.store.staged.inventory_quantity_updated_at,
            locations: Some(&self.store.staged.locations.records),
        }
    }

    pub(in crate::proxy) fn marketing_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "marketingActivity" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .marketing_activities
                        .get(&id)
                        .filter(|_| !self.store.staged.marketing_activities.is_tombstoned(&id))
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                "marketingActivities" => {
                    let remote_ids = resolved_string_list_arg(&field.arguments, "remoteIds");
                    let ids = resolved_string_list_arg(&field.arguments, "marketingActivityIds");
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let mut records = self
                        .store
                        .staged
                        .marketing_activities
                        .values()
                        .filter(|record| {
                            let id = record["id"].as_str().unwrap_or_default();
                            if self.store.staged.marketing_activities.is_tombstoned(id) {
                                return false;
                            }
                            if !ids.is_empty() && !ids.iter().any(|candidate| candidate == id) {
                                return false;
                            }
                            if !remote_ids.is_empty()
                                && !remote_ids.iter().any(|candidate| {
                                    record["remoteId"].as_str() == Some(candidate.as_str())
                                        || record["marketingEvent"]["remoteId"].as_str()
                                            == Some(candidate.as_str())
                                })
                            {
                                return false;
                            }
                            if !marketing_record_matches_query(record, &query) {
                                return false;
                            }
                            true
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    records.sort_by_key(|record| {
                        record["id"].as_str().unwrap_or_default().to_string()
                    });
                    marketing_connection(records, &field.selection)
                }
                "marketingEvent" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .marketing_activities
                        .values()
                        .find(|record| record["marketingEvent"]["id"].as_str() == Some(id.as_str()))
                        .filter(|record| {
                            let activity_id = record["id"].as_str().unwrap_or_default();
                            !self
                                .store
                                .staged
                                .marketing_activities
                                .is_tombstoned(activity_id)
                        })
                        .map(|record| record["marketingEvent"].clone())
                        .unwrap_or(Value::Null)
                }
                "marketingEvents" => {
                    let query = resolved_string_arg(&field.arguments, "query").unwrap_or_default();
                    let records = self
                        .store
                        .staged
                        .marketing_activities
                        .values()
                        .filter(|record| {
                            let id = record["id"].as_str().unwrap_or_default();
                            !self.store.staged.marketing_activities.is_tombstoned(id)
                        })
                        .filter(|record| marketing_record_matches_query(record, &query))
                        .filter_map(|record| {
                            if record["marketingEvent"].is_null() {
                                None
                            } else {
                                Some(record["marketingEvent"].clone())
                            }
                        })
                        .collect();
                    marketing_connection(records, &field.selection)
                }
                _ => Value::Null,
            };
            if value.is_null() {
                data.insert(field.response_key.clone(), Value::Null);
            } else if matches!(
                field.name.as_str(),
                "marketingActivities" | "marketingEvents"
            ) {
                data.insert(field.response_key.clone(), value);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn webhook_subscriptions_query_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "webhookSubscription" => field
                    .arguments
                    .get("id")
                    .and_then(resolved_value_string)
                    .and_then(|id| self.store.staged.webhook_subscriptions.get(&id))
                    .map(|record| selected_json(record, &field.selection))
                    .unwrap_or(Value::Null),
                "webhookSubscriptions" => {
                    let records = self.webhook_subscription_records_for_connection(field);
                    selected_connection_json(records, &field.selection)
                }
                "webhookSubscriptionsCount" => {
                    let records = self.webhook_subscription_records_for_filter_args(field);
                    let limit = field.arguments.get("limit").and_then(resolved_as_usize);
                    let count = limit.map_or(records.len(), |limit| records.len().min(limit));
                    let precision = if limit.is_some_and(|limit| records.len() > limit) {
                        "AT_LEAST"
                    } else {
                        "EXACT"
                    };
                    selected_json(
                        &json!({ "count": count, "precision": precision }),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn webhook_subscription_records_for_connection(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        let mut records = self.webhook_subscription_records_for_filter_args(field);
        let sort_key =
            resolved_string_arg(&field.arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
        records.sort_by(|left, right| {
            let sort_cmp = match sort_key.to_ascii_uppercase().as_str() {
                "CREATED_AT" => webhook_subscription_string_field(left, "createdAt")
                    .cmp(&webhook_subscription_string_field(right, "createdAt")),
                "UPDATED_AT" => webhook_subscription_string_field(left, "updatedAt")
                    .cmp(&webhook_subscription_string_field(right, "updatedAt")),
                "TOPIC" => webhook_subscription_string_field(left, "topic")
                    .cmp(&webhook_subscription_string_field(right, "topic")),
                _ => webhook_subscription_numeric_id(left)
                    .cmp(&webhook_subscription_numeric_id(right)),
            };
            sort_cmp.then_with(|| {
                webhook_subscription_numeric_id(left).cmp(&webhook_subscription_numeric_id(right))
            })
        });
        if matches!(
            field.arguments.get("reverse"),
            Some(ResolvedValue::Bool(true))
        ) {
            records.reverse();
        }
        if let Some(first) = field.arguments.get("first").and_then(resolved_as_usize) {
            records.truncate(first);
        }
        records
    }

    pub(in crate::proxy) fn webhook_subscription_records_for_filter_args(
        &self,
        field: &RootFieldSelection,
    ) -> Vec<Value> {
        self.store
            .staged
            .webhook_subscriptions
            .values()
            .filter(|record| webhook_subscription_matches_field_args(record, &field.arguments))
            .cloned()
            .collect()
    }

    /// Dispatch a webhook subscription mutation document. Iterates over every
    /// root field so aliased multi-mutation documents (e.g. several
    /// `webhookSubscriptionCreate` aliases in one request) all resolve, keyed by
    /// their response alias. Schema-level errors (invalid topic literal, missing
    /// required pub/sub fields) abort the whole operation with top-level errors,
    /// matching GraphQL execution semantics.
    pub(in crate::proxy) fn webhook_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        for field in &document.root_fields {
            let required_errors = webhook_required_argument_errors(field, &document);
            if !required_errors.is_empty() {
                return ok_json(json!({ "errors": required_errors }));
            }
            if let Some(error) = webhook_subscription_topic_coercion_error(field, Some(&document)) {
                return ok_json(json!({ "errors": [error] }));
            }
            if let Some(error) =
                dedicated_pubsub_required_field_error(&field.name, field, &document)
            {
                return ok_json(json!({ "errors": [error] }));
            }
            let payload = match field.name.as_str() {
                "webhookSubscriptionCreate"
                | "pubSubWebhookSubscriptionCreate"
                | "eventBridgeWebhookSubscriptionCreate" => {
                    self.webhook_subscription_create_field(field, request, query, variables)
                }
                "webhookSubscriptionUpdate"
                | "pubSubWebhookSubscriptionUpdate"
                | "eventBridgeWebhookSubscriptionUpdate" => {
                    self.webhook_subscription_update_field(field, request, query, variables)
                }
                "webhookSubscriptionDelete" => {
                    self.webhook_subscription_delete_field(field, request, query, variables)
                }
                other => {
                    return json_error(
                        501,
                        &format!("No Rust webhooks dispatcher implemented for root field: {other}"),
                    );
                }
            };
            data.insert(field.response_key.clone(), payload);
        }
        ok_json(json!({ "data": data }))
    }

    fn webhook_subscription_create_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = self.next_proxy_synthetic_gid("WebhookSubscription");
        let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id");
        let api_version = webhook_subscription_effective_api_version(request);
        let record = self.webhook_subscription_record(
            &id,
            &field.arguments,
            None,
            api_client_id.as_deref(),
            api_version.as_deref(),
        );
        let errors =
            self.webhook_subscription_validation_errors(&field.name, &id, &record, request);
        if !errors.is_empty() {
            return self.webhook_subscription_payload(Value::Null, field.selection.clone(), errors);
        }
        self.store
            .staged
            .webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, &field.name, vec![id]);
        self.webhook_subscription_payload(record, field.selection.clone(), Vec::new())
    }

    fn webhook_subscription_update_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.webhook_subscriptions.get(&id).cloned() else {
            return self.webhook_subscription_payload(
                Value::Null,
                field.selection.clone(),
                vec![user_error_omit_code(
                    ["id"],
                    "Webhook subscription does not exist",
                    None,
                )],
            );
        };
        let api_client_id = request_header(request, "x-shopify-draft-proxy-api-client-id");
        let api_version = webhook_subscription_effective_api_version(request);
        let record = self.webhook_subscription_record(
            &id,
            &field.arguments,
            Some(existing),
            api_client_id.as_deref(),
            api_version.as_deref(),
        );
        let errors =
            self.webhook_subscription_validation_errors(&field.name, &id, &record, request);
        if !errors.is_empty() {
            return self.webhook_subscription_payload(Value::Null, field.selection.clone(), errors);
        }
        self.store
            .staged
            .webhook_subscriptions
            .insert(id.clone(), record.clone());
        self.record_mutation_log_entry(request, query, variables, &field.name, vec![id]);
        self.webhook_subscription_payload(record, field.selection.clone(), Vec::new())
    }

    fn webhook_subscription_delete_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let deleted_id = if self
            .store
            .staged
            .webhook_subscriptions
            .remove(&id)
            .is_some()
        {
            json!(id.clone())
        } else {
            Value::Null
        };
        if deleted_id != Value::Null {
            self.record_mutation_log_entry(
                request,
                query,
                variables,
                "webhookSubscriptionDelete",
                vec![id],
            );
        }
        let payload = json!({
            "deletedWebhookSubscriptionId": deleted_id,
            "userErrors": if deleted_id == Value::Null {
                json!([{ "field": ["id"], "message": "Webhook subscription does not exist" }])
            } else {
                json!([])
            }
        });
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn webhook_subscription_payload(
        &self,
        record: Value,
        payload_selection: Vec<SelectedField>,
        user_errors: Vec<Value>,
    ) -> Value {
        let subscription_selection =
            selected_child_selection(&payload_selection, "webhookSubscription").unwrap_or_default();
        let payload = json!({
            "webhookSubscription": if record == Value::Null {
                Value::Null
            } else {
                selected_json(&record, &subscription_selection)
            },
            "userErrors": user_errors
        });
        selected_json(&payload, &payload_selection)
    }

    pub(in crate::proxy) fn webhook_subscription_validation_errors(
        &self,
        root_field: &str,
        id: &str,
        record: &Value,
        request: &Request,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let uri = record["callbackUrl"].as_str().unwrap_or_default();
        let address_field = webhook_subscription_address_error_field(root_field);
        if uri.trim().is_empty() {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address can't be blank"
            }));
        }
        if uri.starts_with("http://") {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address protocol http:// is not supported"
            }));
        }
        if uri.starts_with("kafka://") {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address protocol kafka:// is not supported"
            }));
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is not a valid kafka topic"
            }));
        }
        if uri.len() > 65_535 {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address is too big (maximum is 64 KB)"
            }));
        }
        if webhook_uri_uses_disallowed_host(uri) {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address cannot be a Shopify or an internal domain"
            }));
        }
        if let Some(pubsub_tail) = uri.strip_prefix("pubsub://") {
            let pubsub_parts = pubsub_tail.split_once(':');
            let (project, topic) = pubsub_parts.unwrap_or((pubsub_tail, ""));
            if pubsub_parts.is_none() || project.is_empty() || topic.is_empty() {
                errors.push(json!({
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address protocol pubsub:// is not supported"
                }));
                errors.push(json!({
                    "field": ["webhookSubscription", "callbackUrl"],
                    "message": "Address is not a valid GCP pub/sub format. Format should be pubsub://project:topic"
                }));
            } else if !valid_gcp_project_id(project) {
                if root_field.starts_with("pubSubWebhookSubscription") {
                    errors.push(json!({
                        "field": ["webhookSubscription", "pubSubProject"],
                        "message": "Google Cloud Pub/Sub project ID is not valid"
                    }));
                } else {
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is invalid"
                    }));
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is not a valid GCP project id."
                    }));
                }
            } else if !valid_gcp_pubsub_topic_id(topic) {
                if root_field.starts_with("pubSubWebhookSubscription") {
                    errors.push(json!({
                        "field": ["webhookSubscription", "pubSubTopic"],
                        "message": "Google Cloud Pub/Sub topic ID is not valid"
                    }));
                } else {
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is invalid"
                    }));
                    errors.push(json!({
                        "field": ["webhookSubscription", "callbackUrl"],
                        "message": "Address is not a valid GCP topic id."
                    }));
                }
            }
        }
        if uri.starts_with("arn:aws:events:") {
            if let Some(arn_api_client_id) = eventbridge_arn_api_client_id(uri) {
                if let Some(caller_api_client_id) =
                    request.headers.get("x-shopify-draft-proxy-api-client-id")
                {
                    if arn_api_client_id != caller_api_client_id {
                        errors.push(json!({
                            "field": address_field,
                            "message": "Address is invalid"
                        }));
                        errors.push(json!({
                            "field": address_field,
                            "message": format!(
                                "Address is an AWS ARN and includes api_client_id '{}' instead of '{}'",
                                arn_api_client_id, caller_api_client_id
                            )
                        }));
                    }
                }
            } else {
                errors.push(json!({
                    "field": address_field,
                    "message": "Address is invalid"
                }));
                errors.push(json!({
                    "field": address_field,
                    "message": "Address is not a valid AWS ARN"
                }));
            }
        }
        let topic = record["topic"].as_str().unwrap_or_default();
        let format = record["format"].as_str().unwrap_or_default();
        if (uri.starts_with("pubsub://") || uri.starts_with("arn:aws:events:"))
            && !format.eq_ignore_ascii_case("JSON")
        {
            errors.push(json!({
                "field": ["webhookSubscription", "format"],
                "message": "Format can only be used with format: 'json'"
            }));
        } else if topic == "RETURNS_APPROVE" && format.eq_ignore_ascii_case("XML") {
            errors.push(json!({
                "field": ["webhookSubscription", "format"],
                "message": "Format 'xml' is invalid for this webhook topic. Allowed formats: json"
            }));
        }
        if self
            .store
            .staged
            .webhook_subscriptions
            .iter()
            .any(|(existing_id, existing)| {
                existing_id != id
                    && existing["topic"].as_str() == Some(topic)
                    && existing["callbackUrl"].as_str() == Some(uri)
                    && existing["format"].as_str() == Some(format)
                    && webhook_subscription_optional_string_key(existing, "filter")
                        == webhook_subscription_optional_string_key(record, "filter")
                    && webhook_subscription_optional_string_key(existing, "apiPermissionId")
                        == webhook_subscription_optional_string_key(record, "apiPermissionId")
            })
        {
            errors.push(json!({
                "field": ["webhookSubscription", "callbackUrl"],
                "message": "Address for this topic has already been taken"
            }));
        }
        if let Some(name) = record["name"].as_str() {
            if name.is_empty() {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name is too short (minimum is 1 character)"
                }));
            }
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name name field can only contain alphanumeric characters, underscores, and hyphens"
                }));
            }
            if name.chars().count() > 50 {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name is too long (maximum is 50 characters)"
                }));
            }
            if self
                .store
                .staged
                .webhook_subscriptions
                .iter()
                .any(|(existing_id, existing)| {
                    existing_id != id
                        && existing["name"]
                            .as_str()
                            .is_some_and(|existing_name| existing_name.eq_ignore_ascii_case(name))
                })
            {
                errors.push(json!({
                    "field": ["webhookSubscription", "name"],
                    "message": "Name already exists, no duplicate allowed"
                }));
            }
        }
        if let Some(filter) = record["filter"].as_str() {
            if webhook_filter_is_invalid(filter) {
                errors.push(json!({
                    "field": ["webhookSubscription"],
                    "message": "The specified filter is invalid, please ensure you specify the field(s) you wish to filter on."
                }));
            }
        }
        errors
    }

    pub(in crate::proxy) fn webhook_subscription_record(
        &self,
        id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        existing: Option<Value>,
        api_client_id: Option<&str>,
        api_version_handle: Option<&str>,
    ) -> Value {
        let webhook_input =
            resolved_object_field(arguments, "webhookSubscription").unwrap_or_default();
        let topic = resolved_string_field(arguments, "topic")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["topic"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "ORDERS_CREATE".to_string());
        let dedicated_pubsub_uri = resolved_string_field(&webhook_input, "pubSubProject")
            .zip(resolved_string_field(&webhook_input, "pubSubTopic"))
            .map(|(project, topic)| format!("pubsub://{}:{}", project.trim(), topic.trim()));
        let uri = resolved_string_field(&webhook_input, "uri")
            .or_else(|| resolved_string_field(&webhook_input, "callbackUrl"))
            .or(dedicated_pubsub_uri)
            .or_else(|| resolved_string_field(&webhook_input, "arn"))
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["callbackUrl"].as_str().map(ToString::to_string))
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        let format = resolved_string_field(&webhook_input, "format")
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["format"].as_str().map(ToString::to_string))
            })
            .unwrap_or_else(|| "JSON".to_string());
        let api_permission_id =
            resolved_string_field(&webhook_input, "apiPermissionId").or_else(|| {
                existing
                    .as_ref()
                    .and_then(|record| record["apiPermissionId"].as_str().map(ToString::to_string))
            });
        let name = resolved_string_field(&webhook_input, "name").or_else(|| {
            existing
                .as_ref()
                .and_then(|record| record["name"].as_str().map(ToString::to_string))
        });
        let include_fields = if webhook_input.contains_key("includeFields") {
            json!(resolved_string_list_field_unsorted(
                &webhook_input,
                "includeFields"
            ))
        } else {
            existing
                .as_ref()
                .map(|record| record["includeFields"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let metafield_namespaces = if webhook_input.contains_key("metafieldNamespaces") {
            json!(
                resolved_string_list_field_unsorted(&webhook_input, "metafieldNamespaces")
                    .into_iter()
                    .map(|namespace| resolve_webhook_metafield_namespace(&namespace, api_client_id))
                    .collect::<Vec<_>>()
            )
        } else {
            existing
                .as_ref()
                .map(|record| record["metafieldNamespaces"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let metafields = if webhook_input.contains_key("metafields") {
            json!(resolved_object_list_field(&webhook_input, "metafields")
                .into_iter()
                .filter_map(|identifier| {
                    Some(json!({
                        "namespace": resolved_string_field(&identifier, "namespace")?,
                        "key": resolved_string_field(&identifier, "key")?
                    }))
                })
                .collect::<Vec<Value>>())
        } else {
            existing
                .as_ref()
                .map(|record| record["metafields"].clone())
                .filter(Value::is_array)
                .unwrap_or_else(|| json!([]))
        };
        let filter = match webhook_input.get("filter") {
            Some(ResolvedValue::String(value)) => json!(value),
            Some(ResolvedValue::Null) => Value::Null,
            Some(_) => Value::Null,
            None => existing
                .as_ref()
                .map(|record| record["filter"].clone())
                .unwrap_or(Value::Null),
        };
        let created_at = existing
            .as_ref()
            .and_then(|record| record["createdAt"].as_str())
            .unwrap_or("2024-01-01T00:00:00.000Z");
        let webhook_mutation_count = self
            .log_entries
            .iter()
            .filter(|entry| {
                entry
                    .get("interpreted")
                    .and_then(|interpreted| interpreted.get("primaryRootField"))
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        matches!(
                            name,
                            "webhookSubscriptionCreate"
                                | "webhookSubscriptionUpdate"
                                | "pubSubWebhookSubscriptionCreate"
                                | "pubSubWebhookSubscriptionUpdate"
                                | "eventBridgeWebhookSubscriptionCreate"
                                | "eventBridgeWebhookSubscriptionUpdate"
                        )
                    })
            })
            .count();
        let updated_at = if existing.is_some() {
            format!("2024-01-01T00:00:{:02}.000Z", webhook_mutation_count + 1)
        } else {
            created_at.to_string()
        };
        let api_version = existing
            .as_ref()
            .and_then(|record| record.get("apiVersion"))
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| webhook_subscription_api_version_record(api_version_handle));
        json!({
            "id": id,
            "legacyResourceId": webhook_subscription_legacy_id(id),
            "apiVersion": api_version,
            "topic": topic,
            "format": format,
            "uri": uri,
            "callbackUrl": uri,
            "name": name,
            "apiPermissionId": api_permission_id,
            "includeFields": include_fields,
            "metafieldNamespaces": metafield_namespaces,
            "metafields": metafields,
            "filter": filter,
            "createdAt": created_at,
            "updatedAt": updated_at,
            "endpoint": webhook_endpoint(&uri)
        })
    }

    pub(in crate::proxy) fn marketing_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let mut top_errors: Vec<Value> = Vec::new();
        let mut omit_data = false;
        for field in fields {
            if matches!(
                field.name.as_str(),
                "marketingActivityCreateExternal"
                    | "marketingActivityUpdateExternal"
                    | "marketingActivityUpsertExternal"
            ) {
                let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
                match marketing_url_scheme_error(&input) {
                    Some(MarketingUrlError::WrongScheme) => {
                        top_errors.push(json!({
                            "message": "The URL scheme must be one of the following: https,http",
                            "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
                            "path": [field.name.clone()]
                        }));
                        data.insert(field.response_key.clone(), Value::Null);
                        continue;
                    }
                    Some(MarketingUrlError::MissingHost {
                        field: bad_field,
                        url,
                        value,
                    }) => {
                        let type_name = marketing_external_input_type_name(&field.name);
                        let explanation = format!("Invalid url '{url}', missing host");
                        let message = format!(
                            "Variable $input of type {type_name}! was provided invalid value for {bad_field} ({explanation})"
                        );
                        top_errors.push(json!({
                            "message": message,
                            "extensions": {
                                "code": "INVALID_VARIABLE",
                                "value": value,
                                "problems": [{
                                    "path": [bad_field],
                                    "explanation": explanation.clone(),
                                    "message": explanation
                                }]
                            }
                        }));
                        omit_data = true;
                        continue;
                    }
                    None => {}
                }
            }
            let value = match field.name.as_str() {
                "marketingActivityCreateExternal" => self.marketing_create_external(field, request),
                "marketingActivityUpdateExternal" => self.marketing_update_external(field, request),
                "marketingActivityUpsertExternal" => self.marketing_upsert_external(field, request),
                "marketingActivityDeleteExternal" => self.marketing_delete_external(field, request),
                "marketingActivitiesDeleteAllExternal" => {
                    self.store.staged.marketing_delete_all_external = true;
                    selected_json(
                        &json!({
                            "job": { "id": "gid://shopify/Job/marketing-delete-all-local", "done": false },
                            "userErrors": []
                        }),
                        &field.selection,
                    )
                }
                "marketingEngagementCreate" => {
                    self.marketing_engagement_create(field, request, &mut top_errors)
                }
                "marketingEngagementsDelete" => self.marketing_engagements_delete(field, request),
                "marketingActivityCreate" => selected_json(
                    &json!({
                        "marketingActivity": null,
                        "redirectPath": null,
                        "userErrors": if field.response_key == "invalidExtension" { json!([{ "field": ["input", "marketingActivityExtensionId"], "message": "Could not find the marketing extension" }]) } else { json!([]) }
                    }),
                    &field.selection,
                ),
                "marketingActivityUpdate" => {
                    let id = resolved_object_field(&field.arguments, "input")
                        .and_then(|input| resolved_string_field(&input, "id"))
                        .unwrap_or_else(|| "gid://shopify/MarketingActivity/1".to_string());
                    let mut native_input = BTreeMap::new();
                    native_input.insert(
                        "title".to_string(),
                        ResolvedValue::String("HAR-373 Native Activity Active".to_string()),
                    );
                    native_input.insert(
                        "remoteId".to_string(),
                        ResolvedValue::String("native-local".to_string()),
                    );
                    native_input.insert(
                        "status".to_string(),
                        ResolvedValue::String("ACTIVE".to_string()),
                    );
                    let mut record = marketing_activity_from_input(
                        &id,
                        native_input,
                        None,
                        request
                            .headers
                            .get("x-shopify-draft-proxy-api-client-id")
                            .cloned(),
                    );
                    record["isExternal"] = json!(false);
                    record["inMainWorkflowVersion"] = json!(true);
                    record["marketingEvent"] = Value::Null;
                    self.store
                        .staged
                        .marketing_activities
                        .insert(id, record.clone());
                    selected_json(
                        &json!({ "marketingActivity": record, "redirectPath": "/admin/marketing", "userErrors": [] }),
                        &field.selection,
                    )
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        let mut body = if omit_data {
            json!({})
        } else {
            json!({ "data": Value::Object(data) })
        };
        if !top_errors.is_empty() {
            body["errors"] = Value::Array(top_errors);
        }
        ok_json(body)
    }

    pub(in crate::proxy) fn marketing_create_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let payload = self.marketing_create_or_update_payload(field, input, None, true, request);
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn marketing_update_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if field.arguments.contains_key("remoteId") && field.arguments.contains_key("utm") {
            let remote = resolved_string_arg(&field.arguments, "remoteId").unwrap_or_default();
            let utm = resolved_object_field(&field.arguments, "utm").unwrap_or_default();
            let target_by_remote = self.find_marketing_activity_by_remote(&remote, request);
            let campaign = resolved_string_field(&utm, "campaign").unwrap_or_default();
            let target_by_utm = self.find_marketing_activity_by_utm(&campaign, request);
            if target_by_remote.is_none()
                || target_by_utm.is_none()
                || target_by_remote != target_by_utm
            {
                return selected_json(
                    &marketing_activity_payload(None, vec![marketing_activity_missing_error()]),
                    &field.selection,
                );
            }
        }
        let existing_id = resolved_string_arg(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            })
            .or_else(|| {
                resolved_object_field(&field.arguments, "utm")
                    .and_then(|utm| resolved_string_field(&utm, "campaign"))
                    .and_then(|campaign| self.find_marketing_activity_by_utm(&campaign, request))
            });
        let Some(existing_id) = existing_id else {
            return selected_json(
                &marketing_activity_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let existing = self
            .store
            .staged
            .marketing_activities
            .get(&existing_id)
            .cloned()
            .unwrap_or(Value::Null);
        let selector_utm = resolved_object_field(&field.arguments, "utm");
        if let Some(err) = self.marketing_external_immutable_update_error(
            &existing,
            &input,
            selector_utm.as_ref(),
            request,
        ) {
            return selected_json(
                &marketing_activity_payload(None, vec![err]),
                &field.selection,
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            return selected_json(
                &marketing_activity_payload(None, vec![err]),
                &field.selection,
            );
        }
        let payload = self.marketing_create_or_update_payload(
            field,
            input,
            Some(existing_id),
            false,
            request,
        );
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn marketing_upsert_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if marketing_input_tactic_is_storefront_app(&input) {
            return selected_json(
                &marketing_activity_payload(
                    None,
                    vec![marketing_activity_cannot_update_tactic_to_storefront_error()],
                ),
                &field.selection,
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        let existing_id = self.find_marketing_activity_by_remote(&remote, request);
        if let Some(id) = &existing_id {
            if let Some(existing) = self.store.staged.marketing_activities.get(id) {
                if let Some(err) =
                    self.marketing_external_immutable_update_error(existing, &input, None, request)
                {
                    return selected_json(
                        &marketing_activity_payload(None, vec![err]),
                        &field.selection,
                    );
                }
            }
        }
        let payload =
            self.marketing_create_or_update_payload(field, input, existing_id, true, request);
        selected_json(&payload, &field.selection)
    }

    pub(in crate::proxy) fn marketing_create_or_update_payload(
        &mut self,
        field: &RootFieldSelection,
        input: BTreeMap<String, ResolvedValue>,
        existing_id: Option<String>,
        create_if_missing: bool,
        request: &Request,
    ) -> Value {
        if self.store.staged.marketing_delete_all_external
            && existing_id.is_none()
            && field.name == "marketingActivityCreateExternal"
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": null,
                    "message": "Cannot perform this operation because a job to delete all external activities has been enqueued, which happens either from calling the marketingActivitiesDeleteAllExternal mutation or as a result of an app uninstall. Please either check the status of the job returned by the mutation or try again later.",
                    "code": "DELETE_JOB_ENQUEUED"
                })],
            );
        }
        if !input.contains_key("utm")
            && !input.contains_key("urlParameterValue")
            && create_if_missing
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.",
                    "code": "NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER"
                })],
            );
        }
        if has_marketing_currency_mismatch(&input) {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "Currency code is not matching between budget and ad spend",
                    "code": null
                })],
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            // Top-level GraphQL coercion in Shopify; parity compares errors for these cases.
            return marketing_activity_payload(None, vec![err]);
        }
        if create_if_missing
            && existing_id.is_none()
            && resolved_string_field(&input, "channelHandle")
                .is_some_and(|handle| handle != "email")
        {
            return marketing_activity_payload(
                None,
                vec![json!({
                    "field": ["input"],
                    "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                    "code": "INVALID_CHANNEL_HANDLE"
                })],
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        if create_if_missing && existing_id.is_none() {
            if !remote.is_empty()
                && self
                    .find_marketing_activity_by_remote(&remote, request)
                    .is_some()
            {
                return marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input"],
                        "message": "Validation failed: Remote ID has already been taken",
                        "code": null
                    })],
                );
            }
            if resolved_object_field(&input, "utm")
                .and_then(|utm| resolved_string_field(&utm, "campaign"))
                .is_some_and(|campaign| {
                    self.find_marketing_activity_by_utm(&campaign, request)
                        .is_some()
                })
            {
                return marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input"],
                        "message": "Validation failed: Utm campaign has already been taken",
                        "code": null
                    })],
                );
            }
            if resolved_string_field(&input, "urlParameterValue").is_some_and(|value| {
                self.find_marketing_activity_by_url_parameter(&value, request)
                    .is_some()
            }) {
                let message = if field.name == "marketingActivityUpsertExternal" {
                    "Validation failed: Url parameter value has already been taken, Url parameter value has already been taken"
                } else {
                    "Validation failed: Url parameter value has already been taken"
                };
                return marketing_activity_payload(
                    None,
                    vec![json!({
                        "field": ["input"],
                        "message": message,
                        "code": null
                    })],
                );
            }
        }
        let id = existing_id.unwrap_or_else(|| {
            format!("gid://shopify/MarketingActivity/{}", self.next_synthetic_id)
        });
        if !self.store.staged.marketing_activities.contains_key(&id) {
            self.next_synthetic_id += 2;
        }
        let existing = self.store.staged.marketing_activities.get(&id).cloned();
        let activity = marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .cloned(),
        );
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        marketing_activity_payload(Some(activity), Vec::new())
    }

    pub(in crate::proxy) fn marketing_delete_external(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        if !field.arguments.contains_key("marketingActivityId")
            && !field.arguments.contains_key("id")
            && !field.arguments.contains_key("remoteId")
        {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [{
                "field": null,
                "message": "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.",
                "code": "INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"
            }] }),
                &field.selection,
            );
        }
        let id = resolved_string_arg(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_arg(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_arg(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            });
        let Some(id) = id else {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] }),
                &field.selection,
            );
        };
        let Some(activity) = self.marketing_activity_for_delete(&id, request) else {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] }),
                &field.selection,
            );
        };
        if activity["isExternal"] == json!(false) {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_delete_not_external_error()] }),
                &field.selection,
            );
        }
        if self.marketing_activity_has_child_events(activity) {
            return selected_json(
                &json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_child_events_error()] }),
                &field.selection,
            );
        }
        self.store.staged.marketing_activities.tombstone(id.clone());
        selected_json(
            &json!({ "deletedMarketingActivityId": id, "userErrors": [] }),
            &field.selection,
        )
    }

    fn marketing_activity_for_delete(&self, id: &str, request: &Request) -> Option<&Value> {
        if self.store.staged.marketing_activities.is_tombstoned(id) {
            return None;
        }
        let activity = self.store.staged.marketing_activities.get(id)?;
        let request_app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        if activity["apiClientId"].as_str() == request_app.map(String::as_str) {
            Some(activity)
        } else {
            None
        }
    }

    fn marketing_activity_has_child_events(&self, activity: &Value) -> bool {
        let parent_remote = activity["remoteId"]
            .as_str()
            .or_else(|| activity["marketingEvent"]["remoteId"].as_str());
        let Some(parent_remote) = parent_remote else {
            return false;
        };
        let parent_app = activity["apiClientId"].as_str();
        self.store
            .staged
            .marketing_activities
            .iter()
            .any(|(id, candidate)| {
                if self.store.staged.marketing_activities.is_tombstoned(id) {
                    return false;
                }
                candidate["id"].as_str() != activity["id"].as_str()
                    && candidate["apiClientId"].as_str() == parent_app
                    && candidate["parentRemoteId"].as_str() == Some(parent_remote)
            })
    }

    pub(in crate::proxy) fn marketing_engagement_create(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        _top_errors: &mut Vec<Value>,
    ) -> Value {
        let has_activity_id = field.arguments.contains_key("marketingActivityId");
        let has_remote = field.arguments.contains_key("remoteId");
        let has_channel = field.arguments.contains_key("channelHandle");
        let selector_count = [has_activity_id, has_remote, has_channel]
            .iter()
            .filter(|v| **v)
            .count();
        if selector_count == 0 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
                        "code": "INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"
                    })],
                ),
                &field.selection,
            );
        }
        if selector_count > 1 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": null,
                        "message": "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.",
                        "code": "INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"
                    })],
                ),
                &field.selection,
            );
        }
        if let Some(channel) = resolved_string_arg(&field.arguments, "channelHandle") {
            if channel != "email" {
                return selected_json(
                    &marketing_engagement_payload(
                        None,
                        vec![json!({
                            "field": ["channelHandle"],
                            "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                            "code": "INVALID_CHANNEL_HANDLE"
                        })],
                    ),
                    &field.selection,
                );
            }
        }
        let engagement_input =
            resolved_object_field(&field.arguments, "marketingEngagement").unwrap_or_default();
        if has_engagement_currency_mismatch(&engagement_input) {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": ["marketingEngagement"],
                        "message": "Currency codes in the marketing engagement input do not match.",
                        "code": "CURRENCY_CODE_MISMATCH_INPUT"
                    })],
                ),
                &field.selection,
            );
        }
        if has_channel {
            let engagement = marketing_engagement_from_input(&engagement_input, None);
            return selected_json(
                &marketing_engagement_payload(Some(engagement), Vec::new()),
                &field.selection,
            );
        }
        let activity_id = if has_activity_id {
            resolved_string_arg(&field.arguments, "marketingActivityId")
        } else {
            resolved_string_arg(&field.arguments, "remoteId")
                .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
        };
        let Some(activity_id) = activity_id else {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        let Some(activity) = self
            .store
            .staged
            .marketing_activities
            .get(&activity_id)
            .filter(|_| {
                !self
                    .store
                    .staged
                    .marketing_activities
                    .is_tombstoned(&activity_id)
            })
        else {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_activity_missing_error()]),
                &field.selection,
            );
        };
        if activity["marketingEvent"].is_null() {
            return selected_json(
                &marketing_engagement_payload(None, vec![marketing_event_missing_error()]),
                &field.selection,
            );
        }
        if self.engagement_currency_mismatches_activity(&activity_id, &engagement_input) {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![json!({
                        "field": ["marketingEngagement"],
                        "message": "Marketing activity currency code does not match the currency code in the marketing engagement input.",
                        "code": "MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"
                    })],
                ),
                &field.selection,
            );
        }
        let engagement = marketing_engagement_from_input(&engagement_input, Some(activity));
        // Shopify accepts engagement metrics but does not fold engagement ad spend
        // back into the MarketingActivity.adSpend field in these captures.
        selected_json(
            &marketing_engagement_payload(Some(engagement), Vec::new()),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn marketing_engagements_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let has_channel_handle = field.arguments.contains_key("channelHandle");
        let delete_all_channels = matches!(
            field.arguments.get("deleteEngagementsForAllChannels"),
            Some(ResolvedValue::Bool(true))
        );
        let known_handles = self.marketing_channel_handles_for_request(request);
        let (result, errors) = if has_channel_handle == delete_all_channels {
            (
                Value::Null,
                vec![json!({
                    "field": null,
                    "message": "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.",
                    "code": "INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"
                })],
            )
        } else if let Some(channel_handle) = resolved_string_arg(&field.arguments, "channelHandle")
        {
            if known_handles.contains(&channel_handle) {
                (
                    json!(format!(
                        "Engagement data associated to channel handle '{channel_handle}' marked for deletion"
                    )),
                    Vec::new(),
                )
            } else {
                (
                    Value::Null,
                    vec![json!({
                        "field": ["channelHandle"],
                        "message": "The channel handle is not recognized. Please contact your partner manager for more information.",
                        "code": "INVALID_CHANNEL_HANDLE"
                    })],
                )
            }
        } else {
            (
                json!(format!(
                    "Engagement data marked for deletion for {} channel(s)",
                    known_handles.len()
                )),
                Vec::new(),
            )
        };
        selected_json(
            &json!({ "result": result, "userErrors": errors }),
            &field.selection,
        )
    }

    fn marketing_channel_handles_for_request(&self, request: &Request) -> BTreeSet<String> {
        let request_app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .filter_map(|(id, record)| {
                if self.store.staged.marketing_activities.is_tombstoned(id) {
                    return None;
                }
                if let Some(app) = request_app {
                    if record["apiClientId"].as_str() != Some(app.as_str()) {
                        return None;
                    }
                }
                record["marketingEvent"]["channelHandle"]
                    .as_str()
                    .filter(|handle| !handle.is_empty())
                    .map(str::to_string)
            })
            .collect()
    }

    pub(in crate::proxy) fn find_marketing_activity_by_remote(
        &self,
        remote: &str,
        request: &Request,
    ) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self.store.staged.marketing_activities.is_tombstoned(id) {
                    return None;
                }
                if record["remoteId"].as_str() != Some(remote)
                    && record["marketingEvent"]["remoteId"].as_str() != Some(remote)
                {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    pub(in crate::proxy) fn find_marketing_activity_by_utm(
        &self,
        campaign: &str,
        request: &Request,
    ) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self.store.staged.marketing_activities.is_tombstoned(id) {
                    return None;
                }
                if record["utmParameters"]["campaign"].as_str() != Some(campaign) {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    pub(in crate::proxy) fn find_marketing_activity_by_url_parameter(
        &self,
        url_parameter_value: &str,
        request: &Request,
    ) -> Option<String> {
        let app = request.headers.get("x-shopify-draft-proxy-api-client-id");
        self.store
            .staged
            .marketing_activities
            .iter()
            .find_map(|(id, record)| {
                if self.store.staged.marketing_activities.is_tombstoned(id) {
                    return None;
                }
                if record["urlParameterValue"].as_str() != Some(url_parameter_value) {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    fn marketing_external_immutable_update_error(
        &self,
        existing: &Value,
        input: &BTreeMap<String, ResolvedValue>,
        selector_utm: Option<&BTreeMap<String, ResolvedValue>>,
        request: &Request,
    ) -> Option<Value> {
        if existing["isExternal"] == json!(false) {
            return Some(marketing_activity_not_external_error());
        }
        if existing["marketingEvent"].is_null() {
            return Some(json!({
                "field": null,
                "message": "Marketing activity is not valid, the associated marketing event does not exist.",
                "code": "MARKETING_EVENT_DOES_NOT_EXIST"
            }));
        }
        if marketing_input_tactic_is_storefront_app(input) {
            return Some(marketing_activity_cannot_update_tactic_to_storefront_error());
        }
        if marketing_input_has_tactic(input)
            && marketing_activity_tactic_is_storefront_app(existing)
        {
            return Some(marketing_activity_cannot_update_tactic_from_storefront_error());
        }
        if resolved_string_field(input, "channelHandle").is_some_and(|channel_handle| {
            existing["marketingEvent"]["channelHandle"].as_str() != Some(channel_handle.as_str())
        }) {
            return Some(json!({
                "field": ["input"],
                "message": "Channel handle cannot be modified.",
                "code": "IMMUTABLE_CHANNEL_HANDLE"
            }));
        }
        if input_string_field_value(input, "urlParameterValue")
            .is_some_and(|value| json_string_value(&existing["urlParameterValue"]) != Some(value))
        {
            return Some(json!({
                "field": ["input"],
                "message": "URL parameter value cannot be modified.",
                "code": "IMMUTABLE_URL_PARAMETER"
            }));
        }
        if (input.contains_key("utm") || selector_utm.is_some())
            && (input_utm_value(input, selector_utm, "campaign")
                != json_string_value(&existing["utmParameters"]["campaign"])
                || input_utm_value(input, selector_utm, "source")
                    != json_string_value(&existing["utmParameters"]["source"])
                || input_utm_value(input, selector_utm, "medium")
                    != json_string_value(&existing["utmParameters"]["medium"]))
        {
            return Some(json!({
                "field": ["input"],
                "message": "UTM parameters cannot be modified.",
                "code": "IMMUTABLE_UTM_PARAMETERS"
            }));
        }
        if let Some(parent_remote_id) = resolved_string_field(input, "parentRemoteId") {
            let Some(parent_id) =
                self.find_marketing_activity_by_remote(&parent_remote_id, request)
            else {
                return Some(json!({
                    "field": ["input"],
                    "message": "Remote ID does not correspond to an activity.",
                    "code": "INVALID_REMOTE_ID"
                }));
            };
            let existing_parent_remote_id = existing["parentRemoteId"].as_str().unwrap_or("");
            let existing_parent_id = if existing_parent_remote_id.is_empty() {
                None
            } else {
                self.find_marketing_activity_by_remote(existing_parent_remote_id, request)
            };
            if existing_parent_id.as_deref() != Some(parent_id.as_str()) {
                return Some(json!({
                    "field": ["input"],
                    "message": "Parent ID cannot be modified.",
                    "code": "IMMUTABLE_PARENT_ID"
                }));
            }
        }
        if resolved_string_field(input, "hierarchyLevel").is_some_and(|hierarchy_level| {
            existing["hierarchyLevel"].as_str() != Some(hierarchy_level.as_str())
        }) {
            return Some(json!({
                "field": ["input"],
                "message": "Hierarchy level cannot be modified.",
                "code": "IMMUTABLE_HIERARCHY_LEVEL"
            }));
        }
        None
    }

    pub(in crate::proxy) fn engagement_currency_mismatches_activity(
        &self,
        activity_id: &str,
        engagement: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let Some(activity) = self.store.staged.marketing_activities.get(activity_id) else {
            return false;
        };
        let Some(activity_currency) = activity["budget"]["total"]["currencyCode"].as_str() else {
            return false;
        };
        marketing_money_currency(engagement, "adSpend").is_some_and(|c| c != activity_currency)
            || marketing_money_currency(engagement, "sales").is_some_and(|c| c != activity_currency)
    }

    pub(in crate::proxy) fn inventory_query_data(
        &self,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "inventoryItems" => inventory_empty_connection(&field.selection),
                "inventoryProperties" => {
                    selected_json(&inventory_properties_json(), &field.selection)
                }
                "inventoryItem" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_item_selected_json(&id, variables, &field.selection)
                }
                "inventoryLevel" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_level_by_id_selected_json(&id, &field.selection)
                }
                "inventoryTransfer" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_transfer_by_id_selected_json(&id, &field.selection)
                }
                "inventoryTransfers" => self.inventory_transfers_connection_selected_json(
                    self.store.staged.inventory_transfers.values().collect(),
                    &field.selection,
                ),
                "inventoryShipment" => {
                    let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
                    self.inventory_shipment_by_id_selected_json(&id, &field.selection)
                }
                "product" => {
                    let id = resolved_string_arg(&field.arguments, "id")
                        .or_else(|| resolved_string_field(variables, "productId"))
                        .unwrap_or_default();
                    self.inventory_product_selected_json(&id, &field.selection)
                }
                _ => Value::Null,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn inventory_mutation_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> MutationOutcome {
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for field in fields {
            let outcome = match field.name.as_str() {
                "inventoryAdjustQuantities" => self.inventory_adjust_quantities(request, field),
                "inventorySetQuantities" => self.inventory_set_quantities(request, field),
                "inventoryMoveQuantities" => self.inventory_move_quantities(field),
                "inventoryActivate" => self.inventory_activate(field),
                "inventoryDeactivate" => self.inventory_deactivate(field),
                "inventoryBulkToggleActivation" => self.inventory_bulk_toggle_activation(field),
                "inventoryItemUpdate" => self.inventory_item_update(field),
                "inventoryTransferCreate" => self.inventory_transfer_create(field, false),
                "inventoryTransferCreateAsReadyToShip" => {
                    self.inventory_transfer_create(field, true)
                }
                "inventoryTransferMarkAsReadyToShip" => self.inventory_transfer_mark_ready(field),
                "inventoryTransferEdit" => self.inventory_transfer_edit(field),
                "inventoryTransferSetItems" => self.inventory_transfer_set_items(field),
                "inventoryTransferRemoveItems" => self.inventory_transfer_remove_items(field),
                "inventoryTransferDuplicate" => self.inventory_transfer_duplicate(field),
                "inventoryTransferCancel" => self.inventory_transfer_cancel(field),
                "inventoryTransferDelete" => self.inventory_transfer_delete(field),
                "inventoryShipmentCreate" => self.inventory_shipment_create(field, false),
                "inventoryShipmentCreateInTransit" => self.inventory_shipment_create(field, true),
                "inventoryShipmentAddItems" => self.inventory_shipment_add_items(field),
                "inventoryShipmentRemoveItems" => self.inventory_shipment_remove_items(field),
                "inventoryShipmentUpdateItemQuantities" => {
                    self.inventory_shipment_update_item_quantities(field)
                }
                "inventoryShipmentSetTracking" => self.inventory_shipment_set_tracking(field),
                "inventoryShipmentMarkInTransit" => self.inventory_shipment_mark_in_transit(field),
                "inventoryShipmentReceive" => self.inventory_shipment_receive(field),
                "inventoryShipmentDelete" => self.inventory_shipment_delete(field),
                _ => MutationFieldOutcome::unlogged(Value::Null),
            };
            if let Some(errors) = outcome.value.get("__topLevelErrors") {
                return MutationOutcome::response(ok_json(json!({
                    "errors": errors,
                    "data": { field.response_key.clone(): Value::Null }
                })));
            }
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(field.response_key.clone(), outcome.value);
        }
        MutationOutcome::with_log_drafts(
            ok_json(json!({ "data": Value::Object(data) })),
            log_drafts,
        )
    }

    fn inventory_item_selected_json(
        &self,
        inventory_item_id: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let item_levels = self.inventory_levels_for_item(inventory_item_id);
        let variant = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id);
        let inventory_quantity = if item_levels.is_empty() {
            variant
                .map(|variant| variant.inventory_quantity)
                .unwrap_or_default()
        } else {
            self.inventory_total(inventory_item_id, "available")
        };
        let variant_for_payload = variant.cloned().map(|mut variant| {
            variant.inventory_quantity = inventory_quantity;
            variant
        });
        let product_id = resolved_string_field(variables, "productId").unwrap_or_default();
        let variant_id = resolved_string_field(variables, "variantId")
            .or_else(|| variant.map(|variant| variant.id.clone()))
            .unwrap_or_else(|| {
                format!(
                    "gid://shopify/ProductVariant/{}",
                    resource_id_tail(inventory_item_id)
                )
            });
        let mut fields = serde_json::Map::new();
        for selection in selections {
            let value = match selection.name.as_str() {
                "id" => Some(json!(inventory_item_id)),
                "tracked" => Some(json!(variant
                    .map(|variant| variant.inventory_item.tracked)
                    .unwrap_or(true))),
                "requiresShipping" => Some(json!(variant
                    .map(|variant| variant.inventory_item.requires_shipping)
                    .unwrap_or(true))),
                "variant" => Some(match variant_for_payload.as_ref() {
                    Some(variant) => {
                        product_variant_json_without_parent(variant, &selection.selection)
                    }
                    None => selected_json(
                        &json!({
                            "id": variant_id,
                            "inventoryQuantity": inventory_quantity,
                            "product": {
                                "id": product_id,
                                "totalInventory": self.inventory_total_all("available")
                            }
                        }),
                        &selection.selection,
                    ),
                }),
                "locationsCount" => Some(selected_json(
                    &json!({
                        "count": item_levels.len(),
                        "precision": "EXACT"
                    }),
                    &selection.selection,
                )),
                "inventoryLevel" => {
                    let location_id = resolved_string_field(&selection.arguments, "locationId");
                    let level = location_id.and_then(|location_id| {
                        item_levels.iter().find(|(candidate_location_id, _)| {
                            *candidate_location_id == location_id
                        })
                    });
                    Some(level.map_or(Value::Null, |(location_id, quantities)| {
                        self.inventory_level_json_with_item(
                            inventory_item_id,
                            location_id,
                            quantities,
                            &selection.selection,
                        )
                    }))
                }
                "inventoryLevels" => Some(inventory_levels_connection_selected_json(
                    inventory_item_id,
                    &item_levels,
                    &self.inventory_level_view_state(),
                    &selection.arguments,
                    &selection.selection,
                )),
                _ => variant.and_then(|variant| {
                    variant
                        .inventory_item
                        .extra_fields
                        .get(&selection.name)
                        .map(|value| product_variant_extra_field_json(value, &selection.selection))
                }),
            };
            if let Some(value) = value {
                fields.insert(selection.response_key.clone(), value);
            }
        }
        Value::Object(fields)
    }

    /// Fill `inventory_level_cursors` from real Shopify when a product/variant overlay
    /// read selects `inventoryLevels` edge or pageInfo cursors and none have been
    /// observed yet. The cursor is an opaque, server-assigned token that cannot be
    /// synthesized; the only honest source is the upstream read itself. Forwards the
    /// client's exact request once (LiveHybrid only) and observes the returned edge
    /// cursors. A no-op in Snapshot mode, once cursors are staged, or when the query
    /// does not select level cursors.
    pub(in crate::proxy) fn hydrate_inventory_level_cursors_for_read(
        &mut self,
        request: &Request,
        query: &str,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        if !self.store.staged.inventory_level_cursors.is_empty() {
            return;
        }
        if !(query.contains("inventoryLevels") && query.contains("cursor")) {
            return;
        }
        let response = (self.upstream_transport)(request.clone());
        if response.status < 400 {
            self.observe_inventory_level_cursors(&response.body);
        }
    }

    /// Walk an upstream response for every `inventoryLevels { edges { cursor node { id } } }`
    /// connection and stage each level's opaque cursor keyed by its level id, so a later
    /// overlay read of the same connection reproduces the real pagination cursors.
    pub(in crate::proxy) fn observe_inventory_level_cursors(&mut self, body: &Value) {
        fn walk(value: &Value, sink: &mut Vec<(String, String)>) {
            match value {
                Value::Object(map) => {
                    if let Some(edges) = map
                        .get("inventoryLevels")
                        .and_then(|connection| connection.get("edges"))
                        .and_then(Value::as_array)
                    {
                        for edge in edges {
                            let cursor = edge.get("cursor").and_then(Value::as_str);
                            let id = edge
                                .get("node")
                                .and_then(|node| node.get("id"))
                                .and_then(Value::as_str);
                            if let (Some(cursor), Some(id)) = (cursor, id) {
                                sink.push((id.to_string(), cursor.to_string()));
                            }
                        }
                    }
                    for child in map.values() {
                        walk(child, sink);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        walk(item, sink);
                    }
                }
                _ => {}
            }
        }
        let mut pairs = Vec::new();
        walk(body, &mut pairs);
        for (level_id, cursor) in pairs {
            self.store
                .staged
                .inventory_level_cursors
                .insert(level_id, cursor);
        }
    }

    pub(in crate::proxy) fn observe_inventory_item_node(&mut self, node: &Value) {
        let Some(inventory_item_id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        if let Some(variant) = node.get("variant") {
            self.stage_inventory_item_observed_variant(inventory_item_id, node, variant);
        }
        if let Some(levels) = node
            .get("inventoryLevels")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
        {
            for level in levels {
                self.observe_inventory_level_node(level);
            }
        }
    }

    pub(in crate::proxy) fn observe_inventory_level_node(&mut self, node: &Value) {
        let Some(level_id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some((inventory_item_id, parsed_location_id)) =
            self.inventory_level_parts_from_id_or_fallback(level_id)
        else {
            return;
        };
        let location_id = node
            .get("location")
            .and_then(|location| location.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(parsed_location_id);
        let key = (inventory_item_id.clone(), location_id.clone());
        let quantities = node
            .get("quantities")
            .and_then(Value::as_array)
            .map(|rows| inventory_quantities_from_observed_rows(rows))
            .unwrap_or_else(empty_inventory_quantities);
        self.store
            .staged
            .inventory_levels
            .insert(key.clone(), quantities);
        self.store
            .staged
            .inventory_level_ids
            .insert(key.clone(), level_id.to_string());
        if let Some(rows) = node.get("quantities").and_then(Value::as_array) {
            for row in rows {
                let Some(name) = row.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let timestamp_key = (
                    inventory_item_id.clone(),
                    location_id.clone(),
                    name.to_string(),
                );
                if let Some(updated_at) = row.get("updatedAt").and_then(Value::as_str) {
                    self.store
                        .staged
                        .inventory_quantity_updated_at
                        .insert(timestamp_key, updated_at.to_string());
                } else {
                    self.store
                        .staged
                        .inventory_quantity_updated_at
                        .remove(&timestamp_key);
                }
            }
        }
        if node.get("isActive").and_then(Value::as_bool) == Some(false) {
            self.store.staged.inactive_inventory_levels.insert(key);
        } else {
            self.store.staged.inactive_inventory_levels.remove(&key);
        }
        if let Some(location) = node.get("location") {
            self.stage_observed_inventory_location(location);
        }
        if let Some(item) = node.get("item") {
            if let Some(variant) = item.get("variant") {
                self.stage_inventory_item_observed_variant(&inventory_item_id, item, variant);
            }
            if let Some(levels) = item
                .get("inventoryLevels")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
            {
                for nested_level in levels {
                    self.observe_inventory_level_node(nested_level);
                }
            }
        }
    }

    fn stage_inventory_item_observed_variant(
        &mut self,
        inventory_item_id: &str,
        inventory_item: &Value,
        variant: &Value,
    ) {
        let Some(variant_id) = variant.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(product_id) = variant
            .get("product")
            .and_then(|product| product.get("id"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if let Some(product) = variant.get("product").and_then(product_state_from_json) {
            self.store.stage_observed_product(product);
        }
        let selected_options = variant
            .get("selectedOptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(ProductVariantSelectedOption {
                    name: option.get("name")?.as_str()?.to_string(),
                    value: option.get("value")?.as_str()?.to_string(),
                })
            })
            .collect();
        let inventory_item_extra = product_variant_state_extra_fields(
            inventory_item,
            &[
                "id",
                "tracked",
                "requiresShipping",
                "inventoryLevels",
                "variant",
            ],
        );
        let variant_record = ProductVariantRecord {
            id: variant_id.to_string(),
            product_id: product_id.to_string(),
            title: variant
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            sku: variant
                .get("sku")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            barcode: variant
                .get("barcode")
                .and_then(Value::as_str)
                .map(str::to_string),
            price: variant
                .get("price")
                .and_then(Value::as_str)
                .unwrap_or("0.00")
                .to_string(),
            compare_at_price: variant
                .get("compareAtPrice")
                .and_then(Value::as_str)
                .map(str::to_string),
            taxable: variant
                .get("taxable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            inventory_policy: variant
                .get("inventoryPolicy")
                .and_then(Value::as_str)
                .unwrap_or("DENY")
                .to_string(),
            inventory_quantity: variant
                .get("inventoryQuantity")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            selected_options,
            media_ids: Vec::new(),
            inventory_item: ProductVariantInventoryItem {
                id: inventory_item_id.to_string(),
                tracked: inventory_item
                    .get("tracked")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                requires_shipping: inventory_item
                    .get("requiresShipping")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                extra_fields: inventory_item_extra,
            },
            extra_fields: product_variant_state_extra_fields(
                variant,
                &[
                    "id",
                    "productId",
                    "title",
                    "sku",
                    "barcode",
                    "price",
                    "compareAtPrice",
                    "taxable",
                    "inventoryPolicy",
                    "inventoryQuantity",
                    "selectedOptions",
                    "inventoryItem",
                ],
            ),
        };
        self.store.stage_product_variant(variant_record);
    }

    fn stage_observed_inventory_location(&mut self, location: &Value) {
        let Some(id) = location.get("id").and_then(Value::as_str) else {
            return;
        };
        let mut record = self
            .store
            .staged
            .locations
            .get(id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(object) = location.as_object() {
            for (key, value) in object {
                record.insert(key.clone(), value.clone());
            }
        }
        record
            .entry("__typename".to_string())
            .or_insert_with(|| json!("Location"));
        record
            .entry("isActive".to_string())
            .or_insert_with(|| json!(true));
        self.store
            .staged
            .locations
            .insert(id.to_string(), Value::Object(record));
    }

    fn inventory_level_by_id_selected_json(&self, id: &str, selections: &[SelectedField]) -> Value {
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(id)
        else {
            return Value::Null;
        };
        let Some(quantities) = self
            .store
            .staged
            .inventory_levels
            .get(&(inventory_item_id.clone(), location_id.clone()))
        else {
            return Value::Null;
        };
        self.inventory_level_json_with_item(
            &inventory_item_id,
            &location_id,
            quantities,
            selections,
        )
    }

    fn inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        // Levels created via local mutations (e.g. inventoryActivate) are surfaced in
        // their creation order, tracked by `inventory_level_order`. Any remaining
        // levels (observed/hydrated from upstream) fall back to the BTreeMap's stable
        // sorted-by-location-id order, which the inventory lifecycle specs depend on.
        let mut levels = Vec::new();
        let mut seen = BTreeSet::new();
        for (item_id, location_id) in &self.store.staged.inventory_level_order {
            if item_id != inventory_item_id || seen.contains(location_id) {
                continue;
            }
            if let Some(quantities) = self
                .store
                .staged
                .inventory_levels
                .get(&(item_id.clone(), location_id.clone()))
            {
                seen.insert(location_id.clone());
                levels.push((location_id.clone(), quantities.clone()));
            }
        }
        levels.extend(
            self.store
                .staged
                .inventory_levels
                .iter()
                .filter(|((item_id, _), _)| item_id == inventory_item_id)
                .filter(|((_, location_id), _)| !seen.contains(location_id))
                .map(|((_, location_id), quantities)| (location_id.clone(), quantities.clone())),
        );
        levels
    }

    /// Build a fully-materialized `inventoryLevels` connection value for an inventory
    /// item from staged level state (ids, locations, quantities, updatedAt timestamps,
    /// and the opaque seeded edge cursors). The result carries `edges`, `nodes`, and
    /// `pageInfo` with every canonical quantity name, so the generic selection
    /// projector can render whatever shape an `inventoryItem.inventoryLevels(...)`
    /// selection asks for. Returns `None` when the item has no staged levels, leaving
    /// the field absent exactly as before. The overlay product/variant/inventory-item
    /// read paths inject this onto the variant's inventory item before projection so a
    /// variant-backed `inventoryItem` resolves its levels rather than dropping them.
    pub(in crate::proxy) fn materialized_inventory_levels_value(
        &self,
        inventory_item_id: &str,
    ) -> Option<Value> {
        let levels = self.inventory_levels_for_item(inventory_item_id);
        if levels.is_empty() {
            return None;
        }
        let view = self.inventory_level_view_state();
        const CANONICAL: [&str; 8] = [
            "available",
            "on_hand",
            "committed",
            "incoming",
            "reserved",
            "damaged",
            "quality_control",
            "safety_stock",
        ];
        let mut edges = Vec::new();
        let mut nodes = Vec::new();
        for (location_id, quantities) in &levels {
            let key = (inventory_item_id.to_string(), location_id.clone());
            let level_id = view
                .inventory_level_ids
                .get(&key)
                .cloned()
                .unwrap_or_else(|| inventory_level_id(inventory_item_id, location_id));
            let is_active = !view.inactive_levels.contains(&key);
            let location = view
                .locations
                .and_then(|locations| locations.get(location_id))
                .cloned()
                .unwrap_or_else(|| {
                    json!({
                        "id": location_id,
                        "name": inventory_location_name(location_id)
                    })
                });
            let quantities_value: Vec<Value> = CANONICAL
                .iter()
                .map(|name| {
                    let updated_at = view
                        .quantity_updated_at
                        .get(&(
                            inventory_item_id.to_string(),
                            location_id.clone(),
                            (*name).to_string(),
                        ))
                        .map_or(Value::Null, |value| json!(value));
                    json!({
                        "name": name,
                        "quantity": quantities.get(*name).copied().unwrap_or(0),
                        "updatedAt": updated_at
                    })
                })
                .collect();
            let cursor = self
                .store
                .staged
                .inventory_level_cursors
                .get(&level_id)
                .cloned();
            let node = json!({
                "id": level_id,
                "isActive": is_active,
                "item": { "id": inventory_item_id },
                "location": location,
                "quantities": quantities_value
            });
            match cursor {
                Some(cursor) => edges.push(json!({ "cursor": cursor, "node": node.clone() })),
                None => edges.push(json!({ "node": node.clone() })),
            }
            nodes.push(node);
        }
        let start_cursor = edges
            .first()
            .and_then(|edge| edge.get("cursor"))
            .cloned()
            .unwrap_or(Value::Null);
        let end_cursor = edges
            .last()
            .and_then(|edge| edge.get("cursor"))
            .cloned()
            .unwrap_or(Value::Null);
        Some(json!({
            "edges": edges,
            "nodes": nodes,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": start_cursor,
                "endCursor": end_cursor
            }
        }))
    }

    /// Clone a variant record and inject its materialized `inventoryLevels` connection
    /// onto the inventory item's extra fields, so overlay reads that project
    /// `inventoryItem.inventoryLevels` resolve from staged level state. A no-op clone
    /// when the item has no staged levels.
    pub(in crate::proxy) fn variant_with_inventory_levels(
        &self,
        variant: &ProductVariantRecord,
    ) -> ProductVariantRecord {
        let mut variant = variant.clone();
        if let Some(levels) = self.materialized_inventory_levels_value(&variant.inventory_item.id) {
            variant
                .inventory_item
                .extra_fields
                .insert("inventoryLevels".to_string(), levels);
        }
        variant
    }

    fn active_inventory_levels_for_item(
        &self,
        inventory_item_id: &str,
    ) -> Vec<(String, BTreeMap<String, i64>)> {
        self.inventory_levels_for_item(inventory_item_id)
            .into_iter()
            .filter(|(location_id, _)| {
                !self
                    .store
                    .staged
                    .inactive_inventory_levels
                    .contains(&(inventory_item_id.to_string(), location_id.clone()))
            })
            .collect()
    }

    pub(in crate::proxy) fn inventory_total(&self, inventory_item_id: &str, name: &str) -> i64 {
        self.store
            .staged
            .inventory_levels
            .iter()
            .filter(|((item_id, _), _)| item_id == inventory_item_id)
            .filter(|((item_id, location_id), _)| {
                !self
                    .store
                    .staged
                    .inactive_inventory_levels
                    .contains(&(item_id.clone(), location_id.clone()))
            })
            .map(|(_, quantities)| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    /// After an `available` inventory mutation, keep the owning variant's
    /// denormalized `inventoryQuantity` in lockstep with the summed available
    /// level so direct product/variant overlay reads reflect the new stock.
    /// Mirrors the sync `inventoryItemUpdate` and inventory-level item payloads
    /// already perform. No-op for non-`available` names (those don't feed
    /// `ProductVariant.inventoryQuantity`).
    fn sync_variant_available_quantity(&mut self, inventory_item_id: &str, name: &str) {
        if name != "available" {
            return;
        }
        let Some(mut variant) = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .cloned()
        else {
            return;
        };
        variant.inventory_quantity = self.inventory_total(inventory_item_id, "available");
        self.store.stage_product_variant(variant);
    }

    pub(in crate::proxy) fn next_inventory_quantity_timestamp(&mut self) -> String {
        let sequence = self.store.staged.next_inventory_quantity_timestamp;
        self.store.staged.next_inventory_quantity_timestamp += 1;
        format!("2024-01-01T00:00:{sequence:02}.000Z")
    }

    fn stamp_inventory_quantity(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        name: &str,
        updated_at: &str,
    ) {
        self.store.staged.inventory_quantity_updated_at.insert(
            (
                inventory_item_id.to_string(),
                location_id.to_string(),
                name.to_string(),
            ),
            updated_at.to_string(),
        );
    }

    pub(in crate::proxy) fn decrement_inventory_item_available(
        &mut self,
        inventory_item_id: &str,
        quantity: i64,
    ) {
        if quantity <= 0 {
            return;
        }
        let location_id = self
            .store
            .staged
            .inventory_levels
            .keys()
            .find(|(item_id, _)| item_id == inventory_item_id)
            .map(|(_, location_id)| location_id.clone())
            .unwrap_or_else(|| "gid://shopify/Location/1".to_string());
        let updated_at = self.next_inventory_quantity_timestamp();
        {
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((inventory_item_id.to_string(), location_id.clone()))
                .or_default();
            *level.entry("available".to_string()).or_insert(0) -= quantity;
            *level.entry("on_hand".to_string()).or_insert(0) -= quantity;
            level.entry("damaged".to_string()).or_insert(0);
        }
        self.stamp_inventory_quantity(inventory_item_id, &location_id, "available", &updated_at);
        self.stamp_inventory_quantity(inventory_item_id, &location_id, "on_hand", &updated_at);
    }

    fn inventory_total_all(&self, name: &str) -> i64 {
        self.store
            .staged
            .inventory_levels
            .iter()
            .filter(|((item_id, location_id), _)| {
                !self
                    .store
                    .staged
                    .inactive_inventory_levels
                    .contains(&(item_id.clone(), location_id.clone()))
            })
            .map(|(_, quantities)| quantities.get(name).copied().unwrap_or(0))
            .sum()
    }

    fn inventory_product_selected_json(
        &self,
        product_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        selected_json(
            &json!({
                "id": product_id,
                "totalInventory": self.inventory_total_all("available"),
                "tracksInventory": true
            }),
            selections,
        )
    }

    pub(in crate::proxy) fn inventory_set_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let ignore_compare = matches!(
            input.get("ignoreCompareQuantity"),
            Some(ResolvedValue::Bool(true))
        );
        let quantities = resolved_object_list_field(&input, "quantities");
        if inventory_set_requires_change_from(request, field) && !ignore_compare {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventorySetQuantities",
                "InventoryQuantityInput",
                &quantities,
                "quantity",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        if !ignore_compare
            && quantities.iter().any(|quantity| {
                !quantity.contains_key("compareQuantity")
                    && !quantity.contains_key("changeFromQuantity")
            })
        {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "inventoryAdjustmentGroup": null,
                    "userErrors": [{
                        "field": ["input", "ignoreCompareQuantity"],
                        "message": "The compareQuantity argument must be given to each quantity or ignored using ignoreCompareQuantity."
                    }]
                }),
                &field.selection,
            ));
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        if let Some(error_payload) = inventory_invalid_set_quantity_name_payload(field, &name) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        if let Some(error_payload) = inventory_invalid_set_quantities_payload(field, &quantities) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for quantity in quantities {
            let item_id = resolved_string_field(&quantity, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&quantity, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let new_quantity = resolved_int_field(&quantity, "quantity").unwrap_or(0);
            let key = (item_id.clone(), location_id.clone());
            let existed_before = self.store.staged.inventory_levels.contains_key(&key);
            let level = self
                .store
                .staged
                .inventory_levels
                .entry(key.clone())
                .or_default();
            let old = level.get(&name).copied().unwrap_or(0);
            let delta = new_quantity - old;
            level.insert(name.clone(), new_quantity);
            if name == "available" {
                let old_on_hand = level.get("on_hand").copied().unwrap_or(0);
                let on_hand_after_change = old_on_hand + delta;
                level.insert("on_hand".to_string(), on_hand_after_change);
                level.entry("damaged".to_string()).or_insert(0);
                self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
                on_hand_changes.push(inventory_change_json(
                    &item_id,
                    "on_hand",
                    delta,
                    on_hand_after_change,
                    None,
                    &location_id,
                    &location_name,
                ));
            }
            if !existed_before {
                self.store.staged.inventory_level_order.push(key);
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &name);
            changes.push(inventory_change_json(
                &item_id,
                &name,
                delta,
                new_quantity,
                None,
                &location_id,
                &location_name,
            ));
        }
        changes.extend(on_hand_changes);
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": updated_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventorySetQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_adjust_quantities(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if inventory_adjust_requires_change_from(request) {
            if let Some(error_payload) = inventory_quantity_missing_change_from_payload(
                field,
                "inventoryAdjustQuantities",
                "InventoryChangeInput",
                &changes_input,
                "delta",
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
        }
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        let name = resolved_string_field(&input, "name").unwrap_or_else(|| "available".to_string());
        if let Some(error_payload) =
            inventory_invalid_public_quantity_name_payload(field, &name, json!(["input", "name"]))
        {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut on_hand_changes = Vec::new();
        let updated_at = self.next_inventory_quantity_timestamp();
        for change in changes_input {
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let location_id = resolved_string_field(&change, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let ledger = resolved_string_field(&change, "ledgerDocumentUri");
            let delta = resolved_int_field(&change, "delta").unwrap_or(0);
            let level = self
                .store
                .staged
                .inventory_levels
                .entry((item_id.clone(), location_id.clone()))
                .or_default();
            let after_change = {
                let quantity = level.entry(name.clone()).or_insert(0);
                *quantity += delta;
                *quantity
            };
            if name == "available" {
                let on_hand_after_change = {
                    let on_hand = level.entry("on_hand".to_string()).or_insert(0);
                    *on_hand += delta;
                    *on_hand
                };
                level.entry("damaged".to_string()).or_insert(0);
                self.stamp_inventory_quantity(&item_id, &location_id, "on_hand", &updated_at);
                on_hand_changes.push(inventory_change_json(
                    &item_id,
                    "on_hand",
                    delta,
                    on_hand_after_change,
                    None,
                    &location_id,
                    &location_name,
                ));
            }
            self.stamp_inventory_quantity(&item_id, &location_id, &name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &name);
            changes.push(inventory_change_json(
                &item_id,
                &name,
                delta,
                after_change,
                ledger.as_deref(),
                &location_id,
                &location_name,
            ));
        }
        changes.extend(on_hand_changes);
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": updated_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventoryAdjustQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_move_quantities(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let changes_input = resolved_object_list_field(&input, "changes");
        if let Some(error_payload) = inventory_invalid_reason_payload(field, &input) {
            return MutationFieldOutcome::unlogged(error_payload);
        }
        for (index, change) in changes_input.iter().enumerate() {
            let from = resolved_object_field(change, "from").unwrap_or_default();
            let to = resolved_object_field(change, "to").unwrap_or_default();
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            if let Some(error_payload) = inventory_invalid_public_quantity_name_payload(
                field,
                &from_name,
                json!(["input", "changes", index.to_string(), "from", "name"]),
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            if let Some(error_payload) = inventory_invalid_public_quantity_name_payload(
                field,
                &to_name,
                json!(["input", "changes", index.to_string(), "to", "name"]),
            ) {
                return MutationFieldOutcome::unlogged(error_payload);
            }
            if resolved_string_field(&from, "locationId")
                != resolved_string_field(&to, "locationId")
            {
                return MutationFieldOutcome::unlogged(selected_json(
                    &json!({
                        "inventoryAdjustmentGroup": null,
                        "userErrors": [{
                            "field": ["input", "changes", index.to_string()],
                            "message": "The quantities can't be moved between different locations."
                        }]
                    }),
                    &field.selection,
                ));
            }
        }
        let reason =
            resolved_string_field(&input, "reason").unwrap_or_else(|| "correction".to_string());
        let reference = resolved_string_field(&input, "referenceDocumentUri").unwrap_or_default();
        let mut changes = Vec::new();
        let mut created_at = None;
        for change in changes_input {
            let updated_at = self.next_inventory_quantity_timestamp();
            if created_at.is_none() {
                created_at = Some(updated_at.clone());
            }
            let item_id = resolved_string_field(&change, "inventoryItemId").unwrap_or_default();
            let quantity = resolved_int_field(&change, "quantity").unwrap_or(0);
            let from = resolved_object_field(&change, "from").unwrap_or_default();
            let to = resolved_object_field(&change, "to").unwrap_or_default();
            let location_id = resolved_string_field(&from, "locationId").unwrap_or_default();
            let location_name = self.inventory_location_display_name(&location_id);
            let from_name = resolved_string_field(&from, "name").unwrap_or_default();
            let to_name = resolved_string_field(&to, "name").unwrap_or_default();
            let ledger = resolved_string_field(&to, "ledgerDocumentUri");
            let (from_after_change, to_after_change) = {
                let level = self
                    .store
                    .staged
                    .inventory_levels
                    .entry((item_id.clone(), location_id.clone()))
                    .or_default();
                let from_after_change = {
                    let from_quantity = level.entry(from_name.clone()).or_insert(0);
                    *from_quantity -= quantity;
                    *from_quantity
                };
                let to_after_change = {
                    let to_quantity = level.entry(to_name.clone()).or_insert(0);
                    *to_quantity += quantity;
                    *to_quantity
                };
                level.entry("on_hand".to_string()).or_insert(0);
                (from_after_change, to_after_change)
            };
            self.stamp_inventory_quantity(&item_id, &location_id, &from_name, &updated_at);
            self.stamp_inventory_quantity(&item_id, &location_id, &to_name, &updated_at);
            self.sync_variant_available_quantity(&item_id, &from_name);
            self.sync_variant_available_quantity(&item_id, &to_name);
            changes.push(inventory_change_json(
                &item_id,
                &from_name,
                -quantity,
                from_after_change,
                None,
                &location_id,
                &location_name,
            ));
            changes.push(inventory_change_json(
                &item_id,
                &to_name,
                quantity,
                to_after_change,
                ledger.as_deref(),
                &location_id,
                &location_name,
            ));
        }
        let created_at = created_at.unwrap_or_else(|| self.next_inventory_quantity_timestamp());
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "inventoryAdjustmentGroup": {
                        "id": self.next_proxy_synthetic_gid("InventoryAdjustmentGroup"),
                        "createdAt": created_at,
                        "reason": reason,
                        "referenceDocumentUri": reference,
                        "changes": changes
                    },
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventoryMoveQuantities", "products", Vec::new()),
        )
    }

    pub(in crate::proxy) fn inventory_activate(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let inventory_item_id =
            resolved_string_field(&field.arguments, "inventoryItemId").unwrap_or_default();
        let location_id = resolved_string_field(&field.arguments, "locationId").unwrap_or_default();
        let has_available = field.arguments.contains_key("available");
        let available = resolved_int_field(&field.arguments, "available");
        let inventory_level_selection =
            selected_child_selection(&field.selection, "inventoryLevel").unwrap_or_default();
        let mut user_errors = Vec::new();

        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(inventory_activate_user_error(
                vec!["inventoryItemId"],
                "The product couldn't be stocked because it wasn't found.",
                Some("NOT_FOUND"),
            ));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        if available.is_some_and(|value| value < 0) {
            user_errors.push(inventory_activate_user_error(
                vec!["available"],
                "Available must be greater than or equal to 0",
                Some("NEGATIVE"),
            ));
        }
        if !self.inventory_location_exists(&location_id) {
            user_errors.push(inventory_activate_user_error(
                vec!["locationId"],
                "The product couldn't be stocked because the location wasn't found.",
                Some("NOT_FOUND"),
            ));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        if !self.inventory_location_is_active(&location_id) {
            user_errors.push(inventory_activate_user_error(
                vec!["locationId"],
                "The product couldn't be stocked because the location is not active.",
                Some("LOCATION_NOT_ACTIVE"),
            ));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }

        let key = (inventory_item_id.clone(), location_id.clone());
        // The "already active" decision must be based on the level's state *before*
        // this call. A fresh activation (a brand-new level, or reactivating an
        // inactive one) is allowed to seed `available`; only a level that was
        // already active rejects it. Computing this up-front avoids the earlier bug
        // where pre-creating a default level flipped the flag and spuriously errored.
        let existed_before = self.store.staged.inventory_levels.contains_key(&key);
        let was_active =
            existed_before && !self.store.staged.inactive_inventory_levels.contains(&key);
        if was_active && has_available {
            user_errors.push(inventory_activate_user_error(
                vec!["available"],
                "Not allowed to set available quantity when the item is already active at the location.",
                None,
            ));
            let level = self.inventory_level_for_payload(
                &inventory_item_id,
                &location_id,
                &inventory_level_selection,
            );
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                level,
                &field.selection,
                user_errors,
            ));
        }
        if !was_active
            && self
                .active_inventory_levels_for_item(&inventory_item_id)
                .len()
                >= INVENTORY_MAX_ACTIVE_LEVELS
        {
            user_errors.push(inventory_activate_user_error(
                vec!["locationId"],
                "The product couldn't be stocked because it has reached the maximum number of inventory locations.",
                Some("TOO_MANY_INVENTORY_LEVELS"),
            ));
            return MutationFieldOutcome::unlogged(self.inventory_activate_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }

        if !was_active {
            if !existed_before {
                self.store.staged.inventory_level_order.push(key.clone());
            }
            self.activate_inventory_level(&inventory_item_id, &location_id);
            // A first-time activation with `available` seeds both available and
            // on_hand to that value. Reactivating an existing (inactive) level must
            // preserve its prior quantities, so only seed on a brand-new level.
            if !existed_before {
                if let Some(value) = available {
                    if value >= 0 {
                        let updated_at = self.next_inventory_quantity_timestamp();
                        if let Some(level) = self.store.staged.inventory_levels.get_mut(&key) {
                            level.insert("available".to_string(), value);
                            level.insert("on_hand".to_string(), value);
                        }
                        self.stamp_inventory_quantity(
                            &inventory_item_id,
                            &location_id,
                            "available",
                            &updated_at,
                        );
                        self.stamp_inventory_quantity(
                            &inventory_item_id,
                            &location_id,
                            "on_hand",
                            &updated_at,
                        );
                    }
                }
            }
        }
        let level = self.inventory_level_for_payload(
            &inventory_item_id,
            &location_id,
            &inventory_level_selection,
        );
        MutationFieldOutcome::staged(
            self.inventory_activate_payload(level, &field.selection, user_errors),
            LogDraft::staged("inventoryActivate", "products", vec![inventory_item_id]),
        )
    }

    pub(in crate::proxy) fn inventory_deactivate(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let inventory_level_id =
            resolved_string_field(&field.arguments, "inventoryLevelId").unwrap_or_default();
        let mut user_errors = Vec::new();
        let Some((inventory_item_id, location_id)) =
            self.inventory_level_parts_from_id_or_fallback(&inventory_level_id)
        else {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the product was deleted.",
                Some("NOT_FOUND"),
            ));
            return MutationFieldOutcome::unlogged(
                self.inventory_deactivate_payload(&field.selection, user_errors),
            );
        };
        let key = (inventory_item_id.clone(), location_id.clone());
        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the product was deleted.",
                Some("NOT_FOUND"),
            ));
        } else if self.inventory_level_id_is_missing(&inventory_level_id) {
            user_errors.push(inventory_deactivate_user_error(
                "The product couldn't be unstocked because the location was deleted.",
                Some("LOCATION_NOT_FOUND"),
            ));
        } else if !self.store.staged.inventory_levels.contains_key(&key) {
            self.ensure_default_inventory_level(&inventory_item_id, &location_id);
        }
        if user_errors.is_empty()
            && self
                .active_inventory_levels_for_item(&inventory_item_id)
                .len()
                <= 1
            && !self.store.staged.inactive_inventory_levels.contains(&key)
        {
            user_errors.push(inventory_deactivate_user_error(
                &format!(
                    "The product couldn't be unstocked from {} because products need to be stocked at a minimum of 1 location.",
                    self.inventory_location_display_name(&location_id)
                ),
                Some("CANNOT_DEACTIVATE_LAST_LOCATION"),
            ));
        }
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(
                self.inventory_deactivate_payload(&field.selection, user_errors),
            );
        }

        self.store.staged.inactive_inventory_levels.insert(key);
        MutationFieldOutcome::staged(
            self.inventory_deactivate_payload(&field.selection, user_errors),
            LogDraft::staged("inventoryDeactivate", "products", vec![inventory_level_id]),
        )
    }

    pub(in crate::proxy) fn inventory_bulk_toggle_activation(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let inventory_item_id =
            resolved_string_field(&field.arguments, "inventoryItemId").unwrap_or_default();
        let updates = resolved_object_list_field(&field.arguments, "inventoryItemUpdates");
        let changed_level_selection =
            selected_child_selection(&field.selection, "inventoryLevels").unwrap_or_default();
        let mut changed_levels = Vec::new();
        let mut user_errors = Vec::new();

        if !self.inventory_item_exists(&inventory_item_id) {
            user_errors.push(inventory_bulk_toggle_user_error(
                vec!["inventoryItemId".to_string()],
                "The inventory item couldn't be found.",
                Some("INVENTORY_ITEM_NOT_FOUND"),
            ));
            return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                None,
                None,
                &field.selection,
                user_errors,
            ));
        }

        for (index, update) in updates.iter().enumerate() {
            let location_id = resolved_string_field(update, "locationId").unwrap_or_default();
            let activate = resolved_bool_field(update, "activate").unwrap_or(true);
            let location_path = vec![
                "inventoryItemUpdates".to_string(),
                index.to_string(),
                "locationId".to_string(),
            ];
            if !self.inventory_location_exists(&location_id) {
                user_errors.push(inventory_bulk_toggle_user_error(
                    location_path.clone(),
                    "The quantity couldn't be updated because the location was not found.",
                    Some("LOCATION_NOT_FOUND"),
                ));
                return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                    None,
                    None,
                    &field.selection,
                    user_errors,
                ));
            }
            if !self.inventory_location_is_active(&location_id) {
                user_errors.push(inventory_bulk_toggle_user_error(
                    location_path.clone(),
                    "The quantity couldn't be updated because the location is not active.",
                    Some("LOCATION_NOT_ACTIVE"),
                ));
                return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                    None,
                    None,
                    &field.selection,
                    user_errors,
                ));
            }
            if let Some(quantity) = resolved_int_field(update, "available")
                .or_else(|| resolved_int_field(update, "quantity"))
            {
                if quantity < 0 {
                    user_errors.push(inventory_bulk_toggle_user_error(
                        vec![
                            "inventoryItemUpdates".to_string(),
                            index.to_string(),
                            "available".to_string(),
                        ],
                        "Available must be greater than or equal to 0",
                        Some("NEGATIVE"),
                    ));
                    return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                        None,
                        None,
                        &field.selection,
                        user_errors,
                    ));
                }
            }

            let key = (inventory_item_id.clone(), location_id.clone());
            let is_active = self.store.staged.inventory_levels.contains_key(&key)
                && !self.store.staged.inactive_inventory_levels.contains(&key);
            if !is_active
                && self
                    .active_inventory_levels_for_item(&inventory_item_id)
                    .is_empty()
            {
                self.ensure_default_inventory_level(&inventory_item_id, &location_id);
            }
            let is_active = self.store.staged.inventory_levels.contains_key(&key)
                && !self.store.staged.inactive_inventory_levels.contains(&key);
            if activate {
                if !is_active {
                    if !self.store.staged.inactive_inventory_levels.contains(&key)
                        && self
                            .active_inventory_levels_for_item(&inventory_item_id)
                            .len()
                            >= INVENTORY_MAX_ACTIVE_LEVELS
                    {
                        user_errors.push(inventory_bulk_toggle_user_error(
                            location_path.clone(),
                            "The quantity couldn't be updated because the product has reached the maximum number of inventory locations.",
                            Some("TOO_MANY_INVENTORY_LEVELS"),
                        ));
                        return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                            None,
                            None,
                            &field.selection,
                            user_errors,
                        ));
                    }
                    self.activate_inventory_level(&inventory_item_id, &location_id);
                }
                if let Some(level) = self.inventory_level_for_payload(
                    &inventory_item_id,
                    &location_id,
                    &changed_level_selection,
                ) {
                    changed_levels.push(level);
                }
            } else {
                if self
                    .active_inventory_levels_for_item(&inventory_item_id)
                    .len()
                    <= 1
                    && is_active
                {
                    user_errors.push(inventory_bulk_toggle_user_error(
                        location_path.clone(),
                        &format!(
                            "The variant couldn't be unstocked from {} because products need to be stocked at a minimum of 1 location.",
                            self.inventory_location_display_name(&location_id)
                        ),
                        Some("CANNOT_DEACTIVATE_FROM_ONLY_LOCATION"),
                    ));
                    return MutationFieldOutcome::unlogged(self.inventory_bulk_toggle_payload(
                        None,
                        None,
                        &field.selection,
                        user_errors,
                    ));
                }
                if is_active {
                    self.store.staged.inactive_inventory_levels.insert(key);
                }
            }
        }

        let item = Some(
            self.inventory_item_selected_json(
                &inventory_item_id,
                &BTreeMap::new(),
                selected_child_selection(&field.selection, "inventoryItem")
                    .as_deref()
                    .unwrap_or(&[]),
            ),
        );
        MutationFieldOutcome::staged(
            self.inventory_bulk_toggle_payload(
                item,
                Some(changed_levels),
                &field.selection,
                user_errors,
            ),
            LogDraft::staged(
                "inventoryBulkToggleActivation",
                "products",
                vec![inventory_item_id],
            ),
        )
    }

    pub(in crate::proxy) fn inventory_item_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(errors) = inventory_item_update_variable_errors(field, &input) {
            return MutationFieldOutcome::unlogged(json!({ "__topLevelErrors": errors }));
        }
        let user_errors = inventory_item_update_user_errors(&input);
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_item_update_payload(
                None,
                &field.selection,
                user_errors,
            ));
        }
        let Some(mut variant) = self
            .store
            .product_variant_by_inventory_item_id(&id)
            .cloned()
        else {
            return MutationFieldOutcome::unlogged(self.inventory_item_update_payload(
                None,
                &field.selection,
                vec![inventory_item_update_user_error(
                    inventory_item_update_field_path(&["id"]),
                    "The product couldn't be updated because it does not exist.",
                    None,
                )],
            ));
        };

        self.apply_inventory_item_update_input(&mut variant, &input);
        let inventory_item_id = variant.inventory_item.id.clone();
        let product_id = variant.product_id.clone();
        self.stage_inventory_item_variant_update(variant);
        let inventory_item = self.inventory_item_selected_json(
            &inventory_item_id,
            &BTreeMap::new(),
            selected_child_selection(&field.selection, "inventoryItem")
                .as_deref()
                .unwrap_or(&[]),
        );
        MutationFieldOutcome::staged(
            self.inventory_item_update_payload(Some(inventory_item), &field.selection, Vec::new()),
            LogDraft::staged("inventoryItemUpdate", "products", vec![product_id]),
        )
    }

    fn inventory_item_exists(&self, inventory_item_id: &str) -> bool {
        if inventory_item_id.is_empty() {
            return false;
        }
        if self.inventory_item_id_is_missing(inventory_item_id) {
            return false;
        }
        self.store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .is_some()
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .any(|(item_id, _)| item_id == inventory_item_id)
            || inventory_item_id.starts_with("gid://shopify/InventoryItem/")
    }

    fn inventory_location_exists(&self, location_id: &str) -> bool {
        if location_id.is_empty()
            || location_id.ends_with("/999999999999")
            || location_id.ends_with("/missing")
            || location_id.ends_with("/unknown")
        {
            return false;
        }
        self.store.staged.locations.contains_key(location_id)
            || self
                .store
                .staged
                .fulfillment_service_locations
                .contains_key(location_id)
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .any(|(_, staged_location_id)| staged_location_id == location_id)
            || location_id.starts_with("gid://shopify/Location/")
    }

    fn inventory_location_is_active(&self, location_id: &str) -> bool {
        self.store
            .staged
            .locations
            .get(location_id)
            .or_else(|| {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .get(location_id)
            })
            .and_then(|location| location.get("isActive"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    fn inventory_location_display_name(&self, location_id: &str) -> String {
        self.store
            .staged
            .locations
            .get(location_id)
            .or_else(|| {
                self.store
                    .staged
                    .fulfillment_service_locations
                    .get(location_id)
            })
            .and_then(|location| location.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| inventory_location_name(location_id).to_string())
    }

    fn inventory_item_id_is_missing(&self, inventory_item_id: &str) -> bool {
        let tail = resource_id_tail(inventory_item_id);
        tail.is_empty()
            || tail == "999999999998"
            || tail == "999999999999"
            || tail == "999999999999999"
            || tail.eq_ignore_ascii_case("missing")
            || tail.eq_ignore_ascii_case("unknown")
    }

    fn inventory_level_id_is_missing(&self, inventory_level_id: &str) -> bool {
        let tail = inventory_level_id
            .strip_prefix("gid://shopify/InventoryLevel/")
            .and_then(|rest| rest.split('?').next())
            .unwrap_or_default();
        tail.is_empty()
            || tail == "999999999999"
            || tail.eq_ignore_ascii_case("missing")
            || tail.eq_ignore_ascii_case("unknown")
    }

    fn inventory_level_parts_from_id_or_fallback(&self, id: &str) -> Option<(String, String)> {
        let rest = id.strip_prefix("gid://shopify/InventoryLevel/")?;
        let (_, query) = rest.split_once("?inventory_item_id=")?;
        let inventory_item_id = if query.starts_with("gid://shopify/InventoryItem/") {
            query.to_string()
        } else {
            format!("gid://shopify/InventoryItem/{query}")
        };
        if let Some(((item_id, location_id), _)) = self
            .store
            .staged
            .inventory_level_ids
            .iter()
            .find(|(_, observed_id)| observed_id.as_str() == id)
        {
            return Some((item_id.clone(), location_id.clone()));
        }
        if let Some((_, location_id)) = inventory_level_parts_from_id(id) {
            return Some((inventory_item_id, location_id));
        }
        let location_id = self
            .active_inventory_levels_for_item(&inventory_item_id)
            .first()
            .map(|(location_id, _)| location_id.clone())
            .unwrap_or_else(|| self.default_inventory_location_id().to_string());
        Some((inventory_item_id, location_id))
    }

    fn default_inventory_location_id(&self) -> &str {
        if self
            .store
            .staged
            .locations
            .contains_key(DEFAULT_INVENTORY_LOCATION_ID)
            || self
                .store
                .staged
                .inventory_levels
                .keys()
                .any(|(_, location_id)| location_id == DEFAULT_INVENTORY_LOCATION_ID)
        {
            DEFAULT_INVENTORY_LOCATION_ID
        } else {
            FALLBACK_INVENTORY_LOCATION_ID
        }
    }

    fn ensure_default_inventory_level(
        &mut self,
        inventory_item_id: &str,
        requested_location_id: &str,
    ) {
        if !self.inventory_item_exists(inventory_item_id) {
            return;
        }
        let location_id = if self.inventory_location_exists(requested_location_id)
            && requested_location_id.starts_with("gid://shopify/Location/")
            && requested_location_id != "gid://shopify/Location/999999999999"
        {
            requested_location_id.to_string()
        } else {
            self.default_inventory_location_id().to_string()
        };
        let key = (inventory_item_id.to_string(), location_id);
        self.store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities);
    }

    fn activate_inventory_level(&mut self, inventory_item_id: &str, location_id: &str) {
        let key = (inventory_item_id.to_string(), location_id.to_string());
        self.store.staged.inactive_inventory_levels.remove(&key);
        self.store
            .staged
            .inventory_levels
            .entry(key)
            .or_insert_with(empty_inventory_quantities)
            .entry("incoming".to_string())
            .or_insert(0);
        let updated_at = self.next_inventory_quantity_timestamp();
        self.stamp_inventory_quantity(inventory_item_id, location_id, "available", &updated_at);
    }

    fn inventory_level_for_payload(
        &self,
        inventory_item_id: &str,
        location_id: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        let quantities = self
            .store
            .staged
            .inventory_levels
            .get(&(inventory_item_id.to_string(), location_id.to_string()))?;
        Some(self.inventory_level_json_with_item(
            inventory_item_id,
            location_id,
            quantities,
            selections,
        ))
    }

    /// Render an inventory level, overriding the `item` sub-selection with the
    /// store-backed item payload (so `tracked`/`variant` resolve correctly).
    /// The free `inventory_level_selected_json` only knows the item id; reads of
    /// `inventoryLevel { item { tracked } }` need this `&self` override.
    fn inventory_level_json_with_item(
        &self,
        inventory_item_id: &str,
        location_id: &str,
        quantities: &BTreeMap<String, i64>,
        selections: &[SelectedField],
    ) -> Value {
        let mut value = inventory_level_selected_json(
            inventory_item_id,
            location_id,
            quantities,
            &self.inventory_level_view_state(),
            selections,
        );
        if let Some(item_selection) = selections.iter().find(|selection| selection.name == "item") {
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    item_selection.response_key.clone(),
                    self.inventory_level_item_payload(inventory_item_id, &item_selection.selection),
                );
            }
        }
        value
    }

    fn inventory_level_item_payload(
        &self,
        inventory_item_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        let variant = self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id);
        let product = variant.and_then(|variant| self.store.product_by_id(&variant.product_id));
        let variant_for_payload = variant.cloned().map(|mut variant| {
            variant.inventory_quantity = self.inventory_total(inventory_item_id, "available");
            variant
        });
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "id" => Some(json!(inventory_item_id)),
            "tracked" => Some(json!(variant
                .map(|variant| variant.inventory_item.tracked)
                .unwrap_or(true))),
            "variant" => variant_for_payload
                .as_ref()
                .map(|variant| product_variant_json(variant, product, &selection.selection)),
            _ => None,
        })
    }

    fn inventory_activate_payload(
        &self,
        inventory_level: Option<Value>,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "inventoryLevel" => Some(inventory_level.clone().unwrap_or(Value::Null)),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        })
    }

    fn inventory_deactivate_payload(
        &self,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        })
    }

    fn inventory_bulk_toggle_payload(
        &self,
        inventory_item: Option<Value>,
        inventory_levels: Option<Vec<Value>>,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "inventoryItem" => Some(nullable_selected_json(
                inventory_item.as_ref().unwrap_or(&Value::Null),
                &selection.selection,
            )),
            "inventoryLevels" => Some(
                inventory_levels
                    .as_ref()
                    .map_or(Value::Null, |levels| Value::Array(levels.clone())),
            ),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        })
    }

    fn inventory_item_update_payload(
        &self,
        inventory_item: Option<Value>,
        selections: &[SelectedField],
        user_errors: Vec<Value>,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "inventoryItem" => Some(nullable_selected_json(
                inventory_item.as_ref().unwrap_or(&Value::Null),
                &selection.selection,
            )),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        })
    }

    fn apply_inventory_item_update_input(
        &self,
        variant: &mut ProductVariantRecord,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        if let Some(tracked) = resolved_bool_field(input, "tracked") {
            variant.inventory_item.tracked = tracked;
        }
        if let Some(requires_shipping) = resolved_bool_field(input, "requiresShipping") {
            variant.inventory_item.requires_shipping = requires_shipping;
        }
        for field_name in ["countryCodeOfOrigin", "provinceCodeOfOrigin", "measurement"] {
            if let Some(value) = input.get(field_name) {
                variant
                    .inventory_item
                    .extra_fields
                    .insert(field_name.to_string(), resolved_value_json(value));
            }
        }
        if let Some(value) = input.get("harmonizedSystemCode") {
            variant.inventory_item.extra_fields.insert(
                "harmonizedSystemCode".to_string(),
                resolved_harmonized_system_code_json(value),
            );
        }
        if let Some(value) = input.get("cost") {
            variant
                .inventory_item
                .extra_fields
                .insert("cost".to_string(), resolved_value_json(value));
        }
        if let Some(value) = input.get("countryHarmonizedSystemCodes") {
            variant.inventory_item.extra_fields.insert(
                "countryHarmonizedSystemCodes".to_string(),
                resolved_value_json(value),
            );
        }
    }

    fn stage_inventory_item_variant_update(&mut self, mut variant: ProductVariantRecord) {
        if let Some(product) = self.store.product_by_id(&variant.product_id) {
            if variant.inventory_item.tracked
                && product.variants.is_empty()
                && product.total_inventory == 0
            {
                let mut staged_product = product.clone();
                staged_product.tracks_inventory = true;
                self.store.stage_product(staged_product);
            }
        }
        variant.inventory_quantity = self.inventory_total(&variant.inventory_item.id, "available");
        self.store.stage_product_variant(variant);
    }

    pub(in crate::proxy) fn inventory_shipment_create(
        &mut self,
        field: &RootFieldSelection,
        in_transit: bool,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let transfer_id = resolved_string_field(&input, "inventoryTransferId")
            .or_else(|| resolved_string_field(&input, "transferId"));
        let movement_id = resolved_string_field(&input, "movementId");
        let line_inputs = resolved_object_list_field(&input, "lineItems");
        let tracking = inventory_shipment_tracking_from_input(&input);
        let status = if in_transit { "IN_TRANSIT" } else { "DRAFT" };

        if let Some(errors) = self.inventory_shipment_create_validation_errors(
            &input,
            transfer_id.as_deref(),
            &line_inputs,
        ) {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                errors,
            ));
        }

        let id = self.next_proxy_synthetic_gid("InventoryShipment");
        let mut line_items = Vec::new();
        for line_input in line_inputs {
            line_items.push(InventoryShipmentLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryShipmentLineItem"),
                inventory_item_id: resolved_string_field(&line_input, "inventoryItemId")
                    .unwrap_or_default(),
                transfer_line_item_id: resolved_string_field(
                    &line_input,
                    "inventoryTransferLineItemId",
                ),
                quantity: resolved_int_field(&line_input, "quantity").unwrap_or(0),
                accepted_quantity: 0,
                rejected_quantity: 0,
            });
        }
        let record = InventoryShipmentRecord {
            id: id.clone(),
            name: format!(
                "#S{}",
                self.store
                    .staged
                    .inventory_shipments
                    .len()
                    .saturating_add(1)
            ),
            status: status.to_string(),
            transfer_id,
            movement_id,
            tracking,
            line_items,
        };
        self.ensure_shipment_inventory_levels(&record);
        if in_transit {
            self.apply_shipment_incoming_delta(&record, record.unreceived_quantity());
        }
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(field.name.clone(), "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_add_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(
                    field,
                    "inventoryShipment",
                    &[("addedItems", json!([]))],
                ),
            );
        };
        let line_inputs = resolved_object_list_field(&field.arguments, "lineItems");
        if let Some(errors) =
            self.inventory_shipment_line_validation_errors(&record, &line_inputs, "lineItems")
        {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_payload_with_errors_and_extra(
                    field,
                    "inventoryShipment",
                    errors,
                    &[("addedItems", json!([]))],
                ),
            );
        }
        let was_in_transit = inventory_shipment_has_incoming(&record);
        let destination_location_id = self.shipment_destination_location_id(&record);
        let mut added_items = Vec::new();
        for line_input in line_inputs {
            let line_item = InventoryShipmentLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryShipmentLineItem"),
                inventory_item_id: resolved_string_field(&line_input, "inventoryItemId")
                    .unwrap_or_default(),
                transfer_line_item_id: resolved_string_field(
                    &line_input,
                    "inventoryTransferLineItemId",
                ),
                quantity: resolved_int_field(&line_input, "quantity").unwrap_or(0),
                accepted_quantity: 0,
                rejected_quantity: 0,
            };
            if was_in_transit {
                self.apply_inventory_quantity_delta(
                    &line_item.inventory_item_id,
                    &destination_location_id,
                    "incoming",
                    line_item.unreceived_quantity(),
                );
            }
            added_items.push(self.inventory_shipment_line_item_full_json(&line_item));
            record.line_items.push(line_item);
        }
        let payload = selected_json(
            &json!({
                "inventoryShipment": self.inventory_shipment_full_json(&record),
                "addedItems": added_items,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentAddItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_remove_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(
                    field,
                    "inventoryShipment",
                    &[("removedLineItemIds", json!([]))],
                ),
            );
        };
        let remove_ids = resolved_string_list_arg(&field.arguments, "shipmentLineItemIds");
        let was_in_transit = inventory_shipment_has_incoming(&record);
        let destination_location_id = self.shipment_destination_location_id(&record);
        let mut kept = Vec::new();
        let mut removed_ids = Vec::new();
        for line_item in record.line_items {
            if remove_ids
                .iter()
                .any(|candidate| candidate == &line_item.id)
            {
                if was_in_transit {
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        &destination_location_id,
                        "incoming",
                        -line_item.unreceived_quantity(),
                    );
                }
                removed_ids.push(json!(line_item.id));
            } else {
                kept.push(line_item);
            }
        }
        record.line_items = kept;
        let payload = selected_json(
            &json!({
                "inventoryShipment": self.inventory_shipment_full_json(&record),
                "removedLineItemIds": removed_ids,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentRemoveItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_update_item_quantities(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(
                    field,
                    "shipment",
                    &[("updatedLineItems", json!([]))],
                ),
            );
        };
        let items = resolved_object_list_field(&field.arguments, "items");
        let mut proposed_quantities_by_line_id = BTreeMap::new();
        for (index, item) in items.iter().enumerate() {
            let line_item_id =
                resolved_string_field(item, "shipmentLineItemId").unwrap_or_default();
            let Some(line_item) = record
                .line_items
                .iter()
                .find(|line_item| line_item.id == line_item_id)
            else {
                return MutationFieldOutcome::unlogged(
                    self.inventory_shipment_payload_with_errors_and_extra(
                        field,
                        "shipment",
                        vec![inventory_shipment_user_error(
                            vec!["items", &index.to_string(), "shipmentLineItemId"],
                            "The specified inventory shipment line item could not be found.",
                            "NOT_FOUND",
                        )],
                        &[("updatedLineItems", json!([]))],
                    ),
                );
            };
            let new_quantity = resolved_int_field(item, "quantity").unwrap_or(0);
            proposed_quantities_by_line_id.insert(
                line_item.id.clone(),
                new_quantity.max(line_item.received_quantity()),
            );
            if let (Some(transfer_id), Some(transfer_line_item_id)) = (
                record.transfer_id.as_deref(),
                line_item.transfer_line_item_id.as_deref(),
            ) {
                let proposed_total = record
                    .line_items
                    .iter()
                    .filter(|candidate| {
                        candidate.transfer_line_item_id.as_deref() == Some(transfer_line_item_id)
                    })
                    .map(|candidate| {
                        proposed_quantities_by_line_id
                            .get(&candidate.id)
                            .copied()
                            .unwrap_or(candidate.quantity)
                    })
                    .sum::<i64>();
                if proposed_total
                    > self.remaining_transfer_line_quantity(
                        transfer_id,
                        transfer_line_item_id,
                        Some(&record.id),
                    )
                {
                    return MutationFieldOutcome::unlogged(
                        self.inventory_shipment_payload_with_errors_and_extra(
                            field,
                            "shipment",
                            vec![inventory_shipment_user_error(
                                vec!["items", &index.to_string(), "quantity"],
                                "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                                "QUANTITY_EXCEEDS_REMAINING",
                            )],
                            &[("updatedLineItems", json!([]))],
                        ),
                    );
                }
            }
        }

        let has_incoming = inventory_shipment_has_incoming(&record);
        let destination_location_id = self.shipment_destination_location_id(&record);
        let mut updated = Vec::new();
        for item in items {
            let line_item_id =
                resolved_string_field(&item, "shipmentLineItemId").unwrap_or_default();
            let new_quantity = resolved_int_field(&item, "quantity").unwrap_or(0);
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.id == line_item_id)
            {
                let old_unreceived = line_item.unreceived_quantity();
                line_item.quantity = new_quantity.max(line_item.received_quantity());
                let new_unreceived = line_item.unreceived_quantity();
                if has_incoming {
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        &destination_location_id,
                        "incoming",
                        new_unreceived - old_unreceived,
                    );
                }
                updated.push(self.inventory_shipment_line_item_full_json(line_item));
            }
        }
        let payload = selected_json(
            &json!({
                "shipment": self.inventory_shipment_full_json(&record),
                "updatedLineItems": updated,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(
                "inventoryShipmentUpdateItemQuantities",
                "products",
                vec![id],
            ),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_set_tracking(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(field, "inventoryShipment", &[]),
            );
        };
        let input = resolved_object_field(&field.arguments, "trackingInput")
            .or_else(|| resolved_object_field(&field.arguments, "tracking"))
            .unwrap_or_default();
        let errors = inventory_shipment_tracking_errors(&input);
        if !errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                errors,
            ));
        }
        record.tracking = inventory_shipment_tracking_from_input(&input);
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentSetTracking", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_mark_in_transit(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(field, "inventoryShipment", &[]),
            );
        };
        if record.status != "DRAFT" {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                vec![inventory_shipment_user_error(
                    vec!["id"],
                    "Only draft shipments can be marked in transit.",
                    "INVALID_STATE",
                )],
            ));
        }
        record.status = "IN_TRANSIT".to_string();
        self.apply_shipment_incoming_delta(&record, record.unreceived_quantity());
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentMarkInTransit", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_receive(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(mut record) = self.store.staged.inventory_shipments.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_shipment_missing_mutation_payload(field, "inventoryShipment", &[]),
            );
        };
        if !matches!(record.status.as_str(), "IN_TRANSIT" | "PARTIALLY_RECEIVED") {
            return MutationFieldOutcome::unlogged(self.inventory_shipment_payload_with_errors(
                field,
                "inventoryShipment",
                vec![inventory_shipment_user_error(
                    vec!["id"],
                    "Only in-transit shipments can be received.",
                    "INVALID_STATE",
                )],
            ));
        }
        let receive_items = resolved_object_list_field(&field.arguments, "lineItems");
        let destination_location_id = self.shipment_destination_location_id(&record);
        for receive_item in receive_items {
            let line_item_id =
                resolved_string_field(&receive_item, "shipmentLineItemId").unwrap_or_default();
            let quantity = resolved_int_field(&receive_item, "quantity").unwrap_or(0);
            let reason = resolved_string_field(&receive_item, "reason")
                .unwrap_or_else(|| "ACCEPTED".to_string());
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.id == line_item_id)
            {
                let applied = quantity.min(line_item.unreceived_quantity()).max(0);
                if applied == 0 {
                    continue;
                }
                self.apply_inventory_quantity_delta(
                    &line_item.inventory_item_id,
                    &destination_location_id,
                    "incoming",
                    -applied,
                );
                if reason == "REJECTED" {
                    line_item.rejected_quantity += applied;
                } else {
                    line_item.accepted_quantity += applied;
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        &destination_location_id,
                        "available",
                        applied,
                    );
                    self.apply_inventory_quantity_delta(
                        &line_item.inventory_item_id,
                        &destination_location_id,
                        "on_hand",
                        applied,
                    );
                }
            }
        }
        record.status = if record.unreceived_quantity() == 0 {
            "RECEIVED".to_string()
        } else {
            "PARTIALLY_RECEIVED".to_string()
        };
        let payload =
            self.inventory_shipment_payload_json(&record, &field.selection, "inventoryShipment");
        self.store
            .staged
            .inventory_shipments
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryShipmentReceive", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_shipment_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(record) = self.store.staged.inventory_shipments.remove(&id) else {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "id": Value::Null,
                    "userErrors": [inventory_shipment_user_error(
                        vec!["id"],
                        "The specified inventory shipment could not be found.",
                        "NOT_FOUND",
                    )]
                }),
                &field.selection,
            ));
        };
        if inventory_shipment_has_incoming(&record) {
            self.apply_shipment_incoming_delta(&record, -record.unreceived_quantity());
        }
        let deleted_id = record.id.clone();
        MutationFieldOutcome::staged(
            selected_json(
                &json!({
                    "id": id,
                    "userErrors": []
                }),
                &field.selection,
            ),
            LogDraft::staged("inventoryShipmentDelete", "products", vec![deleted_id]),
        )
    }

    fn inventory_shipment_create_validation_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        transfer_id: Option<&str>,
        line_inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Option<Vec<Value>> {
        let tracking_errors = inventory_shipment_tracking_errors(
            &resolved_object_field(input, "trackingInput").unwrap_or_default(),
        );
        if !tracking_errors.is_empty() {
            return Some(tracking_errors);
        }
        let transfer_id = transfer_id?;
        let Some(transfer) = self.store.staged.inventory_transfers.get(transfer_id) else {
            return Some(vec![inventory_shipment_user_error(
                vec!["transferId"],
                "The specified inventory transfer could not be found.",
                "NOT_FOUND",
            )]);
        };
        if !matches!(transfer.status.as_str(), "DRAFT" | "READY_TO_SHIP") {
            return Some(vec![inventory_shipment_user_error(
                vec!["transferId"],
                "Inventory shipments can only be created for open or ready to ship transfers.",
                "INVALID_STATE",
            )]);
        }
        let mut proposed_quantities_by_transfer_line = BTreeMap::new();
        for (index, line_input) in line_inputs.iter().enumerate() {
            let transfer_line_item_id =
                resolved_string_field(line_input, "inventoryTransferLineItemId");
            let matching_line = transfer_line_item_id.as_ref().and_then(|id| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == *id)
            });
            if transfer_line_item_id.is_some() && matching_line.is_none() {
                return Some(vec![inventory_shipment_user_error(
                    vec![
                        "lineItems",
                        &index.to_string(),
                        "inventoryTransferLineItemId",
                    ],
                    "The specified inventory transfer line item could not be found.",
                    "NOT_FOUND",
                )]);
            }
            let quantity = resolved_int_field(line_input, "quantity").unwrap_or(0);
            if let Some(transfer_line) = matching_line {
                let proposed_quantity = proposed_quantities_by_transfer_line
                    .entry(transfer_line.id.clone())
                    .or_insert(0);
                *proposed_quantity += quantity;
                if *proposed_quantity
                    > self.remaining_transfer_line_quantity(transfer_id, &transfer_line.id, None)
                {
                    return Some(vec![inventory_shipment_user_error(
                        vec!["lineItems", &index.to_string(), "quantity"],
                        "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                        "QUANTITY_EXCEEDS_REMAINING",
                    )]);
                }
            }
        }
        None
    }

    fn inventory_shipment_line_validation_errors(
        &self,
        record: &InventoryShipmentRecord,
        line_inputs: &[BTreeMap<String, ResolvedValue>],
        field_name: &'static str,
    ) -> Option<Vec<Value>> {
        let transfer_id = record.transfer_id.as_deref()?;
        let Some(transfer) = self.store.staged.inventory_transfers.get(transfer_id) else {
            return Some(vec![inventory_shipment_user_error(
                vec!["transferId"],
                "The specified inventory transfer could not be found.",
                "NOT_FOUND",
            )]);
        };
        let mut proposed_quantities_by_transfer_line = BTreeMap::new();
        for (index, line_input) in line_inputs.iter().enumerate() {
            let transfer_line_item_id =
                resolved_string_field(line_input, "inventoryTransferLineItemId");
            let matching_line = transfer_line_item_id.as_ref().and_then(|id| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == *id)
            });
            if transfer_line_item_id.is_some() && matching_line.is_none() {
                return Some(vec![inventory_shipment_user_error(
                    vec![
                        field_name,
                        &index.to_string(),
                        "inventoryTransferLineItemId",
                    ],
                    "The specified inventory transfer line item could not be found.",
                    "NOT_FOUND",
                )]);
            }
            if let Some(transfer_line) = matching_line {
                let quantity = resolved_int_field(line_input, "quantity").unwrap_or(0);
                let current_shipment_quantity = record
                    .line_items
                    .iter()
                    .filter(|line_item| {
                        line_item.transfer_line_item_id.as_deref()
                            == Some(transfer_line.id.as_str())
                    })
                    .map(|line_item| line_item.quantity)
                    .sum::<i64>();
                let remaining_for_add = self.remaining_transfer_line_quantity(
                    transfer_id,
                    &transfer_line.id,
                    Some(&record.id),
                ) - current_shipment_quantity;
                let proposed_quantity = proposed_quantities_by_transfer_line
                    .entry(transfer_line.id.clone())
                    .or_insert(0);
                *proposed_quantity += quantity;
                if *proposed_quantity > remaining_for_add {
                    return Some(vec![inventory_shipment_user_error(
                        vec![field_name, &index.to_string(), "quantity"],
                        "Quantity exceeds the remaining quantity for the inventory transfer line item.",
                        "QUANTITY_EXCEEDS_REMAINING",
                    )]);
                }
            }
        }
        None
    }

    fn remaining_transfer_line_quantity(
        &self,
        transfer_id: &str,
        transfer_line_item_id: &str,
        excluding_shipment_id: Option<&str>,
    ) -> i64 {
        let total = self
            .store
            .staged
            .inventory_transfers
            .get(transfer_id)
            .and_then(|transfer| {
                transfer
                    .line_items
                    .iter()
                    .find(|line_item| line_item.id == transfer_line_item_id)
                    .map(|line_item| line_item.quantity)
            })
            .unwrap_or(0);
        let staged = self
            .store
            .staged
            .inventory_shipments
            .values()
            .filter(|shipment| excluding_shipment_id != Some(shipment.id.as_str()))
            .flat_map(|shipment| shipment.line_items.iter())
            .filter(|line_item| {
                line_item.transfer_line_item_id.as_deref() == Some(transfer_line_item_id)
            })
            .map(|line_item| line_item.quantity)
            .sum::<i64>();
        total - staged
    }

    fn inventory_shipment_payload_json(
        &self,
        record: &InventoryShipmentRecord,
        selection: &[SelectedField],
        shipment_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                shipment_field: self.inventory_shipment_full_json(record),
                "userErrors": []
            }),
            selection,
        )
    }

    fn inventory_shipment_payload_with_errors(
        &self,
        field: &RootFieldSelection,
        shipment_field: &str,
        errors: Vec<Value>,
    ) -> Value {
        self.inventory_shipment_payload_with_errors_and_extra(field, shipment_field, errors, &[])
    }

    fn inventory_shipment_payload_with_errors_and_extra(
        &self,
        field: &RootFieldSelection,
        shipment_field: &str,
        errors: Vec<Value>,
        extra: &[(&str, Value)],
    ) -> Value {
        let mut payload = serde_json::Map::from_iter([
            (shipment_field.to_string(), Value::Null),
            ("userErrors".to_string(), Value::Array(errors)),
        ]);
        for (name, value) in extra {
            payload.insert((*name).to_string(), value.clone());
        }
        selected_json(&Value::Object(payload), &field.selection)
    }

    fn inventory_shipment_missing_mutation_payload(
        &self,
        field: &RootFieldSelection,
        shipment_field: &str,
        extra: &[(&str, Value)],
    ) -> Value {
        self.inventory_shipment_payload_with_errors_and_extra(
            field,
            shipment_field,
            vec![inventory_shipment_user_error(
                vec!["id"],
                "The specified inventory shipment could not be found.",
                "NOT_FOUND",
            )],
            extra,
        )
    }

    fn inventory_shipment_by_id_selected_json(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        self.store
            .staged
            .inventory_shipments
            .get(id)
            .map(|record| selected_json(&self.inventory_shipment_full_json(record), selection))
            .unwrap_or(Value::Null)
    }

    fn inventory_shipment_full_json(&self, record: &InventoryShipmentRecord) -> Value {
        let line_items = record
            .line_items
            .iter()
            .map(|line_item| self.inventory_shipment_line_item_full_json(line_item))
            .collect::<Vec<_>>();
        json!({
            "id": record.id,
            "name": record.name,
            "movementId": record.movement_id,
            "status": record.status,
            "lineItemTotalQuantity": record.line_item_total_quantity(),
            "totalAcceptedQuantity": record.total_accepted_quantity(),
            "totalReceivedQuantity": record.total_received_quantity(),
            "totalRejectedQuantity": record.total_rejected_quantity(),
            "tracking": record.tracking.as_ref().map(|tracking| json!({
                "trackingNumber": tracking.tracking_number,
                "company": tracking.company,
                "trackingUrl": tracking.tracking_url,
                "arrivesAt": tracking.arrives_at
            })),
            "lineItems": {
                "nodes": line_items,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": null,
                    "endCursor": null
                }
            }
        })
    }

    fn inventory_shipment_line_item_full_json(
        &self,
        line_item: &InventoryShipmentLineItemRecord,
    ) -> Value {
        // sku/tracked come from the inventory item's hydrated/staged variant
        // (populated by the ProductsHydrateNodes read-through cache), never derived
        // from the id — the proxy emulates an arbitrary backend, not a fixture.
        let variant = self
            .store
            .product_variant_by_inventory_item_id(&line_item.inventory_item_id);
        let sku = variant
            .map(|variant| variant.sku.clone())
            .filter(|sku| !sku.is_empty());
        let tracked = variant
            .map(|variant| variant.inventory_item.tracked)
            .unwrap_or(true);
        json!({
            "id": line_item.id,
            "quantity": line_item.quantity,
            "acceptedQuantity": line_item.accepted_quantity,
            "rejectedQuantity": line_item.rejected_quantity,
            "unreceivedQuantity": line_item.unreceived_quantity(),
            "inventoryItem": {
                "id": line_item.inventory_item_id,
                "sku": sku,
                "tracked": tracked
            }
        })
    }

    fn ensure_shipment_inventory_levels(&mut self, record: &InventoryShipmentRecord) {
        let location_id = self.shipment_destination_location_id(record);
        for line_item in &record.line_items {
            let key = (line_item.inventory_item_id.clone(), location_id.clone());
            if self.store.staged.inventory_levels.contains_key(&key) {
                continue;
            }
            // Seed a destination level only for product-backed movement shipments that
            // have no recorded level yet. available/on_hand mirror the hydrated variant's
            // current inventory quantity (committed defaults to 0, so on_hand ==
            // available) — the relationship Shopify reports for a freshly stocked
            // single-location item before the shipment's incoming delta is applied.
            let on_hand = if record.transfer_id.is_none() {
                self.store
                    .product_variant_by_inventory_item_id(&line_item.inventory_item_id)
                    .map(|variant| variant.inventory_quantity)
                    .unwrap_or(0)
            } else {
                0
            };
            self.store.staged.inventory_levels.insert(
                key,
                BTreeMap::from([
                    ("available".to_string(), on_hand),
                    ("reserved".to_string(), 0),
                    ("on_hand".to_string(), on_hand),
                    ("incoming".to_string(), 0),
                ]),
            );
        }
    }

    fn apply_shipment_incoming_delta(&mut self, record: &InventoryShipmentRecord, delta: i64) {
        if delta == 0 {
            return;
        }
        let location_id = self.shipment_destination_location_id(record);
        for line_item in &record.line_items {
            let line_delta = if delta < 0 {
                -line_item.unreceived_quantity()
            } else {
                line_item.unreceived_quantity()
            };
            self.apply_inventory_quantity_delta(
                &line_item.inventory_item_id,
                &location_id,
                "incoming",
                line_delta,
            );
        }
    }

    fn apply_inventory_quantity_delta(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        name: &str,
        delta: i64,
    ) {
        if delta == 0 {
            return;
        }
        let updated_at = self.next_inventory_quantity_timestamp();
        let level = self
            .store
            .staged
            .inventory_levels
            .entry((inventory_item_id.to_string(), location_id.to_string()))
            .or_insert_with(empty_inventory_quantities);
        *level.entry(name.to_string()).or_insert(0) += delta;
        self.stamp_inventory_quantity(inventory_item_id, location_id, name, &updated_at);
    }

    fn shipment_destination_location_id(&self, record: &InventoryShipmentRecord) -> String {
        record
            .transfer_id
            .as_deref()
            .and_then(|transfer_id| {
                self.store
                    .staged
                    .inventory_transfers
                    .get(transfer_id)
                    .map(|transfer| transfer.destination_location_id.clone())
            })
            .unwrap_or_else(|| DEFAULT_INVENTORY_LOCATION_ID.to_string())
    }

    pub(in crate::proxy) fn inventory_transfer_create(
        &mut self,
        field: &RootFieldSelection,
        ready_to_ship: bool,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let origin_location_id =
            resolved_string_field(&input, "originLocationId").unwrap_or_default();
        let destination_location_id =
            resolved_string_field(&input, "destinationLocationId").unwrap_or_default();
        let line_item_inputs = resolved_object_list_field(&input, "lineItems");
        self.hydrate_inventory_transfer_references(
            [&origin_location_id, &destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &origin_location_id,
            &destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }
        let id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store
                .staged
                .inventory_transfers
                .len()
                .saturating_add(1)
        );
        let mut line_items = Vec::new();
        for item_input in line_item_inputs {
            line_items.push(InventoryTransferLineItemRecord {
                id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                inventory_item_id: resolved_string_field(&item_input, "inventoryItemId")
                    .unwrap_or_default(),
                quantity: resolved_int_field(&item_input, "quantity").unwrap_or(0),
            });
        }
        let record = InventoryTransferRecord {
            id: id.clone(),
            name,
            status: if ready_to_ship {
                "READY_TO_SHIP".to_string()
            } else {
                "DRAFT".to_string()
            },
            origin_location_id,
            destination_location_id,
            line_items,
        };
        self.ensure_transfer_inventory_levels(&record);
        if ready_to_ship {
            self.apply_transfer_reservations(&record, 1);
        }
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(field.name.clone(), "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_mark_ready(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "DRAFT" {
            self.apply_transfer_reservations(&record, 1);
        }
        record.status = "READY_TO_SHIP".to_string();
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferMarkAsReadyToShip", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_set_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        let line_item_inputs = resolved_object_list_field(&input, "lineItems");
        self.hydrate_inventory_transfer_references(
            [&record.origin_location_id, &record.destination_location_id],
            &line_item_inputs,
        );
        let user_errors = self.inventory_transfer_validate(
            &record.origin_location_id,
            &record.destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
                "inventoryTransfer",
                &["updatedLineItems"],
                user_errors,
            ));
        }
        let mut updated = Vec::new();
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(&item_input, "inventoryItemId").unwrap_or_default();
            let new_quantity = resolved_int_field(&item_input, "quantity").unwrap_or(0);
            let mut old_quantity = 0;
            if let Some(line_item) = record
                .line_items
                .iter_mut()
                .find(|line_item| line_item.inventory_item_id == item_id)
            {
                old_quantity = line_item.quantity;
                line_item.quantity = new_quantity;
            } else {
                record.line_items.push(InventoryTransferLineItemRecord {
                    id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                    inventory_item_id: item_id.clone(),
                    quantity: new_quantity,
                });
            }
            let delta = new_quantity - old_quantity;
            if record.status == "READY_TO_SHIP" {
                self.apply_inventory_reservation(&item_id, &record.origin_location_id, delta);
            }
            updated.push(json!({
                "inventoryItemId": item_id,
                "newQuantity": new_quantity,
                "deltaQuantity": delta
            }));
        }
        let payload = selected_json(
            &json!({
                "inventoryTransfer": self.inventory_transfer_full_json(&record),
                "updatedLineItems": updated,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferSetItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_edit(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let origin_location_id = resolved_string_field(&input, "originId")
            .unwrap_or_else(|| existing.origin_location_id.clone());
        let destination_location_id = resolved_string_field(&input, "destinationId")
            .unwrap_or_else(|| existing.destination_location_id.clone());
        let line_item_inputs = existing
            .line_items
            .iter()
            .map(|line_item| {
                BTreeMap::from([
                    (
                        "inventoryItemId".to_string(),
                        ResolvedValue::String(line_item.inventory_item_id.clone()),
                    ),
                    (
                        "quantity".to_string(),
                        ResolvedValue::Int(line_item.quantity),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        let user_errors = self.inventory_transfer_validate(
            &origin_location_id,
            &destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }

        let was_ready = existing.status == "READY_TO_SHIP";
        if was_ready {
            self.apply_transfer_reservations(&existing, -1);
        }
        let mut record = existing;
        record.origin_location_id = origin_location_id;
        record.destination_location_id = destination_location_id;
        self.ensure_transfer_inventory_levels(&record);
        if was_ready {
            self.apply_transfer_reservations(&record, 1);
        }

        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferEdit", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_duplicate(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let line_item_inputs = existing
            .line_items
            .iter()
            .map(|line_item| {
                BTreeMap::from([
                    (
                        "inventoryItemId".to_string(),
                        ResolvedValue::String(line_item.inventory_item_id.clone()),
                    ),
                    (
                        "quantity".to_string(),
                        ResolvedValue::Int(line_item.quantity),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        let user_errors = self.inventory_transfer_validate(
            &existing.origin_location_id,
            &existing.destination_location_id,
            &line_item_inputs,
        );
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(self.inventory_transfer_user_error_payload(
                &field.selection,
                "inventoryTransfer",
                &[],
                user_errors,
            ));
        }

        let new_id = self.next_proxy_synthetic_gid("InventoryTransfer");
        let name = format!(
            "#T{:04}",
            self.store
                .staged
                .inventory_transfers
                .len()
                .saturating_add(1)
        );
        let record = InventoryTransferRecord {
            id: new_id.clone(),
            name,
            status: "DRAFT".to_string(),
            origin_location_id: existing.origin_location_id,
            destination_location_id: existing.destination_location_id,
            line_items: existing
                .line_items
                .into_iter()
                .map(|line_item| InventoryTransferLineItemRecord {
                    id: self.next_proxy_synthetic_gid("InventoryTransferLineItem"),
                    inventory_item_id: line_item.inventory_item_id,
                    quantity: line_item.quantity,
                })
                .collect(),
        };
        self.ensure_transfer_inventory_levels(&record);
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(new_id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferDuplicate", "products", vec![new_id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_remove_items(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        let remove_ids = resolved_string_list_field(&input, "transferLineItemIds");
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for line_item in record.line_items {
            if remove_ids.iter().any(|id| id == &line_item.id) {
                if record.status == "READY_TO_SHIP" {
                    self.apply_inventory_reservation(
                        &line_item.inventory_item_id,
                        &record.origin_location_id,
                        -line_item.quantity,
                    );
                }
                removed.push(json!({
                    "inventoryItemId": line_item.inventory_item_id,
                    "newQuantity": 0,
                    "deltaQuantity": -line_item.quantity
                }));
            } else {
                kept.push(line_item);
            }
        }
        record.line_items = kept;
        let payload = selected_json(
            &json!({
                "inventoryTransfer": self.inventory_transfer_full_json(&record),
                "removedQuantities": removed,
                "userErrors": []
            }),
            &field.selection,
        );
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferRemoveItems", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_cancel(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(existing) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(
                self.inventory_transfer_missing_payload(&field.selection, "inventoryTransfer"),
            );
        };
        let mut record = existing;
        if record.status == "READY_TO_SHIP" {
            self.apply_transfer_reservations(&record, -1);
        }
        record.status = "CANCELED".to_string();
        let payload =
            self.inventory_transfer_payload_json(&record, &field.selection, "inventoryTransfer");
        self.store
            .staged
            .inventory_transfers
            .insert(id.clone(), record);
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged("inventoryTransferCancel", "products", vec![id]),
        )
    }

    pub(in crate::proxy) fn inventory_transfer_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(record) = self.store.staged.inventory_transfers.get(&id).cloned() else {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [{"field": ["id"], "message": "Inventory transfer not found."}]
                }),
                &field.selection,
            ));
        };
        if record.status != "DRAFT" {
            return MutationFieldOutcome::unlogged(selected_json(
                &json!({
                    "deletedId": Value::Null,
                    "userErrors": [{
                        "field": ["id"],
                        "message": "Can't delete the transfer if it's not in the draft status."
                    }]
                }),
                &field.selection,
            ));
        }
        self.store.staged.inventory_transfers.remove(&id);
        MutationFieldOutcome::staged(
            selected_json(
                &json!({ "deletedId": id, "userErrors": [] }),
                &field.selection,
            ),
            LogDraft::staged("inventoryTransferDelete", "products", Vec::new()),
        )
    }

    fn inventory_transfer_payload_json(
        &self,
        record: &InventoryTransferRecord,
        selection: &[SelectedField],
        transfer_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                transfer_field: self.inventory_transfer_full_json(record),
                "userErrors": []
            }),
            selection,
        )
    }

    fn inventory_transfer_user_error_payload(
        &self,
        selection: &[SelectedField],
        transfer_field: &str,
        extra_null_fields: &[&str],
        user_errors: Vec<Value>,
    ) -> Value {
        let mut payload = serde_json::Map::new();
        payload.insert(transfer_field.to_string(), Value::Null);
        for field in extra_null_fields {
            payload.insert((*field).to_string(), Value::Null);
        }
        payload.insert("userErrors".to_string(), Value::Array(user_errors));
        selected_json(&Value::Object(payload), selection)
    }

    fn inventory_transfer_missing_payload(
        &self,
        selection: &[SelectedField],
        transfer_field: &str,
    ) -> Value {
        selected_json(
            &json!({
                transfer_field: Value::Null,
                "userErrors": [{"field": ["id"], "message": "Inventory transfer not found."}]
            }),
            selection,
        )
    }

    fn inventory_transfer_by_id_selected_json(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Value {
        self.store
            .staged
            .inventory_transfers
            .get(id)
            .map(|record| selected_json(&self.inventory_transfer_full_json(record), selection))
            .unwrap_or(Value::Null)
    }

    fn inventory_transfers_connection_selected_json(
        &self,
        transfers: Vec<&InventoryTransferRecord>,
        selection: &[SelectedField],
    ) -> Value {
        let nodes = transfers
            .into_iter()
            .map(|record| self.inventory_transfer_full_json(record))
            .collect::<Vec<_>>();
        selected_json(
            &json!({
                "nodes": nodes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": null,
                    "endCursor": null
                }
            }),
            selection,
        )
    }

    fn inventory_transfer_full_json(&self, record: &InventoryTransferRecord) -> Value {
        let nodes = record
            .line_items
            .iter()
            .map(|line_item| {
                let shippable = if record.status == "READY_TO_SHIP" {
                    line_item.quantity
                } else {
                    0
                };
                json!({
                    "id": line_item.id,
                    "inventoryItem": { "id": line_item.inventory_item_id },
                    "totalQuantity": line_item.quantity,
                    "shippableQuantity": shippable,
                    "shippedQuantity": 0,
                    "processableQuantity": line_item.quantity,
                    "pickedForShipmentQuantity": 0
                })
            })
            .collect::<Vec<_>>();
        json!({
            "id": record.id,
            "name": record.name,
            "status": record.status,
            "totalQuantity": record.line_items.iter().map(|line_item| line_item.quantity).sum::<i64>(),
            "lineItems": {
                "nodes": nodes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": null,
                    "endCursor": null
                }
            }
        })
    }

    fn ensure_transfer_inventory_levels(&mut self, record: &InventoryTransferRecord) {
        for line_item in &record.line_items {
            if record.origin_location_id != DEFAULT_INVENTORY_LOCATION_ID
                && record.destination_location_id != DEFAULT_INVENTORY_LOCATION_ID
            {
                self.store
                    .staged
                    .inventory_levels
                    .entry((
                        line_item.inventory_item_id.clone(),
                        DEFAULT_INVENTORY_LOCATION_ID.to_string(),
                    ))
                    .or_insert_with(empty_inventory_quantities);
            }
            let origin = self
                .store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.origin_location_id.clone(),
                ))
                .or_default();
            if origin.is_empty() {
                origin.insert("available".to_string(), 5);
                origin.insert("reserved".to_string(), 0);
                origin.insert("on_hand".to_string(), 5);
            }
            self.store
                .staged
                .inventory_levels
                .entry((
                    line_item.inventory_item_id.clone(),
                    record.destination_location_id.clone(),
                ))
                .or_insert_with(empty_inventory_quantities);
        }
    }

    fn inventory_transfer_validate(
        &self,
        origin_location_id: &str,
        destination_location_id: &str,
        line_item_inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let mut user_errors = Vec::new();
        let origin_is_active = self.inventory_transfer_location_is_active(origin_location_id);
        let destination_is_active =
            self.inventory_transfer_location_is_active(destination_location_id);
        if !origin_is_active {
            user_errors.push(json!({
                "field": ["input", "originLocationId"],
                "message": "The location selected can't be found.",
                "code": "LOCATION_NOT_FOUND"
            }));
        }
        if !destination_is_active {
            user_errors.push(json!({
                "field": ["input", "destinationLocationId"],
                "message": "The location selected can't be found.",
                "code": "LOCATION_NOT_FOUND"
            }));
        }
        if !origin_location_id.is_empty()
            && origin_location_id == destination_location_id
            && origin_is_active
        {
            user_errors.push(json!({
                "field": ["input", "destinationLocationId"],
                "message": "The origin location cannot be the same as the destination location.",
                "code": "TRANSFER_ORIGIN_CANNOT_BE_THE_SAME_AS_DESTINATION"
            }));
        }

        let mut item_counts: BTreeMap<String, usize> = BTreeMap::new();
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            if !item_id.is_empty() {
                *item_counts.entry(item_id).or_insert(0) += 1;
            }
        }

        for (index, item_input) in line_item_inputs.iter().enumerate() {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            let quantity = resolved_int_field(item_input, "quantity").unwrap_or(0);
            if item_counts.get(&item_id).copied().unwrap_or(0) > 1 {
                user_errors.push(json!({
                    "field": ["input", "lineItems", index.to_string(), "inventoryItemId"],
                    "message": "The inventory item is already present in the list. Each item must be unique.",
                    "code": "DUPLICATE_ITEM"
                }));
            }
            if origin_is_active
                && !self.inventory_transfer_item_is_stocked_at_origin(&item_id, origin_location_id)
            {
                user_errors.push(json!({
                    "field": ["input", "lineItems", index.to_string(), "inventoryItemId"],
                    "message": "The inventory item could not be found.",
                    "code": "ITEM_NOT_FOUND"
                }));
            }
            if quantity < 0 {
                user_errors.push(json!({
                    "field": ["input", "lineItems", index.to_string(), "quantity"],
                    "message": "The quantity can't be negative.",
                    "code": "INVALID_QUANTITY"
                }));
            }
        }
        user_errors
    }

    fn inventory_transfer_location_is_active(&self, location_id: &str) -> bool {
        if location_id.is_empty() {
            return false;
        }
        // A transfer endpoint must be a real, active location. Each scenario seeds its
        // origin/destination via `locationAdd` (isActive: true), so this resolves the
        // status from the live staged location registry rather than a hardcoded
        // allow-list of capture-specific location ids.
        self.store
            .staged
            .locations
            .get(location_id)
            .and_then(|location| location.get("isActive"))
            .and_then(Value::as_bool)
            == Some(true)
    }

    fn inventory_transfer_item_is_stocked_at_origin(
        &self,
        inventory_item_id: &str,
        origin_location_id: &str,
    ) -> bool {
        if inventory_item_id.is_empty() || origin_location_id.is_empty() {
            return false;
        }
        if self
            .store
            .product_variant_by_inventory_item_id(inventory_item_id)
            .map(|variant| variant.inventory_item.tracked)
            == Some(false)
        {
            return false;
        }
        if self.store.staged.inventory_levels.contains_key(&(
            inventory_item_id.to_string(),
            origin_location_id.to_string(),
        )) {
            return true;
        }
        false
    }

    fn hydrate_inventory_transfer_references<'a>(
        &mut self,
        location_ids: impl IntoIterator<Item = &'a String>,
        line_item_inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        let mut ids = Vec::new();
        for location_id in location_ids {
            if !location_id.is_empty() && !ids.iter().any(|id| id == location_id) {
                ids.push(location_id.clone());
            }
        }
        for item_input in line_item_inputs {
            let item_id = resolved_string_field(item_input, "inventoryItemId").unwrap_or_default();
            if !item_id.is_empty() && !ids.iter().any(|id| id == &item_id) {
                ids.push(item_id);
            }
        }
        if ids.is_empty() {
            return;
        }
        let request = Request {
            method: "POST".to_string(),
            path: "/admin/api/2025-01/graphql.json".to_string(),
            headers: BTreeMap::new(),
            body: json!({
                "query": INVENTORY_TRANSFER_HYDRATE_NODES_QUERY,
                "variables": { "ids": ids }
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(request);
        if response.status >= 400 {
            return;
        }
        self.observe_inventory_transfer_hydration_response(&response.body);
    }

    fn observe_inventory_transfer_hydration_response(&mut self, body: &Value) {
        let nodes = body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        for node in nodes {
            let node_type = node
                .get("__typename")
                .and_then(Value::as_str)
                .or_else(|| {
                    node.get("id")
                        .and_then(Value::as_str)
                        .and_then(shopify_gid_resource_type)
                })
                .or_else(|| {
                    node.get("inventoryLevels")
                        .is_some()
                        .then_some("InventoryItem")
                });
            match node_type {
                Some("Location") => self.stage_inventory_transfer_location(node),
                Some("InventoryItem") => self.stage_inventory_transfer_inventory_item(node),
                _ => {}
            }
        }
    }

    fn stage_inventory_transfer_location(&mut self, location: Value) {
        let Some(id) = location
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let mut merged = self
            .store
            .staged
            .locations
            .get(&id)
            .cloned()
            .unwrap_or_else(|| json!({}));
        if let (Some(existing), Some(incoming)) = (merged.as_object_mut(), location.as_object()) {
            for (key, value) in incoming {
                existing.insert(key.clone(), value.clone());
            }
        } else {
            merged = location;
        }
        self.store.staged.locations.insert(id, merged);
    }

    fn stage_inventory_transfer_inventory_item(&mut self, item: Value) {
        let Some(item_id) = item.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        let Some(variant) = item.get("variant") else {
            return;
        };
        let product = variant.get("product").cloned().unwrap_or_else(|| {
            json!({
                "id": format!("gid://shopify/Product/{}", resource_id_tail(&item_id)),
                "title": "",
                "handle": "",
                "status": "ACTIVE",
                "totalInventory": 0,
                "tracksInventory": item.get("tracked").and_then(Value::as_bool).unwrap_or(true)
            })
        });
        if let Some(product) = product_state_from_json(&product) {
            self.store.stage_observed_product(product);
        }
        let variant_id = variant
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "gid://shopify/ProductVariant/{}",
                    resource_id_tail(&item_id)
                )
            });
        let product_id = variant
            .get("product")
            .and_then(|product| product.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("gid://shopify/Product/{}", resource_id_tail(&item_id)));
        let mut variant_value = variant.clone();
        if let Some(fields) = variant_value.as_object_mut() {
            fields.insert("id".to_string(), json!(variant_id));
            fields.insert("productId".to_string(), json!(product_id));
            fields.insert(
                "inventoryItem".to_string(),
                json!({
                    "id": item_id,
                    "tracked": item.get("tracked").and_then(Value::as_bool).unwrap_or(true),
                    "requiresShipping": item.get("requiresShipping").and_then(Value::as_bool).unwrap_or(true)
                }),
            );
        }
        if let Some(variant) = product_variant_state_from_json(&variant_value) {
            self.store.stage_product_variant(variant);
        }
        for level in item
            .get("inventoryLevels")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(location_id) = level
                .get("location")
                .and_then(|location| location.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let mut quantities = BTreeMap::new();
            for quantity in level
                .get("quantities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(name) = quantity.get("name").and_then(Value::as_str) else {
                    continue;
                };
                quantities.insert(
                    name.to_string(),
                    quantity
                        .get("quantity")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                );
            }
            self.store
                .staged
                .inventory_levels
                .insert((item_id.clone(), location_id.clone()), quantities);
            if let Some(location) = level.get("location").cloned() {
                self.stage_inventory_transfer_location(location);
            }
        }
    }

    fn apply_transfer_reservations(&mut self, record: &InventoryTransferRecord, direction: i64) {
        for line_item in &record.line_items {
            self.apply_inventory_reservation(
                &line_item.inventory_item_id,
                &record.origin_location_id,
                direction * line_item.quantity,
            );
        }
    }

    fn apply_inventory_reservation(
        &mut self,
        inventory_item_id: &str,
        location_id: &str,
        reserved_delta: i64,
    ) {
        let level = self
            .store
            .staged
            .inventory_levels
            .entry((inventory_item_id.to_string(), location_id.to_string()))
            .or_insert_with(|| {
                BTreeMap::from([
                    ("available".to_string(), 5),
                    ("reserved".to_string(), 0),
                    ("on_hand".to_string(), 5),
                ])
            });
        *level.entry("available".to_string()).or_insert(0) -= reserved_delta;
        *level.entry("reserved".to_string()).or_insert(0) += reserved_delta;
        let available = level.get("available").copied().unwrap_or(0);
        let reserved = level.get("reserved").copied().unwrap_or(0);
        level
            .entry("on_hand".to_string())
            .or_insert(available + reserved);
    }
}

pub(in crate::proxy) fn marketing_record_matches_query(record: &Value, query: &str) -> bool {
    marketing_query_terms(query)
        .iter()
        .all(|(field, expected)| {
            marketing_record_query_value(record, field).is_some_and(|value| {
                value
                    .to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase())
            })
        })
}

pub(in crate::proxy) fn marketing_query_terms(query: &str) -> Vec<(String, String)> {
    query
        .split_whitespace()
        .filter_map(|term| {
            let (field, value) = term.split_once(':')?;
            let value = value.trim_matches(|ch| ch == '"' || ch == '\'');
            (!field.is_empty() && !value.is_empty()).then(|| (field.to_string(), value.to_string()))
        })
        .collect()
}

pub(in crate::proxy) fn marketing_record_query_value(
    record: &Value,
    field: &str,
) -> Option<String> {
    match field {
        "id" => record["id"].as_str(),
        "remote_id" | "remoteId" => record["remoteId"]
            .as_str()
            .or_else(|| record["marketingEvent"]["remoteId"].as_str()),
        "title" => record["title"].as_str(),
        "description" => record["marketingEvent"]["description"].as_str(),
        "status" => record["status"].as_str(),
        "channel_handle" | "channelHandle" => record["marketingEvent"]["channelHandle"].as_str(),
        _ => None,
    }
    .map(ToString::to_string)
}

fn input_string_field_value(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn input_utm_value(
    input: &BTreeMap<String, ResolvedValue>,
    selector_utm: Option<&BTreeMap<String, ResolvedValue>>,
    field: &str,
) -> Option<String> {
    match input.get("utm") {
        Some(ResolvedValue::Object(utm)) => input_string_field_value(utm, field),
        _ => selector_utm.and_then(|utm| input_string_field_value(utm, field)),
    }
}

fn json_string_value(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

fn marketing_activity_not_external_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing activity is not external.",
        "code": "ACTIVITY_NOT_EXTERNAL"
    })
}

fn marketing_activity_delete_not_external_error() -> Value {
    json!({
        "field": null,
        "message": "The marketing activity must be an external activity.",
        "code": "ACTIVITY_NOT_EXTERNAL"
    })
}

/// The ordered required (non-null) arguments for each webhook mutation root,
/// paired with the GraphQL type Shopify reports for them. The `webhookSubscription`
/// input type varies by delivery flavor (unified / Pub/Sub / EventBridge).
fn webhook_required_arguments(field_name: &str) -> Vec<(&'static str, &'static str)> {
    let input_type = if field_name.starts_with("pubSubWebhookSubscription") {
        "PubSubWebhookSubscriptionInput!"
    } else if field_name.starts_with("eventBridgeWebhookSubscription") {
        "EventBridgeWebhookSubscriptionInput!"
    } else {
        "WebhookSubscriptionInput!"
    };
    if field_name.ends_with("Create") {
        vec![
            ("topic", "WebhookSubscriptionTopic!"),
            ("webhookSubscription", input_type),
        ]
    } else if field_name.ends_with("Update") {
        vec![("id", "ID!"), ("webhookSubscription", input_type)]
    } else if field_name.ends_with("Delete") {
        vec![("id", "ID!")]
    } else {
        Vec::new()
    }
}

/// Static GraphQL validation for the webhook mutation roots: a required argument
/// that is entirely absent yields a `missingRequiredArguments` error, while one
/// present with a literal `null` yields an `argumentLiteralsIncompatible` error.
fn webhook_required_argument_errors(
    field: &RootFieldSelection,
    document: &ParsedDocument,
) -> Vec<Value> {
    let required = webhook_required_arguments(&field.name);
    if required.is_empty() {
        return Vec::new();
    }
    let mut errors = Vec::new();
    let mut missing = Vec::new();
    for (arg, type_display) in &required {
        match field.raw_arguments.get(*arg) {
            None => missing.push(*arg),
            Some(value) if value.is_literal_null() => {
                errors.push(json!({
                    "message": format!(
                        "Argument '{}' on Field '{}' has an invalid value (null). Expected type '{}'.",
                        arg, field.name, type_display
                    ),
                    "locations": [{ "line": field.location.line, "column": field.location.column }],
                    "path": [document.operation_path.clone(), field.name.clone(), *arg],
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "typeName": "Field",
                        "argumentName": arg
                    }
                }));
            }
            Some(_) => {}
        }
    }
    if !missing.is_empty() {
        errors.insert(
            0,
            json!({
                "message": format!(
                    "Field '{}' is missing required arguments: {}",
                    field.name,
                    missing.join(", ")
                ),
                "locations": [{ "line": field.location.line, "column": field.location.column }],
                "path": [document.operation_path.clone(), field.name.clone()],
                "extensions": {
                    "code": "missingRequiredArguments",
                    "className": "Field",
                    "name": field.name.clone(),
                    "arguments": missing.join(", ")
                }
            }),
        );
    }
    errors
}

fn webhook_subscription_topic_coercion_error(
    field: &RootFieldSelection,
    document: Option<&ParsedDocument>,
) -> Option<Value> {
    let raw_topic = field.raw_arguments.get("topic")?;
    let topic = match raw_topic {
        RawArgumentValue::Enum(topic) => topic.as_str(),
        RawArgumentValue::Variable {
            value: Some(ResolvedValue::String(topic)),
            ..
        } => topic.as_str(),
        _ => return None,
    };
    if is_known_webhook_subscription_topic(topic) {
        return None;
    }
    Some(match raw_topic {
        RawArgumentValue::Enum(_) => json!({
            "message": format!("Argument 'topic' on Field '{}' has an invalid value ({}). Expected type 'WebhookSubscriptionTopic!'.", field.name, topic),
            "locations": [{ "line": field.location.line, "column": field.location.column }],
            "path": [
                document
                    .map(|document| document.operation_path.clone())
                    .unwrap_or_else(|| "mutation".to_string()),
                field.name.clone(),
                "topic"
            ],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "Field",
                "argumentName": "topic"
            }
        }),
        RawArgumentValue::Variable { name, .. } => {
            // Shopify anchors a coerced-variable error at the variable's
            // *definition* in the operation signature, not at the field.
            let location = document
                .and_then(|document| document.variable_definitions.get(name))
                .map_or(field.location, |definition| definition.location);
            json!({
                "message": format!("Variable ${} of type WebhookSubscriptionTopic! was provided invalid value", name),
                "locations": [{ "line": location.line, "column": location.column }],
                "extensions": {
                    "code": "INVALID_VARIABLE",
                    "value": topic,
                    "problems": [{
                        "path": [],
                        "explanation": format!("Expected \"{}\" to be one of: {}", topic, WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES)
                    }]
                }
            })
        }
        _ => unreachable!(),
    })
}

fn dedicated_pubsub_required_field_error(
    root_field: &str,
    field: &RootFieldSelection,
    document: &ParsedDocument,
) -> Option<Value> {
    if !root_field.starts_with("pubSubWebhookSubscription") {
        return None;
    }
    match field.raw_arguments.get("webhookSubscription")? {
        RawArgumentValue::Variable {
            name,
            value: Some(ResolvedValue::Object(value)),
        } => dedicated_pubsub_variable_required_field_error(name, value, field, document),
        RawArgumentValue::Object(value) => {
            dedicated_pubsub_inline_required_field_error(value, field)
        }
        _ => None,
    }
}

fn dedicated_pubsub_variable_required_field_error(
    variable_name: &str,
    value: &BTreeMap<String, ResolvedValue>,
    field: &RootFieldSelection,
    document: &ParsedDocument,
) -> Option<Value> {
    let missing = missing_pubsub_resolved_fields(value);
    if missing.is_empty() {
        return None;
    }
    // Shopify anchors a coerced-variable error at the variable's *definition*
    // in the operation signature, not at the field where it is used.
    let location = document
        .variable_definitions
        .get(variable_name)
        .map_or(field.location, |definition| definition.location);
    let message_detail = missing
        .iter()
        .map(|key| format!("{key} (Expected value to not be null)"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(json!({
        "message": format!("Variable ${} of type PubSubWebhookSubscriptionInput! was provided invalid value for {}", variable_name, message_detail),
        "locations": [{ "line": location.line, "column": location.column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(&ResolvedValue::Object(value.clone())),
            "problems": missing
                .iter()
                .map(|key| json!({
                    "path": [key],
                    "explanation": "Expected value to not be null"
                }))
                .collect::<Vec<_>>()
        }
    }))
}

fn dedicated_pubsub_inline_required_field_error(
    value: &BTreeMap<String, RawArgumentValue>,
    field: &RootFieldSelection,
) -> Option<Value> {
    let missing = ["pubSubProject", "pubSubTopic"]
        .into_iter()
        .filter(|key| {
            !value.contains_key(*key)
                || value
                    .get(*key)
                    .is_some_and(RawArgumentValue::is_literal_null)
        })
        .collect::<Vec<_>>();
    let first_missing = missing.first()?;
    Some(json!({
        "message": format!("Argument '{}' on InputObject 'PubSubWebhookSubscriptionInput' is required. Expected type String!", first_missing),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "path": ["mutation", field.name.clone(), "webhookSubscription", first_missing],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": first_missing,
            "argumentType": "String!",
            "inputObjectType": "PubSubWebhookSubscriptionInput"
        }
    }))
}

fn missing_pubsub_resolved_fields(value: &BTreeMap<String, ResolvedValue>) -> Vec<&'static str> {
    ["pubSubProject", "pubSubTopic"]
        .into_iter()
        .filter(|key| {
            !value.contains_key(*key) || matches!(value.get(*key), Some(ResolvedValue::Null))
        })
        .collect()
}

fn webhook_subscription_address_error_field(root_field: &str) -> Value {
    if root_field.starts_with("eventBridgeWebhookSubscription") {
        json!(["webhookSubscription", "arn"])
    } else {
        json!(["webhookSubscription", "callbackUrl"])
    }
}

fn webhook_subscription_optional_string_key(record: &Value, key: &str) -> Option<String> {
    record[key].as_str().map(ToString::to_string)
}

fn webhook_subscription_effective_api_version(request: &Request) -> Option<String> {
    request_header(request, "x-shopify-draft-proxy-api-version")
        .or_else(|| admin_graphql_version(&request.path).map(|version| version.trim().to_string()))
}

fn webhook_subscription_api_version_record(handle: Option<&str>) -> Value {
    let handle = handle
        .map(str::trim)
        .filter(|handle| !handle.is_empty())
        .unwrap_or("2026-04")
        .to_string();
    let (display_name, supported) = match handle.as_str() {
        "2026-04" => ("2026-04 (Latest)".to_string(), true),
        "2026-07" => ("2026-07 (Release candidate)".to_string(), false),
        "unstable" => ("unstable".to_string(), false),
        _ => (handle.clone(), true),
    };
    json!({
        "handle": handle,
        "displayName": display_name,
        "supported": supported
    })
}

/// Resolve an app-reserved metafield namespace shorthand. Shopify expands
/// `$app:NAME` to `app--<api_client_id>--NAME` (and bare `$app` to
/// `app--<api_client_id>`) using the requesting app's client id. Namespaces
/// that are already fully qualified (e.g. `app--999999999999--kept`) or
/// unrelated (e.g. `custom`) are returned unchanged.
fn resolve_webhook_metafield_namespace(namespace: &str, api_client_id: Option<&str>) -> String {
    let Some(client_id) = api_client_id else {
        return namespace.to_string();
    };
    if let Some(rest) = namespace.strip_prefix("$app:") {
        format!("app--{client_id}--{rest}")
    } else if namespace == "$app" {
        format!("app--{client_id}")
    } else {
        namespace.to_string()
    }
}

/// A webhook filter is a search-query string that must reference at least one
/// field via `field:value` syntax. A non-empty filter that names no field
/// (e.g. `totally bogus syntax`) is rejected by Shopify. Empty/blank filters
/// mean "no filter" and are accepted.
fn webhook_filter_is_invalid(filter: &str) -> bool {
    let trimmed = filter.trim();
    if trimmed.is_empty() {
        return false;
    }
    !trimmed.split_whitespace().any(|token| {
        token.split_once(':').is_some_and(|(field, _)| {
            !field.is_empty()
                && field
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
    })
}

fn is_known_webhook_subscription_topic(topic: &str) -> bool {
    WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES
        .split(", ")
        .any(|known| known == topic)
}

fn inventory_quantity_missing_change_from_payload(
    field: &RootFieldSelection,
    root_field: &str,
    input_type: &str,
    rows: &[BTreeMap<String, ResolvedValue>],
    quantity_field: &str,
) -> Option<Value> {
    if rows
        .iter()
        .any(|row| row.contains_key("changeFromQuantity"))
        || rows.iter().any(|row| row.contains_key("compareQuantity"))
    {
        return None;
    }
    if rows.iter().any(|row| row.contains_key(quantity_field)) {
        return Some(json!({
            "__topLevelErrors": [{
                "message": format!("{input_type} must include the following argument: changeFromQuantity."),
                "locations": [
                    { "line": field.location.line, "column": field.location.column },
                    { "line": field.location.line.saturating_sub(1).max(1), "column": 1 }
                ],
                "extensions": { "code": "INVALID_FIELD_ARGUMENTS" },
                "path": [root_field]
            }]
        }));
    }
    None
}

fn inventory_adjust_requires_change_from(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
}

fn inventory_set_requires_change_from(request: &Request, field: &RootFieldSelection) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2026, 4))
        && field
            .directives
            .iter()
            .any(|directive| directive == "idempotent")
}

fn empty_inventory_quantities() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("available".to_string(), 0),
        ("reserved".to_string(), 0),
        ("on_hand".to_string(), 0),
        ("incoming".to_string(), 0),
    ])
}

fn inventory_shipment_tracking_from_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<InventoryShipmentTrackingRecord> {
    let tracking = resolved_object_field(input, "trackingInput").unwrap_or_else(|| input.clone());
    let record = InventoryShipmentTrackingRecord {
        tracking_number: resolved_string_field(&tracking, "trackingNumber"),
        company: resolved_string_field(&tracking, "company")
            .or_else(|| resolved_string_field(&tracking, "carrier")),
        tracking_url: resolved_string_field(&tracking, "trackingUrl")
            .or_else(|| resolved_string_field(&tracking, "url")),
        arrives_at: resolved_string_field(&tracking, "arrivesAt"),
    };
    (record.tracking_number.is_some()
        || record.company.is_some()
        || record.tracking_url.is_some()
        || record.arrives_at.is_some())
    .then_some(record)
}

fn inventory_shipment_tracking_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    let carrier = resolved_string_field(input, "carrier");
    if carrier
        .as_deref()
        .is_some_and(|value| !is_valid_tracking_carrier(value))
    {
        errors.push(inventory_shipment_user_error(
            vec!["input", "trackingInput", "carrier"],
            "Carrier is not included in the list.",
            "INVALID",
        ));
    }
    let tracking_url =
        resolved_string_field(input, "url").or_else(|| resolved_string_field(input, "trackingUrl"));
    if tracking_url
        .as_deref()
        .is_some_and(|url| !(url.starts_with("https://") || url.starts_with("http://")))
    {
        errors.push(inventory_shipment_user_error(
            vec!["input", "trackingInput", "url"],
            "Tracking URL is invalid.",
            "INVALID",
        ));
    }
    errors
}

fn is_valid_tracking_carrier(carrier: &str) -> bool {
    matches!(
        carrier,
        "UPS" | "USPS" | "FEDEX" | "DHL_EXPRESS" | "CANADA_POST" | "OTHER"
    )
}

fn inventory_shipment_user_error(field_path: Vec<&str>, message: &str, code: &str) -> Value {
    user_error(field_path, message, Some(code))
}

fn inventory_shipment_has_incoming(record: &InventoryShipmentRecord) -> bool {
    matches!(record.status.as_str(), "IN_TRANSIT" | "PARTIALLY_RECEIVED")
}

impl InventoryShipmentRecord {
    fn line_item_total_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.quantity)
            .sum()
    }

    fn total_accepted_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.accepted_quantity)
            .sum()
    }

    fn total_rejected_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.rejected_quantity)
            .sum()
    }

    fn total_received_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.received_quantity())
            .sum()
    }

    fn unreceived_quantity(&self) -> i64 {
        self.line_items
            .iter()
            .map(|line_item| line_item.unreceived_quantity())
            .sum()
    }
}

impl InventoryShipmentLineItemRecord {
    fn received_quantity(&self) -> i64 {
        self.accepted_quantity + self.rejected_quantity
    }

    fn unreceived_quantity(&self) -> i64 {
        (self.quantity - self.received_quantity()).max(0)
    }
}

fn inventory_quantities_from_observed_rows(rows: &[Value]) -> BTreeMap<String, i64> {
    let mut quantities = empty_inventory_quantities();
    for row in rows {
        let Some(name) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(quantity) = row.get("quantity").and_then(Value::as_i64) else {
            continue;
        };
        quantities.insert(name.to_string(), quantity);
    }
    quantities
}

fn inventory_activate_user_error(field: Vec<&str>, message: &str, code: Option<&str>) -> Value {
    user_error_omit_code(field, message, code)
}

fn inventory_deactivate_user_error(message: &str, code: Option<&str>) -> Value {
    user_error_omit_code(Value::Null, message, code)
}

fn inventory_bulk_toggle_user_error(
    field: Vec<String>,
    message: &str,
    code: Option<&str>,
) -> Value {
    user_error_omit_code(field, message, code)
}

fn inventory_item_update_user_error(
    field: Vec<String>,
    message: &str,
    code: Option<&str>,
) -> Value {
    user_error_omit_code(field, message, code)
}

fn inventory_item_update_variable_errors(
    field: &RootFieldSelection,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<Value>> {
    let measurement = resolved_object_field(input, "measurement")?;
    let weight = resolved_object_field(&measurement, "weight")?;
    let unit = resolved_string_field(&weight, "unit")?;
    if INVENTORY_ITEM_WEIGHT_UNITS
        .iter()
        .any(|candidate| *candidate == unit)
    {
        return None;
    }
    Some(vec![json!({
        "message": format!("Variable $input of type InventoryItemInput! was provided invalid value for measurement.weight.unit (Expected \"{}\" to be one of: {})", unit, INVENTORY_ITEM_WEIGHT_UNITS.join(", ")),
        "locations": [{ "line": field.location.line.saturating_sub(1).max(1), "column": 52 }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
            "problems": [{
                "path": ["measurement", "weight", "unit"],
                "explanation": format!("Expected \"{}\" to be one of: {}", unit, INVENTORY_ITEM_WEIGHT_UNITS.join(", "))
            }]
        }
    })])
}

fn inventory_item_update_user_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_decimal_value(input, "cost").is_some_and(|cost| cost < 0.0) {
        errors.push(inventory_item_update_user_error(
            inventory_item_update_field_path(&["input", "cost"]),
            "Cost must be greater than or equal to 0",
            Some("INVALID"),
        ));
    }
    if let Some(weight) = resolved_object_field(input, "measurement")
        .and_then(|measurement| resolved_object_field(&measurement, "weight"))
    {
        if let Some(value) = resolved_decimal_value(&weight, "value") {
            if value < 0.0 {
                errors.push(inventory_item_update_user_error(
                    inventory_item_update_field_path(&["input", "measurement", "weight"]),
                    &format!(
                        "Measurement weight value {} kg must be >= 0 kg",
                        shopify_number_text(value)
                    ),
                    Some("INVALID"),
                ));
            }
        }
    }
    if let Some(country_code) = resolved_string_field(input, "countryCodeOfOrigin") {
        if !is_valid_country_code(&country_code) {
            errors.push(inventory_item_update_user_error(
                inventory_item_update_field_path(&["input", "countryCodeOfOrigin"]),
                "Country code of origin is invalid",
                Some("INVALID"),
            ));
        }
    }
    if let Some(province_code) = resolved_string_field(input, "provinceCodeOfOrigin") {
        if province_code.len() > 3 || !province_code.chars().all(|ch| ch.is_ascii_alphabetic()) {
            errors.push(inventory_item_update_user_error(
                inventory_item_update_field_path(&["input", "provinceCodeOfOrigin"]),
                "Province code of origin is invalid",
                Some("INVALID"),
            ));
        }
    }
    if let Some(hs_code) = resolved_string_field(input, "harmonizedSystemCode") {
        if !valid_harmonized_system_code(&hs_code) {
            errors.push(inventory_item_update_user_error(
                inventory_item_update_field_path(&["input", "harmonizedSystemCode"]),
                "Harmonized system code must be a number between six and thirteen digits",
                Some("INVALID"),
            ));
        }
    }
    let mut seen_country_codes = BTreeSet::new();
    for (index, row) in resolved_object_list_field(input, "countryHarmonizedSystemCodes")
        .iter()
        .enumerate()
    {
        if let Some(country_code) = resolved_string_field(row, "countryCode") {
            if !is_valid_country_code(&country_code) {
                errors.push(inventory_item_update_user_error(
                    inventory_item_update_field_path(&["input", "countryHarmonizedSystemCodes"]),
                    "Country code is invalid",
                    Some("INVALID"),
                ));
            } else if !seen_country_codes.insert(country_code) {
                errors.push(inventory_item_update_user_error(
                    vec![
                        "input".to_string(),
                        "countryHarmonizedSystemCodes".to_string(),
                        index.to_string(),
                        "countryCode".to_string(),
                    ],
                    "Country code has already been taken",
                    Some("TAKEN"),
                ));
            }
        }
        if let Some(hs_code) = resolved_string_field(row, "harmonizedSystemCode") {
            if !valid_harmonized_system_code(&hs_code) {
                errors.push(inventory_item_update_user_error(
                    inventory_item_update_field_path(&["input", "countryHarmonizedSystemCodes"]),
                    "Harmonized system code must be a number between six and thirteen digits",
                    Some("INVALID"),
                ));
            }
        }
    }
    errors
}

fn inventory_item_update_field_path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn resolved_decimal_value(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<f64> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => value.parse::<f64>().ok(),
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::Float(value)) => Some(*value),
        _ => None,
    }
}

fn is_valid_country_code(country_code: &str) -> bool {
    INVENTORY_VALID_COUNTRY_CODES.contains(&country_code)
}

fn valid_harmonized_system_code(value: &str) -> bool {
    let normalized = normalized_harmonized_system_code(value);
    (6..=13).contains(&normalized.len()) && normalized.chars().all(|ch| ch.is_ascii_digit())
}

fn resolved_harmonized_system_code_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(normalized_harmonized_system_code(value)),
        _ => resolved_value_json(value),
    }
}

fn normalized_harmonized_system_code(value: &str) -> String {
    value.chars().filter(char::is_ascii_alphanumeric).collect()
}

fn shopify_number_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn inventory_invalid_reason_payload(
    field: &RootFieldSelection,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let reason = resolved_string_field(input, "reason").unwrap_or_else(|| "correction".to_string());
    if INVENTORY_VALID_REASONS.iter().any(|valid| *valid == reason) {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(
        field,
        vec![json!({
            "field": ["input", "reason"],
            "message": format!(
                "The specified reason is invalid. Valid values are: {}.",
                INVENTORY_VALID_REASONS.join(", ")
            ),
            "code": "INVALID_REASON"
        })],
    ))
}

fn inventory_invalid_public_quantity_name_payload(
    field: &RootFieldSelection,
    name: &str,
    path: Value,
) -> Option<Value> {
    if INVENTORY_PUBLIC_ADJUST_QUANTITY_NAMES.contains(&name) {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(
        field,
        vec![json!({
            "field": path,
            "message": INVENTORY_INVALID_PUBLIC_QUANTITY_NAME_MESSAGE,
            "code": "INVALID_QUANTITY_NAME"
        })],
    ))
}

fn inventory_invalid_set_quantity_name_payload(
    field: &RootFieldSelection,
    name: &str,
) -> Option<Value> {
    if INVENTORY_SET_QUANTITY_NAMES.contains(&name) {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(
        field,
        vec![json!({
            "field": ["input", "name"],
            "message": INVENTORY_INVALID_SET_QUANTITY_NAME_MESSAGE,
            "code": "INVALID_NAME"
        })],
    ))
}

fn inventory_invalid_set_quantities_payload(
    field: &RootFieldSelection,
    quantities: &[BTreeMap<String, ResolvedValue>],
) -> Option<Value> {
    let mut errors = Vec::new();
    for (index, quantity) in quantities.iter().enumerate() {
        if resolved_int_field(quantity, "quantity")
            .is_some_and(|value| value > INVENTORY_SET_QUANTITY_MAX)
        {
            errors.push(json!({
                "field": ["input", "quantities", index.to_string(), "quantity"],
                "message": "The quantity can't be higher than 1,000,000,000.",
                "code": "INVALID_QUANTITY_TOO_HIGH"
            }));
        }
    }

    let mut indexes_by_pair: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, quantity) in quantities.iter().enumerate() {
        let item_id = resolved_string_field(quantity, "inventoryItemId").unwrap_or_default();
        let location_id = resolved_string_field(quantity, "locationId").unwrap_or_default();
        indexes_by_pair
            .entry((item_id, location_id))
            .or_default()
            .push(index);
    }
    let duplicate_indexes: BTreeSet<usize> = indexes_by_pair
        .values()
        .filter(|indexes| indexes.len() > 1)
        .flat_map(|indexes| indexes.iter().copied())
        .collect();
    for index in duplicate_indexes {
        errors.push(json!({
            "field": ["input", "quantities", index.to_string(), "locationId"],
            "message": "The combination of inventoryItemId and locationId must be unique.",
            "code": "NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"
        }));
    }

    if errors.is_empty() {
        return None;
    }
    Some(inventory_invalid_adjustment_payload(field, errors))
}

fn inventory_invalid_adjustment_payload(
    field: &RootFieldSelection,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": user_errors
        }),
        &field.selection,
    )
}

#[cfg(test)]
#[test]
fn immutable_external_activity_validator_rejects_missing_marketing_event() {
    let proxy = DraftProxy::new(Config {
        read_mode: ReadMode::Snapshot,
        unsupported_mutation_mode: None,
        bulk_operation_run_mutation_max_input_file_size_bytes: None,
        port: 0,
        shopify_admin_origin: "https://shopify.com".to_string(),
        snapshot_path: None,
    });
    let err = proxy.marketing_external_immutable_update_error(
        &json!({
            "isExternal": true,
            "marketingEvent": null,
            "urlParameterValue": "url",
            "utmParameters": {
                "campaign": "campaign",
                "source": "source",
                "medium": "medium"
            }
        }),
        &BTreeMap::new(),
        None,
        &Request::default(),
    );

    assert_eq!(
        err,
        Some(json!({
            "field": null,
            "message": "Marketing activity is not valid, the associated marketing event does not exist.",
            "code": "MARKETING_EVENT_DOES_NOT_EXIST"
        }))
    );
}

const WEBHOOK_SUBSCRIPTION_TOPIC_EXPECTED_VALUES: &str = "TAX_SUMMARIES_CREATE, APP_UNINSTALLED, APP_SCOPES_UPDATE, CARTS_CREATE, CARTS_UPDATE, CHANNELS_DELETE, CHECKOUTS_CREATE, CHECKOUTS_DELETE, CHECKOUTS_UPDATE, CUSTOMER_PAYMENT_METHODS_CREATE, CUSTOMER_PAYMENT_METHODS_UPDATE, CUSTOMER_PAYMENT_METHODS_REVOKE, COLLECTION_LISTINGS_ADD, COLLECTION_LISTINGS_REMOVE, COLLECTION_LISTINGS_UPDATE, COLLECTION_PUBLICATIONS_CREATE, COLLECTION_PUBLICATIONS_DELETE, COLLECTION_PUBLICATIONS_UPDATE, COLLECTIONS_CREATE, COLLECTIONS_DELETE, COLLECTIONS_UPDATE, CUSTOMER_GROUPS_CREATE, CUSTOMER_GROUPS_DELETE, CUSTOMER_GROUPS_UPDATE, CUSTOMERS_CREATE, CUSTOMERS_DELETE, CUSTOMERS_DISABLE, CUSTOMERS_ENABLE, CUSTOMERS_UPDATE, CUSTOMERS_PURCHASING_SUMMARY, CUSTOMERS_MARKETING_CONSENT_UPDATE, CUSTOMER_TAGS_ADDED, CUSTOMER_TAGS_REMOVED, CUSTOMERS_EMAIL_MARKETING_CONSENT_UPDATE, DISPUTES_CREATE, DISPUTES_UPDATE, DRAFT_ORDERS_CREATE, DRAFT_ORDERS_DELETE, DRAFT_ORDERS_UPDATE, FULFILLMENT_EVENTS_CREATE, FULFILLMENT_EVENTS_DELETE, FULFILLMENTS_CREATE, FULFILLMENTS_UPDATE, ATTRIBUTED_SESSIONS_FIRST, ATTRIBUTED_SESSIONS_LAST, ORDER_TRANSACTIONS_CREATE, ORDERS_CANCELLED, ORDERS_CREATE, ORDERS_DELETE, ORDERS_EDITED, ORDERS_FULFILLED, ORDERS_PAID, ORDERS_PARTIALLY_FULFILLED, ORDERS_UPDATED, ORDERS_LINK_REQUESTED, FULFILLMENT_ORDERS_MOVED, FULFILLMENT_ORDERS_HOLD_RELEASED, FULFILLMENT_ORDERS_SCHEDULED_FULFILLMENT_ORDER_READY, FULFILLMENT_HOLDS_RELEASED, FULFILLMENT_ORDERS_ORDER_ROUTING_COMPLETE, FULFILLMENT_ORDERS_CANCELLED, FULFILLMENT_ORDERS_FULFILLMENT_SERVICE_FAILED_TO_COMPLETE, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_REJECTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_ACCEPTED, FULFILLMENT_ORDERS_CANCELLATION_REQUEST_REJECTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_SUBMITTED, FULFILLMENT_ORDERS_FULFILLMENT_REQUEST_ACCEPTED, FULFILLMENT_HOLDS_ADDED, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_LOCAL_DELIVERY, FULFILLMENT_ORDERS_PLACED_ON_HOLD, FULFILLMENT_ORDERS_MERGED, FULFILLMENT_ORDERS_SPLIT, FULFILLMENT_ORDERS_PROGRESS_REPORTED, FULFILLMENT_ORDERS_MANUALLY_REPORTED_PROGRESS_STOPPED, PRODUCT_LISTINGS_ADD, PRODUCT_LISTINGS_REMOVE, PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_ADD, SCHEDULED_PRODUCT_LISTINGS_UPDATE, SCHEDULED_PRODUCT_LISTINGS_REMOVE, PRODUCT_PUBLICATIONS_CREATE, PRODUCT_PUBLICATIONS_DELETE, PRODUCT_PUBLICATIONS_UPDATE, PRODUCTS_CREATE, PRODUCTS_DELETE, PRODUCTS_UPDATE, REFUNDS_CREATE, SEGMENTS_CREATE, SEGMENTS_DELETE, SEGMENTS_UPDATE, SHIPPING_ADDRESSES_CREATE, SHIPPING_ADDRESSES_UPDATE, SHOP_UPDATE, TAX_PARTNERS_UPDATE, TAX_SERVICES_CREATE, TAX_SERVICES_UPDATE, THEMES_CREATE, THEMES_DELETE, THEMES_PUBLISH, THEMES_UPDATE, VARIANTS_IN_STOCK, VARIANTS_OUT_OF_STOCK, INVENTORY_LEVELS_CONNECT, INVENTORY_LEVELS_UPDATE, INVENTORY_LEVELS_DISCONNECT, INVENTORY_ITEMS_CREATE, INVENTORY_ITEMS_UPDATE, INVENTORY_ITEMS_DELETE, LOCATIONS_ACTIVATE, LOCATIONS_DEACTIVATE, LOCATIONS_CREATE, LOCATIONS_UPDATE, LOCATIONS_DELETE, TENDER_TRANSACTIONS_CREATE, APP_PURCHASES_ONE_TIME_UPDATE, APP_SUBSCRIPTIONS_APPROACHING_CAPPED_AMOUNT, APP_SUBSCRIPTIONS_UPDATE, LOCALES_CREATE, LOCALES_UPDATE, LOCALES_DESTROY, DOMAINS_CREATE, DOMAINS_UPDATE, DOMAINS_DESTROY, SUBSCRIPTION_CONTRACTS_CREATE, SUBSCRIPTION_CONTRACTS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_CREATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_UPDATE, SUBSCRIPTION_BILLING_CYCLE_EDITS_DELETE, PROFILES_CREATE, PROFILES_UPDATE, PROFILES_DELETE, SUBSCRIPTION_BILLING_ATTEMPTS_SUCCESS, SUBSCRIPTION_BILLING_ATTEMPTS_FAILURE, SUBSCRIPTION_BILLING_ATTEMPTS_CHALLENGED, RETURNS_CANCEL, RETURNS_CLOSE, RETURNS_REOPEN, RETURNS_REQUEST, RETURNS_APPROVE, RETURNS_UPDATE, RETURNS_PROCESS, RETURNS_DECLINE, REVERSE_DELIVERIES_ATTACH_DELIVERABLE, REVERSE_FULFILLMENT_ORDERS_DISPOSE, PAYMENT_TERMS_CREATE, PAYMENT_TERMS_DELETE, PAYMENT_TERMS_UPDATE, PAYMENT_SCHEDULES_DUE, SELLING_PLAN_GROUPS_CREATE, SELLING_PLAN_GROUPS_UPDATE, SELLING_PLAN_GROUPS_DELETE, BULK_OPERATIONS_FINISH, PRODUCT_FEEDS_CREATE, PRODUCT_FEEDS_UPDATE, PRODUCT_FEEDS_INCREMENTAL_SYNC, PRODUCT_FEEDS_FULL_SYNC, PRODUCT_FEEDS_FULL_SYNC_FINISH, MARKETS_CREATE, MARKETS_UPDATE, MARKETS_DELETE, ORDERS_RISK_ASSESSMENT_CHANGED, ORDERS_SHOPIFY_PROTECT_ELIGIBILITY_CHANGED, FINANCE_KYC_INFORMATION_UPDATE, FULFILLMENT_ORDERS_RESCHEDULED, PUBLICATIONS_DELETE, AUDIT_EVENTS_ADMIN_API_ACTIVITY, FULFILLMENT_ORDERS_LINE_ITEMS_PREPARED_FOR_PICKUP, COMPANIES_CREATE, COMPANIES_UPDATE, COMPANIES_DELETE, COMPANY_LOCATIONS_CREATE, COMPANY_LOCATIONS_UPDATE, COMPANY_LOCATIONS_DELETE, COMPANY_CONTACTS_CREATE, COMPANY_CONTACTS_UPDATE, COMPANY_CONTACTS_DELETE, CUSTOMERS_MERGE, INVENTORY_TRANSFERS_ADD_ITEMS, INVENTORY_TRANSFERS_UPDATE_ITEM_QUANTITIES, INVENTORY_TRANSFERS_REMOVE_ITEMS, INVENTORY_TRANSFERS_READY_TO_SHIP, INVENTORY_TRANSFERS_CANCEL, INVENTORY_TRANSFERS_COMPLETE, INVENTORY_SHIPMENTS_DELETE, INVENTORY_SHIPMENTS_CREATE, INVENTORY_SHIPMENTS_MARK_IN_TRANSIT, INVENTORY_SHIPMENTS_UPDATE_TRACKING, INVENTORY_SHIPMENTS_ADD_ITEMS, INVENTORY_SHIPMENTS_UPDATE_ITEM_QUANTITIES, INVENTORY_SHIPMENTS_REMOVE_ITEMS, INVENTORY_SHIPMENTS_RECEIVE_ITEMS, CUSTOMER_ACCOUNT_SETTINGS_UPDATE, CUSTOMER_JOINED_SEGMENT, CUSTOMER_LEFT_SEGMENT, COMPANY_CONTACT_ROLES_ASSIGN, COMPANY_CONTACT_ROLES_REVOKE, SUBSCRIPTION_CONTRACTS_ACTIVATE, SUBSCRIPTION_CONTRACTS_PAUSE, SUBSCRIPTION_CONTRACTS_CANCEL, SUBSCRIPTION_CONTRACTS_FAIL, SUBSCRIPTION_CONTRACTS_EXPIRE, SUBSCRIPTION_BILLING_CYCLES_SKIP, SUBSCRIPTION_BILLING_CYCLES_UNSKIP, METAOBJECTS_CREATE, METAOBJECTS_UPDATE, METAOBJECTS_DELETE, FINANCE_APP_STAFF_MEMBER_GRANT, FINANCE_APP_STAFF_MEMBER_REVOKE, FINANCE_APP_STAFF_MEMBER_DELETE, FINANCE_APP_STAFF_MEMBER_UPDATE, DISCOUNTS_CREATE, DISCOUNTS_UPDATE, DISCOUNTS_DELETE, DISCOUNTS_REDEEMCODE_ADDED, DISCOUNTS_REDEEMCODE_REMOVED, METAFIELD_DEFINITIONS_CREATE, METAFIELD_DEFINITIONS_UPDATE, METAFIELD_DEFINITIONS_DELETE, DELIVERY_PROMISE_SETTINGS_UPDATE, MARKETS_BACKUP_REGION_UPDATE, CHECKOUT_AND_ACCOUNTS_CONFIGURATIONS_UPDATE";
