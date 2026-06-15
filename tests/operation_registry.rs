use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use shopify_draft_proxy::graphql::OperationType;
use shopify_draft_proxy::operation_registry::{
    default_registry, find_entry, implemented_entries, local_dispatch_roots, operation_capability,
    CapabilityDomain, CapabilityExecution, OperationRegistryEntry,
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
            name: "customerCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Customers,
            execution: CapabilityExecution::StageLocally,
            implemented: true,
            match_names: vec!["customerCreate".to_string()],
            runtime_tests: vec![],
            support_notes: None,
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

    assert_eq!(names, vec!["product", "productCreate", "customerCreate"]);
}

#[test]
fn operation_capability_returns_registry_match_for_local_dispatch_roots_only() {
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

    let customer_create =
        operation_capability(&registry, OperationType::Mutation, Some("customerCreate"));
    assert_eq!(customer_create.domain, CapabilityDomain::Customers);
    assert_eq!(customer_create.execution, CapabilityExecution::StageLocally);
    assert_eq!(
        customer_create.operation_name.as_deref(),
        Some("customerCreate")
    );

    let implemented_without_dispatch_root = operation_capability(
        &registry,
        OperationType::Query,
        Some("definitelyNotALocalDispatchRoot"),
    );
    assert_eq!(
        implemented_without_dispatch_root.domain,
        CapabilityDomain::Unknown
    );
    assert_eq!(
        implemented_without_dispatch_root.execution,
        CapabilityExecution::Passthrough
    );
    assert_eq!(
        implemented_without_dispatch_root.operation_name.as_deref(),
        Some("definitelyNotALocalDispatchRoot")
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

    // Not every implemented operation carries runtime tests: `implemented` now covers the full
    // locally-handled surface, while declared runtime coverage stays scoped to the operations
    // exercised by the uniform table dispatch. Validate every declared runtime-test reference,
    // but do not require one per implemented entry.
    for entry in &default_registry() {
        for runtime_test in &entry.runtime_tests {
            assert!(
                !runtime_test.ends_with(".gleam"),
                "registry entry {} still points at deleted Gleam test {}",
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DispatchKey {
    operation_type: String,
    name: String,
    domain: String,
    execution: String,
}

#[test]
fn local_dispatch_roots_are_a_subset_of_implemented_entries() {
    let implemented: BTreeSet<DispatchKey> = implemented_entries(&default_registry())
        .into_iter()
        .map(dispatch_key_from_registry_entry)
        .collect();
    let dispatch_roots: BTreeSet<DispatchKey> = local_dispatch_roots()
        .iter()
        .map(dispatch_key_from_local_root)
        .collect();

    // The dispatch root table is now the full local-routing inventory. There are no
    // document-gated local handlers outside this table.
    let missing: Vec<&DispatchKey> = dispatch_roots.difference(&implemented).collect();
    assert!(
        missing.is_empty(),
        "every local dispatch root must be an implemented registry entry; missing: {missing:?}"
    );
    let missing_dispatch_roots: Vec<&DispatchKey> =
        implemented.difference(&dispatch_roots).collect();
    assert!(
        missing_dispatch_roots.is_empty(),
        "every implemented registry entry must have a local dispatch root; missing: {missing_dispatch_roots:?}"
    );
}

#[test]
fn implemented_entries_route_through_local_dispatch_roots() {
    let registry = default_registry();
    let dispatch_roots: BTreeSet<DispatchKey> = local_dispatch_roots()
        .iter()
        .map(dispatch_key_from_local_root)
        .collect();

    for entry in implemented_entries(&registry) {
        let capability =
            operation_capability(&registry, entry.operation_type, Some(entry.name.as_str()));
        assert!(
            dispatch_roots.contains(&dispatch_key_from_registry_entry(entry)),
            "{} is implemented and must be present in LOCAL_DISPATCH_ROOTS",
            entry.name
        );
        assert_eq!(
            capability.domain, entry.domain,
            "{} is a dispatch root and must keep its capability domain",
            entry.name
        );
        assert_ne!(
            capability.execution,
            CapabilityExecution::Passthrough,
            "{} is a dispatch root and must dispatch locally",
            entry.name
        );
    }
}

fn dispatch_key_from_registry_entry(entry: &OperationRegistryEntry) -> DispatchKey {
    DispatchKey {
        operation_type: operation_type_name(entry.operation_type).to_string(),
        name: entry.name.clone(),
        domain: entry.domain.registry_name().to_string(),
        execution: entry.execution.registry_name().to_string(),
    }
}

fn dispatch_key_from_local_root(
    root: &shopify_draft_proxy::operation_registry::LocalDispatchRoot,
) -> DispatchKey {
    DispatchKey {
        operation_type: operation_type_name(root.operation_type).to_string(),
        name: root.name.to_string(),
        domain: root.domain.registry_name().to_string(),
        execution: root.execution.registry_name().to_string(),
    }
}

fn operation_type_name(operation_type: OperationType) -> &'static str {
    match operation_type {
        OperationType::Query => "query",
        OperationType::Mutation => "mutation",
        OperationType::Subscription => "subscription",
    }
}
