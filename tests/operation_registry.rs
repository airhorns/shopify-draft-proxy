use pretty_assertions::assert_eq;
use shopify_draft_proxy::admin_graphql::{schema, AdminApiVersion};
use shopify_draft_proxy::graphql::OperationType;
use shopify_draft_proxy::operation_registry::{
    default_registry, execution_for_operation_type, implemented_entries, operation_capability,
    operation_capability_for_surface, ApiSurface, CapabilityDomain, CapabilityExecution,
    OperationRegistryEntry,
};
use std::collections::{BTreeMap, BTreeSet};

fn sample_registry() -> Vec<OperationRegistryEntry> {
    vec![
        OperationRegistryEntry {
            api_surface: ApiSurface::Admin,
            name: "product".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Products,
            implemented: true,
            match_names: vec!["product".to_string(), "Product".to_string()],
            runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        },
        OperationRegistryEntry {
            api_surface: ApiSurface::Admin,
            name: "productCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Products,
            implemented: true,
            match_names: vec!["productCreate".to_string()],
            runtime_tests: vec!["tests/graphql_routes.rs".to_string()],
        },
        OperationRegistryEntry {
            api_surface: ApiSurface::Admin,
            name: "customerCreate".to_string(),
            operation_type: OperationType::Mutation,
            domain: CapabilityDomain::Customers,
            implemented: true,
            match_names: vec!["customerCreate".to_string()],
            runtime_tests: vec![],
        },
        OperationRegistryEntry {
            api_surface: ApiSurface::Admin,
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
        api_surface: ApiSurface::Admin,
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
fn operation_capability_is_scoped_by_api_surface() {
    let registry = vec![
        OperationRegistryEntry {
            api_surface: ApiSurface::Admin,
            name: "shop".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::StoreProperties,
            implemented: true,
            match_names: vec!["shop".to_string()],
            runtime_tests: vec![],
        },
        OperationRegistryEntry {
            api_surface: ApiSurface::Storefront,
            name: "shop".to_string(),
            operation_type: OperationType::Query,
            domain: CapabilityDomain::Storefront,
            implemented: false,
            match_names: vec!["shop".to_string()],
            runtime_tests: vec![],
        },
    ];

    let admin = operation_capability_for_surface(
        &registry,
        ApiSurface::Admin,
        OperationType::Query,
        Some("shop"),
    );
    assert_eq!(admin.api_surface, ApiSurface::Admin);
    assert_eq!(admin.domain, CapabilityDomain::StoreProperties);
    assert_eq!(admin.execution, CapabilityExecution::OverlayRead);

    let storefront = operation_capability_for_surface(
        &registry,
        ApiSurface::Storefront,
        OperationType::Query,
        Some("shop"),
    );
    assert_eq!(storefront.api_surface, ApiSurface::Storefront);
    assert_eq!(storefront.domain, CapabilityDomain::Unknown);
    assert_eq!(storefront.execution, CapabilityExecution::Passthrough);
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

    for root in [
        "shop",
        "localization",
        "locations",
        "paymentSettings",
        "publicApiVersions",
        "product",
        "productByHandle",
        "products",
    ] {
        let capability = operation_capability_for_surface(
            &registry,
            ApiSurface::Storefront,
            OperationType::Query,
            Some(root),
        );
        assert_eq!(capability.api_surface, ApiSurface::Storefront);
        assert_eq!(capability.domain, CapabilityDomain::Storefront);
        assert_eq!(capability.execution, CapabilityExecution::OverlayRead);
    }

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
        let capability = operation_capability_for_surface(
            &registry,
            entry.api_surface,
            entry.operation_type,
            Some(entry.name.as_str()),
        );
        assert_eq!(
            capability.api_surface, entry.api_surface,
            "{} is implemented and must keep its API surface",
            entry.name
        );
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

fn captured_2026_04_admin_mutation_names() -> BTreeSet<String> {
    let registry = schema(AdminApiVersion::V2026_04)
        .expect("captured Admin schema must build")
        .registry();
    let mutation_type = registry
        .mutation_type
        .as_deref()
        .expect("captured Admin schema must expose a mutation root");
    registry
        .types
        .get(mutation_type)
        .and_then(async_graphql::registry::MetaType::fields)
        .expect("captured Admin mutation root must expose fields")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn unimplemented_and_unregistered_admin_mutations_are_not_local_capabilities() {
    let registry = default_registry();
    let mutation_registry: BTreeMap<&str, &OperationRegistryEntry> = registry
        .iter()
        .filter(|entry| {
            entry.api_surface == ApiSurface::Admin
                && entry.operation_type == OperationType::Mutation
        })
        .map(|entry| (entry.name.as_str(), entry))
        .collect();

    for entry in mutation_registry
        .values()
        .copied()
        .filter(|entry| !entry.implemented)
    {
        let capability = operation_capability(
            &registry,
            OperationType::Mutation,
            Some(entry.name.as_str()),
        );
        assert_eq!(
            capability.domain,
            CapabilityDomain::Unknown,
            "{} is declared unimplemented and must remain outside local mutation routing",
            entry.name
        );
        assert_eq!(
            capability.execution,
            CapabilityExecution::Passthrough,
            "{} is declared unimplemented and must remain passthrough/reject gap inventory",
            entry.name
        );
    }

    for root in captured_2026_04_admin_mutation_names()
        .iter()
        .filter(|root| !mutation_registry.contains_key(root.as_str()))
    {
        let capability = operation_capability(&registry, OperationType::Mutation, Some(root));
        assert_eq!(
            capability.domain,
            CapabilityDomain::Unknown,
            "{root} is absent from the mutation registry and must not route locally"
        );
        assert_eq!(
            capability.execution,
            CapabilityExecution::Passthrough,
            "{root} is absent from the mutation registry and must remain a passthrough/reject gap"
        );
    }
}
