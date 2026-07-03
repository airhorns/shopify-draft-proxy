use super::*;

pub(in crate::proxy) fn marketing_connection(
    records: Vec<Value>,
    selection: &[SelectedField],
) -> Value {
    let full = connection_json_with_cursor(
        records,
        |_, record| format!("cursor:{}", record["id"].as_str().unwrap_or("local")),
        empty_page_info(),
    );
    selected_json(&full, selection)
}

pub(in crate::proxy) fn marketing_activity_payload(
    activity: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "marketingActivity": activity.unwrap_or(Value::Null), "userErrors": user_errors })
}

pub(in crate::proxy) fn marketing_engagement_payload(
    engagement: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "marketingEngagement": engagement.unwrap_or(Value::Null), "userErrors": user_errors })
}

pub(in crate::proxy) fn marketing_activity_missing_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing activity does not exist.",
        "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"
    })
}

pub(in crate::proxy) fn marketing_activity_child_events_error() -> Value {
    json!({
        "field": null,
        "message": "This activity has child activities and thus cannot be deleted. Child activities must be deleted before a parent activity.",
        "code": "CANNOT_DELETE_ACTIVITY_WITH_CHILD_EVENTS"
    })
}

pub(in crate::proxy) fn marketing_activity_cannot_update_tactic_to_storefront_error() -> Value {
    json!({
        "field": ["input"],
        "message": "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.",
        "code": "CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"
    })
}

pub(in crate::proxy) fn marketing_activity_cannot_update_tactic_from_storefront_error() -> Value {
    json!({
        "field": ["input"],
        "message": "You can not update an activity tactic from STOREFRONT_APP.",
        "code": "CANNOT_UPDATE_TACTIC_IF_ORIGINALLY_STOREFRONT_APP"
    })
}

pub(in crate::proxy) fn marketing_event_missing_error() -> Value {
    json!({
        "field": null,
        "message": "Marketing event does not exist.",
        "code": "MARKETING_EVENT_DOES_NOT_EXIST"
    })
}

const MARKETING_EVENT_ID_OFFSET: u64 = 1_000_000;

fn marketing_app_json(api_client_id: Option<&str>) -> Value {
    let Some(api_client_id) = api_client_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return json!({ "id": "gid://shopify/App/local", "title": "local" });
    };
    let app_id = if api_client_id.starts_with("gid://shopify/App/") {
        api_client_id.to_string()
    } else {
        shopify_gid("App", api_client_id)
    };
    json!({ "id": app_id, "title": resource_id_tail(api_client_id) })
}

fn non_empty_channel_handle(handle: &str) -> bool {
    !handle.trim().is_empty()
}

