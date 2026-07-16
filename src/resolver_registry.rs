//! Root resolver inventory used by the GraphQL runtime.
//!
//! `ResolverRegistry` is instance-owned by `DraftProxy`. It turns operation
//! metadata into the one lookup used to decide whether a schema root resolves
//! against local state or is eligible for passthrough. Public Storefront roots
//! map to globally unique `storefront*` resolver names, while Admin roots keep
//! their public names. This avoids both cross-surface name collisions and a
//! second routing table beside the exported operation registry.

use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{Arc, OnceLock},
};

use crate::{
    admin_graphql::{AdminApiVersion, FieldResolverInvocation, RootFieldError},
    graphql::{OperationType, ParsedOperation, ResolvedValue, SourceLocation},
    operation_registry::{
        ApiSurface, CapabilityDomain, CapabilityExecution, OperationCapability,
        OperationRegistryEntry,
    },
    proxy::{DraftProxy, Request},
    storefront_graphql::StorefrontApiVersion,
};

use serde_json::Value;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphqlApiVersion {
    Admin(AdminApiVersion),
    Storefront(StorefrontApiVersion),
}

impl GraphqlApiVersion {
    pub(crate) fn surface(self) -> ApiSurface {
        match self {
            Self::Admin(_) => ApiSurface::Admin,
            Self::Storefront(_) => ApiSurface::Storefront,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Admin(version) => version.as_str(),
            Self::Storefront(version) => version.as_str(),
        }
    }
}

/// One engine-validated root invocation. Native resolvers receive the values
/// coerced by the selected surface/version schema rather than reparsing raw
/// variable input.
pub(crate) struct RootInvocation<'a> {
    pub api_surface: ApiSurface,
    pub api_version: GraphqlApiVersion,
    pub response_key: &'a str,
    pub root_name: &'a str,
    /// Location and directives from the caller's selected root field. These
    /// remain available for Shopify-compatible execution errors without
    /// exposing the output selection to domain resolvers.
    pub root_location: SourceLocation,
    pub directives: Vec<String>,
    pub arguments: BTreeMap<String, Value>,
    pub request: &'a Request,
    #[allow(dead_code)]
    pub query: &'a str,
    #[allow(dead_code)]
    pub variables: &'a BTreeMap<String, ResolvedValue>,
    pub operation: &'a ParsedOperation,
    pub mode: LocalResolverMode,
}

impl RootInvocation<'_> {
    pub(crate) fn has_directive(&self, name: &str) -> bool {
        self.directives.iter().any(|directive| directive == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MutationLogDraft {
    pub root_field: String,
    pub operation_name: Option<String>,
    pub staged_resource_ids: Vec<String>,
    pub status: String,
    pub capability_domain: String,
    pub capability_execution: String,
    pub notes: String,
}

impl MutationLogDraft {
    pub(crate) fn staged(
        root_field: impl Into<String>,
        domain: &'static str,
        staged_resource_ids: Vec<String>,
    ) -> Self {
        Self {
            root_field: root_field.into(),
            operation_name: None,
            staged_resource_ids,
            status: "staged".to_string(),
            capability_domain: domain.to_string(),
            capability_execution: "stage-locally".to_string(),
            notes: "Supported mutation staged locally; commit replays the original raw mutation."
                .to_string(),
        }
    }

    pub(crate) fn failed(
        root_field: impl Into<String>,
        domain: &'static str,
        notes: impl Into<String>,
    ) -> Self {
        Self {
            root_field: root_field.into(),
            operation_name: None,
            staged_resource_ids: Vec::new(),
            status: "failed".to_string(),
            capability_domain: domain.to_string(),
            capability_execution: "stage-locally".to_string(),
            notes: notes.into(),
        }
    }

    pub(crate) fn with_operation_name(mut self, operation_name: impl Into<String>) -> Self {
        self.operation_name = Some(operation_name.into());
        self
    }
}

/// Domain result before the GraphQL engine applies field projection and null
/// propagation. Transport status and headers deliberately do not belong here.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolverOutcome<T = Value> {
    pub value: T,
    pub errors: Vec<RootFieldError>,
    pub extensions: BTreeMap<String, Value>,
    pub log_drafts: Vec<MutationLogDraft>,
}

impl<T> ResolverOutcome<T> {
    pub(crate) fn value(value: T) -> Self {
        Self {
            value,
            errors: Vec::new(),
            extensions: BTreeMap::new(),
            log_drafts: Vec::new(),
        }
    }

    pub(crate) fn with_log_draft(mut self, draft: MutationLogDraft) -> Self {
        self.log_drafts.push(draft);
        self
    }

    pub(crate) fn with_errors(mut self, errors: Vec<RootFieldError>) -> Self {
        self.errors = errors;
        self
    }
}

impl ResolverOutcome<Value> {
    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self {
            value: Value::Null,
            errors: vec![RootFieldError {
                message: message.into(),
                extensions: BTreeMap::new(),
                path: Some(Vec::new()),
                locations: Vec::new(),
            }],
            extensions: BTreeMap::new(),
            log_drafts: Vec::new(),
        }
    }
}

