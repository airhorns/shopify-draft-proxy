//! Root resolver inventory used by the GraphQL runtime.
//!
//! `ResolverRegistry` is instance-owned by `DraftProxy`. It turns operation
//! metadata into the one lookup used to decide whether a schema root resolves
//! against local state or is eligible for passthrough. Public Storefront roots
//! map to globally unique `storefront*` resolver names, while Admin roots keep
//! their public names. This avoids both cross-surface name collisions and a
//! second routing table beside the exported operation registry.

use std::{collections::BTreeMap, ops::Deref};

use crate::{
    graphql::{OperationType, ParsedOperation, ResolvedValue},
    operation_registry::{
        ApiSurface, CapabilityDomain, CapabilityExecution, OperationCapability,
        OperationRegistryEntry,
    },
    proxy::{DraftProxy, Request, Response},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalResolverMode {
    OverlayRead,
    StageLocally,
}

impl LocalResolverMode {
    pub(crate) fn from_execution(execution: CapabilityExecution) -> Self {
        match execution {
            CapabilityExecution::OverlayRead => Self::OverlayRead,
            CapabilityExecution::StageLocally => Self::StageLocally,
            CapabilityExecution::Passthrough => {
                panic!("passthrough capabilities cannot register local resolvers")
            }
        }
    }

    pub(crate) fn registry_name(self) -> &'static str {
        match self {
            Self::OverlayRead => "overlay-read",
            Self::StageLocally => "stage-locally",
        }
    }
}

pub(crate) struct RootResolverContext<'a> {
    pub request: &'a Request,
    pub query: &'a str,
    pub variables: &'a BTreeMap<String, ResolvedValue>,
    pub operation: &'a ParsedOperation,
    pub root_name: &'a str,
    pub mode: LocalResolverMode,
}

pub(crate) type ResolverHandler = for<'a> fn(&mut DraftProxy, RootResolverContext<'a>) -> Response;

#[derive(Debug, Clone)]
pub struct ResolverRegistration {
    pub api_surface: ApiSurface,
    pub operation_type: OperationType,
    pub graphql_root_name: String,
    pub resolver_name: String,
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
    pub(crate) handler: ResolverHandler,
}

#[derive(Debug, Clone)]
pub struct ResolverRegistry {
    entries: Vec<OperationRegistryEntry>,
    local_resolvers: BTreeMap<String, ResolverRegistration>,
}

impl ResolverRegistry {
    pub fn new(entries: Vec<OperationRegistryEntry>) -> Self {
        let mut local_resolvers = BTreeMap::new();
        for entry in &entries {
            if !entry.implemented {
                continue;
            }
            let resolver_name = entry.api_surface.resolver_name(&entry.name);
            let registration = ResolverRegistration {
                api_surface: entry.api_surface,
                operation_type: entry.operation_type,
                graphql_root_name: entry.name.clone(),
                resolver_name: resolver_name.clone(),
                domain: entry.domain,
                execution: entry.execution(),
                handler: crate::proxy::resolver_handler_for_domain(entry.domain),
            };
            let previous = local_resolvers.insert(resolver_name.clone(), registration);
            assert!(
                previous.is_none(),
                "duplicate internal GraphQL resolver registration for {resolver_name}",
            );
        }
        Self {
            entries,
            local_resolvers,
        }
    }

    pub fn resolve(&self, operation_type: OperationType, root_name: &str) -> OperationCapability {
        self.resolve_for_surface(ApiSurface::Admin, operation_type, root_name)
    }

    pub fn resolve_for_surface(
        &self,
        api_surface: ApiSurface,
        operation_type: OperationType,
        root_name: &str,
    ) -> OperationCapability {
        self.registration_for_surface(api_surface, operation_type, root_name)
            .map(|registration| OperationCapability {
                api_surface: registration.api_surface,
                domain: registration.domain,
                execution: registration.execution,
            })
            .unwrap_or(OperationCapability {
                api_surface,
                domain: CapabilityDomain::Unknown,
                execution: CapabilityExecution::Passthrough,
            })
    }

