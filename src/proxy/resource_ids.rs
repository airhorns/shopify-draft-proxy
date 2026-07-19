use super::{DraftProxy, StagedRecords, StagedSortValue, Store};

pub(in crate::proxy) const SYNTHETIC_MARKER: &str = "shopify-draft-proxy=synthetic";
const SHOPIFY_GID_PREFIX: &str = "gid://shopify/";

pub(in crate::proxy) fn shopify_gid(resource_type: &str, id: impl std::fmt::Display) -> String {
    format!("{SHOPIFY_GID_PREFIX}{resource_type}/{id}")
}

pub(in crate::proxy) fn synthetic_shopify_gid(
    resource_type: &str,
    id: impl std::fmt::Display,
) -> String {
    format!("{}?{SYNTHETIC_MARKER}", shopify_gid(resource_type, id))
}

pub(in crate::proxy) fn is_synthetic_gid(id: &str) -> bool {
    has_shopify_gid_prefix(id) && id.contains(SYNTHETIC_MARKER)
}

pub(in crate::proxy) fn has_shopify_gid_prefix(id: &str) -> bool {
    id.starts_with(SHOPIFY_GID_PREFIX)
}

pub(in crate::proxy) fn resource_id_path_tail(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

pub(in crate::proxy) fn resource_id_tail(id: &str) -> &str {
    resource_id_path_tail(id)
        .split('?')
        .next()
        .unwrap_or_default()
}

pub(in crate::proxy) fn shopify_gid_tail_for_type<'a>(
    id: &'a str,
    resource_type: &str,
) -> Option<&'a str> {
    typed_shopify_gid_tail(id, resource_type).filter(|tail| !tail.is_empty())
}

pub(in crate::proxy) fn is_shopify_gid_of_type(id: &str, resource_type: &str) -> bool {
    typed_shopify_gid_tail(id, resource_type).is_some()
}

fn shopify_gid_identity(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix(SHOPIFY_GID_PREFIX)?;
    let (resource_type, resource_id) = rest.split_once('/')?;
    let tail = resource_id.split('?').next()?;
    (!resource_type.is_empty() && !tail.is_empty() && !tail.contains('/'))
        .then_some((resource_type, tail))
}

pub(in crate::proxy) fn shopify_gid_identities_overlap(left: &str, right: &str) -> bool {
    shopify_gid_identity(left)
        .zip(shopify_gid_identity(right))
        .is_some_and(|(left, right)| left == right)
}

fn value_contains_shopify_gid_identity(value: &serde_json::Value, candidate: &str) -> bool {
    match value {
        serde_json::Value::String(id) => shopify_gid_identities_overlap(id, candidate),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| value_contains_shopify_gid_identity(value, candidate)),
        serde_json::Value::Object(fields) => fields.iter().any(|(key, value)| {
            shopify_gid_identities_overlap(key, candidate)
                || value_contains_shopify_gid_identity(value, candidate)
        }),
        _ => false,
    }
}

pub(in crate::proxy) fn shopify_gid_resource_type(id: &str) -> Option<&str> {
    let rest = id.strip_prefix(SHOPIFY_GID_PREFIX)?;
    let (resource_type, resource_id) = rest.split_once('/')?;
    (!resource_type.is_empty() && !resource_id.is_empty()).then_some(resource_type)
}

pub(in crate::proxy) fn staged_record_key_for_shopify_gid<T>(
    records: &StagedRecords<T>,
    submitted_id: &str,
    resource_type: &str,
) -> Option<String> {
    if records.records.contains_key(submitted_id) || records.tombstones.contains(submitted_id) {
        return Some(submitted_id.to_string());
    }

    let tail = unmarked_shopify_gid_tail_for_type(submitted_id, resource_type)?;
    records
        .records
        .keys()
        .chain(records.tombstones.iter())
        .find(|candidate| staged_synthetic_key_matches_tail(candidate, resource_type, tail))
        .cloned()
}

fn unmarked_shopify_gid_tail_for_type<'a>(id: &'a str, resource_type: &str) -> Option<&'a str> {
    let tail = typed_shopify_gid_tail(id, resource_type)?;
    (!tail.is_empty() && !tail.contains('/') && !tail.contains('?')).then_some(tail)
}

fn staged_synthetic_key_matches_tail(candidate: &str, resource_type: &str, tail: &str) -> bool {
    is_synthetic_gid(candidate)
        && shopify_gid_tail_for_type(candidate, resource_type)
            .is_some_and(|candidate_tail| resource_id_tail(candidate_tail) == tail)
}

fn typed_shopify_gid_tail<'a>(id: &'a str, resource_type: &str) -> Option<&'a str> {
    let rest = id.strip_prefix(SHOPIFY_GID_PREFIX)?;
    let (candidate_type, tail) = rest.split_once('/')?;
    (candidate_type == resource_type).then_some(tail)
}