pub(crate) type NativeResolverHandler =
    for<'a> fn(&mut DraftProxy, RootInvocation<'a>) -> ResolverOutcome<Value>;

#[derive(Debug, Clone)]
pub(crate) struct ExecutableRootRegistration {
    pub entry: OperationRegistryEntry,
    pub handler: Option<NativeResolverHandler>,
}

pub(crate) type FieldResolverHandler =
    for<'a> fn(&mut DraftProxy, &Request, &FieldResolverInvocation<'a>) -> Result<Value, String>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FieldCoordinate {
    pub api_surface: ApiSurface,
    /// `None` means this implementation is shared by every executable version
    /// on the surface. Exact registrations take precedence at lookup time.
    pub api_version: Option<String>,
    pub parent_type: String,
    pub field_name: String,
}

#[derive(Clone, Copy)]
pub(crate) enum FieldResolverImplementation {
    PropertyBacked,
    /// Argument-bearing, calculated, connection, and cross-domain fields are
    /// owned by one authoritative callback. Canonical parents must not bypass
    /// that callback by materializing a selection-shaped copy of the field.
    Explicit(FieldResolverHandler),
    #[allow(dead_code)]
    DeliberatelyUnsupported(&'static str),
}

impl std::fmt::Debug for FieldResolverImplementation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PropertyBacked => formatter.write_str("PropertyBacked"),
            Self::Explicit(_) => formatter.write_str("ExplicitFieldResolver"),
            Self::DeliberatelyUnsupported(reason) => formatter
                .debug_tuple("DeliberatelyUnsupported")
                .field(reason)
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FieldResolverRegistration {
    pub coordinate: FieldCoordinate,
    pub implementation: FieldResolverImplementation,
    pub provenance: FieldResolverProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldResolverProvenance {
    DeclaredProperty,
    ExplicitResolver,
    LegacySelectionProjector,
    TypeUnsupportedPolicy,
    InferredOrdinaryProperty,
    InferredUnsupportedArgumentField,
}

/// Measurable ownership/debt inventory for the executable field catalog.
/// Shared registrations are counted once; version-inferred decisions are
/// counted once per concrete surface/version coordinate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FieldResolverAudit {
    pub declared_properties: usize,
    pub explicit_resolvers: usize,
    pub legacy_selection_projectors: usize,
    pub type_unsupported_policies: usize,
    pub inferred_ordinary_properties: usize,
    pub inferred_unsupported_argument_fields: usize,
}

impl FieldResolverAudit {
    pub fn total(self) -> usize {
        self.declared_properties
            + self.explicit_resolvers
            + self.legacy_selection_projectors
            + self.type_unsupported_policies
            + self.inferred_ordinary_properties
            + self.inferred_unsupported_argument_fields
    }
}

impl FieldResolverRegistration {
    pub(crate) fn property(api_surface: ApiSurface, parent_type: &str, field_name: &str) -> Self {
        Self {
            coordinate: FieldCoordinate {
                api_surface,
                api_version: None,
                parent_type: parent_type.to_string(),
                field_name: field_name.to_string(),
            },
            implementation: FieldResolverImplementation::PropertyBacked,
            provenance: FieldResolverProvenance::DeclaredProperty,
        }
    }

