use super::*;

pub(super) fn is_online_store_content_query_root(root: &str) -> bool {
    matches!(
        root,
        "article"
            | "articleAuthors"
            | "articles"
            | "articleTags"
            | "blog"
            | "blogs"
            | "blogsCount"
            | "page"
            | "pages"
            | "pagesCount"
            | "comment"
            | "comments"
    )
}

#[derive(Clone, Debug)]
struct OnlineStoreQueryToken {
    field: Option<String>,
    value: String,
}

pub(super) fn online_store_search_decision(
    kind: OnlineStoreKind,
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };

    for token in online_store_query_tokens(query) {
        if token.field.is_none() && token.value.eq_ignore_ascii_case("AND") {
            continue;
        }
        match online_store_search_token_decision(kind, record, &token) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }

    StagedSearchDecision::Match
}

fn online_store_query_tokens(query: &str) -> Vec<OnlineStoreQueryToken> {
    let mut raw_tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for character in query.chars() {
        match quote {
            Some(active_quote) if character == active_quote => {
                quote = None;
                current.push(character);
            }
            Some(_) => current.push(character),
            None if matches!(character, '"' | '\'') => {
                quote = Some(character);
                current.push(character);
            }
            None if character.is_whitespace() => {
                push_online_store_query_token(&mut raw_tokens, &mut current);
            }
            None => current.push(character),
        }
    }
    push_online_store_query_token(&mut raw_tokens, &mut current);

    raw_tokens
        .into_iter()
        .filter_map(|raw| {
            let raw = normalize_online_store_query_value(&raw);
            if raw.is_empty() {
                return None;
            }
            let (field, value) = raw
                .split_once(':')
                .map(|(field, value)| {
                    (
                        Some(field.trim().trim_start_matches('-').to_ascii_lowercase()),
                        normalize_online_store_query_value(value),
                    )
                })
                .unwrap_or_else(|| (None, raw));
            if value.is_empty() {
                return None;
            }
            Some(OnlineStoreQueryToken { field, value })
        })
        .collect()
}

fn push_online_store_query_token(tokens: &mut Vec<String>, current: &mut String) {
    let token = current.trim();
    if !token.is_empty() {
        tokens.push(token.to_string());
    }
    current.clear();
}

fn normalize_online_store_query_value(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character: char| matches!(character, '(' | ')' | ','))
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn online_store_search_token_decision(
    kind: OnlineStoreKind,
    record: &Value,
    token: &OnlineStoreQueryToken,
) -> StagedSearchDecision {
    match token.field.as_deref() {
        Some(field) => online_store_field_search_decision(kind, record, field, &token.value),
        None => online_store_free_text_search_decision(kind, record, &token.value),
    }
}

fn online_store_field_search_decision(
    kind: OnlineStoreKind,
    record: &Value,
    field: &str,
    value: &str,
) -> StagedSearchDecision {
    match kind {
        OnlineStoreKind::Blog => blog_field_search_decision(record, field, value),
        OnlineStoreKind::Page => page_field_search_decision(record, field, value),
        OnlineStoreKind::Article => article_field_search_decision(record, field, value),
        OnlineStoreKind::Comment => comment_field_search_decision(record, field, value),
    }
}

