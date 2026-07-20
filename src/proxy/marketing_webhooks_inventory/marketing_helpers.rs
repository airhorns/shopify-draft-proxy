use super::*;

const MARKETING_CURSOR_METADATA_FIELD: &str = "__draftProxyCursor";

fn marketing_normalized_sort_string(value: Option<&str>) -> StagedSortValue {
    value
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn marketing_gid_tail_sort_value(id: Option<&str>) -> StagedSortValue {
    resource_id_tail_sort_value(id)
}

fn marketing_record_cursor(record: &Value) -> String {
    record[MARKETING_CURSOR_METADATA_FIELD]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            stable_local_connection_cursor("marketing", record["id"].as_str().unwrap_or("local"))
        })
}

fn marketing_record_with_cursor(mut record: Value, cursor: Option<String>) -> Value {
    if let (Some(cursor), Some(object)) = (cursor, record.as_object_mut()) {
        object.insert(MARKETING_CURSOR_METADATA_FIELD.to_string(), json!(cursor));
    }
    record
}

fn marketing_activity_staged_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let id = record["id"].as_str();
    let primary = match sort_key.unwrap_or("CREATED_AT") {
        "ID" => marketing_gid_tail_sort_value(id),
        "TITLE" => marketing_normalized_sort_string(record["title"].as_str()),
        _ => StagedSortValue::String(record["createdAt"].as_str().unwrap_or_default().to_string()),
    };
    vec![primary, marketing_gid_tail_sort_value(id)]
}

fn marketing_event_staged_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let id = record["id"].as_str();
    let primary = match sort_key.unwrap_or("ID") {
        "STARTED_AT" => {
            StagedSortValue::String(record["startedAt"].as_str().unwrap_or_default().to_string())
        }
        _ => marketing_gid_tail_sort_value(id),
    };
    vec![primary, marketing_gid_tail_sort_value(id)]
}

fn marketing_activity_connection(
    records: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    staged_connection_value_with_args(
        records,
        arguments,
        marketing_activity_search_decision,
        marketing_activity_staged_sort_key,
        Value::clone,
        marketing_record_cursor,
    )
}

fn marketing_event_connection(
    records: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    staged_connection_value_with_args(
        records,
        arguments,
        marketing_event_search_decision,
        marketing_event_staged_sort_key,
        Value::clone,
        marketing_record_cursor,
    )
}

fn marketing_activity_remote_identity(record: &Value) -> Option<String> {
    record
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.is_empty())
        .map(|title| format!("title:{title}"))
        .or_else(|| {
            record
                .get("remoteId")
                .and_then(Value::as_str)
                .or_else(|| {
                    record
                        .pointer("/marketingEvent/remoteId")
                        .and_then(Value::as_str)
                })
                .filter(|remote_id| !remote_id.is_empty())
                .map(|remote_id| format!("remoteId:{remote_id}"))
        })
}

fn marketing_event_remote_identity(record: &Value) -> Option<String> {
    record
        .get("remoteId")
        .and_then(Value::as_str)
        .filter(|remote_id| !remote_id.is_empty())
        .map(str::to_string)
}

fn reconcile_marketing_authoritative_rows(
    mut authoritative: Vec<ObservedConnectionRow>,
    local_records: &[Value],
    base_records: Vec<&Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
    remote_identity: fn(&Value) -> Option<String>,
) -> Vec<ObservedConnectionRow> {
    // Callers need not select `remoteId`, so the activity adapter can fall back
    // to caller-visible title only when it identifies exactly one staged row;
    // duplicate titles remain distinct.
    let mut local_candidates = BTreeMap::<String, Vec<&Value>>::new();
    for record in local_records {
        if let Some(remote) = remote_identity(record) {
            local_candidates.entry(remote).or_default().push(record);
        }
    }
    let local_by_remote = local_candidates
        .into_iter()
        .filter_map(|(remote, records)| (records.len() == 1).then(|| (remote, records[0])))
        .collect::<BTreeMap<_, _>>();
    for row in &mut authoritative {
        if let Some(local) =
            remote_identity(&row.node).and_then(|remote| local_by_remote.get(&remote))
        {
            row.node = (*local).clone();
        }
    }

    let boundary = ["after", "before"]
        .into_iter()
        .find_map(|name| resolved_string_field(arguments, name));
    if let Some(boundary) = boundary.filter(|boundary| {
        !authoritative
            .iter()
            .any(|row| row.cursor.as_deref() == Some(boundary.as_str()))
    }) {
        if let Some((base, local)) = base_records.iter().find_map(|base| {
            (base[MARKETING_CURSOR_METADATA_FIELD].as_str() == Some(boundary.as_str()))
                .then(|| remote_identity(base))
                .flatten()
                .and_then(|remote| local_by_remote.get(&remote).map(|local| (*base, *local)))
        }) {
            authoritative.push(ObservedConnectionRow {
                cursor: base[MARKETING_CURSOR_METADATA_FIELD]
                    .as_str()
                    .map(str::to_string),
                node: local.clone(),
            });
        }
    }
    authoritative
}

pub(in crate::proxy) fn marketing_activity_payload(
    activity: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "marketingActivity": activity.unwrap_or(Value::Null), "userErrors": user_errors })
}

fn marketing_activity_error(error: Value) -> Value {
    marketing_activity_payload(None, vec![error])
}
fn marketing_input_error(message: &str, code: Option<&str>) -> Value {
    marketing_activity_error(user_error(["input"], message, code))
}
pub(in crate::proxy) fn marketing_engagement_payload(
    engagement: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "marketingEngagement": engagement.unwrap_or(Value::Null), "userErrors": user_errors })
}

pub(in crate::proxy) fn marketing_activity_missing_error() -> Value {
    user_error(
        Value::Null,
        "Marketing activity does not exist.",
        Some("MARKETING_ACTIVITY_DOES_NOT_EXIST"),
    )
}

pub(in crate::proxy) fn marketing_activity_child_events_error() -> Value {
    user_error(
        Value::Null,
        "This activity has child activities and thus cannot be deleted. Child activities must be deleted before a parent activity.",
        Some("CANNOT_DELETE_ACTIVITY_WITH_CHILD_EVENTS"),
    )
}

pub(in crate::proxy) fn marketing_activity_cannot_update_tactic_to_storefront_error() -> Value {
    user_error(
        ["input"],
        "You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.",
        Some("CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP"),
    )
}

pub(in crate::proxy) fn marketing_activity_cannot_update_tactic_from_storefront_error() -> Value {
    user_error(
        ["input"],
        "You can not update an activity tactic from STOREFRONT_APP.",
        Some("CANNOT_UPDATE_TACTIC_IF_ORIGINALLY_STOREFRONT_APP"),
    )
}

