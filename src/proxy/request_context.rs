use super::*;
use crate::operation_registry::OperationCapability;

/// Stable identity for one request-scoped result or fact.
///
/// Domains own the namespace and components. Keeping components structured
/// avoids delimiter-sensitive string encodings while the request cache owns
/// deduplication and lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::proxy) struct RequestCacheKey {
    scope: &'static str,
    components: Vec<String>,
}

impl RequestCacheKey {
    pub(in crate::proxy) fn new(scope: &'static str, identity: impl Into<String>) -> Self {
        Self {
            scope,
            components: vec![identity.into()],
        }
    }

    pub(in crate::proxy) fn singleton(scope: &'static str) -> Self {
        Self {
            scope,
            components: Vec::new(),
        }
    }

    pub(in crate::proxy) fn composite(scope: &'static str, components: &[&str]) -> Self {
        Self {
            scope,
            components: components
                .iter()
                .map(|component| (*component).to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::proxy) enum EntityEvidenceState {
    Requested,
    Observed,
    Missing,
}

/// Upstream values and facts learned during one GraphQL request.
///
/// The caller response stays alias-shaped for transport fidelity, while
/// `caller_data` is canonicalized once for domain observation and overlays.
#[derive(Clone, Default)]
pub(in crate::proxy) struct RequestCache {
    caller_response: Option<Response>,
    caller_data: Option<Value>,
    caller_selections: BTreeMap<String, Vec<SelectedField>>,
    supplemental_responses: BTreeMap<RequestCacheKey, Response>,
    completed: BTreeSet<RequestCacheKey>,
    entities: BTreeMap<RequestCacheKey, EntityEvidenceState>,
}

impl RequestCache {
    pub(in crate::proxy) fn caller_response(&self) -> Option<&Response> {
        self.caller_response.as_ref()
    }

    pub(in crate::proxy) fn caller_data(&self) -> Option<&Value> {
        self.caller_data.as_ref()
    }

    pub(in crate::proxy) fn set_caller_selections(
        &mut self,
        selections: BTreeMap<String, Vec<SelectedField>>,
    ) {
        self.caller_selections = selections;
    }

    pub(in crate::proxy) fn caller_selections(&self) -> &BTreeMap<String, Vec<SelectedField>> {
        &self.caller_selections
    }

    pub(in crate::proxy) fn record_caller_response(
        &mut self,
        response: Response,
        canonical_data: Value,
    ) {
        self.caller_response = Some(response);
        self.caller_data = Some(canonical_data);
    }

    pub(in crate::proxy) fn supplemental_response(
        &self,
        key: &RequestCacheKey,
    ) -> Option<&Response> {
        self.supplemental_responses.get(key)
    }

    pub(in crate::proxy) fn record_supplemental_response(
        &mut self,
        key: RequestCacheKey,
        response: Response,
    ) {
        self.supplemental_responses.insert(key, response);
    }

    pub(in crate::proxy) fn is_complete(&self, key: &RequestCacheKey) -> bool {
        self.completed.contains(key)
    }

    pub(in crate::proxy) fn mark_complete(&mut self, key: RequestCacheKey) {
        self.completed.insert(key);
    }

    pub(in crate::proxy) fn mark_entity(
        &mut self,
        scope: &'static str,
        id: impl Into<String>,
        state: EntityEvidenceState,
    ) {
        self.mark_entity_key(RequestCacheKey::new(scope, id), state);
    }

    pub(in crate::proxy) fn mark_entity_key(
        &mut self,
        key: RequestCacheKey,
        state: EntityEvidenceState,
    ) {
        self.entities
            .entry(key)
            .and_modify(|existing| {
                // Rediscovering a requested entity must not downgrade stronger
                // positive or negative evidence from an earlier hydration.
                if state != EntityEvidenceState::Requested
                    || *existing == EntityEvidenceState::Requested
                {
                    *existing = state;
                }
            })
            .or_insert(state);
    }

    pub(in crate::proxy) fn entity_state(
        &self,
        scope: &'static str,
        id: &str,
    ) -> Option<EntityEvidenceState> {
        self.entity_state_for_key(&RequestCacheKey::new(scope, id))
    }

    pub(in crate::proxy) fn entity_state_for_key(
        &self,
        key: &RequestCacheKey,
    ) -> Option<EntityEvidenceState> {
        self.entities.get(key).copied()
    }

    pub(in crate::proxy) fn entity_was_requested(&self, scope: &'static str, id: &str) -> bool {
        self.entity_state(scope, id).is_some()
    }

    pub(in crate::proxy) fn entity_was_hydrated(&self, scope: &'static str, id: &str) -> bool {
        matches!(
            self.entity_state(scope, id),
            Some(EntityEvidenceState::Observed | EntityEvidenceState::Missing)
        )
    }

    pub(in crate::proxy) fn entity_is_missing(&self, scope: &'static str, id: &str) -> bool {
        self.entity_state(scope, id) == Some(EntityEvidenceState::Missing)
    }
}

/// Complete, selection-aware information needed by cross-root request policy.
/// Domain methods inspect this context directly; there is no second planner
/// registration graph alongside the resolver registry.
pub(in crate::proxy) struct AdminOperationContext<'a> {
    pub request: &'a Request,
    pub operation_type: OperationType,
    pub roots: &'a [RootFieldSelection],
    pub variables: &'a BTreeMap<String, ResolvedValue>,
    capabilities: Vec<OperationCapability>,
}

impl AdminOperationContext<'_> {
    pub(in crate::proxy) fn has_domain(&self, domain: CapabilityDomain) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability.domain == domain)
    }

    pub(in crate::proxy) fn all_domains(
        &self,
        predicate: impl Fn(CapabilityDomain) -> bool,
    ) -> bool {
        !self.capabilities.is_empty()
            && self
                .capabilities
                .iter()
                .all(|capability| predicate(capability.domain))
    }

    pub(in crate::proxy) fn has_local_root(&self) -> bool {
        self.capabilities.iter().any(|capability| {
            capability.domain != CapabilityDomain::Unknown
                && matches!(
                    capability.execution,
                    CapabilityExecution::OverlayRead | CapabilityExecution::StageLocally
                )
        })
    }

    fn has_passthrough_root(&self) -> bool {
        self.capabilities.iter().any(|capability| {
            capability.domain == CapabilityDomain::Unknown
                || capability.execution == CapabilityExecution::Passthrough
        })
    }
}

