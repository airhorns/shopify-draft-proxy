use super::*;

/// Stable identity for one piece of request-scoped upstream evidence.
///
/// Keys are deliberately opaque to the broker. Domains own their key names,
/// while the broker owns deduplication and lifecycle for the active GraphQL
/// request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::proxy) struct HydrationKey(String);

impl HydrationKey {
    pub(in crate::proxy) fn new(scope: &str, identity: impl AsRef<str>) -> Self {
        Self(format!("{scope}:{}", identity.as_ref()))
    }

    pub(in crate::proxy) fn singleton(scope: &str) -> Self {
        Self(scope.to_string())
    }

    pub(in crate::proxy) fn composite(scope: &str, components: &[&str]) -> Self {
        let mut identity = String::new();
        for component in components {
            identity.push_str(&component.len().to_string());
            identity.push(':');
            identity.push_str(component);
        }
        Self::new(scope, identity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::proxy) enum EntityEvidenceState {
    Requested,
    Observed,
    Missing,
}

#[derive(Clone)]
pub(in crate::proxy) struct HydrationEvidence {
    response: Response,
    covered_response_keys: BTreeSet<String>,
}

impl HydrationEvidence {
    fn new(response: Response, covered_response_keys: BTreeSet<String>) -> Self {
        Self {
            response,
            covered_response_keys,
        }
    }

    pub(in crate::proxy) fn response(&self) -> &Response {
        &self.response
    }

    pub(in crate::proxy) fn covers_response_key(&self, response_key: &str) -> bool {
        self.covered_response_keys.contains(response_key)
    }
}

/// All upstream evidence learned while executing one GraphQL request.
///
/// The caller document remains distinct from supplemental hydration requests:
/// the former retains its alias-shaped transport response and a separately
/// canonicalized observation value, while the latter is keyed by the domain
/// requirement that requested it. Completion and entity facts let planners
/// coordinate without adding domain-specific booleans to `ExecutionSession`.
#[derive(Clone, Default)]
pub(in crate::proxy) struct RequestHydrationBroker {
    caller: Option<HydrationEvidence>,
    caller_data: Option<Value>,
    caller_selections: BTreeMap<String, Vec<SelectedField>>,
    supplemental: BTreeMap<HydrationKey, HydrationEvidence>,
    completed: BTreeSet<HydrationKey>,
    entities: BTreeMap<HydrationKey, EntityEvidenceState>,
}

impl RequestHydrationBroker {
    pub(in crate::proxy) fn caller_response(&self) -> Option<&Response> {
        self.caller.as_ref().map(HydrationEvidence::response)
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
        let covered_response_keys = response
            .body
            .get("data")
            .and_then(Value::as_object)
            .map(|data| data.keys().cloned().collect())
            .unwrap_or_default();
        self.caller = Some(HydrationEvidence::new(response, covered_response_keys));
        self.caller_data = Some(canonical_data);
    }

    pub(in crate::proxy) fn supplemental(&self, key: &HydrationKey) -> Option<&HydrationEvidence> {
        self.supplemental.get(key)
    }

    pub(in crate::proxy) fn record_supplemental(
        &mut self,
        key: HydrationKey,
        response: Response,
        covered_response_keys: BTreeSet<String>,
    ) {
        self.completed.insert(key.clone());
        self.supplemental
            .insert(key, HydrationEvidence::new(response, covered_response_keys));
    }

    pub(in crate::proxy) fn is_complete(&self, key: &HydrationKey) -> bool {
        self.completed.contains(key)
    }

    pub(in crate::proxy) fn mark_complete(&mut self, key: HydrationKey) {
        self.completed.insert(key);
    }

    pub(in crate::proxy) fn mark_entity(
        &mut self,
        scope: &str,
        id: impl Into<String>,
        state: EntityEvidenceState,
    ) {
        self.mark_entity_key(HydrationKey::new(scope, id.into()), state);
    }

    pub(in crate::proxy) fn mark_entity_key(
        &mut self,
        key: HydrationKey,
        state: EntityEvidenceState,
    ) {
        self.entities
            .entry(key)
            .and_modify(|existing| {
                // Planning may rediscover an entity after a preflight has
                // already observed it. A mere request must never downgrade
                // stronger positive or negative evidence.
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
        scope: &str,
        id: &str,
    ) -> Option<EntityEvidenceState> {
        self.entity_state_for_key(&HydrationKey::new(scope, id))
    }

    pub(in crate::proxy) fn entity_state_for_key(
        &self,
        key: &HydrationKey,
    ) -> Option<EntityEvidenceState> {
        self.entities.get(key).copied()
    }

    pub(in crate::proxy) fn entity_was_requested(&self, scope: &str, id: &str) -> bool {
        self.entity_state(scope, id).is_some()
    }

    pub(in crate::proxy) fn entity_was_hydrated(&self, scope: &str, id: &str) -> bool {
        matches!(
            self.entity_state(scope, id),
            Some(EntityEvidenceState::Observed | EntityEvidenceState::Missing)
        )
    }

    pub(in crate::proxy) fn entity_is_missing(&self, scope: &str, id: &str) -> bool {
        self.entity_state(scope, id) == Some(EntityEvidenceState::Missing)
    }
}

impl DraftProxy {
    /// Execute one supplemental GraphQL read at most once during the active
    /// request. A failed response is still evidence that the attempt happened,
    /// so sibling resolvers do not retry it independently.
    pub(in crate::proxy) fn request_hydration_post_once(
        &mut self,
        key: HydrationKey,
        request: &Request,
        body: Value,
    ) -> Response {
        if let Some(evidence) = self.execution_session.hydration.supplemental(&key) {
            return evidence.response().clone();
        }
        let response = self.upstream_post(request, body);
        self.execution_session.hydration.record_supplemental(
            key,
            response.clone(),
            BTreeSet::new(),
        );
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

    #[test]
    fn caller_transport_and_canonical_observation_values_remain_separate() {
        let mut broker = RequestHydrationBroker::default();
        broker.record_caller_response(
            response(json!({ "aliasedProduct": { "aliasedTitle": "Bag" } })),
            json!({ "aliasedProduct": { "title": "Bag" } }),
        );

        assert_eq!(
            broker
                .caller_response()
                .and_then(|response| response.body.pointer("/data/aliasedProduct/aliasedTitle")),
            Some(&json!("Bag"))
        );
        assert_eq!(
            broker
                .caller_data()
                .and_then(|data| data.pointer("/aliasedProduct/title")),
            Some(&json!("Bag"))
        );
    }

    #[test]
    fn supplemental_and_entity_evidence_are_keyed_without_domain_fields() {
        let mut broker = RequestHydrationBroker::default();
        let key = HydrationKey::new("owner-metafields", "gid://shopify/Product/1");
        broker.record_supplemental(
            key.clone(),
            response(json!({ "nodes": [] })),
            BTreeSet::from(["nodes".to_string()]),
        );
        broker.mark_entity(
            "owner-metafields",
            "gid://shopify/Product/1",
            EntityEvidenceState::Missing,
        );

        assert!(broker.is_complete(&key));
        assert!(broker
            .supplemental(&key)
            .is_some_and(|evidence| evidence.covers_response_key("nodes")));
        assert!(broker.entity_was_requested("owner-metafields", "gid://shopify/Product/1"));
        assert!(broker.entity_was_hydrated("owner-metafields", "gid://shopify/Product/1"));
        assert!(broker.entity_is_missing("owner-metafields", "gid://shopify/Product/1"));
    }

    #[test]
    fn composite_keys_do_not_alias_components_with_embedded_separators() {
        assert_ne!(
            HydrationKey::composite("owner-metafield", &["owner:a", "b"]),
            HydrationKey::composite("owner-metafield", &["owner", "a:b"]),
        );
    }

    #[test]
    fn a_repeated_request_does_not_downgrade_observed_entity_evidence() {
        let mut broker = RequestHydrationBroker::default();
        let id = "gid://shopify/Product/1";
        broker.mark_entity("owner-metafields", id, EntityEvidenceState::Observed);
        broker.mark_entity("owner-metafields", id, EntityEvidenceState::Requested);

        assert_eq!(
            broker.entity_state("owner-metafields", id),
            Some(EntityEvidenceState::Observed)
        );
    }
}