    pub(crate) fn legacy_projected_property(
        api_surface: ApiSurface,
        parent_type: &str,
        field_name: &str,
    ) -> Self {
        Self {
            coordinate: FieldCoordinate {
                api_surface,
                api_version: None,
                parent_type: parent_type.to_string(),
                field_name: field_name.to_string(),
            },
            implementation: FieldResolverImplementation::PropertyBacked,
            provenance: FieldResolverProvenance::LegacySelectionProjector,
        }
    }

    pub(crate) fn explicit(
        api_surface: ApiSurface,
        parent_type: &str,
        field_name: &str,
        handler: FieldResolverHandler,
    ) -> Self {
        Self {
            coordinate: FieldCoordinate {
                api_surface,
                api_version: None,
                parent_type: parent_type.to_string(),
                field_name: field_name.to_string(),
            },
            implementation: FieldResolverImplementation::Explicit(handler),
            provenance: FieldResolverProvenance::ExplicitResolver,
        }
    }

    fn unsupported(
        api_surface: ApiSurface,
        api_version: &str,
        parent_type: &str,
        field_name: &str,
        reason: &'static str,
    ) -> Self {
        Self {
            coordinate: FieldCoordinate {
                api_surface,
                api_version: Some(api_version.to_string()),
                parent_type: parent_type.to_string(),
                field_name: field_name.to_string(),
            },
            implementation: FieldResolverImplementation::DeliberatelyUnsupported(reason),
            provenance: FieldResolverProvenance::TypeUnsupportedPolicy,
        }
    }
}

/// Explicit policy for a canonical type whose remaining captured fields are
/// unsupported. This keeps per-field decisions auditable without forcing
/// domains to hand-copy every field from every captured schema version.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldResolverTypePolicy {
    pub api_surface: ApiSurface,
    pub parent_type: &'static str,
    pub unsupported_reason: &'static str,
}

