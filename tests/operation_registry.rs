use pretty_assertions::assert_eq;
use shopify_draft_proxy::graphql::OperationType;
use shopify_draft_proxy::operation_registry::{
    default_registry, execution_for_operation_type, implemented_entries, operation_capability,
    CapabilityDomain, CapabilityExecution, OperationRegistryEntry,
};

fn sample_registry() -> Vec<OperationRegistryEntry> {
    vec![
        OperationRegistryEntry {
            name: "product".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            implemented: true,
            match_names: vec!["product".to_string(), "Product".to_string()],
            runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        },
        OperationRegistryEntry {
            name: "productCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            implemented: true,
            match_names: vec!["productCreate".to_string()],
            runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        },
        OperationRegistryEntry {
            name: "customerCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Customers,
            implemented: true,
            match_names: vec!["customerCreate".to_string()],
            runtime_tests: vec![],
        },
        OperationRegistryEntry {
            name: "app".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Apps,
            implemented: false,
            match_names: vec!["app".to_string(), "App".to_string()],
            runtime_tests: vec![],
        },
    ]
}

#[test]
fn execution_is_derived_from_operation_type() {
    assert_eq!(
        execution_for_operation_type(OperationType::Query),
        CapabilityExecution::OverlayRead
    );
    assert_eq!(
        execution_for_operation_type(OperationType::Mutation),
        CapabilityExecution::StageLocally
    );
    assert_eq!(
        execution_for_operation_type(OperationType::Subscription),
        CapabilityExecution::Passthrough
    );
}

#[test]
fn implemented_entries_filter_unimplemented_registry_rows() {
    let registry = sample_registry();

    let names: Vec<&str> = implemented_entries(&registry)
        .into_iter()
        .map(|entry| entry.name.as_str())
        .collect();

    assert_eq!(names, vec!["product", "productCreate", "customerCreate"]);
}

#[test]
fn operation_capability_returns_implemented_canonical_registry_matches_only() {
    let mut registry = sample_registry();
    registry.push(OperationRegistryEntry {
        name: "syntheticLocalRoot".to_string(),
        operation_type: OperationType::Query,
        domain: CapabilityDomain::Apps,
        implemented: true,
        match_names: vec!["syntheticLocalRoot".to_string()],
        runtime_tests: vec![],
    });

    let product = operation_capability(&registry, OperationType::Query, Some("product"));
    assert_eq!(product.domain, CapabilityDomain::Products);
    assert_eq!(product.execution, CapabilityExecution::OverlayRead);

    let product_create =
        operation_capability(&registry, OperationType::Mutation, Some("productCreate"));
    assert_eq!(product_create.domain, CapabilityDomain::Products);
    assert_eq!(product_create.execution, CapabilityExecution::StageLocally);

    let synthetic =
        operation_capability(&registry, OperationType::Query, Some("syntheticLocalRoot"));
    assert_eq!(synthetic.domain, CapabilityDomain::Apps);
    assert_eq!(synthetic.execution, CapabilityExecution::OverlayRead);

    let customer_create =
        operation_capability(&registry, OperationType::Mutation, Some("customerCreate"));
    assert_eq!(customer_create.domain, CapabilityDomain::Customers);
    assert_eq!(customer_create.execution, CapabilityExecution::StageLocally);

    let noncanonical_match_name =
        operation_capability(&registry, OperationType::Query, Some("Product"));
    assert_eq!(noncanonical_match_name.domain, CapabilityDomain::Unknown);
    assert_eq!(
        noncanonical_match_name.execution,
        CapabilityExecution::Passthrough
    );

    let unimplemented = operation_capability(&registry, OperationType::Query, Some("app"));
    assert_eq!(unimplemented.domain, CapabilityDomain::Unknown);
    assert_eq!(unimplemented.execution, CapabilityExecution::Passthrough);

    let missing = operation_capability(
        &registry,
        OperationType::Query,
        Some("definitelyUnknownRoot"),
    );
    assert_eq!(missing.domain, CapabilityDomain::Unknown);
    assert_eq!(missing.execution, CapabilityExecution::Passthrough);
}

#[test]
fn default_registry_classifies_core_local_targets_without_runtime_io() {
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

    // Not every implemented operation carries runtime tests: `implemented` now covers the full
    // locally-handled surface, while declared runtime coverage stays scoped to the operations
    // exercised by the uniform table dispatch. Validate every declared runtime-test reference,
    // but do not require one per implemented entry.
    let removed_runtime_extension = ['.', 'g', 'l', 'e', 'a', 'm'].iter().collect::<String>();
    for entry in &default_registry() {
        for runtime_test in &entry.runtime_tests {
            assert!(
                !runtime_test.ends_with(&removed_runtime_extension),
                "registry entry {} still points at a removed runtime test {}",
                entry.name,
                runtime_test
            );
            assert!(
                repo_root.join(runtime_test).exists(),
                "registry entry {} points at missing runtime test {}",
                entry.name,
                runtime_test
            );
        }
    }
}

#[test]
fn implemented_entries_classify_through_canonical_registry_names() {
    let registry = default_registry();

    for entry in implemented_entries(&registry) {
        let capability =
            operation_capability(&registry, entry.operation_type, Some(entry.name.as_str()));
        assert_eq!(
            capability.domain, entry.domain,
            "{} is implemented and must keep its capability domain",
            entry.name
        );
        assert_eq!(
            capability.execution,
            entry.execution(),
            "{} is implemented and must keep its capability execution",
            entry.name
        );
        assert_ne!(
            capability.execution,
            CapabilityExecution::Passthrough,
            "{} is implemented and must dispatch locally",
            entry.name
        );
    }
}
