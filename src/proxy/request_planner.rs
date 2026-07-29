use super::*;
use crate::{operation_registry::OperationCapability, resolver_registry::NativeRequestPlanner};
use hydration::HydrationKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::proxy) enum RootReadAuthority {
    Local,
    Upstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::proxy) enum HydrationTrigger {
    BeforeOperation,
    BeforeDomain(CapabilityDomain),
}

pub(crate) struct RequestPlanningInvocation<'a> {
    pub request: &'a Request,
    pub operation_type: OperationType,
    pub roots: &'a [RootFieldSelection],
    pub variables: &'a BTreeMap<String, ResolvedValue>,
    pub capabilities: &'a [OperationCapability],
}

impl RequestPlanningInvocation<'_> {
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

    pub(in crate::proxy) fn has_domain(&self, domain: CapabilityDomain) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability.domain == domain)
    }
}

struct RootExecutionDecision {
    response_key: String,
    capability: OperationCapability,
    authority: RootReadAuthority,
}

type HydrationRunner = Box<dyn FnOnce(&mut DraftProxy) + Send + 'static>;
type CallerObserver = Box<dyn FnOnce(&mut DraftProxy, &Response) + Send + 'static>;

struct PlannedHydration {
    keys: Vec<HydrationKey>,
    trigger: HydrationTrigger,
    runner: Option<HydrationRunner>,
}

/// Declarative execution policy for one validated Admin operation.
///
/// Root registrations contribute authority decisions and keyed hydration work.
/// The runtime only executes this plan; it does not know which commerce domain
/// requested a particular preflight.
pub(crate) struct RequestExecutionPlan {
    operation_type: OperationType,
    roots: Vec<RootExecutionDecision>,
    planned_keys: BTreeSet<HydrationKey>,
    hydrations: Vec<PlannedHydration>,
    caller_observer_keys: BTreeSet<HydrationKey>,
    caller_observers: Vec<Option<CallerObserver>>,
}

impl RequestExecutionPlan {
    pub(in crate::proxy) fn empty() -> Self {
        Self::new(OperationType::Query, &[], &[])
    }

    fn new(
        operation_type: OperationType,
        roots: &[RootFieldSelection],
        capabilities: &[OperationCapability],
    ) -> Self {
        assert_eq!(roots.len(), capabilities.len());
        Self {
            operation_type,
            roots: roots
                .iter()
                .zip(capabilities)
                .map(|(root, capability)| RootExecutionDecision {
                    response_key: root.response_key.clone(),
                    capability: capability.clone(),
                    authority: if capability.domain == CapabilityDomain::Unknown
                        || capability.execution == CapabilityExecution::Passthrough
                    {
                        RootReadAuthority::Upstream
                    } else {
                        RootReadAuthority::Local
                    },
                })
                .collect(),
            planned_keys: BTreeSet::new(),
            hydrations: Vec::new(),
            caller_observer_keys: BTreeSet::new(),
            caller_observers: Vec::new(),
        }
    }

    pub(in crate::proxy) fn has_local_root(&self) -> bool {
        self.roots.iter().any(|root| {
            root.capability.domain != CapabilityDomain::Unknown
                && matches!(
                    root.capability.execution,
                    CapabilityExecution::OverlayRead | CapabilityExecution::StageLocally
                )
        })
    }

    pub(in crate::proxy) fn has_passthrough_root(&self) -> bool {
        self.roots.iter().any(|root| {
            root.capability.domain == CapabilityDomain::Unknown
                || root.capability.execution == CapabilityExecution::Passthrough
        })
    }

    pub(in crate::proxy) fn all_passthrough(&self) -> bool {
        !self.roots.is_empty() && !self.has_local_root() && self.has_passthrough_root()
    }

    pub(in crate::proxy) fn direct_full_query_passthrough(&self) -> bool {
        self.operation_type == OperationType::Query
            && !self.all_passthrough()
            && !self.roots.is_empty()
            && self
                .roots
                .iter()
                .all(|root| root.authority == RootReadAuthority::Upstream)
    }

    pub(in crate::proxy) fn domain_for_response_key(&self, response_key: &str) -> CapabilityDomain {
        self.roots
            .iter()
            .find(|root| root.response_key == response_key)
            .map(|root| root.capability.domain)
            .unwrap_or(CapabilityDomain::Unknown)
    }

