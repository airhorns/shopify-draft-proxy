use super::*;

#[derive(Clone)]
pub(in crate::proxy) struct WebPresenceDraft {
    pub(in crate::proxy) id: String,
    pub(in crate::proxy) default_locale: String,
    pub(in crate::proxy) alternate_locales: Vec<String>,
    pub(in crate::proxy) subfolder_suffix: Option<String>,
    pub(in crate::proxy) domain_id: Option<String>,
}

pub(in crate::proxy) fn web_presence_draft_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    errors: &mut Vec<Value>,
    is_create: bool,
    primary_locale: &str,
) -> WebPresenceDraft {
    let mut draft = existing
        .map(|record| web_presence_draft_from_record(record, primary_locale))
        .unwrap_or_else(|| WebPresenceDraft {
            id: String::new(),
            default_locale: primary_locale.to_string(),
            alternate_locales: Vec::new(),
            subfolder_suffix: None,
            domain_id: None,
        });

    if is_create || input.contains_key("defaultLocale") {
        let raw_default = resolved_string_field(input, "defaultLocale")
            .unwrap_or_else(|| draft.default_locale.clone());
        if raw_default.is_empty() {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                "Default locale can't be blank",
                json!("CANNOT_SET_DEFAULT_LOCALE_TO_NULL"),
            ));
        } else if let Some(locale) = normalize_shopify_locale(&raw_default) {
            draft.default_locale = locale;
        } else {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &invalid_locale_message(&[raw_default]),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("alternateLocales") {
        let raw_alternate_locales = list_string_field(input, "alternateLocales");
        let mut normalized_alternate_locales = Vec::new();
        let mut invalid_locales = Vec::new();
        for raw_locale in raw_alternate_locales {
            if let Some(locale) = normalize_shopify_locale(&raw_locale) {
                if !normalized_alternate_locales.contains(&locale) {
                    normalized_alternate_locales.push(locale);
                }
            } else {
                invalid_locales.push(raw_locale);
            }
        }
        if invalid_locales.is_empty() {
            draft.alternate_locales = normalized_alternate_locales;
        } else {
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &invalid_locale_message(&invalid_locales),
                json!("INVALID"),
            ));
        }
    }

    if is_create || input.contains_key("subfolderSuffix") {
        draft.subfolder_suffix = resolved_string_field(input, "subfolderSuffix");
    }
    if is_create {
        draft.domain_id = resolved_string_field(input, "domainId");
    }

    draft
}