    pub(crate) fn registration(
        &self,
        operation_type: OperationType,
        root_name: &str,
    ) -> Option<&ResolverRegistration> {
        self.registration_for_surface(ApiSurface::Admin, operation_type, root_name)
    }

    pub(crate) fn registration_for_surface(
        &self,
        api_surface: ApiSurface,
        operation_type: OperationType,
        root_name: &str,
    ) -> Option<&ResolverRegistration> {
        let resolver_name = api_surface.resolver_name(root_name);
        self.local_resolvers
            .get(&resolver_name)
            .filter(|registration| {
                registration.api_surface == api_surface
                    && registration.operation_type == operation_type
                    && registration.graphql_root_name == root_name
            })
    }

    pub fn local_resolvers(&self) -> impl Iterator<Item = &ResolverRegistration> {
        self.local_resolvers.values()
    }

    pub fn entries(&self) -> &[OperationRegistryEntry] {
        &self.entries
    }
}

impl Deref for ResolverRegistry {
    type Target = [OperationRegistryEntry];

    fn deref(&self) -> &Self::Target {
        self.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_registry::default_registry;

    #[test]
    fn local_resolution_is_derived_from_the_exported_inventory() {
        let registry = ResolverRegistry::new(default_registry());
        let product = registry.resolve(OperationType::Query, "product");
        assert_eq!(product.domain, CapabilityDomain::Products);
        assert_eq!(product.execution, CapabilityExecution::OverlayRead);

        let unknown = registry.resolve(OperationType::Mutation, "notImplementedHere");
        assert_eq!(unknown.domain, CapabilityDomain::Unknown);
        assert_eq!(unknown.execution, CapabilityExecution::Passthrough);

        assert_eq!(
            registry.local_resolvers().count(),
            registry
                .entries()
                .iter()
                .filter(|entry| entry.implemented)
                .count()
        );
    }

    #[test]
    fn same_named_admin_and_storefront_roots_resolve_independently() {
        let registry = ResolverRegistry::new(default_registry());
        let admin = registry.resolve_for_surface(ApiSurface::Admin, OperationType::Query, "shop");
        let storefront =
            registry.resolve_for_surface(ApiSurface::Storefront, OperationType::Query, "shop");

        assert_eq!(admin.api_surface, ApiSurface::Admin);
        assert_eq!(admin.domain, CapabilityDomain::StoreProperties);
        assert_eq!(admin.execution, CapabilityExecution::OverlayRead);
        assert_eq!(storefront.api_surface, ApiSurface::Storefront);
        assert_eq!(storefront.domain, CapabilityDomain::Storefront);
        assert_eq!(storefront.execution, CapabilityExecution::OverlayRead);

        let admin_registration = registry
            .registration_for_surface(ApiSurface::Admin, OperationType::Query, "shop")
            .expect("Admin shop should have its own resolver registration");
        let storefront_registration = registry
            .registration_for_surface(ApiSurface::Storefront, OperationType::Query, "shop")
            .expect("Storefront shop should have its own resolver registration");
        assert_eq!(admin_registration.api_surface, ApiSurface::Admin);
        assert_eq!(storefront_registration.api_surface, ApiSurface::Storefront);
        assert_eq!(admin_registration.graphql_root_name, "shop");
        assert_eq!(admin_registration.resolver_name, "shop");
        assert_eq!(storefront_registration.graphql_root_name, "shop");
        assert_eq!(storefront_registration.resolver_name, "storefrontShop");
        assert_eq!(
            ApiSurface::Storefront.resolver_name("products"),
            "storefrontProducts"
        );
        assert!(!std::ptr::fn_addr_eq(
            admin_registration.handler,
            storefront_registration.handler
        ));
    }
}