    pub(in crate::proxy) fn set_root_authority(
        &mut self,
        response_key: &str,
        authority: RootReadAuthority,
    ) {
        if let Some(root) = self
            .roots
            .iter_mut()
            .find(|root| root.response_key == response_key)
        {
            root.authority = authority;
        }
    }

    pub(in crate::proxy) fn set_domain_authority(
        &mut self,
        domain: CapabilityDomain,
        authority: RootReadAuthority,
    ) {
        for root in &mut self.roots {
            if root.capability.domain == domain {
                root.authority = authority;
            }
        }
    }

    pub(in crate::proxy) fn set_named_roots_authority(
        &mut self,
        root_names: &[&str],
        roots: &[RootFieldSelection],
        authority: RootReadAuthority,
    ) {
        for root in roots {
            if root_names.contains(&root.name.as_str()) {
                self.set_root_authority(&root.response_key, authority);
            }
        }
    }

    pub(in crate::proxy) fn add_hydration(
        &mut self,
        keys: impl IntoIterator<Item = HydrationKey>,
        trigger: HydrationTrigger,
        runner: impl FnOnce(&mut DraftProxy) + Send + 'static,
    ) {
        let keys = keys.into_iter().collect::<Vec<_>>();
        if keys.is_empty() || keys.iter().all(|key| self.planned_keys.contains(key)) {
            return;
        }
        self.planned_keys.extend(keys.iter().cloned());
        self.hydrations.push(PlannedHydration {
            keys,
            trigger,
            runner: Some(Box::new(runner)),
        });
    }

    pub(in crate::proxy) fn add_caller_observer(
        &mut self,
        key: HydrationKey,
        observer: impl FnOnce(&mut DraftProxy, &Response) + Send + 'static,
    ) {
        if !self.caller_observer_keys.insert(key) {
            return;
        }
        self.caller_observers.push(Some(Box::new(observer)));
    }

    pub(in crate::proxy) fn execute_before_operation(&mut self, proxy: &mut DraftProxy) {
        self.execute_hydrations(proxy, HydrationTrigger::BeforeOperation);
    }

    pub(in crate::proxy) fn execute_before_domain(
        &mut self,
        proxy: &mut DraftProxy,
        domain: CapabilityDomain,
    ) {
        self.execute_hydrations(proxy, HydrationTrigger::BeforeDomain(domain));
    }

    fn execute_hydrations(&mut self, proxy: &mut DraftProxy, trigger: HydrationTrigger) {
        let mut ready = Vec::new();
        for hydration in &mut self.hydrations {
            if hydration.trigger != trigger {
                continue;
            }
            let already_complete = hydration
                .keys
                .iter()
                .all(|key| proxy.execution_session.hydration.is_complete(key));
            if already_complete {
                hydration.runner = None;
                continue;
            }
            if let Some(runner) = hydration.runner.take() {
                ready.push((hydration.keys.clone(), runner));
            }
        }
        for (keys, runner) in ready {
            runner(proxy);
            for key in keys {
                proxy.execution_session.hydration.mark_complete(key);
            }
        }
    }

