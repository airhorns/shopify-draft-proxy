use pretty_assertions::assert_eq;
use shopify_draft_proxy::graphql::OperationType;
use shopify_draft_proxy::operation_registry::{
    default_registry, find_entry, implemented_entries, operation_capability, CapabilityDomain,
    CapabilityExecution, OperationRegistryEntry,
};

fn sample_registry() -> Vec<OperationRegistryEntry> {
    vec![
        OperationRegistryEntry {
            name: "product".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::OverlayRead,
            implemented: true,
            match_names: vec!["product".to_string(), "Product".to_string()],
            runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
            support_notes: None,
        },
        OperationRegistryEntry {
            name: "productCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: vec!["productCreate".to_string()],
            runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
            support_notes: Some("stages products locally".to_string()),
        },
        OperationRegistryEntry {
            name: "app".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Apps,
            execution: CapabilityExecution::OverlayRead,
            implemented: false,
            match_names: vec!["app".to_string(), "App".to_string()],
            runtime_tests: vec![],
            support_notes: None,
        },
    ]
}

#[test]
fn registry_finds_first_nonempty_candidate_by_type_and_match_name() {
    let registry = sample_registry();

    let entry = find_entry(
        &registry,
        OperationType::Query,
        &[None, Some(""), Some("Product")],
    )
    .expect("Product match name should resolve to product entry");

    assert_eq!(entry.name, "product");
    assert_eq!(entry.domain, CapabilityDomain::Products);
    assert_eq!(entry.execution, CapabilityExecution::OverlayRead);
    assert!(find_entry(&registry, OperationType::Mutation, &[Some("Product")]).is_none());
}

#[test]
fn implemented_entries_filter_unimplemented_registry_rows() {
    let registry = sample_registry();

    let names: Vec<&str> = implemented_entries(&registry)
        .into_iter()
        .map(|entry| entry.name.as_str())
        .collect();

    assert_eq!(names, vec!["product", "productCreate"]);
}

#[test]
fn operation_capability_returns_registry_match_for_implemented_roots_only() {
    let registry = sample_registry();

    let product = operation_capability(&registry, OperationType::Query, Some("product"));
    assert_eq!(product.domain, CapabilityDomain::Products);
    assert_eq!(product.execution, CapabilityExecution::OverlayRead);
    assert_eq!(product.operation_name.as_deref(), Some("product"));

    let product_create =
        operation_capability(&registry, OperationType::Mutation, Some("productCreate"));
    assert_eq!(product_create.domain, CapabilityDomain::Products);
    assert_eq!(product_create.execution, CapabilityExecution::StageLocally);
    assert_eq!(
        product_create.operation_name.as_deref(),
        Some("productCreate")
    );

    let unimplemented = operation_capability(&registry, OperationType::Query, Some("app"));
    assert_eq!(unimplemented.domain, CapabilityDomain::Unknown);
    assert_eq!(unimplemented.execution, CapabilityExecution::Passthrough);
    assert_eq!(unimplemented.operation_name.as_deref(), Some("app"));

    let missing = operation_capability(
        &registry,
        OperationType::Query,
        Some("definitelyUnknownRoot"),
    );
    assert_eq!(missing.domain, CapabilityDomain::Unknown);
    assert_eq!(missing.execution, CapabilityExecution::Passthrough);
    assert_eq!(
        missing.operation_name.as_deref(),
        Some("definitelyUnknownRoot")
    );
}

#[test]
fn default_registry_classifies_core_port_targets_without_runtime_io() {
    let registry = default_registry();

    let product = operation_capability(&registry, OperationType::Query, Some("product"));
    assert_eq!(product.domain, CapabilityDomain::Products);
    assert_eq!(product.execution, CapabilityExecution::OverlayRead);

    let product_create =
        operation_capability(&registry, OperationType::Mutation, Some("productCreate"));
    assert_eq!(product_create.domain, CapabilityDomain::Products);
    assert_eq!(product_create.execution, CapabilityExecution::StageLocally);

    let order_saved_searches =
        operation_capability(&registry, OperationType::Query, Some("orderSavedSearches"));
    assert_eq!(order_saved_searches.domain, CapabilityDomain::SavedSearches);
    assert_eq!(
        order_saved_searches.execution,
        CapabilityExecution::OverlayRead
    );

    let saved_search_create = operation_capability(
        &registry,
        OperationType::Mutation,
        Some("savedSearchCreate"),
    );
    assert_eq!(saved_search_create.domain, CapabilityDomain::SavedSearches);
    assert_eq!(
        saved_search_create.execution,
        CapabilityExecution::StageLocally
    );

    let app = operation_capability(&registry, OperationType::Query, Some("app"));
    assert_eq!(app.domain, CapabilityDomain::Unknown);
    assert_eq!(app.execution, CapabilityExecution::Passthrough);
}

#[test]
fn default_registry_runtime_tests_reference_current_rust_coverage_files() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    for entry in implemented_entries(&default_registry()) {
        assert!(
            !entry.runtime_tests.is_empty(),
            "implemented registry entry {} should declare executable runtime coverage",
            entry.name
        );

        for runtime_test in &entry.runtime_tests {
            assert!(
                !runtime_test.ends_with(".gleam"),
                "implemented registry entry {} still points at deleted Gleam test {}",
                entry.name,
                runtime_test
            );
            assert!(
                repo_root.join(runtime_test).exists(),
                "implemented registry entry {} points at missing runtime test {}",
                entry.name,
                runtime_test
            );
        }
    }
}