impl FieldResolverTypePolicy {
    pub(crate) fn unsupported_remaining(
        api_surface: ApiSurface,
        parent_type: &'static str,
        unsupported_reason: &'static str,
    ) -> Self {
        Self {
            api_surface,
            parent_type,
            unsupported_reason,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolverRegistration {
    pub api_surface: ApiSurface,
    pub operation_type: OperationType,
    pub graphql_root_name: String,
    pub resolver_name: String,
    pub domain: CapabilityDomain,
    pub execution: CapabilityExecution,
    pub(crate) handler: NativeResolverHandler,
}

#[derive(Debug, Clone)]
pub struct ResolverRegistry {
    entries: Vec<OperationRegistryEntry>,
    local_resolvers: BTreeMap<String, ResolverRegistration>,
    field_resolvers: Arc<BTreeMap<FieldCoordinate, FieldResolverRegistration>>,
}

impl ResolverRegistry {
    pub fn new(entries: Vec<OperationRegistryEntry>) -> Self {
        let executable = crate::operation_registry::default_executable_registry()
            .into_iter()
            .filter_map(|registration| {
                assert_eq!(
                    registration.entry.implemented,
                    registration.handler.is_some(),
                    "GraphQL root {}.{} must derive implemented state from callback presence",
                    registration.entry.api_surface.registry_name(),
                    registration.entry.name,
                );
                let handler = registration.handler?;
                Some((
                    (
                        registration.entry.api_surface,
                        registration.entry.operation_type,
                        registration.entry.name.clone(),
                    ),
                    (registration.entry, handler),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let mut local_resolvers = BTreeMap::new();
        for entry in &entries {
            if !entry.implemented {
                continue;
            }
            let resolver_name = entry.api_surface.resolver_name(&entry.name);
            let binding = executable
                .get(&(entry.api_surface, entry.operation_type, entry.name.clone()))
                .unwrap_or_else(|| {
                    panic!(
                        "implemented GraphQL root {}.{} has no direct resolver registration",
                        entry.api_surface.registry_name(),
                        entry.name,
                    )
                });
            assert_eq!(
                binding.0.domain,
                entry.domain,
                "direct resolver registration for {}.{} has a different capability domain",
                entry.api_surface.registry_name(),
                entry.name,
            );
            let registration = ResolverRegistration {
                api_surface: entry.api_surface,
                operation_type: entry.operation_type,
                graphql_root_name: entry.name.clone(),
                resolver_name: resolver_name.clone(),
                domain: entry.domain,
                execution: entry.execution(),
                handler: binding.1,
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
            field_resolvers: default_field_resolver_catalog(),
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

    pub(crate) fn field_registration(
        &self,
        api_surface: ApiSurface,
        api_version: &str,
        parent_type: &str,
        field_name: &str,
    ) -> Option<&FieldResolverRegistration> {
        let exact = FieldCoordinate {
            api_surface,
            api_version: Some(api_version.to_string()),
            parent_type: parent_type.to_string(),
            field_name: field_name.to_string(),
        };
        self.field_resolvers.get(&exact).or_else(|| {
            self.field_resolvers.get(&FieldCoordinate {
                api_version: None,
                ..exact
            })
        })
    }

    pub(crate) fn field_implementation(
        &self,
        api_surface: ApiSurface,
        api_version: &str,
        parent_type: &str,
        field_name: &str,
    ) -> FieldResolverImplementation {
        self.field_registration(api_surface, api_version, parent_type, field_name)
            .map(|registration| registration.implementation)
            .unwrap_or(FieldResolverImplementation::DeliberatelyUnsupported(
                "the field is not present in this surface/version's captured executable schema",
            ))
    }

    pub fn field_resolver_audit(&self) -> FieldResolverAudit {
        let mut audit = FieldResolverAudit::default();
        for registration in self.field_resolvers.values() {
            match registration.provenance {
                FieldResolverProvenance::DeclaredProperty => audit.declared_properties += 1,
                FieldResolverProvenance::ExplicitResolver => audit.explicit_resolvers += 1,
                FieldResolverProvenance::LegacySelectionProjector => {
                    audit.legacy_selection_projectors += 1;
                }
                FieldResolverProvenance::TypeUnsupportedPolicy => {
                    audit.type_unsupported_policies += 1;
                }
                FieldResolverProvenance::InferredOrdinaryProperty => {
                    audit.inferred_ordinary_properties += 1;
                }
                FieldResolverProvenance::InferredUnsupportedArgumentField => {
                    audit.inferred_unsupported_argument_fields += 1;
                }
            }
        }
        audit
    }
}

fn default_field_resolver_catalog() -> Arc<BTreeMap<FieldCoordinate, FieldResolverRegistration>> {
    static CATALOG: OnceLock<Arc<BTreeMap<FieldCoordinate, FieldResolverRegistration>>> =
        OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| Arc::new(build_field_resolver_catalog())))
}

fn build_field_resolver_catalog() -> BTreeMap<FieldCoordinate, FieldResolverRegistration> {
    let mut field_resolvers = BTreeMap::new();
    for registration in crate::proxy::field_resolver_registrations() {
        let coordinate = registration.coordinate.clone();
        let previous = field_resolvers.insert(coordinate.clone(), registration);
        assert!(
            previous.is_none(),
            "duplicate GraphQL field resolver registration for {}.{} ({})",
            coordinate.parent_type,
            coordinate.field_name,
            coordinate.api_surface.registry_name(),
        );
    }

    for policy in crate::proxy::field_resolver_type_policies() {
        let mut saw_type = false;
        match policy.api_surface {
            ApiSurface::Admin => {
                for version in AdminApiVersion::ALL {
                    let schema = crate::admin_graphql::schema(version).unwrap_or_else(|error| {
                        panic!(
                            "could not classify {} fields for {version}: {error}",
                            policy.parent_type
                        )
                    });
                    saw_type |= register_unsupported_type_fields(
                        &mut field_resolvers,
                        policy,
                        version.as_str(),
                        schema,
                    );
                }
            }
            ApiSurface::Storefront => {
                for version in StorefrontApiVersion::ALL {
                    let schema =
                        crate::storefront_graphql::schema(version).unwrap_or_else(|error| {
                            panic!(
                                "could not classify {} fields for {version}: {error}",
                                policy.parent_type
                            )
                        });
                    saw_type |= register_unsupported_type_fields(
                        &mut field_resolvers,
                        policy,
                        version.as_str(),
                        schema,
                    );
                }
            }
        }
        assert!(
            saw_type,
            "canonical GraphQL type {} does not exist on the {} schemas",
            policy.parent_type,
            policy.api_surface.registry_name(),
        );
    }

    for version in AdminApiVersion::ALL {
        let schema = crate::admin_graphql::schema(version)
            .unwrap_or_else(|error| panic!("could not catalog Admin {version} fields: {error}"));
        register_captured_schema_field_decisions(
            &mut field_resolvers,
            ApiSurface::Admin,
            version.as_str(),
            schema,
        );
    }
    for version in StorefrontApiVersion::ALL {
        let schema = crate::storefront_graphql::schema(version).unwrap_or_else(|error| {
            panic!("could not catalog Storefront {version} fields: {error}")
        });
        register_captured_schema_field_decisions(
            &mut field_resolvers,
            ApiSurface::Storefront,
            version.as_str(),
            schema,
        );
    }

    field_resolvers
}

fn register_unsupported_type_fields(
    field_resolvers: &mut BTreeMap<FieldCoordinate, FieldResolverRegistration>,
    policy: FieldResolverTypePolicy,
    version: &str,
    schema: &async_graphql::dynamic::Schema,
) -> bool {
    let Some(fields) = schema
        .registry()
        .types
        .get(policy.parent_type)
        .and_then(|schema_type| schema_type.fields())
    else {
        return false;
    };
    for field_name in fields.keys() {
        let coordinate = FieldCoordinate {
            api_surface: policy.api_surface,
            api_version: Some(version.to_string()),
            parent_type: policy.parent_type.to_string(),
            field_name: field_name.clone(),
        };
        let shared_coordinate = FieldCoordinate {
            api_version: None,
            ..coordinate.clone()
        };
        if !field_resolvers.contains_key(&shared_coordinate) {
            field_resolvers.entry(coordinate).or_insert_with(|| {
                FieldResolverRegistration::unsupported(
                    policy.api_surface,
                    version,
                    policy.parent_type,
                    field_name,
                    policy.unsupported_reason,
                )
            });
        }
    }
    true
}

fn register_captured_schema_field_decisions(
    field_resolvers: &mut BTreeMap<FieldCoordinate, FieldResolverRegistration>,
    api_surface: ApiSurface,
    version: &str,
    schema: &async_graphql::dynamic::Schema,
) {
    const UNREGISTERED_ARGUMENT_FIELD_REASON: &str =
        "argument-bearing field has no explicit local resolver";
    for (parent_type, schema_type) in &schema.registry().types {
        let Some(fields) = schema_type.fields() else {
            continue;
        };
        for (field_name, field) in fields {
            let coordinate = FieldCoordinate {
                api_surface,
                api_version: Some(version.to_string()),
                parent_type: parent_type.clone(),
                field_name: field_name.clone(),
            };
            let shared_coordinate = FieldCoordinate {
                api_version: None,
                ..coordinate.clone()
            };
            if field_resolvers.contains_key(&shared_coordinate)
                || field_resolvers.contains_key(&coordinate)
            {
                continue;
            }
            field_resolvers.insert(
                coordinate.clone(),
                FieldResolverRegistration {
                    coordinate,
                    implementation: if field.args.is_empty() {
                        FieldResolverImplementation::PropertyBacked
                    } else {
                        FieldResolverImplementation::DeliberatelyUnsupported(
                            UNREGISTERED_ARGUMENT_FIELD_REASON,
                        )
                    },
                    provenance: if field.args.is_empty() {
                        FieldResolverProvenance::InferredOrdinaryProperty
                    } else {
                        FieldResolverProvenance::InferredUnsupportedArgumentField
                    },
                },
            );
        }
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
    use crate::{admin_graphql, operation_registry::default_registry, storefront_graphql};

    fn assert_type_fields_classified(
        registry: &ResolverRegistry,
        surface: ApiSurface,
        version: &str,
        schema: &async_graphql::dynamic::Schema,
        type_name: &str,
    ) {
        let schema_type = schema
            .registry()
            .types
            .get(type_name)
            .unwrap_or_else(|| panic!("{type_name} should exist on the captured schema"));
        let fields = schema_type
            .fields()
            .unwrap_or_else(|| panic!("{type_name} should expose output fields"));
        let missing = fields
            .keys()
            .filter(|field_name| {
                registry
                    .field_registration(surface, version, type_name, field_name)
                    .is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "{}.{type_name} has unclassified fields: {}",
            surface.registry_name(),
            missing.join(", ")
        );
    }

    fn root_named_type(
        schema: &async_graphql::dynamic::Schema,
        operation_type: OperationType,
        root_name: &str,
    ) -> Option<String> {
        let type_name = match operation_type {
            OperationType::Query => Some(schema.registry().query_type.as_str()),
            OperationType::Mutation => schema.registry().mutation_type.as_deref(),
            OperationType::Subscription => schema.registry().subscription_type.as_deref(),
        }?;
        let field = schema
            .registry()
            .types
            .get(type_name)?
            .field_by_name(root_name)?;
        named_type(&field.ty)
    }

    fn named_type(type_ref: &str) -> Option<String> {
        type_ref
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .find(|segment| !segment.is_empty())
            .map(str::to_string)
    }

    fn assert_reachable_fields_classified(
        registry: &ResolverRegistry,
        surface: ApiSurface,
        version: &str,
        schema: &async_graphql::dynamic::Schema,
    ) {
        use async_graphql::registry::MetaType;

        let mut pending = registry
            .local_resolvers()
            .filter(|registration| registration.api_surface == surface)
            .filter_map(|registration| {
                root_named_type(
                    schema,
                    registration.operation_type,
                    &registration.graphql_root_name,
                )
            })
            .collect::<Vec<_>>();
        let mut visited = std::collections::BTreeSet::new();
        let mut classified = 0usize;
        while let Some(type_name) = pending.pop() {
            if !visited.insert(type_name.clone()) {
                continue;
            }
            let Some(meta_type) = schema.registry().types.get(&type_name) else {
                continue;
            };
            match meta_type {
                MetaType::Object { fields, .. } => {
                    for (field_name, field) in fields {
                        assert!(
                            registry
                                .field_registration(surface, version, &type_name, field_name)
                                .is_some(),
                            "{} {version} {type_name}.{field_name} should be classified",
                            surface.registry_name(),
                        );
                        classified += 1;
                        if let Some(child) = named_type(&field.ty) {
                            pending.push(child);
                        }
                    }
                }
                MetaType::Interface {
                    fields,
                    possible_types,
                    ..
                } => {
                    for (field_name, field) in fields {
                        assert!(
                            registry
                                .field_registration(surface, version, &type_name, field_name)
                                .is_some(),
                            "{} {version} {type_name}.{field_name} should be classified",
                            surface.registry_name(),
                        );
                        classified += 1;
                        if let Some(child) = named_type(&field.ty) {
                            pending.push(child);
                        }
                    }
                    pending.extend(possible_types.iter().cloned());
                }
                MetaType::Union { possible_types, .. } => {
                    pending.extend(possible_types.iter().cloned());
                }
                MetaType::Scalar { .. } | MetaType::Enum { .. } | MetaType::InputObject { .. } => {}
            }
        }
        assert!(
            classified > 0,
            "{} {version} should expose locally reachable fields",
            surface.registry_name(),
        );
    }

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
    fn field_resolver_debt_is_explicit_and_cannot_grow_silently() {
        let registry = ResolverRegistry::new(default_registry());
        let audit = registry.field_resolver_audit();
        assert_eq!(audit.total(), registry.field_resolvers.len());
        assert!(audit.explicit_resolvers > 0);
        assert!(audit.inferred_unsupported_argument_fields > 0);

        // Reduce this number whenever a canonical parent plus explicit field
        // resolver replaces a SelectedField projector. Raising it requires an
        // intentional review of newly introduced compatibility debt.
        assert_eq!(audit.legacy_selection_projectors, 98);
    }

    #[test]
    fn inferred_argument_fields_are_never_silently_property_backed() {
        let registry = ResolverRegistry::new(default_registry());
        let inferred = registry
            .field_resolvers
            .values()
            .filter(|registration| {
                registration.provenance == FieldResolverProvenance::InferredUnsupportedArgumentField
            })
            .collect::<Vec<_>>();
        assert!(!inferred.is_empty());
        for registration in inferred {
            assert!(matches!(
                registration.implementation,
                FieldResolverImplementation::DeliberatelyUnsupported(_)
            ));
        }
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
        let admin_collection = registry
            .registration_for_surface(ApiSurface::Admin, OperationType::Query, "collection")
            .expect("Admin collection should keep its domain resolver");
        let storefront_collection = registry
            .registration_for_surface(ApiSurface::Storefront, OperationType::Query, "collection")
            .expect("Storefront collection should have a surface-qualified resolver");
        assert_eq!(admin_collection.resolver_name, "collection");
        assert_eq!(storefront_collection.resolver_name, "storefrontCollection");
        assert_eq!(admin_collection.domain, CapabilityDomain::StoreProperties);
        assert_eq!(storefront_collection.domain, CapabilityDomain::Storefront);
        assert!(!std::ptr::fn_addr_eq(
            admin_collection.handler,
            storefront_collection.handler
        ));
        assert!(!std::ptr::fn_addr_eq(
            admin_registration.handler,
            storefront_registration.handler
        ));
    }

    #[test]
    fn saved_search_fields_have_surface_qualified_property_and_explicit_resolvers() {
        let registry = ResolverRegistry::new(default_registry());
        let id = registry
            .field_registration(ApiSurface::Admin, "2026-07", "SavedSearch", "id")
            .expect("SavedSearch.id should be classified");
        assert!(matches!(
            id.implementation,
            FieldResolverImplementation::PropertyBacked
        ));

        let filters = registry
            .field_registration(ApiSurface::Admin, "2026-07", "SavedSearch", "filters")
            .expect("SavedSearch.filters should be classified");
        assert!(matches!(
            filters.implementation,
            FieldResolverImplementation::Explicit(_)
        ));
        assert!(registry
            .field_registration(ApiSurface::Storefront, "2026-04", "SavedSearch", "filters")
            .is_none());

        for (parent_type, field_name) in [
            ("DeliveryPromiseParticipant", "owner"),
            ("StoreCreditAccount", "transactions"),
        ] {
            let registration = registry
                .field_registration(ApiSurface::Admin, "2026-07", parent_type, field_name)
                .unwrap_or_else(|| panic!("{parent_type}.{field_name} should be classified"));
            assert!(matches!(
                registration.implementation,
                FieldResolverImplementation::Explicit(_)
            ));
        }
    }

    #[test]
    fn every_captured_and_locally_reachable_field_is_explicitly_classified() {
        let registry = ResolverRegistry::new(default_registry());
        for version in AdminApiVersion::ALL {
            let schema = admin_graphql::schema(version)
                .unwrap_or_else(|error| panic!("{version} schema should build: {error}"));
            for (type_name, schema_type) in &schema.registry().types {
                if schema_type.fields().is_none() {
                    continue;
                }
                assert_type_fields_classified(
                    &registry,
                    ApiSurface::Admin,
                    version.as_str(),
                    schema,
                    type_name,
                );
            }
            assert_reachable_fields_classified(
                &registry,
                ApiSurface::Admin,
                version.as_str(),
                schema,
            );
        }
        for version in StorefrontApiVersion::ALL {
            let schema = storefront_graphql::schema(version)
                .unwrap_or_else(|error| panic!("{version} schema should build: {error}"));
            for (type_name, schema_type) in &schema.registry().types {
                if schema_type.fields().is_none() {
                    continue;
                }
                assert_type_fields_classified(
                    &registry,
                    ApiSurface::Storefront,
                    version.as_str(),
                    schema,
                    type_name,
                );
            }
            assert_reachable_fields_classified(
                &registry,
                ApiSurface::Storefront,
                version.as_str(),
                schema,
            );
        }

        assert!(matches!(
            registry.field_implementation(
                ApiSurface::Storefront,
                "2026-04",
                "Shop",
                "notInTheCapturedSchema",
            ),
            FieldResolverImplementation::DeliberatelyUnsupported(_)
        ));
    }
}