pub(in crate::proxy) fn marketing_activity_from_input(
    id: &str,
    input: BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    api_client_id: Option<String>,
    new_marketing_event_id: Option<String>,
    timestamp: &str,
    shop_currency_code: &str,
) -> Value {
    let old = existing.cloned().unwrap_or_else(|| json!({}));
    let title = resolved_string_field(&input, "title").unwrap_or_else(|| {
        old["title"]
            .as_str()
            .unwrap_or("Marketing activity")
            .to_string()
    });
    let remote_id = resolved_string_field(&input, "remoteId")
        .or_else(|| old["remoteId"].as_str().map(str::to_string));
    let status = resolved_string_field(&input, "status")
        .unwrap_or_else(|| old["status"].as_str().unwrap_or("UNDEFINED").to_string());
    let tactic = resolved_string_field(&input, "tactic")
        .unwrap_or_else(|| old["tactic"].as_str().unwrap_or("NEWSLETTER").to_string());
    let channel_type = resolved_string_field(&input, "marketingChannelType").unwrap_or_else(|| {
        old["marketingChannelType"]
            .as_str()
            .unwrap_or("EMAIL")
            .to_string()
    });
    let remote_url = resolved_string_field(&input, "remoteUrl").or_else(|| {
        old["marketingEvent"]["manageUrl"]
            .as_str()
            .map(str::to_string)
    });
    let preview_url = resolved_string_field(&input, "previewUrl").or_else(|| {
        old["marketingEvent"]["previewUrl"]
            .as_str()
            .map(str::to_string)
    });
    let url_parameter_value = resolved_string_field(&input, "urlParameterValue")
        .or_else(|| old["urlParameterValue"].as_str().map(str::to_string));
    let channel_handle = resolved_string_field(&input, "channelHandle")
        .map(Value::String)
        .or_else(|| old["marketingEvent"].get("channelHandle").cloned())
        .unwrap_or(Value::Null);
    let utm = resolved_object_field(&input, "utm");
    let old_utm = &old["utmParameters"];
    let campaign = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "campaign"))
        .or_else(|| old_utm["campaign"].as_str().map(str::to_string));
    let source = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "source"))
        .or_else(|| old_utm["source"].as_str().map(str::to_string));
    let medium = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "medium"))
        .or_else(|| old_utm["medium"].as_str().map(str::to_string));
    let source_medium = marketing_source_and_medium(
        &channel_type,
        &tactic,
        resolved_string_field(&input, "referringDomain").as_deref(),
    )
    .map(Value::String)
    .unwrap_or(Value::Null);
    let remote_id_value = remote_id.map(Value::String).unwrap_or(Value::Null);
    let campaign_value = campaign.map(Value::String).unwrap_or(Value::Null);
    let source_value = source.map(Value::String).unwrap_or(Value::Null);
    let medium_value = medium.map(Value::String).unwrap_or(Value::Null);
    let event_id = old["marketingEvent"]["id"]
        .as_str()
        .map(str::to_string)
        .or(new_marketing_event_id)
        .unwrap_or_else(|| shopify_gid("MarketingEvent", "local"));
    let status_label = marketing_status_label(&status, &tactic, None);
    let budget = resolved_object_field(&input, "budget")
        .map(|budget| marketing_budget_json(budget, shop_currency_code))
        .unwrap_or_else(|| old.get("budget").cloned().unwrap_or(Value::Null));
    let ad_spend = resolved_object_field(&input, "adSpend")
        .map(|obj| {
            json!({
                "amount": resolved_string_field(&obj, "amount")
                    .map(|a| normalize_money_amount(&a))
                    .unwrap_or_default(),
                "currencyCode": resolved_string_field(&obj, "currencyCode")
                    .unwrap_or_else(|| shop_currency_code.to_string())
            })
        })
        .unwrap_or_else(|| old.get("adSpend").cloned().unwrap_or(Value::Null));
    let scheduled_to_end_at = resolved_string_field(&input, "scheduledEnd")
        .map(Value::String)
        .unwrap_or_else(|| old["marketingEvent"]["scheduledToEndAt"].clone());
    let created_at = old["createdAt"].as_str().unwrap_or(timestamp);
    let started_at = old["marketingEvent"]["startedAt"]
        .as_str()
        .unwrap_or(timestamp);
    let app = marketing_app_json(api_client_id.as_deref());
    json!({
        "__typename": "MarketingActivity",
        "id": id,
        "apiClientId": api_client_id,
        "title": title,
        "remoteId": remote_id_value.clone(),
        "createdAt": created_at,
        "updatedAt": timestamp,
        "status": status,
        "statusLabel": status_label,
        "targetStatus": null,
        "tactic": tactic,
        "marketingChannelType": channel_type,
        "sourceAndMedium": source_medium.clone(),
        "isExternal": true,
        "inMainWorkflowVersion": false,
        "urlParameterValue": url_parameter_value,
        "parentRemoteId": resolved_string_field(&input, "parentRemoteId")
            .map(Value::String)
            .unwrap_or_else(|| old.get("parentRemoteId").cloned().unwrap_or(Value::Null)),
        "hierarchyLevel": resolved_string_field(&input, "hierarchyLevel")
            .map(Value::String)
            .unwrap_or_else(|| old.get("hierarchyLevel").cloned().unwrap_or(Value::Null)),
        "utmParameters": {
            "campaign": campaign_value.clone(),
            "source": source_value.clone(),
            "medium": medium_value.clone()
        },
        "budget": budget,
        "adSpend": ad_spend,
        "app": app,
        "marketingEvent": {
            "__typename": "MarketingEvent",
            "id": event_id,
            "type": tactic,
            "remoteId": remote_id_value,
            "channelHandle": channel_handle,
            "startedAt": started_at,
            "endedAt": if matches!(status.as_str(), "INACTIVE" | "DELETED_EXTERNALLY") { json!(timestamp) } else { Value::Null },
            "scheduledToEndAt": scheduled_to_end_at,
            "manageUrl": remote_url,
            "previewUrl": preview_url,
            "utmCampaign": campaign_value,
            "utmMedium": medium_value,
            "utmSource": source_value,
            "description": title,
            "marketingChannelType": channel_type,
            "sourceAndMedium": source_medium
        }
    })
}

