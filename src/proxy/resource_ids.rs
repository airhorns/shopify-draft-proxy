use super::DraftProxy;

pub(in crate::proxy) const SYNTHETIC_MARKER: &str = "shopify-draft-proxy=synthetic";

pub(in crate::proxy) fn shopify_gid(resource_type: &str, id: impl std::fmt::Display) -> String {
    format!("gid://shopify/{resource_type}/{id}")
}

pub(in crate::proxy) fn synthetic_shopify_gid(
    resource_type: &str,
    id: impl std::fmt::Display,
) -> String {
    format!("{}?{SYNTHETIC_MARKER}", shopify_gid(resource_type, id))
}

pub(in crate::proxy) fn is_synthetic_gid(id: &str) -> bool {
    id.starts_with("gid://shopify/") && id.contains(SYNTHETIC_MARKER)
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
    let rest = id.strip_prefix("gid://shopify/")?;
    let (candidate_type, tail) = rest.split_once('/')?;
    (candidate_type == resource_type && !tail.is_empty()).then_some(tail)
}

pub(in crate::proxy) fn is_shopify_gid_of_type(id: &str, resource_type: &str) -> bool {
    shopify_gid_tail_for_type(id, resource_type).is_some()
}

pub(in crate::proxy) fn shopify_gid_resource_type(id: &str) -> Option<&str> {
    let rest = id.strip_prefix("gid://shopify/")?;
    let (resource_type, resource_id) = rest.split_once('/')?;
    (!resource_type.is_empty() && !resource_id.is_empty()).then_some(resource_type)
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

    /// Mint a plain `gid://shopify/<type>/<id>` without the proxy-synthetic
    /// marker, mirroring Gleam `synthetic_identity.make_synthetic_gid`. Used for
    /// entities (e.g. media files) the proxy fabricates with stable identifiers
    /// rather than commit-rewritten placeholders.
    pub(in crate::proxy) fn next_synthetic_gid(&mut self, resource_type: &str) -> String {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        shopify_gid(resource_type, id)
    }

    /// Reserve a synthetic id for a mutation-log entry, mirroring the
    /// `make_synthetic_gid(_, "MutationLogEntry")` reservation Gleam performs at
    /// the start of every successful mutation. This keeps entity ids in lockstep
    /// with the reference implementation (each mutation advances the counter once
    /// for its log entry before allocating the resources it creates).
    pub(in crate::proxy) fn reserve_synthetic_log_id(&mut self) {
        self.next_synthetic_id += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_plain_and_synthetic_shopify_gids() {
        assert_eq!(shopify_gid("Product", 42), "gid://shopify/Product/42");
        assert_eq!(
            synthetic_shopify_gid("Product", 42),
            "gid://shopify/Product/42?shopify-draft-proxy=synthetic"
        );
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
}
