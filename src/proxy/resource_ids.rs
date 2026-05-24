pub(in crate::proxy) fn shopify_gid(resource_type: &str, id: impl std::fmt::Display) -> String {
    format!("gid://shopify/{resource_type}/{id}")
}

pub(in crate::proxy) fn synthetic_shopify_gid(
    resource_type: &str,
    id: impl std::fmt::Display,
) -> String {
    format!(
        "{}?shopify-draft-proxy=synthetic",
        shopify_gid(resource_type, id)
    )
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

pub(in crate::proxy) fn shopify_gid_resource_type(id: &str) -> Option<&str> {
    let rest = id.strip_prefix("gid://shopify/")?;
    let (resource_type, resource_id) = rest.split_once('/')?;
    (!resource_type.is_empty() && !resource_id.is_empty()).then_some(resource_type)
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
    fn extracts_shopify_gid_resource_types_only_for_complete_shopify_gids() {
        assert_eq!(
            shopify_gid_resource_type("gid://shopify/Customer/123"),
            Some("Customer")
        );
        assert_eq!(shopify_gid_resource_type("gid://shopify/Customer/"), None);
        assert_eq!(shopify_gid_resource_type("not-a-gid"), None);
    }
}
