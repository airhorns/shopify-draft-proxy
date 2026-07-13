//! Root resolver inventory used by the GraphQL runtime.
//!
//! `ResolverRegistry` is instance-owned by `DraftProxy`. It turns operation
//! metadata into the one lookup used to decide whether a schema root resolves
//! against local state or is eligible for passthrough. This avoids maintaining
//! a second routing table beside the exported operation registry.

use std::{collections::BTreeMap, ops::Deref};

use crate::{
    graphql::OperationType,
    operation_registry::{
        CapabilityDomain, CapabilityExecution, OperationCapability, OperationRegistryEntry,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolverRegistration {
    pub operation_type: OperationType,
    pub root_name: String,
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
}

#[derive(Debug, Clone)]
pub struct ResolverRegistry {
    entries: Vec<OperationRegistryEntry>,
    local_roots: BTreeMap<(OperationType, String), ResolverRegistration>,
}

impl ResolverRegistry {
    pub fn new(entries: Vec<OperationRegistryEntry>) -> Self {
        let mut local_roots = BTreeMap::new();
        for entry in &entries {
            if !entry.implemented {
                continue;
            }
            let key = (entry.operation_type, entry.name.clone());
            let registration = ResolverRegistration {
                operation_type: entry.operation_type,
                root_name: entry.name.clone(),
                domain: entry.domain,
                execution: entry.execution(),
            };
            let previous = local_roots.insert(key, registration);
            assert!(
                previous.is_none(),
                "duplicate local GraphQL resolver registration for {}",
                entry.name
            );
        }
        Self {
            entries,
            local_roots,
        }
    }

    pub fn resolve(&self, operation_type: OperationType, root_name: &str) -> OperationCapability {
        self.local_roots
            .get(&(operation_type, root_name.to_string()))
            .map(|registration| OperationCapability {
                domain: registration.domain,
                execution: registration.execution,
            })
            .unwrap_or(OperationCapability {
                domain: CapabilityDomain::Unknown,
                execution: CapabilityExecution::Passthrough,
            })
    }

    pub fn local_resolvers(&self) -> impl Iterator<Item = &ResolverRegistration> {
        self.local_roots.values()
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
}
