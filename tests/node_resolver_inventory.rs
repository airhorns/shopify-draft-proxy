use shopify_draft_proxy::node_resolver_inventory::{
    default_node_resolver_inventory, default_node_resolver_inventory_json_value,
};
use std::collections::BTreeSet;

#[test]
fn default_node_resolver_inventory_is_unique_and_sorted() {
    let inventory = default_node_resolver_inventory();
    let mut names = Vec::new();
    for entry in inventory {
        assert!(
            !entry.type_name.trim().is_empty(),
            "Node resolver inventory entries must name the Shopify type"
        );
        assert!(
            !entry.resolver.trim().is_empty(),
            "{} must name the Rust resolver path",
            entry.type_name
        );
        names.push(entry.type_name);
    }

    let unique_names: BTreeSet<_> = names.iter().copied().collect();
    assert_eq!(
        unique_names.len(),
        names.len(),
        "Node resolver inventory must not contain duplicate type names"
    );

    let sorted_names: Vec<_> = unique_names.into_iter().collect();
    assert_eq!(
        sorted_names, names,
        "Node resolver inventory should stay sorted for auditable diffs"
    );

    let exported = default_node_resolver_inventory_json_value();
    let exported_entries = exported
        .as_array()
        .expect("Node resolver inventory exporter should return a JSON array");
    assert_eq!(
        exported_entries.len(),
        inventory.len(),
        "JSON exporter should include every inventory entry"
    );
    for exported_entry in exported_entries {
        assert!(
            exported_entry
                .get("typeName")
                .and_then(|value| value.as_str())
                .is_some(),
            "exported entries should include a typeName"
        );
        assert!(
            exported_entry
                .get("resolver")
                .and_then(|value| value.as_str())
                .is_some(),
            "exported entries should include a resolver"
        );
        assert!(
            exported_entry
                .get("behavior")
                .and_then(|value| value.as_str())
                .is_some(),
            "exported entries should include a behavior"
        );
    }
}
