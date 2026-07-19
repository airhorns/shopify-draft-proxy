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

pub(in crate::proxy) fn marketing_activity_from_input(
    id: &str,
    input: BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    api_client_id: Option<String>,
) -> Value {
    let old = existing.cloned().unwrap_or_else(|| json!({}));
    let title = resolved_string_field(&input, "title").unwrap_or_else(|| {
        old["title"]
            .as_str()
            .unwrap_or("Marketing activity")
            .to_string()
    });
    let remote_id = resolved_string_field(&input, "remoteId").unwrap_or_else(|| {
        old["remoteId"]
            .as_str()
            .unwrap_or("local-remote")
            .to_string()
    });
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
        .unwrap_or_else(|| {
            old_utm["campaign"]
                .as_str()
                .unwrap_or(&remote_id)
                .to_string()
        });
    let source = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "source"))
        .unwrap_or_else(|| {
            old_utm["source"]
                .as_str()
                .unwrap_or("newsletter")
                .to_string()
        });
    let medium = utm
        .as_ref()
        .and_then(|u| resolved_string_field(u, "medium"))
        .unwrap_or_else(|| old_utm["medium"].as_str().unwrap_or("email").to_string());
    let source_medium = marketing_source_and_medium(
        &channel_type,
        &tactic,
        resolved_string_field(&input, "referringDomain").as_deref(),
    );
    let numeric = resource_id_path_tail(id);
    let event_id = old["marketingEvent"]["id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "gid://shopify/MarketingEvent/{}",
                numeric.parse::<u64>().unwrap_or(1) + 1
            )
        });
    let status_label = marketing_status_label(&status, &tactic, None);
    let budget = resolved_object_field(&input, "budget")
        .map(marketing_budget_json)
        .unwrap_or_else(|| old.get("budget").cloned().unwrap_or(Value::Null));
    let ad_spend = resolved_object_field(&input, "adSpend")
        .map(|obj| {
            json!({
                "amount": resolved_string_field(&obj, "amount")
                    .map(|a| normalized_money_amount(&a))
                    .unwrap_or_default(),
                "currencyCode": resolved_string_field(&obj, "currencyCode")
                    .unwrap_or_else(|| "USD".to_string())
            })
        })
        .unwrap_or_else(|| old.get("adSpend").cloned().unwrap_or(Value::Null));
    let scheduled_to_end_at = resolved_string_field(&input, "scheduledEnd")
        .map(Value::String)
        .unwrap_or_else(|| old["marketingEvent"]["scheduledToEndAt"].clone());
    json!({
        "__typename": "MarketingActivity",
        "id": id,
        "apiClientId": api_client_id,
        "title": title,
        "remoteId": remote_id,
        "createdAt": old["createdAt"].as_str().unwrap_or("2026-05-05T00:00:00Z"),
        "updatedAt": "2026-05-05T00:00:00Z",
        "status": status,
        "statusLabel": status_label,
        "targetStatus": null,
        "tactic": tactic,
        "marketingChannelType": channel_type,
        "sourceAndMedium": source_medium,
        "isExternal": true,
        "inMainWorkflowVersion": false,
        "urlParameterValue": url_parameter_value,
        "parentRemoteId": resolved_string_field(&input, "parentRemoteId")
            .map(Value::String)
            .unwrap_or_else(|| old.get("parentRemoteId").cloned().unwrap_or(Value::Null)),
        "hierarchyLevel": resolved_string_field(&input, "hierarchyLevel")
            .map(Value::String)
            .unwrap_or_else(|| old.get("hierarchyLevel").cloned().unwrap_or(Value::Null)),
        "utmParameters": { "campaign": campaign, "source": source, "medium": medium },
        "budget": budget,
        "adSpend": ad_spend,
        "app": { "id": "gid://shopify/App/1", "title": "Draft proxy app" },
        "marketingEvent": {
            "__typename": "MarketingEvent",
            "id": event_id,
            "type": tactic,
            "remoteId": remote_id,
            "channelHandle": channel_handle,
            "startedAt": "2026-05-05T00:00:00Z",
            "endedAt": if matches!(status.as_str(), "INACTIVE" | "DELETED_EXTERNALLY") { json!("2026-05-05T00:00:00Z") } else { Value::Null },
            "scheduledToEndAt": scheduled_to_end_at,
            "manageUrl": remote_url,
            "previewUrl": preview_url,
            "utmCampaign": campaign,
            "utmMedium": medium,
            "utmSource": source,
            "description": title,
            "marketingChannelType": channel_type,
            "sourceAndMedium": source_medium
        }
    })
}