pub(in crate::proxy) fn marketing_event_missing_error() -> Value {
    user_error(
        Value::Null,
        "Marketing event does not exist.",
        Some("MARKETING_EVENT_DOES_NOT_EXIST"),
    )
}

const MARKETING_EVENT_ID_OFFSET: u64 = 1_000_000;

#[derive(Clone)]
pub(in crate::proxy) struct MarketingActivityAppContext {
    api_client_id: Option<String>,
    app: Value,
}

fn marketing_app_json(app: Option<&Value>, fallback_app_id: &str) -> Value {
    let app_id = app
        .and_then(|app| app.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback_app_id.to_string());
    let handle = app
        .and_then(|app| app.get("handle"))
        .and_then(Value::as_str)
        .filter(|handle| !handle.trim().is_empty())
        .unwrap_or("shopify-draft-proxy");
    let title = app
        .and_then(|app| app.get("title"))
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .filter(|title| !resource_id_tail(&app_id).eq_ignore_ascii_case(title.trim()))
        .unwrap_or(handle);
    json!({ "__typename": "App", "id": app_id, "handle": handle, "title": title })
}

fn non_empty_channel_handle(handle: &str) -> bool {
    !handle.trim().is_empty()
}

fn recognized_marketing_channel_handle(handle: &str) -> bool {
    matches!(handle.trim(), "email")
}