fn blog_field_search_decision(record: &Value, field: &str, value: &str) -> StagedSearchDecision {
    match field {
        "id" => id_search_decision(record, value),
        "title" => string_field_search_decision(record, "title", value),
        "handle" => string_field_search_decision(record, "handle", value),
        "created_at" => string_field_search_decision(record, "createdAt", value),
        "updated_at" => string_field_search_decision(record, "updatedAt", value),
        "tag" => array_field_search_decision(record, "tags", value),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn page_field_search_decision(record: &Value, field: &str, value: &str) -> StagedSearchDecision {
    match field {
        "id" => id_search_decision(record, value),
        "title" => string_field_search_decision(record, "title", value),
        "handle" => string_field_search_decision(record, "handle", value),
        "body" => string_field_search_decision(record, "body", value),
        "created_at" => string_field_search_decision(record, "createdAt", value),
        "updated_at" => string_field_search_decision(record, "updatedAt", value),
        "published_at" => string_field_search_decision(record, "publishedAt", value),
        "published_status" => published_status_search_decision(record, value),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn article_field_search_decision(record: &Value, field: &str, value: &str) -> StagedSearchDecision {
    match field {
        "id" => id_search_decision(record, value),
        "title" => string_field_search_decision(record, "title", value),
        "handle" => string_field_search_decision(record, "handle", value),
        "body" => string_field_search_decision(record, "body", value),
        "summary" => string_field_search_decision(record, "summary", value),
        "created_at" => string_field_search_decision(record, "createdAt", value),
        "updated_at" => string_field_search_decision(record, "updatedAt", value),
        "published_at" => string_field_search_decision(record, "publishedAt", value),
        "published_status" => published_status_search_decision(record, value),
        "author" => StagedSearchDecision::from_bool(online_store_search_string_matches(
            record
                .get("author")
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str),
            value,
        )),
        "blog_id" => StagedSearchDecision::from_bool(
            online_store_search_string_matches(record.get("blogId").and_then(Value::as_str), value)
                || online_store_search_string_matches(
                    record
                        .get("blog")
                        .and_then(|blog| blog.get("id"))
                        .and_then(Value::as_str),
                    value,
                ),
        ),
        "blog_title" => StagedSearchDecision::from_bool(online_store_search_string_matches(
            record
                .get("blog")
                .and_then(|blog| blog.get("title"))
                .and_then(Value::as_str),
            value,
        )),
        "tag" => array_field_search_decision(record, "tags", value),
        "tag_not" => StagedSearchDecision::from_bool(!array_field_matches(record, "tags", value)),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn comment_field_search_decision(record: &Value, field: &str, value: &str) -> StagedSearchDecision {
    match field {
        "id" => id_search_decision(record, value),
        "body" => StagedSearchDecision::from_bool(
            online_store_search_string_matches(record.get("body").and_then(Value::as_str), value)
                || online_store_search_string_matches(
                    record.get("bodyHtml").and_then(Value::as_str),
                    value,
                ),
        ),
        "status" => string_field_search_decision(record, "status", value),
        "created_at" => string_field_search_decision(record, "createdAt", value),
        "updated_at" => string_field_search_decision(record, "updatedAt", value),
        "published_at" => string_field_search_decision(record, "publishedAt", value),
        "published_status" => published_status_search_decision(record, value),
        "article_id" => StagedSearchDecision::from_bool(
            online_store_search_string_matches(
                record.get("articleId").and_then(Value::as_str),
                value,
            ) || online_store_search_string_matches(
                record
                    .get("article")
                    .and_then(|article| article.get("id"))
                    .and_then(Value::as_str),
                value,
            ),
        ),
        "article_title" => StagedSearchDecision::from_bool(online_store_search_string_matches(
            record
                .get("article")
                .and_then(|article| article.get("title"))
                .and_then(Value::as_str),
            value,
        )),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn online_store_free_text_search_decision(
    kind: OnlineStoreKind,
    record: &Value,
    value: &str,
) -> StagedSearchDecision {
    let fields = match kind {
        OnlineStoreKind::Blog => vec!["title", "handle"],
        OnlineStoreKind::Page => vec!["title", "handle", "body", "bodySummary"],
        OnlineStoreKind::Article => vec!["title", "handle", "body", "summary"],
        OnlineStoreKind::Comment => vec!["body", "bodyHtml", "status"],
    };
    let field_match = fields.into_iter().any(|field| {
        online_store_search_string_matches(record.get(field).and_then(Value::as_str), value)
    });
    let related_match = match kind {
        OnlineStoreKind::Article => {
            online_store_search_string_matches(
                record
                    .get("author")
                    .and_then(|author| author.get("name"))
                    .and_then(Value::as_str),
                value,
            ) || online_store_search_string_matches(
                record
                    .get("blog")
                    .and_then(|blog| blog.get("title"))
                    .and_then(Value::as_str),
                value,
            ) || array_field_matches(record, "tags", value)
        }
        OnlineStoreKind::Blog => array_field_matches(record, "tags", value),
        OnlineStoreKind::Comment => online_store_search_string_matches(
            record
                .get("article")
                .and_then(|article| article.get("title"))
                .and_then(Value::as_str),
            value,
        ),
        OnlineStoreKind::Page => false,
    };
    StagedSearchDecision::from_bool(field_match || related_match)
}

fn string_field_search_decision(record: &Value, field: &str, value: &str) -> StagedSearchDecision {
    StagedSearchDecision::from_bool(online_store_search_string_matches(
        record.get(field).and_then(Value::as_str),
        value,
    ))
}

fn array_field_search_decision(record: &Value, field: &str, value: &str) -> StagedSearchDecision {
    StagedSearchDecision::from_bool(array_field_matches(record, field, value))
}

fn array_field_matches(record: &Value, field: &str, value: &str) -> bool {
    record
        .get(field)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .any(|entry| online_store_search_string_matches(entry.as_str(), value))
        })
        .unwrap_or(false)
}

fn id_search_decision(record: &Value, value: &str) -> StagedSearchDecision {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let tail = resource_id_tail(id);
    StagedSearchDecision::from_bool(
        online_store_search_string_matches(Some(id), value)
            || online_store_search_string_matches(Some(tail), value),
    )
}

fn published_status_search_decision(record: &Value, value: &str) -> StagedSearchDecision {
    let is_published = record
        .get("isPublished")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match value.to_ascii_lowercase().as_str() {
        "any" => StagedSearchDecision::Match,
        "published" | "visible" | "true" => StagedSearchDecision::from_bool(is_published),
        "unpublished" | "hidden" | "false" => StagedSearchDecision::from_bool(!is_published),
        _ => StagedSearchDecision::Unsupported,
    }
}

fn online_store_search_string_matches(actual: Option<&str>, expected: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    if expected.is_empty() {
        return true;
    }
    let actual = actual.unwrap_or_default().to_ascii_lowercase();
    if let Some(prefix) = expected.strip_suffix('*') {
        return actual
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&expected)
}

pub(super) fn online_store_sort_key(
    kind: OnlineStoreKind,
    record: &Value,
    sort_key: &str,
) -> StagedSortKey {
    let normalized = sort_key.to_ascii_uppercase();
    let primary = match kind {
        OnlineStoreKind::Blog => match normalized.as_str() {
            "TITLE" => online_store_sort_string(record, "title"),
            "HANDLE" => online_store_sort_string(record, "handle"),
            "CREATED_AT" => online_store_sort_string(record, "createdAt"),
            "UPDATED_AT" => online_store_sort_string(record, "updatedAt"),
            "ID" | "RELEVANCE" => online_store_gid_tail_sort_value(record),
            _ => online_store_gid_tail_sort_value(record),
        },
        OnlineStoreKind::Page => match normalized.as_str() {
            "TITLE" => online_store_sort_string(record, "title"),
            "HANDLE" => online_store_sort_string(record, "handle"),
            "CREATED_AT" => online_store_sort_string(record, "createdAt"),
            "UPDATED_AT" => online_store_sort_string(record, "updatedAt"),
            "PUBLISHED_AT" => online_store_nullable_sort_string(record, "publishedAt"),
            "ID" | "RELEVANCE" => online_store_gid_tail_sort_value(record),
            _ => online_store_gid_tail_sort_value(record),
        },
        OnlineStoreKind::Article => match normalized.as_str() {
            "TITLE" => online_store_sort_string(record, "title"),
            "HANDLE" => online_store_sort_string(record, "handle"),
            "AUTHOR" => record
                .get("author")
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str)
                .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
                .unwrap_or(StagedSortValue::Null),
            "BLOG_TITLE" => record
                .get("blog")
                .and_then(|blog| blog.get("title"))
                .and_then(Value::as_str)
                .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
                .unwrap_or(StagedSortValue::Null),
            "CREATED_AT" => online_store_sort_string(record, "createdAt"),
            "UPDATED_AT" => online_store_sort_string(record, "updatedAt"),
            "PUBLISHED_AT" => online_store_nullable_sort_string(record, "publishedAt"),
            "ID" | "RELEVANCE" => online_store_gid_tail_sort_value(record),
            _ => online_store_gid_tail_sort_value(record),
        },
        OnlineStoreKind::Comment => match normalized.as_str() {
            "STATUS" => online_store_sort_string(record, "status"),
            "CREATED_AT" => online_store_sort_string(record, "createdAt"),
            "UPDATED_AT" => online_store_sort_string(record, "updatedAt"),
            "PUBLISHED_AT" => online_store_nullable_sort_string(record, "publishedAt"),
            "ID" | "RELEVANCE" => online_store_gid_tail_sort_value(record),
            _ => online_store_gid_tail_sort_value(record),
        },
    };
    vec![primary, online_store_gid_tail_sort_value(record)]
}

fn online_store_sort_string(record: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(
        record
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn online_store_nullable_sort_string(record: &Value, field: &str) -> StagedSortValue {
    record
        .get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_string()))
        .unwrap_or(StagedSortValue::Null)
}

fn online_store_gid_tail_sort_value(record: &Value) -> StagedSortValue {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}