pub(in crate::proxy) fn resource_id_tail_sort_value(id: Option<&str>) -> StagedSortValue {
    let tail = id.map(resource_id_tail).unwrap_or_default();
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

pub(in crate::proxy) fn resource_id_matches_gid_or_tail(id: &str, value: &str) -> bool {
    id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value
}

pub(in crate::proxy) fn metafield_owner_gid_resource_type(id: &str) -> String {
    shopify_gid_resource_type(id).unwrap_or(id).to_string()
}

impl DraftProxy {
    pub(in crate::proxy) fn next_proxy_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        synthetic_shopify_gid(resource_type, id)
    }

    pub(in crate::proxy) fn next_proxy_synthetic_gid_avoiding<IdentityKnown>(
        &mut self,
        resource_type: &str,
        identity_known: IdentityKnown,
    ) -> String
    where
        IdentityKnown: Fn(&Store, &str) -> bool,
    {
        loop {
            let candidate = self.next_proxy_synthetic_gid(resource_type);
            let present_in_log = self
                .log_entries
                .iter()
                .any(|entry| value_contains_shopify_gid_identity(entry, &candidate));
            if !identity_known(&self.store, &candidate) && !present_in_log {
                return candidate;
            }
        }
    }

    /// Mint a plain `gid://shopify/<type>/<id>` without the proxy-synthetic
    /// marker. Used for
    /// entities (e.g. media files) the proxy fabricates with stable identifiers
    /// rather than commit-rewritten placeholders.
    pub(in crate::proxy) fn next_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        shopify_gid(resource_type, id)
    }

    /// Reserve a synthetic id for a mutation-log entry at the start of every successful mutation. This keeps entity ids in lockstep with the current synthetic-id contract: each mutation advances the counter once for its log entry before allocating the resources it creates.
    pub(in crate::proxy) fn reserve_synthetic_log_id(&mut self) {
        self.next_synthetic_id += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn builds_plain_and_synthetic_shopify_gids() {
        assert_eq!(shopify_gid("Product", 42), "gid://shopify/Product/42");
        assert_eq!(
            synthetic_shopify_gid("Product", 42),
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        );
        assert!(shopify_gid_identities_overlap(
            "gid://shopify/Product/42",
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        ));
        assert!(!shopify_gid_identities_overlap(
            "gid://shopify/Product/42",
            "gid://shopify/Customer/42?shopify-draft-proxy=synthetic"
        ));
        assert!(!shopify_gid_identities_overlap(
            "gid://shopify/Market/42",
            "gid://shopify/Market/Region/42"
        ));
    }

    #[test]
    fn extracts_resource_id_tails_with_and_without_query_strings() {
        assert_eq!(resource_id_path_tail("gid://shopify/Product/42"), "42");
        assert_eq!(
            resource_id_path_tail("gid://shopify/Product/42?shopify-draft-proxy=synthetic"),
            "42?shopify-draft-proxy=synthetic"
        );
        assert_eq!(
            resource_id_tail("gid://shopify/Product/42?shopify-draft-proxy=synthetic"),
            "42"
        );
        assert_eq!(resource_id_tail("42"), "42");
    }

    #[test]
    fn extracts_type_checked_shopify_gid_tails() {
        assert_eq!(
            shopify_gid_tail_for_type("gid://shopify/Product/42", "Product"),
            Some("42")
        );
        assert_eq!(
            shopify_gid_tail_for_type(
                "gid://shopify/Product/42?shopify-draft-proxy=synthetic",
                "Product"
            ),
            Some("42?shopify-draft-proxy=synthetic")
        );
        assert_eq!(
            shopify_gid_tail_for_type("gid://shopify/Product/42", "Customer"),
            None
        );
        assert!(is_shopify_gid_of_type(
            "gid://shopify/Product/42",
            "Product"
        ));
        assert!(is_shopify_gid_of_type("gid://shopify/Product/", "Product"));
        assert_eq!(
            shopify_gid_tail_for_type("gid://shopify/Product/", "Product"),
            None
        );
        assert!(has_shopify_gid_prefix("gid://shopify/"));
    }

    #[test]
    fn compares_ids_against_full_gid_tail_and_path_tail() {
        let synthetic = "gid://shopify/Product/42?shopify-draft-proxy=synthetic";
        assert!(resource_id_matches_gid_or_tail(synthetic, synthetic));
        assert!(resource_id_matches_gid_or_tail(synthetic, "42"));
        assert!(resource_id_matches_gid_or_tail(
            synthetic,
            "42?shopify-draft-proxy=synthetic"
        ));
        assert!(!resource_id_matches_gid_or_tail(synthetic, "43"));
    }

    #[test]
    fn sorts_gid_tails_as_numeric_then_lowercase_string() {
        assert_eq!(
            resource_id_tail_sort_value(Some("gid://shopify/Product/42")),
            StagedSortValue::I64(42)
        );
        assert_eq!(
            resource_id_tail_sort_value(Some("gid://shopify/Product/abc")),
            StagedSortValue::String("abc".to_string())
        );
        assert_eq!(
            resource_id_tail_sort_value(Some("gid://shopify/Product/ABC")),
            StagedSortValue::String("abc".to_string())
        );
    }

    #[test]
    fn extracts_shopify_gid_resource_types_only_for_complete_shopify_gids() {
        assert_eq!(
            shopify_gid_resource_type("gid://shopify/Customer/123"),
            Some("Customer")
        );
        assert_eq!(
            shopify_gid_resource_type("gid://shopify/Customer/123?shopify-draft-proxy=synthetic"),
            Some("Customer")
        );
        assert_eq!(shopify_gid_resource_type("gid://shopify/Customer/"), None);
        assert_eq!(shopify_gid_resource_type("not-a-gid"), None);
    }

    #[test]
    fn detects_synthetic_shopify_gids() {
        assert!(is_synthetic_gid(
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        ));
        assert!(!is_synthetic_gid("gid://shopify/Product/42"));
        assert!(!is_synthetic_gid("not-a-gid?shopify-draft-proxy=synthetic"));
    }

    #[test]
    fn maps_metafield_owner_gid_types_without_collapsing_unknown_resource_types() {
        assert_eq!(
            metafield_owner_gid_resource_type("gid://shopify/ProductVariant/1"),
            "ProductVariant"
        );
        assert_eq!(
            metafield_owner_gid_resource_type("gid://shopify/Company/1"),
            "Company"
        );
        assert_eq!(
            metafield_owner_gid_resource_type("gid://shopify/Unknown/1"),
            "Unknown"
        );
        assert_eq!(metafield_owner_gid_resource_type("not-a-gid"), "not-a-gid");
    }

    fn staged_records_with_ids(ids: &[&str]) -> StagedRecords<Value> {
        let mut records = StagedRecords::default();
        for id in ids {
            records.insert((*id).to_string(), json!({"id": id}));
        }
        records
    }

    #[test]
    fn resolves_exact_staged_keys_before_canonical_synthetic_fallback() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let records = staged_records_with_ids(&[&synthetic, &canonical]);

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &synthetic, "Metaobject"),
            Some(synthetic.clone())
        );
        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(canonical)
        );
    }

    #[test]
    fn resolves_unmarked_canonical_gid_to_staged_synthetic_key() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let records = staged_records_with_ids(&[&synthetic]);

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(synthetic)
        );
    }

    #[test]
    fn resolves_unmarked_canonical_gid_to_synthetic_tombstone_key() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let mut records = staged_records_with_ids(&[&synthetic]);
        records.remove(&synthetic);
        records.tombstone(synthetic.clone());

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(synthetic)
        );
    }

    #[test]
    fn resolves_exact_tombstone_before_canonical_synthetic_fallback() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let canonical = shopify_gid("Metaobject", 42);
        let mut records = staged_records_with_ids(&[&synthetic]);
        records.tombstone(canonical.clone());

        assert_eq!(
            staged_record_key_for_shopify_gid(&records, &canonical, "Metaobject"),
            Some(canonical)
        );
    }

    #[test]
    fn rejects_noncanonical_or_wrong_type_staged_key_fallbacks() {
        let synthetic = synthetic_shopify_gid("Metaobject", 42);
        let definition_synthetic = synthetic_shopify_gid("MetaobjectDefinition", 42);
        let records = staged_records_with_ids(&[&synthetic, &definition_synthetic]);

        for rejected in [
            "gid://shopify/Metaobject/43?shopify-draft-proxy=synthetic",
            "gid://shopify/Metaobject/42?other=query",
            "gid://shopify/Metaobject/",
            "gid://shopify/Metaobject/42/extra",
            "gid://shopify/Product/42",
            "42",
            "not-a-gid",
            "gid://shopify/",
        ] {
            assert_eq!(
                staged_record_key_for_shopify_gid(&records, rejected, "Metaobject"),
                None,
                "{rejected} should not resolve by fallback"
            );
        }
        assert_eq!(
            staged_record_key_for_shopify_gid(
                &records,
                "gid://shopify/Metaobject/43",
                "Metaobject"
            ),
            None
        );
        assert_eq!(
            staged_record_key_for_shopify_gid(
                &records,
                "gid://shopify/Metaobject/42",
                "MetaobjectDefinition",
            ),
            None
        );
    }
}