pub(in crate::proxy) fn native_marketing_activity_from_input(
    id: &str,
    mut input: BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    api_client_id: Option<String>,
    timestamp: &str,
    shop_currency_code: &str,
) -> Value {
    if !input.contains_key("title") {
        if let Some(title) = input.get("marketingActivityTitle").cloned() {
            input.insert("title".to_string(), title);
        }
    }
    let target_status = resolved_string_field(&input, "targetStatus")
        .map(Value::String)
        .unwrap_or_else(|| {
            existing
                .and_then(|old| old.get("targetStatus").cloned())
                .unwrap_or(Value::Null)
        });
    let mut activity = marketing_activity_from_input(
        id,
        input,
        existing,
        api_client_id,
        None,
        timestamp,
        shop_currency_code,
    );
    activity["isExternal"] = json!(false);
    activity["inMainWorkflowVersion"] = json!(true);
    activity["targetStatus"] = target_status;
    activity["marketingEvent"] = Value::Null;
    if let (Some(status), Some(tactic)) = (activity["status"].as_str(), activity["tactic"].as_str())
    {
        activity["statusLabel"] = json!(marketing_status_label(
            status,
            tactic,
            activity["targetStatus"].as_str(),
        ));
    }
    activity
}

pub(in crate::proxy) fn native_marketing_activity_extension_error(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let extension_id =
        resolved_string_field(input, "marketingActivityExtensionId").unwrap_or_default();
    let tail = resource_id_tail(&extension_id);
    let missing_zero_id = !tail.is_empty()
        && tail
            .chars()
            .all(|character| character == '0' || character == '-');
    if extension_id.is_empty() || missing_zero_id {
        Some(user_error_omit_code(
            ["input", "marketingActivityExtensionId"],
            "Could not find the marketing extension",
            None,
        ))
    } else {
        None
    }
}

pub(in crate::proxy) fn marketing_budget_json(
    input: BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    let total = resolved_object_field(&input, "total").unwrap_or_default();
    json!({
        "budgetType": resolved_string_field(&input, "budgetType").unwrap_or_else(|| "DAILY".to_string()),
        "total": {
            "amount": resolved_string_field(&total, "amount").unwrap_or_else(|| "0.00".to_string()),
            "currencyCode": resolved_string_field(&total, "currencyCode").unwrap_or_else(|| shop_currency_code.to_string())
        }
    })
}