pub(in crate::proxy) fn marketing_budget_json(input: BTreeMap<String, ResolvedValue>) -> Value {
    let total = resolved_object_field(&input, "total").unwrap_or_default();
    json!({
        "budgetType": resolved_string_field(&input, "budgetType").unwrap_or_else(|| "DAILY".to_string()),
        "total": {
            "amount": resolved_string_field(&total, "amount").unwrap_or_else(|| "0.00".to_string()),
            "currencyCode": resolved_string_field(&total, "currencyCode").unwrap_or_else(|| "USD".to_string())
        }
    })
}

pub(in crate::proxy) fn marketing_engagement_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    activity: Option<&Value>,
    channel_handle: Option<&str>,
) -> Value {
    let money = |key: &str| marketing_money_json(input, key);
    json!({
        "__typename": "MarketingEngagement",
        "occurredOn": resolved_string_field(input, "occurredOn"),
        "utcOffset": resolved_string_field(input, "utcOffset"),
        "isCumulative": resolved_bool_field(input, "isCumulative"),
        "impressionsCount": resolved_int_field(input, "impressionsCount"),
        "viewsCount": resolved_int_field(input, "viewsCount"),
        "clicksCount": resolved_int_field(input, "clicksCount"),
        "uniqueClicksCount": resolved_int_field(input, "uniqueClicksCount"),
        "sharesCount": resolved_int_field(input, "sharesCount"),
        "favoritesCount": resolved_int_field(input, "favoritesCount"),
        "commentsCount": resolved_int_field(input, "commentsCount"),
        "unsubscribesCount": resolved_int_field(input, "unsubscribesCount"),
        "complaintsCount": resolved_int_field(input, "complaintsCount"),
        "failsCount": resolved_int_field(input, "failsCount"),
        "sendsCount": resolved_int_field(input, "sendsCount"),
        "uniqueViewsCount": resolved_int_field(input, "uniqueViewsCount"),
        "sessionsCount": resolved_int_field(input, "sessionsCount"),
        "adSpend": money("adSpend"),
        "sales": money("sales"),
        "orders": resolved_string_field(input, "orders"),
        "primaryConversions": resolved_string_field(input, "primaryConversions"),
        "allConversions": resolved_string_field(input, "allConversions"),
        "firstTimeCustomers": resolved_string_field(input, "firstTimeCustomers"),
        "returningCustomers": resolved_string_field(input, "returningCustomers"),
        "channelHandle": channel_handle,
        "marketingActivity": activity.cloned().unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn marketing_money_json(
    input: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Value {
    let Some(obj) = resolved_object_field(input, key) else {
        return Value::Null;
    };
    json!({
        "amount": resolved_string_field(&obj, "amount").unwrap_or_default(),
        "currencyCode": resolved_string_field(&obj, "currencyCode").unwrap_or_else(|| "USD".to_string())
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
) -> String {
    match (channel, tactic, referring_domain) {
        ("EMAIL", "ABANDONED_CART", _) => "Abandoned cart email",
        ("SEARCH", "AFFILIATE", _) => "Affiliate link",
        ("DISPLAY", "LOYALTY", _) => "Loyalty program",
        ("DISPLAY", "RETARGETING", Some("facebook.com")) => "Facebook retargeting ad",
        ("DISPLAY", "RETARGETING", _) => "Retargeting ad",
        ("SEARCH", "MESSAGE", Some("facebook.com")) => "Message via Facebook Messenger",
        ("SEARCH", "MESSAGE", Some("twitter.com")) => "Twitter message",
        ("SEARCH", "AD", Some("instagram.com")) => "Instagram ad",
        ("SEARCH", "AD", Some(domain)) => return format!("{domain} ad"),
        ("SEARCH", "AD", _) => "Search ad",
        (_, "AD", _) => "Ad",
        ("EMAIL", "NEWSLETTER", _) => "Email newsletter",
        _ => "Email newsletter",
    }
    .to_string()
}
