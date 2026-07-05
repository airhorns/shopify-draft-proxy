use super::*;
use crate::proxy::search::{
    parse_search_query, search_comparator, search_string_matches, ParsedSearchTerm,
};

pub(in crate::proxy) fn product_matches_search_query(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    query: &str,
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let Some(expression) = parse_search_query(query) else {
        return false;
    };
    expression.matches_with(&mut |term| product_search_term_matches(product, variants, term))
}

fn product_search_term_matches(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    term: &ParsedSearchTerm,
) -> bool {
    let value = term.value.trim();
    if value.is_empty() {
        return true;
    }
    match term.field.as_deref() {
        Some("id") => product_matches_search_id(product, value),
        Some("status") => product.status.eq_ignore_ascii_case(value),
        Some("vendor") => product_search_string_matches(&product.vendor, value),
        Some("product_type") => product_search_string_matches(&product.product_type, value),
        Some("title") => product_search_string_matches(&product.title, value),
        Some("handle") => product_search_string_matches(&product.handle, value),
        Some("tag") => product_matches_search_tag(product, value),
        Some("tag_not") => !product_matches_search_tag(product, value),
        Some("sku") => product_matches_search_sku(product, variants, value),
        Some("barcode") => product_matches_search_barcode(product, variants, value),
        Some("gift_card") => product_matches_search_gift_card(product, value),
        Some("collection_id") => product_matches_search_collection_id(product, value),
        Some("published_status") => product_matches_published_status(product, value),
        Some("published_at") => product_matches_published_at(product, value),
        Some("created_at") => product_matches_date_query(&product.created_at, value),
        Some("updated_at") => product_matches_date_query(&product.updated_at, value),
        Some(_) => false,
        None => product_matches_free_text(product, variants, value),
    }
}

fn product_matches_free_text(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    product_search_string_matches(&product.title, value)
        || product_search_string_matches(&product.handle, value)
        || product_search_string_matches(&product.vendor, value)
        || product_search_string_matches(&product.product_type, value)
        || product_matches_search_tag(product, value)
        || product_matches_search_sku(product, variants, value)
}

fn product_matches_search_id(product: &ProductRecord, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    product.id == value || resource_id_path_tail(&product.id) == value
}

fn product_matches_search_tag(product: &ProductRecord, value: &str) -> bool {
    product
        .tags
        .iter()
        .any(|tag| product_search_string_matches(tag, value))
}

fn product_matches_search_sku(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    variants
        .iter()
        .any(|variant| product_search_string_matches(&variant.sku, value))
        || product.variants.iter().any(|variant| {
            variant
                .get("sku")
                .and_then(Value::as_str)
                .is_some_and(|sku| product_search_string_matches(sku, value))
        })
}

fn product_matches_search_barcode(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    variants.iter().any(|variant| {
        variant
            .barcode
            .as_deref()
            .is_some_and(|barcode| product_search_string_matches(barcode, value))
    }) || product.variants.iter().any(|variant| {
        variant
            .get("barcode")
            .and_then(Value::as_str)
            .is_some_and(|barcode| product_search_string_matches(barcode, value))
    })
}

fn product_matches_search_gift_card(product: &ProductRecord, value: &str) -> bool {
    let actual = product
        .extra_fields
        .get("isGiftCard")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match value.to_ascii_lowercase().as_str() {
        "true" => actual,
        "false" => !actual,
        _ => false,
    }
}

fn product_matches_search_collection_id(product: &ProductRecord, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    product.collections.iter().any(|collection| {
        collection
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == value || resource_id_path_tail(id) == value)
    })
}

pub(in crate::proxy) fn product_search_string_matches(actual: &str, query_value: &str) -> bool {
    search_string_matches(actual, query_value)
}

fn product_matches_published_status(product: &ProductRecord, value: &str) -> bool {
    let published = product_is_published(product);
    match value.to_ascii_lowercase().as_str() {
        "published" => published,
        "unpublished" => !published,
        "any" => true,
        _ => false,
    }
}

fn product_matches_published_at(product: &ProductRecord, value: &str) -> bool {
    product
        .extra_fields
        .get("publishedAt")
        .and_then(Value::as_str)
        .is_some_and(|published_at| product_matches_date_query(published_at, value))
}

fn product_is_published(product: &ProductRecord) -> bool {
    product
        .extra_fields
        .get("publishedAt")
        .is_some_and(|published_at| !published_at.is_null())
        || !product_visible_publication_entries(product).is_empty()
}

pub(in crate::proxy) fn product_matches_date_query(actual: &str, query_value: &str) -> bool {
    let (operator, expected) = search_comparator(query_value);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}