pub(in crate::proxy) fn marketing_engagement_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    activity: Option<&Value>,
    shop_currency_code: &str,
) -> Value {
    let money = |key: &str| marketing_money_json(input, key, shop_currency_code);
    json!({
        "__typename": "MarketingEngagement",
        "occurredOn": resolved_string_field(input, "occurredOn"),
        "utcOffset": resolved_string_field(input, "utcOffset"),
        "isCumulative": resolved_bool_field(input, "isCumulative"),
        "impressionsCount": resolved_int_field(input, "impressionsCount"),
        "viewsCount": resolved_int_field(input, "viewsCount"),
        "clicksCount": resolved_int_field(input, "clicksCount"),
        "uniqueClicksCount": resolved_int_field(input, "uniqueClicksCount"),
        "adSpend": money("adSpend"),
        "sales": money("sales"),
        "orders": resolved_string_field(input, "orders"),
        "primaryConversions": resolved_string_field(input, "primaryConversions"),
        "allConversions": resolved_string_field(input, "allConversions"),
        "firstTimeCustomers": resolved_string_field(input, "firstTimeCustomers"),
        "returningCustomers": resolved_string_field(input, "returningCustomers"),
        "marketingActivity": activity.cloned().unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn marketing_money_json(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
    shop_currency_code: &str,
) -> Value {
    let Some(obj) = resolved_object_field(input, key) else {
        return Value::Null;
    };
    json!({
        "amount": resolved_string_field(&obj, "amount").unwrap_or_default(),
        "currencyCode": resolved_string_field(&obj, "currencyCode").unwrap_or_else(|| shop_currency_code.to_string())
    })
}

pub(in crate::proxy) fn marketing_money_currency(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Option<String> {
    resolved_object_field(input, key).and_then(|obj| resolved_string_field(&obj, "currencyCode"))
}

pub(in crate::proxy) fn has_marketing_currency_mismatch(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let mut currencies = BTreeSet::new();
    if let Some(c) = resolved_object_field(input, "budget")
        .and_then(|b| resolved_object_field(&b, "total"))
        .and_then(|t| resolved_string_field(&t, "currencyCode"))
    {
        currencies.insert(c);
    }
    if let Some(c) = marketing_money_currency(input, "adSpend") {
        currencies.insert(c);
    }
    currencies.len() > 1
}

pub(in crate::proxy) fn has_engagement_currency_mismatch(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    let mut currencies = BTreeSet::new();
    for key in ["adSpend", "sales"] {
        if let Some(c) = marketing_money_currency(input, key) {
            currencies.insert(c);
        }
    }
    currencies.len() > 1
}

pub(in crate::proxy) fn invalid_marketing_url_error(
    input: &BTreeMap<String, ResolvedValue>,
    _root: &str,
) -> Option<Value> {
    for (field, value) in [
        ("remoteUrl", resolved_string_field(input, "remoteUrl")),
        ("previewUrl", resolved_string_field(input, "previewUrl")),
    ] {
        if let Some(url) = value {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Some(json!({
                    "field": ["input", field],
                    "message": format!("{} is not a valid URL", field),
                    "code": "INVALID"
                }));
            }
        }
    }
    None
}

/// How Shopify's URL scalar treats a marketing URL value.
pub(in crate::proxy) enum MarketingUrlError {
    /// The value parsed as a URL but its scheme is not http/https. Shopify rejects this
    /// at field-argument validation: a top-level INVALID_FIELD_ARGUMENTS error with the
    /// root field nulled out in `data`.
    WrongScheme,
    /// The value could not be coerced to the URL scalar at all (opaque scheme, no host,
    /// e.g. `data:`/`javascript:`). Shopify rejects this during variable coercion: a
    /// top-level INVALID_VARIABLE error and no execution of the operation.
    MissingHost {
        field: String,
        url: String,
        value: Value,
    },
}

enum UrlScheme {
    Valid,
    WrongScheme,
    MissingHost,
}

fn classify_url_scheme(url: &str) -> UrlScheme {
    let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
    if scheme == "http" || scheme == "https" {
        return UrlScheme::Valid;
    }
    let rest = &url[scheme.len()..];
    // A URI with an authority component (`scheme://host...`) coerces to the URL scalar
    // successfully (so does `mailto:`, a recognized hierarchical scheme); only then does
    // the http/https field validator reject it. Opaque schemes with no host fail coercion.
    if rest.starts_with("://") || scheme == "mailto" {
        UrlScheme::WrongScheme
    } else {
        UrlScheme::MissingHost
    }
}

/// Returns the marketing URL scheme error (if any) for the URL-typed fields of an external
/// marketing activity input, modelling Shopify's URL scalar coercion + scheme validation.
pub(in crate::proxy) fn marketing_url_scheme_error(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<MarketingUrlError> {
    for field in ["remoteUrl", "remotePreviewImageUrl"] {
        if let Some(url) = resolved_string_field(input, field) {
            match classify_url_scheme(&url) {
                UrlScheme::Valid => {}
                UrlScheme::WrongScheme => return Some(MarketingUrlError::WrongScheme),
                UrlScheme::MissingHost => {
                    return Some(MarketingUrlError::MissingHost {
                        field: field.to_string(),
                        url,
                        value: resolved_variables_json(input),
                    });
                }
            }
        }
    }
    None
}

pub(in crate::proxy) fn marketing_external_input_type_name(root_field: &str) -> &'static str {
    match root_field {
        "marketingActivityUpdateExternal" => "MarketingActivityUpdateExternalInput",
        "marketingActivityUpsertExternal" => "MarketingActivityUpsertExternalInput",
        _ => "MarketingActivityCreateExternalInput",
    }
}

pub(in crate::proxy) fn marketing_input_has_tactic(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    input.contains_key("tactic")
}

pub(in crate::proxy) fn marketing_input_tactic_is_storefront_app(
    input: &BTreeMap<String, ResolvedValue>,
) -> bool {
    input
        .get("tactic")
        .is_some_and(|value| matches!(value, ResolvedValue::String(t) if t == "STOREFRONT" || t == "STOREFRONT_APP"))
}

pub(in crate::proxy) fn marketing_activity_tactic_is_storefront_app(activity: &Value) -> bool {
    matches!(
        activity["tactic"].as_str(),
        Some("STOREFRONT") | Some("STOREFRONT_APP")
    )
}

pub(in crate::proxy) fn marketing_status_label(
    status: &str,
    tactic: &str,
    target_status: Option<&str>,
) -> String {
    if target_status == Some("PAUSED") {
        return "Pausing".to_string();
    }
    match (status, tactic) {
        ("PENDING", "AD") => "In review",
        ("ACTIVE", "POST") => "Posting",
        ("ACTIVE", _) => "Sending",
        ("PAUSED", _) => "Paused",
        ("INACTIVE", "POST") => "Posted",
        ("INACTIVE", "NEWSLETTER") => "Sent",
        ("INACTIVE", _) => "Ended",
        ("DELETED_EXTERNALLY", _) => "Deleted",
        ("UNDEFINED", _) => "Undefined",
        _ => status,
    }
    .to_string()
}

pub(in crate::proxy) fn marketing_source_and_medium(
    channel: &str,
    tactic: &str,
    referring_domain: Option<&str>,
) -> Option<String> {
    let value = match (channel, tactic, referring_domain) {
        ("EMAIL", "ABANDONED_CART", _) => "Abandoned cart email",
        ("SEARCH", "AFFILIATE", _) => "Affiliate link",
        ("DISPLAY", "LOYALTY", _) => "Loyalty program",
        ("DISPLAY", "RETARGETING", Some("facebook.com")) => "Facebook retargeting ad",
        ("DISPLAY", "RETARGETING", _) => "Retargeting ad",
        ("SEARCH", "MESSAGE", Some("facebook.com")) => "Message via Facebook Messenger",
        ("SEARCH", "MESSAGE", Some("twitter.com")) => "Twitter message",
        ("SEARCH", "AD", Some("instagram.com")) => "Instagram ad",
        ("SEARCH", "AD", Some(domain)) => return Some(format!("{domain} ad")),
        ("SEARCH", "AD", _) => "Search ad",
        (_, "AD", _) => "Ad",
        ("EMAIL", "NEWSLETTER", _) => "Email newsletter",
        _ => return None,
    };
    Some(value.to_string())
}

impl DraftProxy {
    pub(in crate::proxy) fn marketing_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "marketingActivity" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                    let query =
                        resolved_string_field(&field.arguments, "query").unwrap_or_default();
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
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
                    let query =
                        resolved_string_field(&field.arguments, "query").unwrap_or_default();
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
                Some(Value::Null)
            } else if matches!(
                field.name.as_str(),
                "marketingActivities" | "marketingEvents"
            ) {
                Some(value)
            } else {
                Some(selected_json(&value, &field.selection))
            }
        })
    }

    pub(in crate::proxy) fn marketing_mutation(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
    ) -> Response {
        let mut top_errors: Vec<Value> = Vec::new();
        let mut omit_data = false;
        let data = root_payload_json(fields, |field| {
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
                        return Some(Value::Null);
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
                        return None;
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
                "marketingEngagementCreate" => self.marketing_engagement_create(field, request),
                "marketingEngagementsDelete" => self.marketing_engagements_delete(field, request),
                "marketingActivityCreate" => self.marketing_create_native(field, request),
                "marketingActivityUpdate" => self.marketing_update_native(field, request),
                _ => Value::Null,
            };
            Some(value)
        });
        let mut body = if omit_data {
            json!({})
        } else {
            json!({ "data": data })
        };
        if !top_errors.is_empty() {
            body["errors"] = Value::Array(top_errors);
        }
        ok_json(body)
    }

    pub(in crate::proxy) fn marketing_create_native(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(error) = native_marketing_activity_extension_error(&input) {
            return selected_json(
                &json!({ "marketingActivity": null, "redirectPath": null, "userErrors": [error] }),
                &field.selection,
            );
        }
        let id = self.next_proxy_synthetic_gid("MarketingActivity");
        let timestamp = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let activity = native_marketing_activity_from_input(
            &id,
            input,
            None,
            request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .cloned(),
            &timestamp,
            &shop_currency_code,
        );
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        selected_json(
            &json!({ "marketingActivity": activity, "redirectPath": null, "userErrors": [] }),
            &field.selection,
        )
    }

    pub(in crate::proxy) fn marketing_update_native(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id")
            .unwrap_or_else(|| self.next_proxy_synthetic_gid("MarketingActivity"));
        let existing = self.store.staged.marketing_activities.get(&id).cloned();
        let timestamp = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let activity = native_marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .cloned(),
            &timestamp,
            &shop_currency_code,
        );
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        selected_json(
            &json!({ "marketingActivity": activity, "redirectPath": "/admin/marketing", "userErrors": [] }),
            &field.selection,
        )
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
            let remote = resolved_string_field(&field.arguments, "remoteId").unwrap_or_default();
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
        let existing_id = resolved_string_field(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_field(&field.arguments, "remoteId")
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
                vec![user_error(Value::Null, "Cannot perform this operation because a job to delete all external activities has been enqueued, which happens either from calling the marketingActivitiesDeleteAllExternal mutation or as a result of an app uninstall. Please either check the status of the job returned by the mutation or try again later.", Some("DELETE_JOB_ENQUEUED"))],
            );
        }
        if !input.contains_key("utm")
            && !input.contains_key("urlParameterValue")
            && create_if_missing
        {
            return marketing_activity_payload(
                None,
                vec![user_error(["input"], "Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.", Some("NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER"))],
            );
        }
        if has_marketing_currency_mismatch(&input) {
            return marketing_activity_payload(
                None,
                vec![user_error(
                    ["input"],
                    "Currency code is not matching between budget and ad spend",
                    None,
                )],
            );
        }
        if let Some(err) = invalid_marketing_url_error(&input, &field.name) {
            // Top-level GraphQL coercion in Shopify; parity compares errors for these cases.
            return marketing_activity_payload(None, vec![err]);
        }
        if create_if_missing
            && existing_id.is_none()
            && resolved_string_field(&input, "channelHandle")
                .is_some_and(|handle| !non_empty_channel_handle(&handle))
        {
            return marketing_activity_payload(
                None,
                vec![user_error(["input"], "The channel handle is not recognized. Please contact your partner manager for more information.", Some("INVALID_CHANNEL_HANDLE"))],
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
                    vec![user_error(
                        ["input"],
                        "Validation failed: Remote ID has already been taken",
                        None,
                    )],
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
                    vec![user_error(
                        ["input"],
                        "Validation failed: Utm campaign has already been taken",
                        None,
                    )],
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
                    vec![user_error(["input"], message, None)],
                );
            }
        }
        let new_marketing_event_id = if existing_id.is_none() {
            Some(shopify_gid(
                "MarketingEvent",
                self.next_synthetic_id + MARKETING_EVENT_ID_OFFSET,
            ))
        } else {
            None
        };
        let id = existing_id.unwrap_or_else(|| {
            let id = shopify_gid("MarketingActivity", self.next_synthetic_id);
            self.next_synthetic_id += 1;
            id
        });
        let existing = self.store.staged.marketing_activities.get(&id).cloned();
        let timestamp = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let activity = marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            request
                .headers
                .get("x-shopify-draft-proxy-api-client-id")
                .cloned(),
            new_marketing_event_id,
            &timestamp,
            &shop_currency_code,
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
                &json!({ "deletedMarketingActivityId": null, "userErrors": [user_error(Value::Null, "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.", Some("INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"))] }),
                &field.selection,
            );
        }
        let id = resolved_string_field(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_field(&field.arguments, "remoteId")
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
                    vec![user_error(Value::Null, "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.", Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"))],
                ),
                &field.selection,
            );
        }
        if selector_count > 1 {
            return selected_json(
                &marketing_engagement_payload(
                    None,
                    vec![user_error(Value::Null, "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.", Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"))],
                ),
                &field.selection,
            );
        }
        if let Some(channel) = resolved_string_field(&field.arguments, "channelHandle") {
            if !non_empty_channel_handle(&channel) {
                return selected_json(
                    &marketing_engagement_payload(
                        None,
                        vec![user_error(["channelHandle"], "The channel handle is not recognized. Please contact your partner manager for more information.", Some("INVALID_CHANNEL_HANDLE"))],
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
                    vec![user_error(
                        ["marketingEngagement"],
                        "Currency codes in the marketing engagement input do not match.",
                        Some("CURRENCY_CODE_MISMATCH_INPUT"),
                    )],
                ),
                &field.selection,
            );
        }
        if has_channel {
            let shop_currency_code = self.store.shop_currency_code();
            let engagement =
                marketing_engagement_from_input(&engagement_input, None, &shop_currency_code);
            return selected_json(
                &marketing_engagement_payload(Some(engagement), Vec::new()),
                &field.selection,
            );
        }
        let activity_id = if has_activity_id {
            resolved_string_field(&field.arguments, "marketingActivityId")
        } else {
            resolved_string_field(&field.arguments, "remoteId")
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
                    vec![user_error(["marketingEngagement"], "Marketing activity currency code does not match the currency code in the marketing engagement input.", Some("MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"))],
                ),
                &field.selection,
            );
        }
        let shop_currency_code = self.store.shop_currency_code();
        let engagement =
            marketing_engagement_from_input(&engagement_input, Some(activity), &shop_currency_code);
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
                vec![user_error(Value::Null, "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.", Some("INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"))],
            )
        } else if let Some(channel_handle) =
            resolved_string_field(&field.arguments, "channelHandle")
        {
            if non_empty_channel_handle(&channel_handle) {
                (
                    json!(format!(
                        "Engagement data associated to channel handle '{channel_handle}' marked for deletion"
                    )),
                    Vec::new(),
                )
            } else {
                (
                    Value::Null,
                    vec![user_error(["channelHandle"], "The channel handle is not recognized. Please contact your partner manager for more information.", Some("INVALID_CHANNEL_HANDLE"))],
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
        self.find_marketing_activity_matching(request, |record| {
            record["remoteId"].as_str() == Some(remote)
                || record["marketingEvent"]["remoteId"].as_str() == Some(remote)
        })
    }

    pub(in crate::proxy) fn find_marketing_activity_by_utm(
        &self,
        campaign: &str,
        request: &Request,
    ) -> Option<String> {
        self.find_marketing_activity_matching(request, |record| {
            record["utmParameters"]["campaign"].as_str() == Some(campaign)
        })
    }

    pub(in crate::proxy) fn find_marketing_activity_by_url_parameter(
        &self,
        url_parameter_value: &str,
        request: &Request,
    ) -> Option<String> {
        self.find_marketing_activity_matching(request, |record| {
            record["urlParameterValue"].as_str() == Some(url_parameter_value)
        })
    }

    fn find_marketing_activity_matching(
        &self,
        request: &Request,
        matches_record: impl Fn(&Value) -> bool,
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
                if !matches_record(record) {
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
            return Some(user_error(
                Value::Null,
                "Marketing activity is not valid, the associated marketing event does not exist.",
                Some("MARKETING_EVENT_DOES_NOT_EXIST"),
            ));
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
            return Some(user_error(
                ["input"],
                "Channel handle cannot be modified.",
                Some("IMMUTABLE_CHANNEL_HANDLE"),
            ));
        }
        if resolved_string_field(input, "urlParameterValue")
            .is_some_and(|value| existing["urlParameterValue"].as_str() != Some(value.as_str()))
        {
            return Some(user_error(
                ["input"],
                "URL parameter value cannot be modified.",
                Some("IMMUTABLE_URL_PARAMETER"),
            ));
        }
        let existing_utm_value = |field| {
            existing["utmParameters"][field]
                .as_str()
                .map(str::to_string)
        };
        if (input.contains_key("utm") || selector_utm.is_some())
            && (input_utm_value(input, selector_utm, "campaign") != existing_utm_value("campaign")
                || input_utm_value(input, selector_utm, "source") != existing_utm_value("source")
                || input_utm_value(input, selector_utm, "medium") != existing_utm_value("medium"))
        {
            return Some(user_error(
                ["input"],
                "UTM parameters cannot be modified.",
                Some("IMMUTABLE_UTM_PARAMETERS"),
            ));
        }
        if let Some(parent_remote_id) = resolved_string_field(input, "parentRemoteId") {
            let Some(parent_id) =
                self.find_marketing_activity_by_remote(&parent_remote_id, request)
            else {
                return Some(user_error(
                    ["input"],
                    "Remote ID does not correspond to an activity.",
                    Some("INVALID_REMOTE_ID"),
                ));
            };
            let existing_parent_remote_id = existing["parentRemoteId"].as_str().unwrap_or("");
            let existing_parent_id = if existing_parent_remote_id.is_empty() {
                None
            } else {
                self.find_marketing_activity_by_remote(existing_parent_remote_id, request)
            };
            if existing_parent_id.as_deref() != Some(parent_id.as_str()) {
                return Some(user_error(
                    ["input"],
                    "Parent ID cannot be modified.",
                    Some("IMMUTABLE_PARENT_ID"),
                ));
            }
        }
        if resolved_string_field(input, "hierarchyLevel").is_some_and(|hierarchy_level| {
            existing["hierarchyLevel"].as_str() != Some(hierarchy_level.as_str())
        }) {
            return Some(user_error(
                ["input"],
                "Hierarchy level cannot be modified.",
                Some("IMMUTABLE_HIERARCHY_LEVEL"),
            ));
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

fn input_utm_value(
    input: &BTreeMap<String, ResolvedValue>,
    selector_utm: Option<&BTreeMap<String, ResolvedValue>>,
    field: &str,
) -> Option<String> {
    match input.get("utm") {
        Some(ResolvedValue::Object(utm)) => resolved_string_field(utm, field),
        _ => selector_utm.and_then(|utm| resolved_string_field(utm, field)),
    }
}

fn marketing_activity_not_external_error() -> Value {
    user_error(
        Value::Null,
        "Marketing activity is not external.",
        Some("ACTIVITY_NOT_EXTERNAL"),
    )
}

fn marketing_activity_delete_not_external_error() -> Value {
    user_error(
        Value::Null,
        "The marketing activity must be an external activity.",
        Some("ACTIVITY_NOT_EXTERNAL"),
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
        Some(user_error(
            Value::Null,
            "Marketing activity is not valid, the associated marketing event does not exist.",
            Some("MARKETING_EVENT_DOES_NOT_EXIST")
        ))
    );
}