#[derive(Clone, Copy, Default)]
pub(in crate::proxy) struct AdminRequestDisposition {
    pub has_local_root: bool,
    pub has_passthrough_root: bool,
    pub direct_full_query_passthrough: bool,
    pub observe_upstream_shop: bool,
}

impl DraftProxy {
    pub(in crate::proxy) fn admin_operation_context<'a>(
        &self,
        request: &'a Request,
        operation_type: OperationType,
        roots: &'a [RootFieldSelection],
        variables: &'a BTreeMap<String, ResolvedValue>,
    ) -> AdminOperationContext<'a> {
        AdminOperationContext {
            request,
            operation_type,
            roots,
            variables,
            capabilities: roots
                .iter()
                .map(|root| self.registry.resolve(operation_type, &root.name))
                .collect(),
        }
    }

    pub(in crate::proxy) fn admin_request_disposition(
        &self,
        context: &AdminOperationContext<'_>,
    ) -> AdminRequestDisposition {
        let has_local_root = context.has_local_root();
        let has_passthrough_root = context.has_passthrough_root();
        let shop_passthrough = self.shop_query_is_upstream_authoritative(context);
        let direct_full_query_passthrough = self.product_query_is_upstream_authoritative(context)
            || shop_passthrough
            || self.events_query_is_upstream_authoritative(context)
            || self.delivery_settings_query_is_upstream_authoritative(context)
            || self.admin_platform_query_is_upstream_authoritative(context);
        AdminRequestDisposition {
            has_local_root,
            has_passthrough_root,
            direct_full_query_passthrough,
            observe_upstream_shop: shop_passthrough,
        }
    }

    /// Run the few cross-root preparations that genuinely need the complete
    /// selected operation. Ordinary hydration remains inside its domain root.
    pub(in crate::proxy) fn prepare_admin_operation(
        &mut self,
        context: &AdminOperationContext<'_>,
    ) {
        self.prepare_owner_metafield_read(context);
        self.prepare_localization_markets_query(context);
        self.prepare_node_query(context);
    }

    /// Execute one supplemental GraphQL read at most once during the active
    /// request. A failed response is cached too, so sibling resolvers do not
    /// independently retry the same evidence request.
    pub(in crate::proxy) fn request_hydration_post_once(
        &mut self,
        key: RequestCacheKey,
        request: &Request,
        body: Value,
    ) -> Response {
        if let Some(response) = self
            .execution_session
            .request_cache
            .supplemental_response(&key)
        {
            return response.clone();
        }
        let response = self.upstream_post(request, body);
        self.execution_session
            .request_cache
            .record_supplemental_response(key, response.clone());
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(data: Value) -> Response {
        Response {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({ "data": data }),
        }
    }

    fn disposition(query: &str, read_mode: ReadMode) -> AdminRequestDisposition {
        let variables = BTreeMap::new();
        let document = parsed_document(query, &variables).expect("query should parse");
        let proxy = DraftProxy::new(Config {
            read_mode,
            ..Config::default()
        });
        let request = Request::default();
        let context = proxy.admin_operation_context(
            &request,
            document.operation_type,
            &document.root_fields,
            &variables,
        );
        proxy.admin_request_disposition(&context)
    }

    #[test]
    fn caller_transport_and_canonical_values_remain_separate() {
        let mut cache = RequestCache::default();
        cache.record_caller_response(
            response(json!({ "aliasedProduct": { "aliasedTitle": "Bag" } })),
            json!({ "aliasedProduct": { "title": "Bag" } }),
        );

        assert_eq!(
            cache
                .caller_response()
                .and_then(|response| response.body.pointer("/data/aliasedProduct/aliasedTitle")),
            Some(&json!("Bag"))
        );
        assert_eq!(
            cache
                .caller_data()
                .and_then(|data| data.pointer("/aliasedProduct/title")),
            Some(&json!("Bag"))
        );
    }

    #[test]
    fn cache_keys_and_entity_evidence_preserve_their_invariants() {
        assert_ne!(
            RequestCacheKey::composite("owner-metafield", &["owner:a", "b"]),
            RequestCacheKey::composite("owner-metafield", &["owner", "a:b"]),
        );

        let mut cache = RequestCache::default();
        let id = "gid://shopify/Product/1";
        cache.mark_entity("owner-metafields", id, EntityEvidenceState::Observed);
        cache.mark_entity("owner-metafields", id, EntityEvidenceState::Requested);

        assert_eq!(
            cache.entity_state("owner-metafields", id),
            Some(EntityEvidenceState::Observed)
        );
        assert!(cache.entity_was_requested("owner-metafields", id));
        assert!(cache.entity_was_hydrated("owner-metafields", id));
        assert!(!cache.entity_is_missing("owner-metafields", id));
    }

    #[test]
    fn request_disposition_preserves_whole_document_passthrough_cases() {
        assert!(
            disposition(
                "query { products(first: 1) { nodes { id } } }",
                ReadMode::LiveHybrid,
            )
            .direct_full_query_passthrough
        );
        assert!(
            disposition(
                "query { one: shop { id } two: shop { name } }",
                ReadMode::LiveHybrid,
            )
            .direct_full_query_passthrough
        );
        assert!(
            !disposition(
                "query { events(first: 1) { nodes { id } } }",
                ReadMode::Snapshot,
            )
            .direct_full_query_passthrough
        );
    }
}