pub(in crate::proxy) fn web_presence_draft_from_record(
    record: &Value,
    primary_locale: &str,
) -> WebPresenceDraft {
    WebPresenceDraft {
        id: record["id"].as_str().unwrap_or_default().to_string(),
        default_locale: record["defaultLocale"]["locale"]
            .as_str()
            .unwrap_or(primary_locale)
            .to_string(),
        alternate_locales: record["alternateLocales"]
            .as_array()
            .map(|locales| {
                locales
                    .iter()
                    .filter_map(|locale| locale["locale"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        subfolder_suffix: record["subfolderSuffix"].as_str().map(str::to_string),
        domain_id: record["domain"]["id"].as_str().map(str::to_string),
    }
}

pub(in crate::proxy) fn web_presence_validate_routing_and_uniqueness(
    draft: &WebPresenceDraft,
    input: &BTreeMap<String, ResolvedValue>,
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    is_create: bool,
    linked_domain: Option<&Value>,
    errors: &mut Vec<Value>,
) {
    let has_domain = draft.domain_id.is_some();
    let has_subfolder = draft.subfolder_suffix.is_some();
    // A domainId makes this a domain-backed presence: Shopify validates the domain
    // reference and ignores the subfolder-routing rules (subfolder format,
    // cannot-have-both, locale duplication). A domainId that does not resolve to a
    // real domain fails with DOMAIN_NOT_FOUND, reported ahead of any locale errors
    // already collected by web_presence_draft_from_input.
    if has_domain {
        if is_create && linked_domain.is_none() {
            errors.insert(
                0,
                market_user_error(
                    vec!["input", "domainId"],
                    "Domain does not exist",
                    json!("DOMAIN_NOT_FOUND"),
                ),
            );
        }
        return;
    }
    if is_create && !has_subfolder {
        errors.push(market_user_error(
            vec!["input"],
            "One of `subfolderSuffix` or `domainId` is required.",
            json!("REQUIRES_DOMAIN_OR_SUBFOLDER"),
        ));
    }
    if let Some(suffix) = draft.subfolder_suffix.as_deref() {
        if is_create || input.contains_key("subfolderSuffix") {
            errors.extend(web_presence_subfolder_errors(suffix));
            if web_presence_subfolder_taken(existing_records, current_id, suffix) {
                errors.push(market_user_error(
                    vec!["input", "subfolderSuffix"],
                    "Subfolder suffix has already been taken",
                    json!("TAKEN"),
                ));
            }
        }
    }
    // Duplicate-language detection across the default + alternate locales. Shopify
    // raises a `defaultLocale` error when the default repeats an alternate, and a
    // separate `alternateLocales` error listing the offending languages. The listed
    // set is the alternates alone when they already collide with each other, or the
    // default prepended to the alternates when the collision is default-vs-alternate.
    let default_collides = draft
        .alternate_locales
        .iter()
        .any(|locale| locale == &draft.default_locale);
    let alternates_internal_dup = {
        let mut seen = std::collections::HashSet::new();
        !draft
            .alternate_locales
            .iter()
            .all(|locale| seen.insert(locale.clone()))
    };
    if default_collides || alternates_internal_dup {
        if default_collides && (is_create || input.contains_key("defaultLocale")) {
            errors.push(market_user_error(
                vec!["input", "defaultLocale"],
                &format!(
                    "Default locale The alternate languages already include {}.",
                    draft.default_locale
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
        if input.contains_key("alternateLocales") {
            let listed: Vec<String> = if alternates_internal_dup {
                draft.alternate_locales.clone()
            } else {
                std::iter::once(draft.default_locale.clone())
                    .chain(draft.alternate_locales.iter().cloned())
                    .collect()
            };
            errors.push(market_user_error(
                vec!["input", "alternateLocales"],
                &format!(
                    "Alternate locales Duplicates were found in the following languages: {}",
                    humanize_and_list(&listed, " and ")
                ),
                json!("DUPLICATE_LANGUAGES"),
            ));
        }
    }
}

/// Join a list with commas and a trailing "and": `[a]`->`a`, `[a,b]`->`a and b`,
/// `[a,b,c]`->`a, b, and c` (Shopify's duplicate-language error phrasing).
fn humanize_and_list(items: &[String], two_item_separator: &str) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first}{two_item_separator}{second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

pub(in crate::proxy) fn web_presence_subfolder_errors(suffix: &str) -> Vec<Value> {
    let mut errors = Vec::new();
    if suffix.len() < 2 {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must be at least 2 letters",
            json!("SUBFOLDER_SUFFIX_MUST_BE_AT_LEAST_2_LETTERS"),
        ));
    }
    if suffix == "Latn" {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix cannot be a script code",
            json!("SUBFOLDER_SUFFIX_CANNOT_BE_SCRIPT_CODE"),
        ));
    } else if !suffix.chars().all(char::is_alphabetic) {
        errors.push(market_user_error(
            vec!["input", "subfolderSuffix"],
            "Subfolder suffix must contain only letters",
            json!("SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS"),
        ));
    }
    errors
}

pub(in crate::proxy) fn web_presence_subfolder_taken(
    existing_records: &BTreeMap<String, Value>,
    current_id: Option<&str>,
    suffix: &str,
) -> bool {
    existing_records.iter().any(|(id, record)| {
        current_id != Some(id.as_str()) && record["subfolderSuffix"].as_str() == Some(suffix)
    })
}

pub(in crate::proxy) fn normalize_shopify_locale(raw_locale: &str) -> Option<String> {
    let mut parts = raw_locale.split('-');
    let language = parts.next()?.to_ascii_lowercase();
    if !default_available_language_subtag_is_supported(&language) {
        return None;
    }
    let mut normalized = vec![language];
    for part in parts {
        if part.len() == 4 && part.chars().all(char::is_alphabetic) {
            let mut chars = part.chars();
            let first = chars.next()?.to_uppercase().collect::<String>();
            normalized.push(format!("{}{}", first, chars.as_str().to_ascii_lowercase()));
        } else if part.len() == 2 && part.chars().all(char::is_alphabetic) {
            normalized.push(part.to_ascii_uppercase());
        } else if part.len() == 3 && part.chars().all(|ch| ch.is_ascii_digit()) {
            normalized.push(part.to_string());
        } else {
            return None;
        }
    }
    Some(normalized.join("-"))
}

pub(in crate::proxy) fn invalid_locale_message(invalid_locales: &[String]) -> String {
    if invalid_locales.is_empty() {
        "Invalid locale codes".to_string()
    } else {
        format!(
            "Invalid locale codes: {}",
            humanize_and_list(invalid_locales, ", and ")
        )
    }
}

pub(in crate::proxy) fn market_web_presence_helper_record(
    draft: &WebPresenceDraft,
    shop_domain: &str,
    linked_domain: Option<&Value>,
) -> Value {
    let shop_origin = format!("https://{shop_domain}");
    // A linked custom domain routes through its own host, not the shop's
    // myshopify domain. The domain is resolved from the proxy's effective shop
    // state, so restored/hydrated stores are not limited to a single baked id.
    let linked_domain_host = linked_domain
        .and_then(|domain| domain.get("host"))
        .and_then(Value::as_str);
    let domain = linked_domain.cloned().unwrap_or(Value::Null);
    // Shopify lists root URLs as the default locale first, then the alternate
    // locales sorted alphabetically by locale code (the `alternateLocales` field
    // itself preserves the caller's input order; only `rootUrls` is sorted).
    let mut sorted_alternates = draft.alternate_locales.clone();
    sorted_alternates.sort();
    let locales = std::iter::once(draft.default_locale.clone())
        .chain(sorted_alternates)
        .collect::<Vec<_>>();
    // Shopify roots a subfolder web presence at `/{language}-{suffix}/` for every
    // locale, including the default (the language subtag of e.g. `en-us`/`fr-CA`
    // collapses to `en`/`fr`). Domain-backed presences serve the default locale at
    // the domain root (`/`) and each alternate at `/{language}/` on the domain host.
    let root_urls = locales
        .iter()
        .map(|locale| {
            let language = locale.split('-').next().unwrap_or(locale.as_str());
            let url = if let Some(host) = &linked_domain_host {
                if locale == &draft.default_locale {
                    format!("https://{host}/")
                } else {
                    format!("https://{host}/{language}/")
                }
            } else {
                let suffix = draft.subfolder_suffix.as_deref().unwrap_or_default();
                format!("{shop_origin}/{language}-{suffix}/")
            };
            json!({"locale": locale, "url": url})
        })
        .collect::<Vec<_>>();
    json!({
        "id": draft.id,
        "subfolderSuffix": draft.subfolder_suffix,
        "domain": domain,
        "rootUrls": root_urls,
        "defaultLocale": locale_record(&draft.default_locale, true),
        "alternateLocales": draft.alternate_locales.iter().map(|locale| locale_record(locale, false)).collect::<Vec<_>>(),
        "markets": {"nodes": []}
    })
}

pub(in crate::proxy) fn locale_record(locale: &str, primary: bool) -> Value {
    json!({
        "locale": locale,
        "name": shopify_locale_name(locale),
        "primary": primary,
        "published": true
    })
}

fn shopify_locale_name(locale: &str) -> String {
    if let Some(name) = default_available_locale_name(locale) {
        return name.to_string();
    }
    let language = locale.split('-').next().unwrap_or(locale);
    default_available_language_subtag_name(language)
        .unwrap_or("English")
        .to_string()
}