    pub(in crate::proxy) fn observe_caller_response(
        &mut self,
        proxy: &mut DraftProxy,
        response: &Response,
    ) {
        for observer in &mut self.caller_observers {
            if let Some(observer) = observer.take() {
                observer(proxy, response);
            }
        }
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn admin_request_execution_plan(
        &self,
        request: &Request,
        operation_type: OperationType,
        roots: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> RequestExecutionPlan {
        let capabilities = roots
            .iter()
            .map(|root| self.registry.resolve(operation_type, &root.name))
            .collect::<Vec<_>>();
        let mut plan = RequestExecutionPlan::new(operation_type, roots, &capabilities);
        let invocation = RequestPlanningInvocation {
            request,
            operation_type,
            roots,
            variables,
            capabilities: &capabilities,
        };
        let mut planners = Vec::<NativeRequestPlanner>::new();
        for root in roots {
            let Some(registration) = self.registry.registration(operation_type, &root.name) else {
                continue;
            };
            for planner in &registration.request_planners {
                if !planners
                    .iter()
                    .any(|existing| std::ptr::fn_addr_eq(*existing, *planner))
                {
                    planners.push(*planner);
                }
            }
        }
        for planner in planners {
            planner(self, &invocation, &mut plan);
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn empty_plan() -> RequestExecutionPlan {
        RequestExecutionPlan::empty()
    }

    fn planned_query(query: &str, read_mode: ReadMode) -> RequestExecutionPlan {
        let variables = BTreeMap::new();
        let document = parsed_document(query, &variables).expect("query should parse");
        let proxy = DraftProxy::new(Config {
            read_mode,
            ..Config::default()
        });
        proxy.admin_request_execution_plan(
            &Request::default(),
            document.operation_type,
            &document.root_fields,
            &variables,
        )
    }

    #[test]
    fn keyed_hydrations_run_once_at_their_declared_trigger() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut plan = empty_plan();
        for _ in 0..2 {
            let runs = Arc::clone(&runs);
            plan.add_hydration(
                [HydrationKey::singleton("discount-references")],
                HydrationTrigger::BeforeDomain(CapabilityDomain::Discounts),
                move |_| {
                    runs.fetch_add(1, Ordering::SeqCst);
                },
            );
        }
        let mut proxy = DraftProxy::new(Config::default());

        plan.execute_before_operation(&mut proxy);
        assert_eq!(runs.load(Ordering::SeqCst), 0);
        plan.execute_before_domain(&mut proxy, CapabilityDomain::Discounts);
        plan.execute_before_domain(&mut proxy, CapabilityDomain::Discounts);
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn caller_observers_are_consumed_once() {
        let observations = Arc::new(AtomicUsize::new(0));
        let mut plan = empty_plan();
        let captured = Arc::clone(&observations);
        plan.add_caller_observer(HydrationKey::singleton("observer"), move |_, _| {
            captured.fetch_add(1, Ordering::SeqCst);
        });
        plan.add_caller_observer(HydrationKey::singleton("observer"), |_, _| {
            panic!("duplicate keyed observer should not be retained");
        });
        let mut proxy = DraftProxy::new(Config::default());
        let response = Response {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({ "data": {} }),
        };

        plan.observe_caller_response(&mut proxy, &response);
        plan.observe_caller_response(&mut proxy, &response);
        assert_eq!(observations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn registry_planners_choose_whole_document_authority() {
        let product_plan = planned_query(
            "query { products(first: 1) { nodes { id } } }",
            ReadMode::LiveHybrid,
        );
        assert!(product_plan.direct_full_query_passthrough());

        let shop_plan = planned_query(
            "query { one: shop { id } two: shop { name } }",
            ReadMode::LiveHybrid,
        );
        assert!(shop_plan.direct_full_query_passthrough());

        let snapshot_events = planned_query(
            "query { events(first: 1) { nodes { id } } }",
            ReadMode::Snapshot,
        );
        assert!(!snapshot_events.direct_full_query_passthrough());
    }

    #[test]
    fn registry_planners_schedule_node_and_discount_hydration_at_safe_boundaries() {
        let node_plan = planned_query(
            "query { first: node(id: \"gid://shopify/Product/1\") { id } second: nodes(ids: [\"gid://shopify/Product/2\"]) { id } }",
            ReadMode::LiveHybrid,
        );
        assert_eq!(node_plan.hydrations.len(), 1);
        assert_eq!(
            node_plan.hydrations[0].trigger,
            HydrationTrigger::BeforeOperation
        );

        let variables = BTreeMap::new();
        let document = parsed_document(
            "mutation { discountCodeActivate(id: \"gid://shopify/DiscountCodeNode/1\") { userErrors { message } } }",
            &variables,
        )
        .expect("mutation should parse");
        let proxy = DraftProxy::new(Config::default());
        let discount_plan = proxy.admin_request_execution_plan(
            &Request::default(),
            document.operation_type,
            &document.root_fields,
            &variables,
        );
        assert_eq!(discount_plan.hydrations.len(), 1);
        assert_eq!(
            discount_plan.hydrations[0].trigger,
            HydrationTrigger::BeforeDomain(CapabilityDomain::Discounts)
        );
    }
}
