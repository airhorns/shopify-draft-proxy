use super::*;

pub(super) fn discount_bulk_selector_count(arguments: &BTreeMap<String, ResolvedValue>) -> usize {
    let ids_present = arguments.contains_key("ids");
    let search_present = arguments.contains_key("search");
    let saved_search_present =
        arguments.contains_key("savedSearchId") || arguments.contains_key("saved_search_id");
    ids_present as usize + search_present as usize + saved_search_present as usize
}

pub(super) fn discount_redeem_code_bulk_creation(
    codes: &[String],
    existing: &BTreeSet<String>,
    pending: bool,
) -> Value {
    let failed_count = if pending {
        0
    } else {
        codes
            .iter()
            .enumerate()
            .filter(|(index, code)| !redeem_code_accepted(code, codes, *index, existing))
            .count()
    };
    let imported_count = if pending {
        0
    } else {
        codes.len() - failed_count
    };
    // The caller assigns the synthetic creation id; this id is always overwritten.
    json!({
        "id": Value::Null,
        "done": !pending,
        "codesCount": codes.len(),
        "importedCount": imported_count,
        "failedCount": failed_count,
        "codes": connection_json_with_empty_edges(
            codes.iter().enumerate().map(|(index, code)| {
                discount_redeem_code_bulk_creation_node(code, codes, index, existing, pending)
            }).collect::<Vec<_>>()
        )
    })
}

pub(super) fn discount_redeem_code_bulk_creation_node(
    code: &str,
    codes: &[String],
    index: usize,
    existing: &BTreeSet<String>,
    pending: bool,
) -> Value {
    let errors = if pending {
        Vec::new()
    } else {
        redeem_code_errors(code, codes, index, existing)
    };
    let accepted = errors.is_empty();
    json!({
        "code": code,
        "errors": errors,
        "discountRedeemCode": if pending || !accepted { Value::Null } else { json!({
            "id": synthetic_shopify_gid("DiscountRedeemCode", stable_redeem_code_suffix(code)),
            "code": code
        }) }
    })
}

/// Whether a `discountRedeemCodeBulkAdd` `codes` argument was supplied as a bare
/// `[String!]` list (the legacy local-runtime shape) rather than the schema
/// `[DiscountRedeemCodeInput!]` object list. String submissions complete
/// synchronously; object submissions follow Shopify's async creation shape.
pub(super) fn redeem_codes_are_string_inputs(arguments: &BTreeMap<String, ResolvedValue>) -> bool {
    match arguments.get("codes") {
        Some(ResolvedValue::List(items)) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| matches!(item, ResolvedValue::String(_)))
        }
        _ => false,
    }
}

pub(super) fn resolved_redeem_codes(arguments: &BTreeMap<String, ResolvedValue>) -> Vec<String> {
    match arguments.get("codes") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => match object.get("code") {
                    Some(ResolvedValue::String(code)) => Some(code.clone()),
                    _ => None,
                },
                ResolvedValue::String(code) => Some(code.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn redeem_code_accepted(
    code: &str,
    codes: &[String],
    index: usize,
    existing: &BTreeSet<String>,
) -> bool {
    redeem_code_errors(code, codes, index, existing).is_empty()
}

/// Per-code validation for a `discountRedeemCodeBulkAdd` submission. `existing`
/// is the set of codes (uppercased) already assigned to any discount in the
/// shop before this batch; `codes`/`index` locate the code within the batch so
/// duplicates within the same submission can be detected.
pub(super) fn redeem_code_errors(
    code: &str,
    codes: &[String],
    index: usize,
    existing: &BTreeSet<String>,
) -> Vec<Value> {
    if code.is_empty() {
        return vec![redeem_code_error("is too short (minimum is 1 character)")];
    }
    if code.contains('\n') || code.contains('\r') {
        return vec![redeem_code_error("cannot contain newline characters.")];
    }
    if code.chars().count() > 255 {
        return vec![redeem_code_error("is too long (maximum is 255 characters)")];
    }
    let normalized = code.to_ascii_uppercase();
    // A second (or later) occurrence of the same code within this submission.
    if codes
        .iter()
        .take(index)
        .any(|candidate| candidate.to_ascii_uppercase() == normalized)
    {
        return vec![redeem_code_error(
            "Codes must be unique within BulkDiscountCodeCreation",
        )];
    }
    // The code is already assigned to some discount in the shop.
    if existing.contains(&normalized) {
        return vec![redeem_code_error(
            "must be unique. Please try a different code.",
        )];
    }
    Vec::new()
}

pub(super) fn redeem_code_error(message: &str) -> Value {
    user_error_with_extra_info(["code"], message, None, Value::Null)
}

pub(super) fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}