fn old_pointer_string(old: &Value, pointer: &str) -> Option<String> {
    old.pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
}
pub(in crate::proxy) fn marketing_activity_from_input(
    id: &str,
    input: BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    app_context: MarketingActivityAppContext,
    new_marketing_event_id: Option<String>,
    timestamp: &str,
    shop_currency_code: &str,
) -> Value {
    let MarketingActivityAppContext { api_client_id, app } = app_context;
    let old = existing.cloned().unwrap_or_else(|| json!({}));
    let utm = resolved_object_field(&input, "utm");
    macro_rules! input_or_old {
        ($key:literal, $pointer:literal) => {
            resolved_string_field(&input, $key).or_else(|| old_pointer_string(&old, $pointer))
        };
    }
    macro_rules! input_or_old_value {
        ($key:literal, $pointer:literal) => {
            input_or_old!($key, $pointer)
                .map(Value::String)
                .unwrap_or_else(|| old.pointer($pointer).cloned().unwrap_or(Value::Null))
        };
    }
    macro_rules! utm_or_old {
        ($key:literal, $pointer:literal) => {
            utm.as_ref()
                .and_then(|u| resolved_string_field(u, $key))
                .or_else(|| old_pointer_string(&old, $pointer))
        };
    }
    let title =
        input_or_old!("title", "/title").unwrap_or_else(|| "Marketing activity".to_string());
    let remote_id = resolved_string_field(&input, "remoteId").or_else(|| {
        old.pointer("/marketingEvent/remoteId")
            .or_else(|| old.pointer("/remoteId"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let status = input_or_old!("status", "/status").unwrap_or_else(|| "UNDEFINED".to_string());
    let tactic = input_or_old!("tactic", "/tactic").unwrap_or_else(|| "NEWSLETTER".to_string());
    let channel_type = input_or_old!("marketingChannelType", "/marketingChannelType")
        .unwrap_or_else(|| "EMAIL".to_string());
    let remote_url = input_or_old!("remoteUrl", "/marketingEvent/manageUrl");
    let preview_url = input_or_old!("remotePreviewImageUrl", "/marketingEvent/previewUrl");
    let url_parameter_value = input_or_old!("urlParameterValue", "/urlParameterValue");
    let channel_handle = input_or_old_value!("channelHandle", "/marketingEvent/channelHandle");
    let campaign = utm_or_old!("campaign", "/utmParameters/campaign");
    let source = utm_or_old!("source", "/utmParameters/source");
    let medium = utm_or_old!("medium", "/utmParameters/medium");
    let source_medium = marketing_source_and_medium(
        &channel_type,
        &tactic,
        resolved_string_field(&input, "referringDomain").as_deref(),
    );
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
    let scheduled_to_end_at =
        input_or_old_value!("scheduledEnd", "/marketingEvent/scheduledToEndAt");
    let created_at = old["createdAt"].as_str().unwrap_or(timestamp);
    let started_at = old["marketingEvent"]["startedAt"]
        .as_str()
        .unwrap_or(timestamp);
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
        "sourceAndMedium": source_medium,
        "isExternal": true,
        "inMainWorkflowVersion": false,
        "urlParameterValue": url_parameter_value,
        "parentRemoteId": input_or_old_value!("parentRemoteId", "/parentRemoteId"),
        "hierarchyLevel": input_or_old_value!("hierarchyLevel", "/hierarchyLevel"),
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
    app_context: MarketingActivityAppContext,
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
        app_context,
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
        (
            "remotePreviewImageUrl",
            resolved_string_field(input, "remotePreviewImageUrl"),
        ),
    ] {
        if let Some(url) = value {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Some(user_error(
                    vec!["input".to_string(), field.to_string()],
                    &format!("{field} is not a valid URL"),
                    Some("INVALID"),
                ));
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
    let referring_domain = referring_domain.and_then(marketing_referring_domain_label);
    match tactic {
        "ABANDONED_CART" => "Abandoned cart email".to_string(),
        "AFFILIATE" => "Affiliate link".to_string(),
        "LOYALTY" => "Loyalty program".to_string(),
        "RETARGETING" => referring_domain
            .map(|domain| format!("{domain} retargeting ad"))
            .unwrap_or_else(|| "Retargeting ad".to_string()),
        "MESSAGE" => match referring_domain.as_deref() {
            Some("Facebook") => "Message via Facebook Messenger".to_string(),
            Some(domain) => format!("{domain} message"),
            None => "Message".to_string(),
        },
        "AD" => referring_domain
            .map(|domain| format!("{domain} ad"))
            .unwrap_or_else(|| marketing_channel_tactic_source_and_medium(channel, tactic)),
        _ => marketing_channel_tactic_source_and_medium(channel, tactic),
    }
}

fn marketing_referring_domain_label(domain: &str) -> Option<String> {
    let trimmed = domain.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowercase = trimmed.to_ascii_lowercase();
    let normalized = lowercase.strip_prefix("www.").unwrap_or(&lowercase);
    match normalized {
        "facebook" | "facebook.com" => Some("Facebook".to_string()),
        "twitter" | "twitter.com" => Some("Twitter".to_string()),
        "instagram" | "instagram.com" => Some("Instagram".to_string()),
        domain if domain.ends_with(".facebook.com") => Some("Facebook".to_string()),
        domain if domain.ends_with(".twitter.com") => Some("Twitter".to_string()),
        domain if domain.ends_with(".instagram.com") => Some("Instagram".to_string()),
        _ => Some(trimmed.to_string()),
    }
}

fn marketing_channel_tactic_source_and_medium(channel: &str, tactic: &str) -> String {
    let channel = marketing_titleize_enum(channel);
    let tactic = marketing_humanize_enum(tactic);
    match (channel.is_empty(), tactic.is_empty()) {
        (true, true) => String::new(),
        (true, false) => marketing_titleize_phrase(&tactic),
        (false, true) => channel,
        (false, false) => format!("{channel} {tactic}"),
    }
}

fn marketing_titleize_enum(value: &str) -> String {
    marketing_titleize_phrase(&marketing_humanize_enum(value))
}

fn marketing_titleize_phrase(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => {
            let mut output = String::new();
            output.extend(first.to_uppercase());
            output.push_str(chars.as_str());
            output
        }
        None => String::new(),
    }
}

fn marketing_humanize_enum(value: &str) -> String {
    value
        .trim()
        .split('_')
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn marketing_record_id(record: &Value) -> Option<String> {
    record.get("id").and_then(Value::as_str).map(str::to_string)
}

fn marketing_connection_entries(connection: &Value) -> Vec<(Value, Option<String>)> {
    let mut cursors_by_id = BTreeMap::<String, String>::new();
    let mut edge_entries = Vec::<(Value, Option<String>)>::new();

    for edge in connection
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(node) = edge.get("node").filter(|node| node.is_object()) else {
            continue;
        };
        let cursor = edge
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let (Some(id), Some(cursor)) = (marketing_record_id(node), cursor.clone()) {
            cursors_by_id.insert(id, cursor);
        }
        edge_entries.push((node.clone(), cursor));
    }

    let mut entries = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for node in connection
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| node.is_object())
    {
        let id = marketing_record_id(node);
        if let Some(id) = &id {
            seen_ids.insert(id.clone());
        }
        let cursor = id.and_then(|id| cursors_by_id.get(&id).cloned());
        entries.push((node.clone(), cursor));
    }

    for (node, cursor) in edge_entries {
        if marketing_record_id(&node).is_some_and(|id| seen_ids.contains(&id)) {
            continue;
        }
        entries.push((node, cursor));
    }

    entries
}

fn merge_observed_marketing_value(target: &mut Value, observed: &Value) {
    match (target, observed) {
        (Value::Object(target), Value::Object(observed)) => {
            for (key, value) in observed {
                match target.get_mut(key) {
                    Some(existing) => merge_observed_marketing_value(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, observed) => {
            *target = observed.clone();
        }
    }
}

impl DraftProxy {
    fn marketing_app_context_for_request(&self, request: &Request) -> MarketingActivityAppContext {
        let request_installation = current_app_installation_from_request(request);
        let request_app_id = app_id_from_installation(&request_installation)
            .unwrap_or_else(|| request_app_gid(request));
        let api_client_id = request
            .headers
            .get("x-shopify-draft-proxy-api-client-id")
            .cloned();
        if let Some(installed_app_id) =
            self.current_app_installation_app_id_for_request(&request_app_id)
        {
            if let Some(installation) = self.store.staged.installed_apps.get(&installed_app_id) {
                return MarketingActivityAppContext {
                    api_client_id,
                    app: marketing_app_json(installation.get("app"), &request_app_id),
                };
            }
        }
        MarketingActivityAppContext {
            api_client_id,
            app: marketing_app_json(request_installation.get("app"), &request_app_id),
        }
    }

    pub(super) fn marketing_query_outcome(
        &mut self,
        request: &Request,
        field: &MarketingRootInput,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::LiveHybrid {
            let mut outcome = if field.operation_has_local_boundary {
                ResolverOutcome::value(Value::Null)
            } else {
                self.cached_or_forward_upstream_root_outcome(request, &field.response_key)
            };
            if !field.operation_has_local_boundary && outcome.errors.is_empty() {
                self.observe_marketing_upstream_response(
                    field,
                    &json!({ "data": { (&field.response_key): outcome.value.clone() } }),
                );
            }
            if (!self.store.has_marketing_overlay_state() && !field.operation_has_local_boundary)
                || !outcome.errors.is_empty()
            {
                return outcome;
            }
            if matches!(
                field.name.as_str(),
                "marketingActivities" | "marketingEvents"
            ) {
                let staged_impact = self
                    .store
                    .staged
                    .marketing_activities
                    .len()
                    .saturating_add(self.store.staged.marketing_activities.tombstones.len());
                let required_node_selection = if field.name == "marketingActivities" {
                    "id title createdAt updatedAt remoteId isExternal apiClientId marketingEvent { id remoteId startedAt }"
                } else {
                    "id type remoteId startedAt endedAt scheduledToEndAt"
                };
                let authoritative = if field.operation_has_local_boundary
                    || outcome.value.get("nodes").is_some()
                    || outcome.value.get("edges").is_some()
                {
                    self.bounded_connection_overlay_window(
                        request,
                        ConnectionOverlayRequest {
                            root_name: &field.name,
                            arguments: &field.arguments,
                            raw_arguments: &field.raw_arguments,
                            selection: &field.selection,
                            variable_definitions: &field.variable_definitions,
                            variables: &field.variables,
                            required_node_selection,
                        },
                        &outcome.value,
                        staged_impact,
                    )
                } else {
                    outcome.value.clone()
                };
                self.observe_marketing_upstream_response(
                    field,
                    &json!({ "data": { (&field.response_key): authoritative.clone() } }),
                );
                outcome.value =
                    self.marketing_overlay_connection_value(request, field, &authoritative);
            } else {
                outcome.value = self.marketing_query_value(request, field);
            }
            outcome.value_source = crate::admin_graphql::ResolverValueSource::Local;
            return outcome;
        }
        ResolverOutcome::value(self.marketing_query_value(request, field))
    }

    fn marketing_overlay_connection_value(
        &self,
        request: &Request,
        field: &MarketingRootInput,
        authoritative: &Value,
    ) -> Value {
        let mut authoritative_rows = observed_connection_rows(authoritative);
        if authoritative_rows.is_empty() {
            let records = if field.name == "marketingActivities" {
                self.store.base.marketing_activities.ordered_values()
            } else {
                self.store.base.marketing_events.ordered_values()
            };
            authoritative_rows = records
                .into_iter()
                .map(|record| ObservedConnectionRow {
                    cursor: record[MARKETING_CURSOR_METADATA_FIELD]
                        .as_str()
                        .map(str::to_string),
                    node: record.clone(),
                })
                .collect();
        }
        match field.name.as_str() {
            "marketingActivities" => {
                let remote_ids = resolved_string_list_arg(&field.arguments, "remoteIds");
                let ids = resolved_string_list_arg(&field.arguments, "marketingActivityIds");
                let local_records = self
                    .store
                    .staged
                    .marketing_activities
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                let authoritative_rows = reconcile_marketing_authoritative_rows(
                    authoritative_rows,
                    &local_records,
                    self.store.base.marketing_activities.ordered_values(),
                    &field.arguments,
                    marketing_activity_remote_identity,
                );
                overlay_connection_value(
                    ConnectionOverlayInput {
                        authoritative: authoritative_rows,
                        local_records,
                        tombstones: &self.store.staged.marketing_activities.tombstones,
                        arguments: &field.arguments,
                        source_page_info: &authoritative["pageInfo"],
                    },
                    |record, query| {
                        let id = record["id"].as_str().unwrap_or_default();
                        let matches_ids =
                            ids.is_empty() || ids.iter().any(|candidate| candidate == id);
                        let matches_remote_ids = remote_ids.is_empty()
                            || remote_ids.iter().any(|candidate| {
                                record["remoteId"].as_str() == Some(candidate.as_str())
                                    || record["marketingEvent"]["remoteId"].as_str()
                                        == Some(candidate.as_str())
                            });
                        if matches_ids
                            && matches_remote_ids
                            && !self.marketing_activity_hidden_by_delete_all(record, request)
                        {
                            marketing_activity_search_decision(record, query)
                        } else {
                            StagedSearchDecision::NoMatch
                        }
                    },
                    marketing_activity_staged_sort_key,
                    Value::clone,
                    |record| {
                        stable_local_connection_cursor(
                            "marketingActivities",
                            record["id"].as_str().unwrap_or_default(),
                        )
                    },
                )
            }
            "marketingEvents" => {
                let local_records = self
                    .store
                    .staged
                    .marketing_activities
                    .values()
                    .filter_map(|activity| {
                        activity
                            .get("marketingEvent")
                            .filter(|event| event.is_object())
                            .cloned()
                    })
                    .collect::<Vec<_>>();
                let authoritative_rows = reconcile_marketing_authoritative_rows(
                    authoritative_rows,
                    &local_records,
                    self.store.base.marketing_events.ordered_values(),
                    &field.arguments,
                    marketing_event_remote_identity,
                );
                let tombstones = self
                    .store
                    .staged
                    .marketing_activities
                    .tombstones
                    .iter()
                    .filter_map(|id| self.store.base.marketing_activities.get(id))
                    .filter_map(|activity| {
                        activity
                            .pointer("/marketingEvent/id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect::<BTreeSet<_>>();
                overlay_connection_value(
                    ConnectionOverlayInput {
                        authoritative: authoritative_rows,
                        local_records,
                        tombstones: &tombstones,
                        arguments: &field.arguments,
                        source_page_info: &authoritative["pageInfo"],
                    },
                    marketing_event_search_decision,
                    marketing_event_staged_sort_key,
                    Value::clone,
                    |record| {
                        stable_local_connection_cursor(
                            "marketingEvents",
                            record["id"].as_str().unwrap_or_default(),
                        )
                    },
                )
            }
            _ => Value::Null,
        }
    }

    fn observe_marketing_upstream_response(&mut self, field: &MarketingRootInput, body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        let value = data
            .get(&field.response_key)
            .or_else(|| data.get(&field.name))
            .unwrap_or(&Value::Null);
        match field.name.as_str() {
            "marketingActivity" => self.observe_base_marketing_activity(value.clone(), None),
            "marketingActivities" => {
                for (activity, cursor) in marketing_connection_entries(value) {
                    self.observe_base_marketing_activity(activity, cursor);
                }
            }
            "marketingEvent" => self.observe_base_marketing_event(value.clone(), None),
            "marketingEvents" => {
                for (event, cursor) in marketing_connection_entries(value) {
                    self.observe_base_marketing_event(event, cursor);
                }
            }
            _ => {}
        }
    }

    fn observe_base_marketing_activity(&mut self, activity: Value, cursor: Option<String>) {
        let Some(id) = marketing_record_id(&activity) else {
            return;
        };
        let mut activity = marketing_record_with_cursor(activity, cursor);
        if let Some(existing) = self.store.base.marketing_activities.get(&id) {
            let mut merged = existing.clone();
            merge_observed_marketing_value(&mut merged, &activity);
            activity = merged;
        }
        if let Some(event) = activity
            .get("marketingEvent")
            .filter(|event| event.is_object())
            .cloned()
        {
            self.observe_base_marketing_event(event, None);
        }
        self.store.base.marketing_activities.insert(id, activity);
    }

    fn observe_base_marketing_event(&mut self, event: Value, cursor: Option<String>) {
        let Some(id) = marketing_record_id(&event) else {
            return;
        };
        let mut event = marketing_record_with_cursor(event, cursor);
        if let Some(existing) = self.store.base.marketing_events.get(&id) {
            let mut merged = existing.clone();
            merge_observed_marketing_value(&mut merged, &event);
            event = merged;
        }
        self.store.base.marketing_events.insert(id, event);
    }

    fn marketing_query_value(&self, request: &Request, field: &MarketingRootInput) -> Value {
        match field.name.as_str() {
            "marketingActivity" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.store
                    .marketing_activity_by_id(&id)
                    .filter(|record| !self.marketing_activity_hidden_by_delete_all(record, request))
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "marketingActivities" => {
                let remote_ids = resolved_string_list_arg(&field.arguments, "remoteIds");
                let ids = resolved_string_list_arg(&field.arguments, "marketingActivityIds");
                let records = self
                    .store
                    .marketing_activities()
                    .into_iter()
                    .filter(|record| {
                        let id = record["id"].as_str().unwrap_or_default();
                        if self.marketing_activity_hidden_by_delete_all(record, request) {
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
                        true
                    })
                    .collect::<Vec<_>>();
                marketing_activity_connection(records, &field.arguments)
            }
            "marketingEvent" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                self.store.marketing_event_by_id(&id).unwrap_or(Value::Null)
            }
            "marketingEvents" => {
                let records = self.store.marketing_events().into_iter().collect();
                marketing_event_connection(records, &field.arguments)
            }
            _ => Value::Null,
        }
    }

    fn marketing_activity_hidden_by_delete_all(
        &self,
        activity: &Value,
        _request: &Request,
    ) -> bool {
        if !activity["isExternal"].as_bool().unwrap_or(true) {
            return false;
        }
        if self.store.staged.marketing_delete_all_external {
            return true;
        }
        let Some(activity_app_id) = activity["apiClientId"].as_str() else {
            return false;
        };
        self.store
            .staged
            .marketing_delete_all_external_app_ids
            .contains(activity_app_id)
    }

    pub(super) fn marketing_mutation_outcome(
        &mut self,
        field: &MarketingRootInput,
        request: &Request,
    ) -> (ResolverOutcome<Value>, Vec<String>) {
        let mut top_errors: Vec<Value> = Vec::new();
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
                }
                None => {}
            }
            if !top_errors.is_empty() {
                return (
                    ResolverOutcome::value(Value::Null).with_errors(root_field_errors_from_json(
                        &top_errors,
                        &field.response_key,
                    )),
                    Vec::new(),
                );
            }
        }
        let value = match field.name.as_str() {
            "marketingActivityCreateExternal" => self.marketing_create_external(field, request),
            "marketingActivityUpdateExternal" => self.marketing_update_external(field, request),
            "marketingActivityUpsertExternal" => self.marketing_upsert_external(field, request),
            "marketingActivityDeleteExternal" => self.marketing_delete_external(field, request),
            "marketingActivitiesDeleteAllExternal" => {
                if let Some(api_client_id) = request.headers.get(API_CLIENT_ID_HEADER) {
                    self.store
                        .staged
                        .marketing_delete_all_external_app_ids
                        .insert(api_client_id.clone());
                } else {
                    self.store.staged.marketing_delete_all_external = true;
                }
                let job_id = self.next_proxy_synthetic_gid("Job");
                json!({
                    "job": { "id": job_id, "done": false },
                    "userErrors": []
                })
            }
            "marketingEngagementCreate" => self.marketing_engagement_create(field, request),
            "marketingEngagementsDelete" => self.marketing_engagements_delete(field, request),
            "marketingActivityCreate" => self.marketing_create_native(field, request),
            "marketingActivityUpdate" => self.marketing_update_native(field, request),
            _ => Value::Null,
        };
        let staged_ids = value["marketingActivity"]["id"]
            .as_str()
            .map(ToString::to_string)
            .into_iter()
            .collect();
        (ResolverOutcome::value(value), staged_ids)
    }

    fn marketing_create_native(&mut self, field: &MarketingRootInput, request: &Request) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if let Some(error) = native_marketing_activity_extension_error(&input) {
            return json!({
                "marketingActivity": null,
                "redirectPath": null,
                "userErrors": [error]
            });
        }
        let id = self.next_proxy_synthetic_gid("MarketingActivity");
        let timestamp = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let app_context = self.marketing_app_context_for_request(request);
        let activity = native_marketing_activity_from_input(
            &id,
            input,
            None,
            app_context,
            &timestamp,
            &shop_currency_code,
        );
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        json!({ "marketingActivity": activity, "redirectPath": null, "userErrors": [] })
    }

    fn marketing_update_native(&mut self, field: &MarketingRootInput, request: &Request) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id")
            .unwrap_or_else(|| self.next_proxy_synthetic_gid("MarketingActivity"));
        let existing = self.store.marketing_activity_by_id(&id).cloned();
        let timestamp = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let app_context = self.marketing_app_context_for_request(request);
        let activity = native_marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            app_context,
            &timestamp,
            &shop_currency_code,
        );
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        json!({ "marketingActivity": activity, "redirectPath": "/admin/marketing", "userErrors": [] })
    }

    fn marketing_create_external(
        &mut self,
        field: &MarketingRootInput,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        self.marketing_create_or_update_payload(field, input, None, true, request)
    }

    fn marketing_update_external(
        &mut self,
        field: &MarketingRootInput,
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
                return marketing_activity_error(marketing_activity_missing_error());
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
            return marketing_activity_error(marketing_activity_missing_error());
        };
        let existing = self
            .store
            .marketing_activity_by_id(&existing_id)
            .cloned()
            .unwrap_or(Value::Null);
        let selector_utm = resolved_object_field(&field.arguments, "utm");
        if let Some(err) = self.marketing_external_immutable_update_error(
            &existing,
            &input,
            selector_utm.as_ref(),
            request,
        ) {
            return marketing_activity_error(err);
        }
        self.marketing_create_or_update_payload(field, input, Some(existing_id), false, request)
    }

    fn marketing_upsert_external(
        &mut self,
        field: &MarketingRootInput,
        request: &Request,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if marketing_input_tactic_is_storefront_app(&input) {
            return marketing_activity_payload(
                None,
                vec![marketing_activity_cannot_update_tactic_to_storefront_error()],
            );
        }
        let remote = resolved_string_field(&input, "remoteId").unwrap_or_default();
        let existing_id = self.find_marketing_activity_by_remote(&remote, request);
        if let Some(id) = &existing_id {
            if let Some(existing) = self.store.marketing_activity_by_id(id) {
                if let Some(err) =
                    self.marketing_external_immutable_update_error(existing, &input, None, request)
                {
                    return marketing_activity_error(err);
                }
            }
        }
        self.marketing_create_or_update_payload(field, input, existing_id, true, request)
    }

    fn marketing_create_or_update_payload(
        &mut self,
        field: &MarketingRootInput,
        input: BTreeMap<String, ResolvedValue>,
        existing_id: Option<String>,
        create_if_missing: bool,
        request: &Request,
    ) -> Value {
        if let Some(payload) = self.marketing_create_or_update_error(
            field,
            &input,
            existing_id.as_deref(),
            create_if_missing,
            request,
        ) {
            return payload;
        }
        let activity = self.stage_marketing_create_or_update(input, existing_id, request);
        marketing_activity_payload(Some(activity), Vec::new())
    }

    fn marketing_create_or_update_error(
        &self,
        field: &MarketingRootInput,
        input: &BTreeMap<String, ResolvedValue>,
        existing_id: Option<&str>,
        create_if_missing: bool,
        request: &Request,
    ) -> Option<Value> {
        if self.marketing_delete_all_external_blocks_request(request)
            && existing_id.is_none()
            && field.name == "marketingActivityCreateExternal"
        {
            return Some(marketing_activity_error(user_error(Value::Null, "Cannot perform this operation because a job to delete all external activities has been enqueued, which happens either from calling the marketingActivitiesDeleteAllExternal mutation or as a result of an app uninstall. Please either check the status of the job returned by the mutation or try again later.", Some("DELETE_JOB_ENQUEUED"))));
        }
        if !input.contains_key("utm")
            && !input.contains_key("urlParameterValue")
            && create_if_missing
        {
            return Some(marketing_input_error("Non-hierarchical marketing activities must have UTM parameters or a URL parameter value.", Some("NON_HIERARCHIAL_REQUIRES_UTM_URL_PARAMETER")));
        }
        if let (false, Some(err)) = (
            create_if_missing,
            invalid_marketing_url_error(input, &field.name),
        ) {
            return Some(marketing_activity_error(err));
        }
        if has_marketing_currency_mismatch(input) {
            return Some(marketing_input_error(
                "Currency code is not matching between budget and ad spend",
                None,
            ));
        }
        // Top-level GraphQL coercion in Shopify; parity compares errors for these cases.
        if let (true, Some(err)) = (
            create_if_missing,
            invalid_marketing_url_error(input, &field.name),
        ) {
            return Some(marketing_activity_error(err));
        }
        if create_if_missing
            && existing_id.is_none()
            && resolved_string_field(input, "channelHandle")
                .is_some_and(|handle| !recognized_marketing_channel_handle(&handle))
        {
            return Some(marketing_input_error("The channel handle is not recognized. Please contact your partner manager for more information.", Some("INVALID_CHANNEL_HANDLE")));
        }
        if create_if_missing && existing_id.is_none() {
            return self.marketing_create_duplicate_error(field, input, request);
        }
        None
    }

    fn marketing_delete_all_external_blocks_request(&self, request: &Request) -> bool {
        self.store.staged.marketing_delete_all_external
            || request
                .headers
                .get(API_CLIENT_ID_HEADER)
                .is_some_and(|api_client_id| {
                    self.store
                        .staged
                        .marketing_delete_all_external_app_ids
                        .contains(api_client_id)
                })
    }

    fn marketing_create_duplicate_error(
        &self,
        field: &MarketingRootInput,
        input: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Option<Value> {
        let remote = resolved_string_field(input, "remoteId").unwrap_or_default();
        if !remote.is_empty()
            && self
                .find_marketing_activity_by_remote(&remote, request)
                .is_some()
        {
            return Some(marketing_input_error(
                "Validation failed: Remote ID has already been taken",
                None,
            ));
        }
        if resolved_object_field(input, "utm")
            .and_then(|utm| resolved_string_field(&utm, "campaign"))
            .is_some_and(|campaign| {
                self.find_marketing_activity_by_utm(&campaign, request)
                    .is_some()
            })
        {
            return Some(marketing_input_error(
                "Validation failed: Utm campaign has already been taken",
                None,
            ));
        }
        if resolved_string_field(input, "urlParameterValue").is_some_and(|value| {
            self.find_marketing_activity_by_url_parameter(&value, request)
                .is_some()
        }) {
            let message = if field.name == "marketingActivityUpsertExternal" {
                "Validation failed: Url parameter value has already been taken, Url parameter value has already been taken"
            } else {
                "Validation failed: Url parameter value has already been taken"
            };
            return Some(marketing_input_error(message, None));
        }
        None
    }

    fn stage_marketing_create_or_update(
        &mut self,
        input: BTreeMap<String, ResolvedValue>,
        existing_id: Option<String>,
        request: &Request,
    ) -> Value {
        let new_marketing_event_id = existing_id.is_none().then(|| {
            shopify_gid(
                "MarketingEvent",
                self.next_synthetic_id + MARKETING_EVENT_ID_OFFSET,
            )
        });
        let id = existing_id.unwrap_or_else(|| {
            let id = shopify_gid("MarketingActivity", self.next_synthetic_id);
            self.next_synthetic_id += 1;
            id
        });
        let existing = self.store.marketing_activity_by_id(&id).cloned();
        let timestamp = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let app_context = self.marketing_app_context_for_request(request);
        let activity = marketing_activity_from_input(
            &id,
            input,
            existing.as_ref(),
            app_context,
            new_marketing_event_id,
            &timestamp,
            &shop_currency_code,
        );
        self.store
            .staged
            .marketing_activities
            .insert(id, activity.clone());
        activity
    }

    fn marketing_delete_external(
        &mut self,
        field: &MarketingRootInput,
        request: &Request,
    ) -> Value {
        if !field.arguments.contains_key("marketingActivityId")
            && !field.arguments.contains_key("id")
            && !field.arguments.contains_key("remoteId")
        {
            return json!({ "deletedMarketingActivityId": null, "userErrors": [user_error(Value::Null, "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.", Some("INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"))] });
        }
        let id = resolved_string_field(&field.arguments, "marketingActivityId")
            .or_else(|| resolved_string_field(&field.arguments, "id"))
            .or_else(|| {
                resolved_string_field(&field.arguments, "remoteId")
                    .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
            });
        let Some(id) = id else {
            return json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] });
        };
        let Some(activity) = self.marketing_activity_for_delete(&id, request) else {
            return json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_missing_error()] });
        };
        if activity["isExternal"] == json!(false) {
            return json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_delete_not_external_error()] });
        }
        if self.marketing_activity_has_child_events(&activity) {
            return json!({ "deletedMarketingActivityId": null, "userErrors": [marketing_activity_child_events_error()] });
        }
        self.store.staged.marketing_activities.tombstone(id.clone());
        json!({ "deletedMarketingActivityId": id, "userErrors": [] })
    }

    fn marketing_activity_for_delete(&self, id: &str, request: &Request) -> Option<Value> {
        if self.store.staged.marketing_activities.is_tombstoned(id) {
            return None;
        }
        let activity = self.store.marketing_activity_by_id(id)?;
        let request_app = request.headers.get(API_CLIENT_ID_HEADER);
        if activity["apiClientId"].as_str() == request_app.map(String::as_str) {
            Some(activity.clone())
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
            .marketing_activities()
            .into_iter()
            .any(|candidate| {
                candidate["id"].as_str() != activity["id"].as_str()
                    && candidate["apiClientId"].as_str() == parent_app
                    && candidate["parentRemoteId"].as_str() == Some(parent_remote)
            })
    }

    fn marketing_engagement_create(
        &mut self,
        field: &MarketingRootInput,
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
            return marketing_engagement_payload(
                None,
                vec![user_error(Value::Null, "No identifier found. For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.", Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENT_MISSING"))],
            );
        }
        if selector_count > 1 {
            return marketing_engagement_payload(
                None,
                vec![user_error(Value::Null, "For activity level engagement, either the marketing activity ID or remote ID must be provided. For channel level engagement, the channel handle must be provided.", Some("INVALID_MARKETING_ENGAGEMENT_ARGUMENTS"))],
            );
        }
        if let Some(channel) = resolved_string_field(&field.arguments, "channelHandle") {
            if !recognized_marketing_channel_handle(&channel) {
                return marketing_engagement_payload(
                    None,
                    vec![user_error(["channelHandle"], "The channel handle is not recognized. Please contact your partner manager for more information.", Some("INVALID_CHANNEL_HANDLE"))],
                );
            }
        }
        let engagement_input =
            resolved_object_field(&field.arguments, "marketingEngagement").unwrap_or_default();
        if has_engagement_currency_mismatch(&engagement_input) {
            return marketing_engagement_payload(
                None,
                vec![user_error(
                    ["marketingEngagement"],
                    "Currency codes in the marketing engagement input do not match.",
                    Some("CURRENCY_CODE_MISMATCH_INPUT"),
                )],
            );
        }
        if has_channel {
            let shop_currency_code = self.store.shop_currency_code();
            let engagement =
                marketing_engagement_from_input(&engagement_input, None, &shop_currency_code);
            return marketing_engagement_payload(Some(engagement), Vec::new());
        }
        let activity_id = if has_activity_id {
            resolved_string_field(&field.arguments, "marketingActivityId")
        } else {
            resolved_string_field(&field.arguments, "remoteId")
                .and_then(|remote| self.find_marketing_activity_by_remote(&remote, request))
        };
        let Some(activity_id) = activity_id else {
            return marketing_engagement_payload(None, vec![marketing_activity_missing_error()]);
        };
        let Some(activity) = self.store.marketing_activity_by_id(&activity_id) else {
            return marketing_engagement_payload(None, vec![marketing_activity_missing_error()]);
        };
        if activity["marketingEvent"].is_null() {
            return marketing_engagement_payload(None, vec![marketing_event_missing_error()]);
        }
        if self.engagement_currency_mismatches_activity(&activity_id, &engagement_input) {
            return marketing_engagement_payload(
                None,
                vec![user_error(["marketingEngagement"], "Marketing activity currency code does not match the currency code in the marketing engagement input.", Some("MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH"))],
            );
        }
        let shop_currency_code = self.store.shop_currency_code();
        let engagement =
            marketing_engagement_from_input(&engagement_input, Some(activity), &shop_currency_code);
        // Shopify accepts engagement metrics but does not fold engagement ad spend
        // back into the MarketingActivity.adSpend field in these captures.
        marketing_engagement_payload(Some(engagement), Vec::new())
    }

    fn marketing_engagements_delete(
        &mut self,
        field: &MarketingRootInput,
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
        json!({ "result": result, "userErrors": errors })
    }

    fn marketing_channel_handles_for_request(&self, request: &Request) -> BTreeSet<String> {
        let request_app = request.headers.get(API_CLIENT_ID_HEADER);
        self.store
            .marketing_activities()
            .into_iter()
            .filter_map(|record| {
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
        let app = request.headers.get(API_CLIENT_ID_HEADER);
        self.store
            .marketing_activities()
            .into_iter()
            .find_map(|record| {
                if !matches_record(&record) {
                    return None;
                }
                let record_app = record["apiClientId"].as_str();
                if app.map(String::as_str) == record_app {
                    record["id"].as_str().map(str::to_string)
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
        let Some(activity) = self.store.marketing_activity_by_id(activity_id) else {
            return false;
        };
        let Some(activity_currency) = activity["budget"]["total"]["currencyCode"].as_str() else {
            return false;
        };
        marketing_money_currency(engagement, "adSpend").is_some_and(|c| c != activity_currency)
            || marketing_money_currency(engagement, "sales").is_some_and(|c| c != activity_currency)
    }
}

pub(in crate::proxy) fn marketing_activity_search_decision(
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    marketing_search_decision(record, query, marketing_activity_term_matches)
}

pub(in crate::proxy) fn marketing_event_search_decision(
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    marketing_search_decision(record, query, marketing_event_term_matches)
}

fn marketing_search_decision(
    record: &Value,
    query: Option<&str>,
    term_matches: fn(&Value, &str) -> bool,
) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    let mut group_matches = true;
    let mut group_has_terms = false;
    for term in marketing_query_terms(query) {
        if term.eq_ignore_ascii_case("OR") {
            if group_has_terms && group_matches {
                return StagedSearchDecision::Match;
            }
            group_matches = true;
            group_has_terms = false;
            continue;
        }
        if term.eq_ignore_ascii_case("AND") {
            continue;
        }
        group_has_terms = true;
        if !term_matches(record, &term) {
            group_matches = false;
        }
    }
    if !group_has_terms || group_matches {
        StagedSearchDecision::Match
    } else {
        StagedSearchDecision::NoMatch
    }
}

fn marketing_query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in query.chars() {
        match quote {
            Some(active_quote) if ch == active_quote => {
                current.push(ch);
                quote = None;
            }
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => {
                current.push(ch);
                quote = Some(ch);
            }
            None if ch.is_whitespace() => {
                let term = current.trim();
                if !term.is_empty() {
                    terms.push(term.to_string());
                    current.clear();
                }
            }
            None => current.push(ch),
        }
    }

    let term = current.trim();
    if !term.is_empty() {
        terms.push(term.to_string());
    }

    terms
}

fn marketing_activity_term_matches(record: &Value, term: &str) -> bool {
    let term = marketing_unquote(term.trim());
    if term.is_empty() {
        return true;
    }
    let Some((field, value)) = term.split_once(':') else {
        return marketing_default_query_matches(record, term);
    };
    let value = marketing_unquote(value.trim());
    if value.is_empty() {
        return true;
    }
    match field.to_ascii_lowercase().as_str() {
        "default" => marketing_default_query_matches(record, value),
        "app_id" | "appid" | "api_client_id" | "apiclientid" => {
            marketing_any_text_matches(record, &[&["apiClientId"], &["app", "id"]], value)
        }
        "app_name" | "appname" => marketing_any_text_matches(record, &[&["app", "title"]], value),
        "created_at" | "createdat" => {
            marketing_ordered_text_matches(marketing_path_string(record, &["createdAt"]), value)
        }
        "id" => marketing_id_matches(marketing_path_string(record, &["id"]), value),
        "marketing_campaign_id" | "marketingcampaignid" => marketing_any_text_matches(
            record,
            &[&["marketingCampaignId"], &["marketingCampaign", "id"]],
            value,
        ),
        "remote_id" | "remoteid" => marketing_any_text_matches(
            record,
            &[&["remoteId"], &["marketingEvent", "remoteId"]],
            value,
        ),
        "scheduled_to_end_at" | "scheduledtoendat" => marketing_ordered_text_matches(
            marketing_path_string(record, &["marketingEvent", "scheduledToEndAt"]),
            value,
        ),
        "scheduled_to_start_at" | "scheduledtostartat" => marketing_ordered_text_matches(
            marketing_path_string(record, &["marketingEvent", "scheduledToStartAt"]),
            value,
        ),
        "tactic" => {
            marketing_any_text_matches(record, &[&["tactic"], &["marketingEvent", "type"]], value)
        }
        "title" => marketing_any_text_matches(record, &[&["title"]], value),
        "updated_at" | "updatedat" => {
            marketing_ordered_text_matches(marketing_path_string(record, &["updatedAt"]), value)
        }
        "description" => {
            marketing_any_text_matches(record, &[&["marketingEvent", "description"]], value)
        }
        "status" => marketing_any_text_matches(record, &[&["status"]], value),
        "channel_handle" | "channelhandle" => {
            marketing_any_text_matches(record, &[&["marketingEvent", "channelHandle"]], value)
        }
        _ => marketing_default_query_matches(record, value),
    }
}

fn marketing_event_term_matches(record: &Value, term: &str) -> bool {
    let term = marketing_unquote(term.trim());
    if term.is_empty() {
        return true;
    }
    let Some((field, value)) = term.split_once(':') else {
        return marketing_default_query_matches(record, term);
    };
    let value = marketing_unquote(value.trim());
    if value.is_empty() {
        return true;
    }
    match field.to_ascii_lowercase().as_str() {
        "default" => marketing_default_query_matches(record, value),
        "id" => marketing_id_matches(marketing_path_string(record, &["id"]), value),
        "remote_id" | "remoteid" => marketing_any_text_matches(record, &[&["remoteId"]], value),
        "description" | "title" => marketing_any_text_matches(record, &[&["description"]], value),
        "channel_handle" | "channelhandle" => {
            marketing_any_text_matches(record, &[&["channelHandle"]], value)
        }
        "started_at" | "startedat" | "scheduled_to_start_at" | "scheduledtostartat" => {
            marketing_ordered_text_matches(marketing_path_string(record, &["startedAt"]), value)
        }
        "scheduled_to_end_at" | "scheduledtoendat" => marketing_ordered_text_matches(
            marketing_path_string(record, &["scheduledToEndAt"]),
            value,
        ),
        "tactic" | "type" => marketing_any_text_matches(record, &[&["type"]], value),
        _ => marketing_default_query_matches(record, value),
    }
}

fn marketing_unquote(value: &str) -> &str {
    value.trim_matches(|ch| ch == '"' || ch == '\'')
}

fn marketing_path_string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str()
}

fn marketing_any_text_matches(record: &Value, paths: &[&[&str]], expected: &str) -> bool {
    paths.iter().any(|path| {
        marketing_path_string(record, path)
            .is_some_and(|actual| marketing_text_matches(actual, expected))
    })
}

fn marketing_text_matches(actual: &str, expected: &str) -> bool {
    actual
        .to_ascii_lowercase()
        .contains(&expected.to_ascii_lowercase())
        || resource_id_tail(actual).eq_ignore_ascii_case(expected)
}

fn marketing_default_query_matches(record: &Value, expected: &str) -> bool {
    let mut values = Vec::new();
    marketing_collect_string_values(record, &mut values);
    values
        .iter()
        .any(|actual| marketing_text_matches(actual, expected))
}

fn marketing_collect_string_values(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::String(value) => values.push(value.clone()),
        Value::Array(items) => {
            for item in items {
                marketing_collect_string_values(item, values);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if key == MARKETING_CURSOR_METADATA_FIELD {
                    continue;
                }
                marketing_collect_string_values(value, values);
            }
        }
        _ => {}
    }
}

fn marketing_id_matches(actual: Option<&str>, expected: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    if let Some((operator, target)) = marketing_query_comparison(expected) {
        return marketing_ordered_compare(
            resource_id_tail(actual),
            resource_id_tail(target),
            operator,
        );
    }
    actual.eq_ignore_ascii_case(expected)
        || resource_id_tail(actual).eq_ignore_ascii_case(resource_id_tail(expected))
        || marketing_text_matches(actual, expected)
}

fn marketing_ordered_text_matches(actual: Option<&str>, expected: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    if let Some((operator, target)) = marketing_query_comparison(expected) {
        return marketing_ordered_compare(actual, target, operator);
    }
    marketing_text_matches(actual, expected)
}

fn marketing_query_comparison(value: &str) -> Option<(&str, &str)> {
    let (operator, target) = comparison_operator_prefix(value, &[">=", "<=", ">", "<"])?;
    let target = marketing_unquote(target.trim());
    (!target.is_empty()).then_some((operator, target))
}

fn marketing_ordered_compare(actual: &str, target: &str, operator: &str) -> bool {
    if let (Ok(actual), Ok(target)) = (actual.parse::<i64>(), target.parse::<i64>()) {
        return match operator {
            ">" => actual > target,
            ">=" => actual >= target,
            "<" => actual < target,
            "<=" => actual <= target,
            _ => false,
        };
    }
    match operator {
        ">" => actual > target,
        ">=" => actual >= target,
        "<" => actual < target,
        "<=" => actual <= target,
        _ => false,
    }
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
fn marketing_source_and_medium_uses_tactic_before_channel_and_aliases_domains() {
    assert_eq!(
        marketing_source_and_medium("SEARCH", "ABANDONED_CART", None),
        "Abandoned cart email"
    );
    assert_eq!(
        marketing_source_and_medium("EMAIL", "AFFILIATE", None),
        "Affiliate link"
    );
    assert_eq!(
        marketing_source_and_medium("SEARCH", "LOYALTY", None),
        "Loyalty program"
    );
    assert_eq!(
        marketing_source_and_medium("EMAIL", "RETARGETING", Some("twitter.com")),
        "Twitter retargeting ad"
    );
    assert_eq!(
        marketing_source_and_medium("DISPLAY", "MESSAGE", Some("instagram.com")),
        "Instagram message"
    );
    assert_eq!(
        marketing_source_and_medium("DISPLAY", "AD", Some("partner.example")),
        "partner.example ad"
    );
    assert_eq!(
        marketing_source_and_medium("SEARCH", "NEWSLETTER", None),
        "Search newsletter"
    );
    assert_eq!(
        marketing_source_and_medium("", "NEWSLETTER", None),
        "Newsletter"
    );
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
