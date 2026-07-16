//! Request-scoped execution bridge between the versioned GraphQL engine and
//! domain-owned root resolvers.

use super::graphql_error_compat::{
    directive_variable_mismatch_error, product_create_argument_arity_error,
    required_variable_error, shopify_engine_response, shopify_root_id_errors,
};
use super::*;
use crate::admin_graphql::{
    self, AdminApiVersion, FieldResolverInvocation, FieldResolverResult, RootExecutionContext,
    RootFieldError, RootFieldExecutor, RootFieldInvocation, RootFieldResult,
};
use crate::graphql::{DirectiveSelection, VariableDefinitionInfo};
use crate::resolver_registry::{
    FieldResolverImplementation, GraphqlApiVersion, LocalResolverMode, ResolverOutcome,
};

const INTERNAL_HTTP_STATUS_EXTENSION: &str = "__draftProxyHttpStatus";

struct ProxyRootExecutor {
    proxy: Arc<std::sync::Mutex<DraftProxy>>,
    original_request: Request,
    version: AdminApiVersion,
    root_calls: BTreeMap<String, PreparedRootCall>,
    root_locations: BTreeMap<String, SourceLocation>,
    discount_preflight: Option<(Request, Vec<RootFieldSelection>)>,
    discount_preflight_done: std::sync::Mutex<bool>,
    delivery_promise_mutation: Option<PreparedAtomicMutation>,
    delivery_promise_outcomes: std::sync::Mutex<Option<BTreeMap<String, ResolverOutcome<Value>>>>,
    full_passthrough_request: Option<Request>,
    full_passthrough_direct: bool,
    observe_direct_shop_passthrough: bool,
    full_passthrough_response: Arc<std::sync::Mutex<Option<Response>>>,
    reject_mixed_mutation: bool,
    resolved_responses: Arc<std::sync::Mutex<BTreeMap<String, Response>>>,
    resolved_extensions: Arc<std::sync::Mutex<BTreeMap<String, Value>>>,
}

#[derive(Debug, Clone)]
struct PreparedRootCall {
    request: Request,
    query: String,
    variables: BTreeMap<String, ResolvedValue>,
    operation: crate::graphql::ParsedOperation,
    field: RootFieldSelection,
}

#[derive(Debug, Clone)]
struct PreparedAtomicMutation {
    request: Request,
    query: String,
    variables: BTreeMap<String, ResolvedValue>,
}

pub(crate) fn field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    let mut registrations = saved_search_field_resolver_registrations();
    registrations.extend(product_field_resolver_registrations());
    registrations.extend(super::selling_plans::selling_plan_field_resolver_registrations());
    registrations.extend(inventory_field_resolver_registrations());
    registrations.extend(super::privacy::privacy_field_resolver_registrations());
    registrations.extend(event_field_resolver_registrations());
    registrations.extend(return_field_resolver_registrations());
    registrations.extend(super::b2b_customers::customer_field_resolver_registrations());
    registrations.extend(super::b2b_customers::b2b_company_field_resolver_registrations());
    registrations
        .extend(super::localization_markets_catalogs::markets_field_resolver_registrations());
    registrations
        .extend(super::admin_shipping_gift_cards::delivery_promise_field_resolver_registrations());
    registrations.extend(super::storefront::storefront_field_resolver_registrations());
    registrations.extend(legacy_projected_field_resolver_registrations());
    registrations
}

/// Argument-bearing fields still rendered by a domain's legacy
/// `SelectedField` projector. Keeping this list explicit prevents the captured
/// schema from silently blessing new fields as property-backed. Each entry is
/// deleted when its parent type moves to canonical values plus a real field
/// resolver.
fn legacy_projected_field_resolver_registrations() -> Vec<FieldResolverRegistration> {
    let admin = [
        ("AppInstallation", "allSubscriptions"),
        ("AppInstallation", "oneTimePurchases"),
        ("AppSubscriptionLineItem", "usageRecords"),
        ("Article", "comments"),
        ("Article", "commentsCount"),
        ("Article", "metafield"),
        ("Article", "metafields"),
        ("Blog", "articles"),
        ("Blog", "articlesCount"),
        ("CalculatedOrder", "lineItems"),
        ("CartTransform", "metafield"),
        ("CombinedListing", "combinedListingChildren"),
        ("Company", "contactRoles"),
        ("Company", "contacts"),
        ("Company", "locations"),
        ("Company", "orders"),
        ("Company", "draftOrders"),
        ("CompanyContact", "roleAssignments"),
        ("CompanyLocation", "roleAssignments"),
        ("CompanyLocation", "orders"),
        ("CompanyLocation", "draftOrders"),
        ("CompanyLocation", "staffMemberAssignments"),
        ("CompanyLocationCatalog", "companyLocations"),
        ("Customer", "addressesV2"),
        ("Customer", "metafield"),
        ("Customer", "metafields"),
        ("Customer", "orders"),
        ("Customer", "paymentMethods"),
        ("Customer", "storeCreditAccounts"),
        ("DeliveryCustomization", "metafield"),
        ("DeliveryCustomization", "metafields"),
        ("DeliveryProfile", "profileItems"),
        ("DeliveryProfile", "profileLocationGroups"),
        ("DeliveryLocationGroup", "locations"),
        ("DeliveryProfileItem", "variants"),
        ("DeliveryProfileLocationGroup", "locationGroupZones"),
        ("DeliveryLocationGroupZone", "methodDefinitions"),
        ("DiscountCodeApp", "codes"),
        ("DiscountCodeBasic", "codes"),
        ("DiscountCodeBxgy", "codes"),
        ("DiscountCodeFreeShipping", "codes"),
        ("DiscountCodeNode", "metafields"),
        ("DiscountRedeemCodeBulkCreation", "codes"),
        ("DiscountProducts", "products"),
        ("DiscountProducts", "productVariants"),
        ("DiscountCollections", "collections"),
        ("DraftOrder", "lineItems"),
        ("Fulfillment", "trackingInfo"),
        ("Fulfillment", "events"),
        ("Fulfillment", "fulfillmentLineItems"),
        ("FulfillmentConstraintRule", "metafields"),
        ("FulfillmentConstraintRule", "metafield"),
        ("FulfillmentOrder", "lineItems"),
        ("FulfillmentOrder", "merchantRequests"),
        ("GiftCard", "transactions"),
        ("Image", "url"),
        ("Location", "inventoryLevels"),
        ("Location", "metafield"),
        ("Location", "metafields"),
        ("LineItem", "taxLines"),
        ("Market", "catalogs"),
        ("Market", "webPresences"),
        ("MarketCatalog", "markets"),
        ("MarketLocalizableResource", "marketLocalizations"),
        ("MarketsResolvedValues", "catalogs"),
        ("MarketWebPresence", "markets"),
        ("MarketsResolvedValues", "webPresences"),
        ("MetafieldDefinition", "metafieldsCount"),
        ("MetafieldDefinitionConstraints", "values"),
        ("Metaobject", "field"),
        ("MetaobjectField", "references"),
        ("MetaobjectDefinition", "metaobjects"),
        ("OnlineStoreTheme", "files"),
        ("Order", "events"),
        ("Order", "fulfillmentOrders"),
        ("Order", "fulfillments"),
        ("Order", "lineItems"),
        ("Order", "refunds"),
        ("Order", "returns"),
        ("Order", "shippingLines"),
        ("Order", "transactions"),
        ("PaymentCustomization", "metafield"),
        ("PaymentCustomization", "metafields"),
        ("PaymentTerms", "paymentSchedules"),
        ("Refund", "refundLineItems"),
        ("Refund", "transactions"),
        ("Return", "returnLineItems"),
        ("Return", "reverseFulfillmentOrders"),
        ("ReturnableFulfillment", "returnableFulfillmentLineItems"),
        ("RegionsCondition", "regions"),
        ("Shop", "metafield"),
        ("Shop", "metafields"),
        ("ShopAddress", "formatted"),
        ("ShopPolicy", "translations"),
        ("TranslatableResource", "translatableContent"),
        ("TranslatableResource", "translations"),
        ("Validation", "metafields"),
    ];
    let mut registrations = admin
        .into_iter()
        .map(|(parent_type, field_name)| {
            FieldResolverRegistration::legacy_projected_property(
                ApiSurface::Admin,
                parent_type,
                field_name,
            )
        })
        .collect::<Vec<_>>();
    registrations.push(FieldResolverRegistration::legacy_projected_property(
        ApiSurface::Storefront,
        "Customer",
        "addresses",
    ));
    registrations
}

pub(crate) fn field_resolver_type_policies() -> Vec<FieldResolverTypePolicy> {
    let mut policies = product_field_resolver_type_policies();
    policies.extend(super::selling_plans::selling_plan_field_resolver_type_policies());
    policies.extend(inventory_field_resolver_type_policies());
    policies.extend(super::storefront::storefront_field_resolver_type_policies());
    policies
}

/// Temporarily make this proxy available to `'static` GraphQL resolver data
/// without risking replacement of the caller-owned instance. The normal path
/// moves the proxy back out of the request Arc. Exceptional paths clone the
/// latest guarded value before resuming the unwind, so `self` is never left as
/// the fresh placeholder used during execution.
pub(in crate::proxy) fn with_request_owned_proxy<T>(
    proxy: &mut DraftProxy,
    run: impl FnOnce(Arc<std::sync::Mutex<DraftProxy>>) -> T,
) -> T {
    let placeholder = DraftProxy::new(proxy.config.clone());
    let owned_proxy = std::mem::replace(proxy, placeholder);
    let shared_proxy = Arc::new(std::sync::Mutex::new(owned_proxy));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(Arc::clone(&shared_proxy))
    }));

    let mut restored_proxy = match Arc::try_unwrap(shared_proxy) {
        Ok(proxy) => match proxy.into_inner() {
            Ok(proxy) => proxy,
            Err(poisoned) => poisoned.into_inner(),
        },
        Err(proxy) => match proxy.lock() {
            Ok(proxy) => proxy.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        },
    };
    restored_proxy.engine_mutation_log_start = None;
    restored_proxy.engine_discount_refs_preflighted = false;
    *proxy = restored_proxy;

    match outcome {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn shared_root_response(responses: &BTreeMap<String, Response>) -> Option<&Response> {
    let mut responses = responses.values();
    let first = responses.next()?;
    responses.all(|response| response == first).then_some(first)
}

impl RootFieldExecutor for ProxyRootExecutor {
    fn execute_root(&self, invocation: RootFieldInvocation) -> Result<RootFieldResult, String> {
        let RootFieldInvocation {
            response_key,
            root_name,
            arguments,
        } = invocation;
        if self.reject_mixed_mutation {
            return Err(
                "A mutation operation cannot mix locally staged and passthrough root fields."
                    .to_string(),
            );
        }
        if matches!(
            root_name.as_str(),
            "deliveryPromiseProviderUpsert" | "deliveryPromiseParticipantsUpdate"
        ) {
            if let Some(prepared) = &self.delivery_promise_mutation {
                let mut cached = self.delivery_promise_outcomes.lock().map_err(|_| {
                    "Delivery-promise mutation outcome lock was poisoned".to_string()
                })?;
                if cached.is_none() {
                    let mut proxy = self
                        .proxy
                        .lock()
                        .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                    let mut outcomes = proxy.delivery_promise_mutation(
                        &prepared.query,
                        &prepared.variables,
                        &prepared.request,
                        &response_key,
                    );
                    for outcome in outcomes.values_mut() {
                        for draft in outcome.log_drafts.drain(..) {
                            proxy.record_mutation_log_draft(
                                &prepared.request,
                                &prepared.query,
                                &prepared.variables,
                                draft,
                            );
                        }
                    }
                    *cached = Some(outcomes);
                }
                let outcome = cached
                    .as_ref()
                    .expect("delivery-promise outcomes should be cached")
                    .get(&response_key)
                    .cloned()
                    .unwrap_or_else(|| ResolverOutcome::value(Value::Null));
                self.resolved_responses
                    .lock()
                    .map_err(|_| "Admin GraphQL resolved response lock was poisoned".to_string())?
                    .insert(
                        response_key.clone(),
                        resolver_outcome_wire_response(
                            &response_key,
                            &outcome.value,
                            &outcome.errors,
                            &outcome.extensions,
                        ),
                    );
                self.resolved_extensions
                    .lock()
                    .map_err(|_| "Admin GraphQL resolver extensions lock was poisoned".to_string())?
                    .extend(outcome.extensions);
                return Ok(RootFieldResult {
                    value: outcome.value,
                    errors: outcome.errors,
                });
            }
        }
        if let Some((request, fields)) = &self.discount_preflight {
            let mut done = self
                .discount_preflight_done
                .lock()
                .map_err(|_| "Admin GraphQL discount preflight lock was poisoned".to_string())?;
            if !*done {
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                proxy.hydrate_discount_item_refs(request, fields);
                proxy.hydrate_discount_context_refs(request, fields);
                proxy.engine_discount_refs_preflighted = true;
                *done = true;
            }
        }
        let response = if let Some(request) = &self.full_passthrough_request {
            let mut cached = self
                .full_passthrough_response
                .lock()
                .map_err(|_| "Admin GraphQL passthrough response lock was poisoned".to_string())?;
            if cached.is_none() {
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                let response = if self.full_passthrough_direct {
                    (proxy.upstream_transport)(request.clone())
                } else {
                    proxy.execute_passthrough_graphql(request)
                };
                if self.observe_direct_shop_passthrough && (200..300).contains(&response.status) {
                    proxy.hydrate_shop_state_from_response_data(&response.body["data"]);
                    proxy.observe_nodes_response(&response);
                }
                *cached = Some(response);
            }
            cached
                .as_ref()
                .expect("passthrough response should be cached")
                .clone()
        } else {
            let mut call = self.root_calls.get(&response_key).cloned().ok_or_else(|| {
                format!(
                    "No request-scoped resolver input was prepared for GraphQL root `{root_name}`"
                )
            })?;
            call.field.arguments = arguments
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
                .collect();
            let registration = self
                .proxy
                .lock()
                .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?
                .registry
                .registration(call.operation.operation_type, &call.field.name)
                .cloned();
            if let Some(registration) = registration {
                let handler = registration.handler;
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                let outcome = handler(
                    &mut proxy,
                    crate::resolver_registry::RootInvocation {
                        api_surface: ApiSurface::Admin,
                        api_version: GraphqlApiVersion::Admin(self.version),
                        response_key: &response_key,
                        root_name: &root_name,
                        root_location: call.field.location,
                        directives: call.field.directives.clone(),
                        arguments,
                        request: &call.request,
                        query: &call.query,
                        variables: &call.variables,
                        operation: &call.operation,
                        mode: LocalResolverMode::from_execution(registration.execution),
                    },
                );
                let ResolverOutcome {
                    value,
                    mut errors,
                    extensions,
                    log_drafts,
                } = outcome;
                if let Some(location) = self.root_locations.get(&response_key) {
                    for error in &mut errors {
                        if error
                            .extensions
                            .get("code")
                            .and_then(Value::as_str)
                            .is_some_and(|code| {
                                matches!(code, "BAD_REQUEST" | "MAX_INPUT_SIZE_EXCEEDED")
                            })
                        {
                            error.locations = vec![async_graphql::Pos {
                                line: location.line,
                                column: location.column,
                            }];
                        }
                    }
                }
                for draft in log_drafts {
                    proxy.record_mutation_log_draft(
                        &call.request,
                        &call.query,
                        &call.variables,
                        draft,
                    );
                }
                drop(proxy);
                self.resolved_responses
                    .lock()
                    .map_err(|_| "Admin GraphQL resolved response lock was poisoned".to_string())?
                    .entry(response_key.clone())
                    .or_insert_with(|| {
                        resolver_outcome_wire_response(&response_key, &value, &errors, &extensions)
                    });
                self.resolved_extensions
                    .lock()
                    .map_err(|_| "Admin GraphQL resolver extensions lock was poisoned".to_string())?
                    .extend(extensions);
                return Ok(RootFieldResult { value, errors });
            }
            let mut proxy = self
                .proxy
                .lock()
                .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
            proxy.resolve_prevalidated_graphql_root_call(&call)
        };
        self.resolved_responses
            .lock()
            .map_err(|_| "Admin GraphQL resolved response lock was poisoned".to_string())?
            .insert(response_key.clone(), response.clone());
        let value = response
            .body
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get(&response_key))
            .cloned()
            .unwrap_or(Value::Null);
        let mut errors = response
            .body
            .get("errors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|error| {
                let error_path = error.get("path").and_then(Value::as_array);
                if error_path
                    .and_then(|path| path.first())
                    .and_then(Value::as_str)
                    .is_some_and(|root| root != response_key)
                {
                    return None;
                }
                let error_code = error.pointer("/extensions/code").and_then(Value::as_str);
                let locations =
                    if matches!(error_code, Some("BAD_REQUEST" | "MAX_INPUT_SIZE_EXCEEDED")) {
                        self.root_locations
                            .get(&response_key)
                            .map(|location| {
                                vec![async_graphql::Pos {
                                    line: location.line,
                                    column: location.column,
                                }]
                            })
                            .unwrap_or_default()
                    } else {
                        error
                            .get("locations")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|location| {
                                Some(async_graphql::Pos {
                                    line: location.get("line")?.as_u64()? as usize,
                                    column: location.get("column")?.as_u64()? as usize,
                                })
                            })
                            .collect()
                    };
                Some(RootFieldError {
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("GraphQL root resolver failed")
                        .to_string(),
                    extensions: error
                        .get("extensions")
                        .and_then(Value::as_object)
                        .map(|extensions| {
                            extensions
                                .iter()
                                .map(|(key, value)| (key.clone(), value.clone()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    path: error_path
                        .map(|path| {
                            path.iter()
                                .skip(1)
                                .filter_map(|segment| match segment {
                                    Value::String(field) => {
                                        Some(async_graphql::PathSegment::Field(field.clone()))
                                    }
                                    Value::Number(index) => index.as_u64().map(|index| {
                                        async_graphql::PathSegment::Index(index as usize)
                                    }),
                                    _ => None,
                                })
                                .collect()
                        })
                        // HTTP/dispatcher failures historically gained the current
                        // root path at the GraphQL execution boundary. Status-200
                        // resolver errors without a path are intentionally pathless.
                        .or_else(|| (response.status >= 400).then(Vec::new)),
                    locations,
                })
            })
            .collect::<Vec<_>>();
        if errors.is_empty() && response.status >= 400 {
            errors.push(RootFieldError {
                message: format!(
                    "GraphQL root `{root_name}` failed with status {}",
                    response.status
                ),
                extensions: BTreeMap::new(),
                path: Some(Vec::new()),
                locations: Vec::new(),
            });
        }
        Ok(RootFieldResult { value, errors })
    }

    fn execute_field(
        &self,
        invocation: FieldResolverInvocation<'_>,
    ) -> Result<FieldResolverResult, String> {
        let mut proxy = self
            .proxy
            .lock()
            .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
        let implementation = proxy.registry.field_implementation(
            invocation.api_surface,
            invocation.api_version,
            &invocation.parent_type,
            &invocation.field_name,
        );
        match implementation {
            FieldResolverImplementation::PropertyBacked => Ok(FieldResolverResult::PropertyBacked),
            FieldResolverImplementation::Explicit(handler) => {
                handler(&mut proxy, &self.original_request, &invocation)
                    .map(FieldResolverResult::Resolved)
            }
            FieldResolverImplementation::DeliberatelyUnsupported(reason) => Ok(
                FieldResolverResult::DeliberatelyUnsupported(reason.to_string()),
            ),
        }
    }
}

pub(in crate::proxy) fn operation_selection_error_response(
    error: OperationSelectionError,
) -> Response {
    match error {
        OperationSelectionError::MultipleOperationsRequireOperationName => ok_json(json!({
            "errors": [{ "message": "An operation name is required" }]
        })),
        OperationSelectionError::UnknownOperationName(operation_name) => ok_json(json!({
            "errors": [{ "message": format!("No operation named \"{operation_name}\"") }]
        })),
        OperationSelectionError::Parse => json_error(400, "Could not parse GraphQL operation"),
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn execution_root_fields(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Vec<RootFieldSelection>> {
        root_fields(query, variables)
    }

    pub(in crate::proxy) fn execution_root_field(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_name: &str,
    ) -> Option<RootFieldSelection> {
        self.execution_root_fields(query, variables)?
            .into_iter()
            .find(|field| field.name == root_name)
    }

    pub(in crate::proxy) fn execution_primary_root_field(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<RootFieldSelection> {
        self.execution_root_fields(query, variables)?
            .into_iter()
            .next()
    }

    pub(in crate::proxy) fn execution_primary_root_response_parts(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        default_response_key: impl FnOnce() -> String,
    ) -> (String, Vec<SelectedField>, BTreeMap<String, ResolvedValue>) {
        self.execution_primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| (default_response_key(), Vec::new(), BTreeMap::new()))
    }

    pub(in crate::proxy) fn execution_primary_root_response_selection(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        default_response_key: impl FnOnce() -> String,
    ) -> (String, Vec<SelectedField>) {
        self.execution_primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| (default_response_key(), Vec::new()))
    }

    pub(in crate::proxy) fn root_fields_or_error(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Result<Vec<RootFieldSelection>, Response> {
        self.execution_root_fields(query, variables)
            .ok_or_else(|| json_error(400, "Could not parse GraphQL operation"))
    }

    pub(in crate::proxy) fn record_mutation_log_draft(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        draft: LogDraft,
    ) {
        let root_field = draft.root_field;
        let operation_name = draft
            .operation_name
            .map(Value::String)
            .unwrap_or(Value::Null);
        let staged_resource_ids = draft.staged_resource_ids;
        let status = draft.status;
        let capability_domain = draft.capability_domain;
        let capability_execution = draft.capability_execution;
        let notes = draft.notes;
        let root_fields = parse_operation_with_variables(query, variables)
            .map(|operation| operation.root_fields)
            .unwrap_or_else(|| vec![root_field.clone()]);
        self.log_entries.push(json!({
            "id": format!("log-{}", self.log_entries.len() + 1),
            "operationName": operation_name,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": status,
            "interpreted": {
                "operationType": "mutation",
                "operationName": root_field.clone(),
                "rootFields": root_fields,
                "primaryRootField": root_field.clone(),
                "capability": {
                    "operationName": root_field,
                    "domain": capability_domain,
                    "execution": capability_execution
                }
            },
            "notes": notes
        }));
    }

    /// Execute an Admin GraphQL request through the captured versioned schema.
    /// Domain code is reached only through root field resolvers; the GraphQL
    /// engine owns the executable language and response projection.
    pub(in crate::proxy) fn execute_graphql(&mut self, request: &Request) -> Response {
        self.request_owner_metafield_hydrated_ids.clear();
        self.request_upstream_query_response = None;
        self.request_localization_context_preflighted = false;
        self.request_markets_query_preflighted = false;
        self.request_mixed_discount_local_read = false;
        self.request_node_query_outcomes = None;
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let Some(version) = AdminApiVersion::from_route(&request.path) else {
            return json_error(404, "No captured Admin GraphQL schema for this route");
        };
        let schema = match admin_graphql::schema(version) {
            Ok(schema) => schema,
            Err(error) => {
                return json_error(
                    500,
                    &format!("Could not initialize Admin GraphQL {version}: {error}"),
                );
            }
        };

        let selected_query = selected_operation_query(
            &graphql_request.query,
            graphql_request.operation_name.as_deref(),
        )
        .ok();
        let prepared = selected_query.as_deref().and_then(|query| {
            let variables =
                variables_with_operation_defaults(query, &graphql_request.variables, None).ok()?;
            let document = parsed_document(query, &variables)?;
            let single_root = document.root_fields.len() == 1;
            let root_calls = document
                .root_fields
                .iter()
                .map(|field| {
                    let field_query = if single_root {
                        query.to_string()
                    } else {
                        single_root_transport_query(
                            document.operation_type,
                            field,
                            &document.variable_definitions,
                        )
                    };
                    let field_request = if single_root {
                        request.clone()
                    } else {
                        Request {
                            method: request.method.clone(),
                            path: request.path.clone(),
                            headers: request.headers.clone(),
                            body: json!({
                                "query": field_query.clone(),
                                "variables": resolved_variables_json(&variables)
                            })
                            .to_string(),
                        }
                    };
                    (
                        field.response_key.clone(),
                        PreparedRootCall {
                            request: field_request,
                            // Transitional selection-aware domains receive the isolated
                            // one-root document. The engine normalizes multi-root mutation
                            // logging back to the caller's original document after execution.
                            query: field_query,
                            variables: variables.clone(),
                            operation: crate::graphql::ParsedOperation {
                                operation_type: document.operation_type,
                                root_fields: vec![field.name.clone()],
                            },
                            field: field.clone(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            Some((document, variables, root_calls))
        });

        let (operation_type, root_names, root_calls) = prepared
            .as_ref()
            .map(|(document, _, root_calls)| {
                (
                    Some(document.operation_type),
                    document
                        .root_fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>(),
                    root_calls.clone(),
                )
            })
            .unwrap_or((None, Vec::new(), BTreeMap::new()));
        let capabilities = operation_type.map_or_else(Vec::new, |operation_type| {
            root_names
                .iter()
                .map(|root| self.registry.resolve(operation_type, root))
                .collect::<Vec<_>>()
        });
        let has_local_root = capabilities.iter().any(|capability| {
            capability.domain != CapabilityDomain::Unknown
                && matches!(
                    capability.execution,
                    CapabilityExecution::OverlayRead | CapabilityExecution::StageLocally
                )
        });
        let has_passthrough_root = capabilities.iter().any(|capability| {
            capability.domain == CapabilityDomain::Unknown
                || capability.execution == CapabilityExecution::Passthrough
        });

        // A mixed mutation cannot be split without changing its atomicity or
        // risking a supported write upstream. Reject it before any resolver is
        // invoked. Queries can safely combine an upstream read with local
        // overlay roots.
        let reject_mixed_mutation = operation_type == Some(OperationType::Mutation)
            && has_local_root
            && has_passthrough_root;

        let all_passthrough = !root_names.is_empty() && !has_local_root && has_passthrough_root;
        let product_original_query_passthrough =
            prepared.as_ref().is_some_and(|(document, _, _)| {
                document.operation_type == OperationType::Query
                    && capabilities
                        .iter()
                        .any(|capability| capability.domain == CapabilityDomain::Products)
                    && capabilities.iter().all(|capability| {
                        matches!(
                            capability.domain,
                            CapabilityDomain::Products | CapabilityDomain::Unknown
                        )
                    })
                    && self.product_read_needs_upstream(&document.root_fields)
            });
        let shop_original_query_passthrough =
            prepared.as_ref().is_some_and(|(document, variables, _)| {
                let Some(query) = selected_query.as_deref() else {
                    return false;
                };
                document.operation_type == OperationType::Query
                    && !document.root_fields.is_empty()
                    && document
                        .root_fields
                        .iter()
                        .all(|field| field.name == "shop")
                    && !self.should_handle_shop_policy_query_locally()
                    && !self.should_route_owner_metafields_read(query, variables)
            });
        let direct_full_query_passthrough = product_original_query_passthrough
            || (self.config.read_mode == ReadMode::LiveHybrid && shop_original_query_passthrough)
            || (self.config.read_mode == ReadMode::LiveHybrid
                && operation_type == Some(OperationType::Query)
                && ((!capabilities.is_empty()
                    && capabilities
                        .iter()
                        .all(|capability| capability.domain == CapabilityDomain::Events))
                    || (!root_names.is_empty()
                        && root_names.iter().all(|root| {
                            matches!(
                                root.as_str(),
                                "deliverySettings" | "deliveryPromiseSettings"
                            )
                        }))
                    || (capabilities
                        .iter()
                        .any(|capability| capability.domain == CapabilityDomain::AdminPlatform)
                        && capabilities
                            .iter()
                            .any(|capability| capability.domain == CapabilityDomain::Unknown)
                        && capabilities.iter().all(|capability| {
                            matches!(
                                capability.domain,
                                CapabilityDomain::AdminPlatform | CapabilityDomain::Unknown
                            )
                        }))));
        if let Some((document, _, _)) = prepared.as_ref() {
            if let Some(error) = required_variable_error(document, &graphql_request.variables) {
                return ok_json(json!({ "errors": [error] }));
            }
            if let Some(body) = product_create_argument_arity_error(document) {
                return ok_json(body);
            }
            if let Some(error) = directive_variable_mismatch_error(
                document,
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                &graphql_request.variables,
            ) {
                return ok_json(json!({ "errors": [error] }));
            }
            let id_errors = shopify_root_id_errors(
                version,
                document,
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                &graphql_request.variables,
            );
            if !id_errors.is_empty() {
                return ok_json(json!({ "errors": id_errors }));
            }
        }
        let root_locations = prepared
            .as_ref()
            .map(|(document, _, _)| {
                document
                    .root_fields
                    .iter()
                    .map(|field| (field.response_key.clone(), field.location))
                    .collect()
            })
            .unwrap_or_default();
        let discount_preflight = prepared.as_ref().and_then(|(document, _, _)| {
            (document.operation_type == OperationType::Mutation
                && capabilities
                    .iter()
                    .any(|capability| capability.domain == CapabilityDomain::Discounts))
            .then(|| (request.clone(), document.root_fields.clone()))
        });
        let owner_metafield_preflight = prepared.as_ref().and_then(|(document, variables, _)| {
            let query = selected_query.as_deref()?;
            (document.operation_type == OperationType::Query
                && has_local_root
                && !product_original_query_passthrough
                && self.should_route_owner_metafields_read(query, variables))
            .then(|| {
                (
                    request.clone(),
                    document.root_fields.clone(),
                    variables.clone(),
                )
            })
        });
        let mixed_discount_local_read = prepared.as_ref().is_some_and(|(document, _, _)| {
            document.operation_type == OperationType::Query
                && capabilities
                    .iter()
                    .all(|capability| capability.domain == CapabilityDomain::Discounts)
                && self.discount_query_has_mixed_local_read(&document.root_fields)
        });
        let overlay_query_preflight = prepared.as_ref().and_then(|(document, variables, _)| {
            if self.config.read_mode != ReadMode::LiveHybrid
                || document.operation_type != OperationType::Query
                || direct_full_query_passthrough
            {
                return None;
            }
            let b2b = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::B2b)
                && self.b2b_query_has_staged_match(&document.root_fields)
                && DraftProxy::b2b_query_has_catalog_root(&document.root_fields);
            let customers = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Customers)
                && self.should_handle_customer_overlay_read(&document.root_fields)
                && self.customer_overlay_needs_upstream_data(&document.root_fields);
            let functions = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Functions)
                && !self.function_read_has_local_overlay(&document.root_fields);
            let gift_cards = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::GiftCards)
                && self.gift_card_read_needs_upstream(&document.root_fields);
            let marketing = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Marketing);
            let media_saved_searches = document
                .root_fields
                .iter()
                .all(|field| matches!(field.name.as_str(), "files" | "fileSavedSearches"));
            let metaobjects = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Metaobjects);
            let discounts = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Discounts)
                && !mixed_discount_local_read
                && (self.should_forward_cold_discount_read(&document.root_fields)
                    || self.discount_read_needs_live_hydration(&document.root_fields));
            let orders = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Orders)
                && document
                    .root_fields
                    .iter()
                    .all(|field| matches!(field.name.as_str(), "order" | "orders" | "ordersCount"))
                && (document.root_fields.len() > 1
                    || self.order_query_needs_shared_upstream(&document.root_fields));
            let markets = capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Markets)
                && self.markets_should_fetch_upstream(&document.root_fields, variables);
            (b2b || customers
                || functions
                || gift_cards
                || marketing
                || media_saved_searches
                || metaobjects
                || discounts
                || orders
                || markets)
                .then(|| (request.clone(), document.root_fields.clone()))
        });
        let localization_context_preflight =
            prepared.as_ref().and_then(|(document, variables, _)| {
                let mixed_surface = capabilities
                    .iter()
                    .any(|capability| capability.domain == CapabilityDomain::Localization)
                    && capabilities
                        .iter()
                        .any(|capability| capability.domain == CapabilityDomain::Markets)
                    && capabilities.iter().all(|capability| {
                        matches!(
                            capability.domain,
                            CapabilityDomain::Localization | CapabilityDomain::Markets
                        )
                    });
                let has_local_localization_root = document.root_fields.iter().any(|field| {
                    matches!(
                        field.name.as_str(),
                        "translatableResource"
                            | "translatableResources"
                            | "translatableResourcesByIds"
                    ) && !self.localization_should_fetch_upstream(&field.name)
                });
                let has_locale_catalog = document
                    .root_fields
                    .iter()
                    .any(|field| matches!(field.name.as_str(), "shopLocales" | "availableLocales"));
                (self.config.read_mode == ReadMode::LiveHybrid
                    && document.operation_type == OperationType::Query
                    && mixed_surface
                    && has_local_localization_root
                    && self.markets_should_fetch_upstream(&document.root_fields, variables))
                .then(|| {
                    (
                        request.clone(),
                        document.root_fields.clone(),
                        has_locale_catalog,
                    )
                })
            });
        let markets_query_context = operation_type == Some(OperationType::Query)
            && !root_names.is_empty()
            && capabilities
                .iter()
                .all(|capability| capability.domain == CapabilityDomain::Markets);
        let node_query_preflight = prepared.as_ref().and_then(|(document, variables, _)| {
            let query = selected_query.as_ref()?;
            (document.operation_type == OperationType::Query
                && document.root_fields.len() > 1
                && document
                    .root_fields
                    .iter()
                    .all(|field| matches!(field.name.as_str(), "node" | "nodes")))
            .then(|| {
                (
                    request.clone(),
                    query.clone(),
                    variables.clone(),
                    document.root_fields.clone(),
                )
            })
        });
        let delivery_promise_mutation = prepared.as_ref().and_then(|(document, variables, _)| {
            let query = selected_query.as_ref()?;
            let promise_root_count = document
                .root_fields
                .iter()
                .filter(|field| {
                    matches!(
                        field.name.as_str(),
                        "deliveryPromiseProviderUpsert" | "deliveryPromiseParticipantsUpdate"
                    )
                })
                .count();
            (document.operation_type == OperationType::Mutation && promise_root_count > 1).then(
                || PreparedAtomicMutation {
                    request: request.clone(),
                    query: query.clone(),
                    variables: variables.clone(),
                },
            )
        });
        // `async-graphql`'s dynamic builder cannot register custom directive
        // definitions. Preserve Shopify's executable `@idempotent` contract in
        // the domain request, while removing only that directive from the copy
        // validated/executed by the engine. All other directives remain under
        // normal GraphQL validation.
        let engine_query = expand_bare_store_credit_origin_selections(
            &strip_idempotent_directives(&graphql_request.query),
        );
        let engine_variables = resolved_variables_json(&graphql_request.variables);
        let engine_operation_name = graphql_request.operation_name;
        let (
            engine_response,
            resolved_responses,
            resolved_extensions,
            full_passthrough_response,
            log_start,
        ) = with_request_owned_proxy(self, move |shared_proxy| {
            let resolved_responses = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
            let resolved_extensions = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
            let full_passthrough_response = Arc::new(std::sync::Mutex::new(None));
            let log_start = {
                let mut proxy = shared_proxy
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some((request, fields, variables)) = &owner_metafield_preflight {
                    proxy.hydrate_owner_metafield_read_fields(request, fields, variables);
                }
                proxy.request_mixed_discount_local_read = mixed_discount_local_read;
                if let Some((request, fields)) = &overlay_query_preflight {
                    let response = (proxy.upstream_transport)(request.clone());
                    if (200..300).contains(&response.status) {
                        proxy.hydrate_function_metadata_from_response_data(&response.body["data"]);
                        proxy.mark_function_read_fields_hydrated(fields);
                        proxy.hydrate_gift_card_read_state_from_response(
                            fields,
                            &response.body["data"],
                        );
                        proxy.observe_order_read_response(request, &response);
                        proxy.observe_discount_read_response(fields, &response);
                    }
                    proxy.request_upstream_query_response = Some(response);
                }
                if let Some((request, fields, use_original_request)) =
                    &localization_context_preflight
                {
                    proxy.preflight_localization_markets_context(
                        request,
                        fields,
                        *use_original_request,
                    );
                }
                if markets_query_context {
                    proxy.request_markets_query_preflighted = true;
                }
                if let Some((request, query, variables, fields)) = &node_query_preflight {
                    let mut outcomes = BTreeMap::new();
                    for field in fields {
                        outcomes.insert(
                            field.response_key.clone(),
                            proxy.resolve_node_query_fields(
                                request,
                                query,
                                variables,
                                fields,
                                &field.response_key,
                            ),
                        );
                    }
                    proxy.request_node_query_outcomes = Some(outcomes);
                }
                let log_start = proxy.log_entries.len();
                if operation_type == Some(OperationType::Mutation) && has_local_root {
                    proxy.engine_mutation_log_start = Some(log_start);
                }
                log_start
            };
            let root_executor: Arc<dyn RootFieldExecutor> = Arc::new(ProxyRootExecutor {
                proxy: Arc::clone(&shared_proxy),
                original_request: request.clone(),
                version,
                root_calls,
                root_locations,
                discount_preflight,
                discount_preflight_done: std::sync::Mutex::new(false),
                delivery_promise_mutation,
                delivery_promise_outcomes: std::sync::Mutex::new(None),
                full_passthrough_request: (all_passthrough || direct_full_query_passthrough)
                    .then(|| request.clone()),
                full_passthrough_direct: direct_full_query_passthrough,
                observe_direct_shop_passthrough: shop_original_query_passthrough,
                full_passthrough_response: Arc::clone(&full_passthrough_response),
                reject_mixed_mutation,
                resolved_responses: Arc::clone(&resolved_responses),
                resolved_extensions: Arc::clone(&resolved_extensions),
            });
            let mut engine_request = async_graphql::Request::new(engine_query)
                .variables(async_graphql::Variables::from_json(engine_variables))
                .data(RootExecutionContext {
                    executor: Arc::clone(&root_executor),
                });
            if let Some(operation_name) = engine_operation_name {
                engine_request = engine_request.operation_name(operation_name);
            }
            let engine_response = futures_executor::block_on(schema.execute(engine_request));
            drop(root_executor);
            let resolved_responses = resolved_responses
                .lock()
                .map(|responses| responses.clone())
                .unwrap_or_default();
            let resolved_extensions = resolved_extensions
                .lock()
                .map(|extensions| extensions.clone())
                .unwrap_or_default();
            let full_passthrough_response = full_passthrough_response
                .lock()
                .ok()
                .and_then(|response| response.clone());
            (
                engine_response,
                resolved_responses,
                resolved_extensions,
                full_passthrough_response,
                log_start,
            )
        });

        if operation_type == Some(OperationType::Mutation) && has_local_root {
            let variables = prepared
                .as_ref()
                .map(|(_, variables, _)| variables)
                .unwrap_or(&graphql_request.variables);
            self.normalize_engine_mutation_log(
                log_start,
                request,
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                variables,
                &root_names,
            );
        }

        if let Some(response) = full_passthrough_response {
            return response;
        }

        let authoritative_upstream_response =
            shared_root_response(&resolved_responses).filter(|response| {
                (200..300).contains(&response.status)
                    && response.body.get("errors").is_none()
                    && response.body.pointer("/extensions/cost").is_some()
            });
        let authoritative_passthrough_omission = authoritative_upstream_response.is_some()
            && engine_response.errors.iter().any(|error| {
                error
                    .message
                    .starts_with("Local resolver did not implement `")
                    || (error.message == "internal: non-null types require a return value"
                        && error.path.first().is_some_and(|segment| {
                            let async_graphql::PathSegment::Field(root) = segment else {
                                return false;
                            };
                            authoritative_upstream_response.is_some_and(|response| {
                                response
                                    .body
                                    .get("data")
                                    .and_then(Value::as_object)
                                    .is_some_and(|data| !data.contains_key(root))
                            })
                        }))
            });
        let body = if authoritative_passthrough_omission {
            // A read-through resolver can return Shopify's already-executed
            // response verbatim. Shopify occasionally omits selected roots or
            // nested fields from that response without reporting an error. Do
            // not reinterpret an otherwise successful, cost-bearing upstream
            // envelope as a local resolver failure.
            authoritative_upstream_response
                .map(|response| response.body.clone())
                .unwrap_or_else(|| json!({ "data": Value::Null }))
        } else {
            shopify_engine_response(
                engine_response,
                version,
                prepared.as_ref().map(|(document, _, _)| document),
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                prepared
                    .as_ref()
                    .map(|(_, variables, _)| variables)
                    .unwrap_or(&graphql_request.variables),
                &graphql_request.variable_input_orders,
            )
        };
        let mut body = body;
        restore_resolved_null_list_items(&mut body, &resolved_responses);
        let resolver_http_status = resolved_extensions
            .get(INTERNAL_HTTP_STATUS_EXTENSION)
            .and_then(Value::as_u64)
            .and_then(|status| u16::try_from(status).ok());
        strip_cloud_webhook_callback_urls(&mut body);
        merge_native_resolver_extensions(&mut body, &resolved_extensions);
        merge_resolved_extensions(&mut body, &resolved_responses);
        if let Some(response) = shared_root_response(&resolved_responses) {
            if let (Some(projected), Some(resolved)) =
                (body.as_object_mut(), response.body.as_object())
            {
                for (name, value) in resolved {
                    if !matches!(name.as_str(), "data" | "errors") {
                        projected
                            .entry(name.clone())
                            .or_insert_with(|| value.clone());
                    }
                }
            }
            return Response {
                status: resolver_http_status.unwrap_or(response.status),
                headers: response.headers.clone(),
                body,
            };
        }
        Response {
            status: resolver_http_status.unwrap_or(200),
            headers: BTreeMap::new(),
            body,
        }
    }

    fn normalize_engine_mutation_log(
        &mut self,
        log_start: usize,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_fields: &[String],
    ) {
        if log_start >= self.log_entries.len() {
            return;
        }
        let mut entries = self.log_entries.drain(log_start..).collect::<Vec<_>>();
        if entries.len() == 1 {
            let entry = &mut entries[0];
            entry["query"] = json!(query);
            entry["variables"] = resolved_variables_json(variables);
            entry["rawBody"] = json!(request.body.clone());
            entry["path"] = json!(request.path.clone());
            entry["interpreted"]["rootFields"] = json!(root_fields);
            self.log_entries.extend(entries);
            return;
        }

        let staged_resource_ids = entries
            .iter()
            .filter_map(|entry| entry.get("stagedResourceIds").and_then(Value::as_array))
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let status = if entries
            .iter()
            .any(|entry| entry.get("status") == Some(&json!("failed")))
        {
            "failed"
        } else if entries
            .iter()
            .any(|entry| entry.get("status") == Some(&json!("staged")))
        {
            "staged"
        } else {
            "proxied"
        };
        let primary_root = root_fields.first().cloned().unwrap_or_default();
        self.log_entries.push(json!({
            "id": format!("log-{}", log_start + 1),
            "operationName": null,
            "path": request.path,
            "query": query,
            "variables": resolved_variables_json(variables),
            "rawBody": request.body,
            "stagedResourceIds": staged_resource_ids,
            "status": status,
            "interpreted": {
                "operationType": "mutation",
                "rootFields": root_fields,
                "primaryRootField": primary_root,
                "execution": "schema-resolvers"
            },
            "notes": "Executed serially as one validated GraphQL mutation operation."
        }));
    }

    pub(in crate::proxy) fn execute_passthrough_graphql(&mut self, request: &Request) -> Response {
        self.resolve_registered_graphql(request, None, None)
    }

    pub(in crate::proxy) fn resolve_nested_graphql_request(
        &mut self,
        request: &Request,
    ) -> Response {
        self.resolve_registered_graphql(request, None, None)
    }

    fn resolve_prevalidated_graphql_root_call(&mut self, call: &PreparedRootCall) -> Response {
        self.resolve_registered_graphql(&call.request, Some(call), None)
    }

    fn resolve_registered_graphql(
        &mut self,
        request: &Request,
        prepared: Option<&PreparedRootCall>,
        preferred_root: Option<&str>,
    ) -> Response {
        let (request, query, variables, operation, root_field_name) = if let Some(call) = prepared {
            (
                &call.request,
                call.query.clone(),
                call.variables.clone(),
                call.operation.clone(),
                call.field.name.clone(),
            )
        } else {
            let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
                return json_error(400, "Expected JSON body with a string `query`");
            };
            let raw_query = graphql_request.query;
            let requested_operation_name = graphql_request.operation_name.as_deref();

            let selection = match selected_operation(&raw_query, requested_operation_name) {
                Ok(selection) => selection,
                Err(error) => return operation_selection_error_response(error),
            };
            let query = if selection.requires_filtered_document {
                match selected_operation_query(&raw_query, requested_operation_name) {
                    Ok(query) => query,
                    Err(error) => return operation_selection_error_response(error),
                }
            } else {
                raw_query
            };
            let variables =
                match variables_with_operation_defaults(&query, &graphql_request.variables, None) {
                    Ok(variables) => variables,
                    Err(error) => return operation_selection_error_response(error),
                };

            let Some(operation) = parse_operation_with_variables(&query, &variables) else {
                return json_error(400, "Could not parse GraphQL operation");
            };
            let Some(root_field) = preferred_root
                .map(str::to_string)
                .or_else(|| operation.primary_root_field().map(str::to_string))
            else {
                return ok_json(json!({ "data": {} }));
            };
            (request, query, variables, operation, root_field)
        };
        let root_field = root_field_name.as_str();

        if operation.root_fields.len() > 1
            && operation.operation_type == OperationType::Query
            && self.should_route_owner_metafields_read(&query, &variables)
        {
            return self.owner_metafields_read_response(request, &query, &variables);
        }

        let capability = self.registry.resolve(operation.operation_type, root_field);
        if capability.domain == CapabilityDomain::Products
            && operation.operation_type == OperationType::Mutation
            && self
                .execution_root_fields(&query, &variables)
                .as_deref()
                .is_some_and(product_root_fields_select_shop_currency_money)
        {
            self.hydrate_shop_pricing_state_if_missing(request, true, false);
        }

        let Some(registration) = self
            .registry
            .registration(operation.operation_type, root_field)
        else {
            return self.dispatch_unknown_passthrough_or_legacy_error(
                request,
                &query,
                &variables,
                operation.operation_type,
                &operation.root_fields,
                root_field,
            );
        };
        let handler = registration.handler;
        let Some(version) = AdminApiVersion::from_route(&request.path) else {
            return json_error(404, "No captured Admin GraphQL schema for this route");
        };
        let response_key = prepared
            .map(|call| call.field.response_key.as_str())
            .unwrap_or(root_field);
        let compatibility_root_field = prepared
            .is_none()
            .then(|| {
                root_fields(&query, &variables)
                    .and_then(|fields| fields.into_iter().find(|field| field.name == root_field))
            })
            .flatten();
        let arguments = prepared
            .map(|call| {
                call.field
                    .arguments
                    .iter()
                    .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                    .collect()
            })
            .or_else(|| {
                compatibility_root_field.as_ref().map(|field| {
                    field
                        .arguments
                        .iter()
                        .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                        .collect()
                })
            })
            .unwrap_or_default();
        let root_metadata = prepared
            .map(|call| (call.field.directives.clone(), call.field.location))
            .or_else(|| {
                compatibility_root_field
                    .as_ref()
                    .map(|field| (field.directives.clone(), field.location))
            });
        let (directives, root_location) =
            root_metadata.unwrap_or_else(|| (Vec::new(), SourceLocation { line: 1, column: 1 }));
        let outcome = handler(
            self,
            crate::resolver_registry::RootInvocation {
                api_surface: ApiSurface::Admin,
                api_version: GraphqlApiVersion::Admin(version),
                response_key,
                root_name: root_field,
                root_location,
                directives,
                arguments,
                request,
                query: &query,
                variables: &variables,
                operation: &operation,
                mode: LocalResolverMode::from_execution(registration.execution),
            },
        );
        resolver_outcome_compat_response(self, request, &query, &variables, response_key, outcome)
    }
}

fn resolver_outcome_compat_response(
    proxy: &mut DraftProxy,
    request: &Request,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    response_key: &str,
    outcome: ResolverOutcome<Value>,
) -> Response {
    let ResolverOutcome {
        value,
        errors,
        extensions,
        log_drafts,
    } = outcome;
    for draft in log_drafts {
        proxy.record_mutation_log_draft(request, query, variables, draft);
    }
    let mut body = json!({ "data": { response_key: value } });
    if !errors.is_empty() {
        body["errors"] = Value::Array(
            errors
                .into_iter()
                .map(|error| {
                    let mut value = json!({ "message": error.message });
                    if !error.extensions.is_empty() {
                        value["extensions"] = Value::Object(error.extensions.into_iter().collect());
                    }
                    value
                })
                .collect(),
        );
    }
    if !extensions.is_empty() {
        body["extensions"] = Value::Object(extensions.into_iter().collect());
    }
    ok_json(body)
}

pub(in crate::proxy) fn resolver_outcome_from_response(
    response: Response,
    response_key: &str,
) -> ResolverOutcome<Value> {
    let status = response.status;
    let body = response.body;
    let value = body
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get(response_key))
        .cloned()
        .unwrap_or(Value::Null);
    let errors = root_field_errors_from_json_with_default_path(
        body.get("errors")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        response_key,
        false,
    );
    let extensions = body
        .get("extensions")
        .and_then(Value::as_object)
        .map(|extensions| {
            extensions
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut outcome = ResolverOutcome {
        value,
        errors,
        extensions,
        log_drafts: Vec::new(),
    };
    if outcome.errors.is_empty() && status >= 400 {
        outcome.errors.push(RootFieldError {
            message: format!("GraphQL root failed with status {status}"),
            extensions: BTreeMap::new(),
            path: Some(Vec::new()),
            locations: Vec::new(),
        });
    }
    if status != 200 {
        outcome
            .extensions
            .insert(INTERNAL_HTTP_STATUS_EXTENSION.to_string(), json!(status));
    }
    outcome
}

/// GraphQL-level result of one upstream transport call. Domain hydration may
/// inspect `data`, but HTTP status/headers remain confined to this boundary.
pub(in crate::proxy) struct UpstreamGraphqlResult {
    pub outcome: ResolverOutcome<Value>,
    pub data: Value,
    pub transport_succeeded: bool,
}

fn upstream_graphql_result(response: Response, response_key: &str) -> UpstreamGraphqlResult {
    let data = response.body.get("data").cloned().unwrap_or(Value::Null);
    let transport_succeeded = response.status < 400;
    UpstreamGraphqlResult {
        outcome: resolver_outcome_from_response(response, response_key),
        data,
        transport_succeeded,
    }
}

/// Translate Shopify-compatible JSON error objects into the engine's typed
/// resolver errors. This is deliberately part of the error-compatibility
/// boundary; domain values and transport responses do not cross it.
pub(in crate::proxy) fn root_field_errors_from_json(
    errors: &[Value],
    response_key: &str,
) -> Vec<RootFieldError> {
    root_field_errors_from_json_with_default_path(errors, response_key, false)
}

pub(in crate::proxy) fn graphql_error_outcome(
    errors: Vec<Value>,
    response_key: &str,
) -> ResolverOutcome<Value> {
    ResolverOutcome::value(Value::Null)
        .with_errors(root_field_errors_from_json(&errors, response_key))
}

pub(in crate::proxy) fn resolver_http_error_outcome(
    status: u16,
    message: impl Into<String>,
) -> ResolverOutcome<Value> {
    let mut outcome = ResolverOutcome::error(message);
    outcome
        .extensions
        .insert(INTERNAL_HTTP_STATUS_EXTENSION.to_string(), json!(status));
    outcome
}

fn root_field_errors_from_json_with_default_path(
    errors: &[Value],
    response_key: &str,
    default_root_path: bool,
) -> Vec<RootFieldError> {
    errors
        .iter()
        .filter_map(|error| {
            let error_path = error.get("path").and_then(Value::as_array);
            let root_path_index = error_path.and_then(|path| {
                path.iter()
                    .position(|segment| segment.as_str() == Some(response_key))
            });
            if error_path.is_some() && root_path_index.is_none() {
                return None;
            }
            Some(RootFieldError {
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("GraphQL root resolver failed")
                    .to_string(),
                extensions: error
                    .get("extensions")
                    .and_then(Value::as_object)
                    .map(|extensions| {
                        extensions
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                path: error_path
                    .map(|path| {
                        path.iter()
                            .skip(root_path_index.unwrap_or(0) + 1)
                            .filter_map(|segment| match segment {
                                Value::String(field) => {
                                    Some(async_graphql::PathSegment::Field(field.clone()))
                                }
                                Value::Number(index) => index
                                    .as_u64()
                                    .map(|index| async_graphql::PathSegment::Index(index as usize)),
                                _ => None,
                            })
                            .collect()
                    })
                    // Legacy HTTP/dispatcher failures historically gained the
                    // active root path at the engine boundary. Preserve that
                    // behavior until those fail-closed roots become native
                    // GraphQL outcomes.
                    .or_else(|| default_root_path.then(Vec::new)),
                locations: error
                    .get("locations")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|location| {
                        Some(async_graphql::Pos {
                            line: location.get("line")?.as_u64()? as usize,
                            column: location.get("column")?.as_u64()? as usize,
                        })
                    })
                    .collect(),
            })
        })
        .collect()
}

impl DraftProxy {
    /// Forward one cold locally-registered read at the transport boundary and
    /// expose only its GraphQL result to the domain resolver. Domain code must
    /// not traffic in the proxy's HTTP `Response` type.
    pub(in crate::proxy) fn forward_upstream_root_outcome(
        &self,
        request: &Request,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        resolver_outcome_from_response((self.upstream_transport)(request.clone()), response_key)
    }

    /// Reuse the request-scoped upstream response when the executor already
    /// fetched the complete operation; otherwise perform the cold read once.
    pub(in crate::proxy) fn cached_or_forward_upstream_root_outcome(
        &self,
        request: &Request,
        response_key: &str,
    ) -> ResolverOutcome<Value> {
        let response = self
            .request_upstream_query_response
            .clone()
            .unwrap_or_else(|| (self.upstream_transport)(request.clone()));
        resolver_outcome_from_response(response, response_key)
    }

    /// Decode a request-scoped upstream response once while also exposing its
    /// GraphQL `data` object to store hydration code.
    pub(in crate::proxy) fn cached_or_forward_upstream_graphql_result(
        &self,
        request: &Request,
        response_key: &str,
    ) -> UpstreamGraphqlResult {
        let response = self
            .request_upstream_query_response
            .clone()
            .unwrap_or_else(|| (self.upstream_transport)(request.clone()));
        upstream_graphql_result(response, response_key)
    }
}

fn resolver_outcome_wire_response(
    response_key: &str,
    value: &Value,
    errors: &[RootFieldError],
    extensions: &BTreeMap<String, Value>,
) -> Response {
    let mut body = json!({ "data": { response_key: value.clone() } });
    if !errors.is_empty() {
        body["errors"] = Value::Array(
            errors
                .iter()
                .map(|error| {
                    let mut value = json!({ "message": error.message });
                    if !error.extensions.is_empty() {
                        value["extensions"] =
                            Value::Object(error.extensions.clone().into_iter().collect());
                    }
                    value
                })
                .collect(),
        );
    }
    let public_extensions = extensions
        .iter()
        .filter(|(name, _)| name.as_str() != INTERNAL_HTTP_STATUS_EXTENSION)
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    if !public_extensions.is_empty() {
        body["extensions"] = Value::Object(public_extensions);
    }
    ok_json(body)
}

fn merge_resolved_extensions(body: &mut Value, responses: &BTreeMap<String, Response>) {
    let Some(body_object) = body.as_object_mut() else {
        return;
    };
    for response in responses.values() {
        let Some(source) = response.body.get("extensions").and_then(Value::as_object) else {
            continue;
        };
        let target = body_object.entry("extensions").or_insert_with(|| json!({}));
        let Some(target) = target.as_object_mut() else {
            continue;
        };
        for (name, value) in source {
            match (target.get_mut(name), value) {
                (Some(Value::Array(existing)), Value::Array(additional)) => {
                    for item in additional {
                        if !existing.contains(item) {
                            existing.push(item.clone());
                        }
                    }
                }
                (Some(_), _) => {}
                (None, _) => {
                    target.insert(name.clone(), value.clone());
                }
            }
        }
    }
}

fn merge_native_resolver_extensions(body: &mut Value, extensions: &BTreeMap<String, Value>) {
    if extensions.is_empty() {
        return;
    }
    let Some(body) = body.as_object_mut() else {
        return;
    };
    let target = body.entry("extensions").or_insert_with(|| json!({}));
    let Some(target) = target.as_object_mut() else {
        return;
    };
    for (name, value) in extensions {
        if name == INTERNAL_HTTP_STATUS_EXTENSION {
            continue;
        }
        target.entry(name.clone()).or_insert_with(|| value.clone());
    }
}

fn strip_cloud_webhook_callback_urls(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                strip_cloud_webhook_callback_urls(value);
            }
        }
        Value::Object(object) => {
            let cloud_endpoint = object.get("endpoint").is_some_and(|endpoint| {
                matches!(
                    endpoint.get("__typename").and_then(Value::as_str),
                    Some("WebhookPubSubEndpoint" | "WebhookEventBridgeEndpoint")
                ) || endpoint.get("pubSubProject").is_some()
                    || endpoint.get("pubSubTopic").is_some()
                    || endpoint.get("arn").is_some()
            });
            if cloud_endpoint {
                // Shopify omits the deprecated non-null callbackUrl field for
                // cloud webhook destinations. The local record carries a
                // placeholder only long enough for GraphQL non-null execution;
                // it must not escape in the wire response.
                object.remove("callbackUrl");
            }
            for value in object.values_mut() {
                strip_cloud_webhook_callback_urls(value);
            }
        }
        _ => {}
    }
}

fn restore_resolved_null_list_items(
    projected_body: &mut Value,
    resolved_responses: &BTreeMap<String, Response>,
) {
    let Some(projected_data) = projected_body
        .get_mut("data")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for (response_key, response) in resolved_responses {
        let Some(projected) = projected_data.get_mut(response_key) else {
            continue;
        };
        let Some(resolved) = response
            .body
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get(response_key))
        else {
            continue;
        };
        restore_null_list_items(projected, resolved);
    }
}

fn restore_null_list_items(projected: &mut Value, resolved: &Value) {
    match (projected, resolved) {
        (Value::Array(projected), Value::Array(resolved)) => {
            for (projected, resolved) in projected.iter_mut().zip(resolved) {
                if resolved.is_null() {
                    *projected = Value::Null;
                } else {
                    restore_null_list_items(projected, resolved);
                }
            }
        }
        (Value::Object(projected), Value::Object(resolved)) => {
            for (field_name, resolved) in resolved {
                if let Some(projected) = projected.get_mut(field_name) {
                    restore_null_list_items(projected, resolved);
                }
            }
        }
        _ => {}
    }
}

fn strip_idempotent_directives(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut output = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'#' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'"' if bytes.get(index..index + 3) == Some(b"\"\"\"") => {
                index += 3;
                while index < bytes.len() {
                    if bytes.get(index..index + 3) == Some(b"\"\"\"") {
                        index += 3;
                        break;
                    }
                    index += 1;
                }
            }
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'@' if bytes.get(index + 1..index + 11) == Some(b"idempotent")
                && bytes
                    .get(index + 11)
                    .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_') =>
            {
                let start = index;
                index += 11;
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                if bytes.get(index) == Some(&b'(') {
                    let mut depth = 0usize;
                    while index < bytes.len() {
                        match bytes[index] {
                            b'"' => {
                                index += 1;
                                while index < bytes.len() {
                                    match bytes[index] {
                                        b'\\' => index = (index + 2).min(bytes.len()),
                                        b'"' => {
                                            index += 1;
                                            break;
                                        }
                                        _ => index += 1,
                                    }
                                }
                            }
                            b'(' => {
                                depth += 1;
                                index += 1;
                            }
                            b')' => {
                                depth = depth.saturating_sub(1);
                                index += 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => index += 1,
                        }
                    }
                }
                for byte in &mut output[start..index] {
                    if !matches!(*byte, b'\n' | b'\r') {
                        *byte = b' ';
                    }
                }
            }
            _ => index += 1,
        }
    }
    blank_unused_idempotency_key_definition(&mut output);
    // Every replaced byte is ASCII and untouched spans retain their original
    // UTF-8, so this conversion cannot fail.
    String::from_utf8(output).expect("directive stripping should preserve UTF-8")
}

/// Shopify accepts a bare `origin` selection on store-credit transactions even
/// though introspection exposes `StoreCreditAccountTransactionOrigin` as a
/// union. Captured responses currently return `null` for that selection. Keep
/// the executable schema honest for ordinary union selections, but add the
/// smallest valid selection to the engine-only document for this observed
/// Shopify exception. Domain handlers and response cleanup still use the
/// caller's original document, so the synthetic `__typename` never leaks.
fn expand_bare_store_credit_origin_selections(query: &str) -> String {
    if !(query.contains("storeCreditAccountCredit")
        || query.contains("storeCreditAccountDebit")
        || query.contains("StoreCreditAccountTransaction")
        || query.contains("StoreCreditAccountCreditTransaction")
        || query.contains("StoreCreditAccountDebitTransaction"))
    {
        return query.to_string();
    }

    let bytes = query.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'#' => {
                let start = index;
                index += 1;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
            }
            b'"' if bytes.get(index..index + 3) == Some(b"\"\"\"") => {
                let start = index;
                index += 3;
                while index < bytes.len() {
                    if bytes.get(index..index + 3) == Some(b"\"\"\"") {
                        index += 3;
                        break;
                    }
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
            }
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                output.extend_from_slice(&bytes[start..index]);
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
                if &bytes[start..index] != b"origin" {
                    continue;
                }
                let mut next = index;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if bytes.get(next).is_some_and(|next| {
                    matches!(*next, b'}' | b',' | b'.')
                        || next.is_ascii_alphabetic()
                        || *next == b'_'
                }) {
                    output.extend_from_slice(b" { __typename }");
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).expect("store-credit query expansion should preserve UTF-8")
}

fn blank_unused_idempotency_key_definition(output: &mut [u8]) {
    const VARIABLE: &[u8] = b"$idempotencyKey";
    let positions = output
        .windows(VARIABLE.len())
        .enumerate()
        .filter_map(|(index, candidate)| (candidate == VARIABLE).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != 1 {
        return;
    }
    let start = positions[0];
    let mut end = start + VARIABLE.len();
    while end < output.len() && !matches!(output[end], b',' | b')') {
        end += 1;
    }
    for byte in &mut output[start..end] {
        if !matches!(*byte, b'\n' | b'\r') {
            *byte = b' ';
        }
    }
}

fn single_root_transport_query(
    operation_type: OperationType,
    field: &RootFieldSelection,
    variable_definitions: &BTreeMap<String, VariableDefinitionInfo>,
) -> String {
    let variable_definitions = serialize_used_variable_definitions(field, variable_definitions);
    format!(
        "{}{} {{ {} }}",
        operation_type.keyword(),
        variable_definitions,
        serialize_root_field(field)
    )
}

fn serialize_used_variable_definitions(
    field: &RootFieldSelection,
    variable_definitions: &BTreeMap<String, VariableDefinitionInfo>,
) -> String {
    let mut used_variables = std::collections::BTreeSet::new();
    for value in field.raw_arguments.values() {
        collect_raw_argument_variables(value, &mut used_variables);
    }
    for directive in &field.raw_directives {
        for value in directive.raw_arguments.values() {
            collect_raw_argument_variables(value, &mut used_variables);
        }
    }
    if used_variables.is_empty() {
        return String::new();
    }

    let definitions = used_variables
        .iter()
        .filter_map(|name| {
            variable_definitions
                .get(name.as_str())
                .map(|definition| format!("${}: {}", definition.name, definition.type_display))
        })
        .collect::<Vec<_>>();
    if definitions.is_empty() {
        String::new()
    } else {
        format!("({})", definitions.join(", "))
    }
}

fn collect_raw_argument_variables(
    value: &RawArgumentValue,
    variables: &mut std::collections::BTreeSet<String>,
) {
    match value {
        RawArgumentValue::List(values) => {
            for value in values {
                collect_raw_argument_variables(value, variables);
            }
        }
        RawArgumentValue::Object(fields) => {
            for value in fields.values() {
                collect_raw_argument_variables(value, variables);
            }
        }
        RawArgumentValue::Variable { name, .. } => {
            variables.insert(name.clone());
        }
        RawArgumentValue::String(_)
        | RawArgumentValue::Int(_)
        | RawArgumentValue::Float(_)
        | RawArgumentValue::Bool(_)
        | RawArgumentValue::Null
        | RawArgumentValue::Enum(_) => {}
    }
}

fn serialize_root_field(field: &RootFieldSelection) -> String {
    let mut output = String::new();
    if field.response_key != field.name {
        output.push_str(&field.response_key);
        output.push_str(": ");
    }
    output.push_str(&field.name);
    output.push_str(&serialize_raw_arguments(&field.raw_arguments));
    if field.raw_directives.is_empty() {
        for directive in &field.directives {
            output.push_str(" @");
            output.push_str(directive);
        }
    } else {
        for directive in &field.raw_directives {
            output.push_str(&serialize_raw_directive(directive));
        }
    }
    output.push_str(&serialize_selection_set(&field.selection));
    output
}

fn serialize_raw_directive(directive: &DirectiveSelection) -> String {
    format!(
        " @{}{}",
        directive.name,
        serialize_raw_arguments(&directive.raw_arguments)
    )
}

fn serialize_selected_field(field: &SelectedField) -> String {
    let mut output = String::new();
    if field.response_key != field.name {
        output.push_str(&field.response_key);
        output.push_str(": ");
    }
    output.push_str(&field.name);
    output.push_str(&serialize_resolved_arguments(&field.arguments));
    output.push_str(&serialize_selection_set(&field.selection));

    match field.type_condition.as_deref() {
        Some(type_condition) => format!("... on {type_condition} {{ {output} }}"),
        None => output,
    }
}

fn serialize_selection_set(selection: &[SelectedField]) -> String {
    if selection.is_empty() {
        return String::new();
    }
    format!(
        " {{ {} }}",
        selection
            .iter()
            .map(serialize_selected_field)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn serialize_raw_arguments(arguments: &BTreeMap<String, RawArgumentValue>) -> String {
    if arguments.is_empty() {
        return String::new();
    }
    format!(
        "({})",
        arguments
            .iter()
            .map(|(name, value)| format!("{name}: {}", serialize_raw_argument_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn serialize_resolved_arguments(arguments: &BTreeMap<String, ResolvedValue>) -> String {
    if arguments.is_empty() {
        return String::new();
    }
    format!(
        "({})",
        arguments
            .iter()
            .map(|(name, value)| format!("{name}: {}", serialize_resolved_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn serialize_raw_argument_value(value: &RawArgumentValue) -> String {
    match value {
        RawArgumentValue::String(value) => quote_graphql_string(value),
        RawArgumentValue::Int(value) => value.to_string(),
        RawArgumentValue::Float(value) => value.to_string(),
        RawArgumentValue::Bool(value) => value.to_string(),
        RawArgumentValue::Null => "null".to_string(),
        RawArgumentValue::Enum(value) => value.clone(),
        RawArgumentValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(serialize_raw_argument_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RawArgumentValue::Object(fields) => serialize_raw_object(fields),
        RawArgumentValue::Variable { name, .. } => format!("${name}"),
    }
}

fn serialize_raw_object(fields: &BTreeMap<String, RawArgumentValue>) -> String {
    format!(
        "{{ {} }}",
        fields
            .iter()
            .map(|(name, value)| format!("{name}: {}", serialize_raw_argument_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn serialize_resolved_value(value: &ResolvedValue) -> String {
    match value {
        ResolvedValue::String(value) => quote_graphql_string(value),
        ResolvedValue::Int(value) => value.to_string(),
        ResolvedValue::Float(value) => value.to_string(),
        ResolvedValue::Bool(value) => value.to_string(),
        ResolvedValue::Null => "null".to_string(),
        ResolvedValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(serialize_resolved_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ResolvedValue::Object(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", serialize_resolved_value(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn quote_graphql_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod graphql_runtime_tests {
    use super::{expand_bare_store_credit_origin_selections, with_request_owned_proxy};
    use crate::operation_registry::{default_executable_registry, default_registry};
    use crate::proxy::{Config, DraftProxy};
    use crate::resolver_registry::ResolverRegistry;

    #[test]
    fn registered_roots_use_their_direct_callbacks() {
        let registry = ResolverRegistry::new(default_registry());
        let executable = default_executable_registry();
        for registration in registry.local_resolvers() {
            let declared = executable
                .iter()
                .find(|declared| {
                    declared.entry.api_surface == registration.api_surface
                        && declared.entry.operation_type == registration.operation_type
                        && declared.entry.name == registration.graphql_root_name
                })
                .expect("local root should have a direct executable declaration");
            assert!(std::ptr::fn_addr_eq(
                declared
                    .handler
                    .expect("implemented root should carry its direct callback"),
                registration.handler
            ));
        }
    }

    #[test]
    fn expands_only_bare_store_credit_origin_fields_for_engine_validation() {
        let query = r#"
            mutation StoreCredit {
              storeCreditAccountCredit(id: "gid://shopify/Customer/1", creditInput: { creditAmount: { amount: "1", currencyCode: USD } }) {
                storeCreditAccountTransaction {
                  origin
                  account { id }
                }
              }
            }
        "#;
        let expanded = expand_bare_store_credit_origin_selections(query);
        assert!(expanded.contains("origin { __typename }"));
        assert!(expanded.contains("account { id }"));

        let selected = query.replace("origin\n", "origin { __typename }\n");
        assert_eq!(
            expand_bare_store_credit_origin_selections(&selected),
            selected
        );

        let node_query =
            "query { nodes(ids: []) { ... on StoreCreditAccountCreditTransaction { origin } } }";
        assert!(expand_bare_store_credit_origin_selections(node_query)
            .contains("origin { __typename }"));
    }

    #[test]
    fn does_not_rewrite_origin_inside_inputs_strings_or_other_operations() {
        let store_credit = r#"
            mutation StoreCredit($input: ExampleInput = { origin: "origin" }) {
              storeCreditAccountDebit(id: "gid://shopify/StoreCreditAccount/1", debitInput: { debitAmount: { amount: "1", currencyCode: USD } }) {
                userErrors { message }
              }
            }
        "#;
        assert_eq!(
            expand_bare_store_credit_origin_selections(store_credit),
            store_credit
        );
        let unrelated = "query Inventory { inventoryTransfers(first: 1) { nodes { origin } } }";
        assert_eq!(
            expand_bare_store_credit_origin_selections(unrelated),
            unrelated
        );
    }

    #[test]
    fn request_owned_proxy_restores_latest_state_after_unwind() {
        let mut proxy = DraftProxy::new(Config::default());
        proxy.next_synthetic_id = 17;

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_request_owned_proxy(&mut proxy, |shared| {
                shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .next_synthetic_id = 29;
                panic!("resolver panic");
            });
        }));

        assert!(outcome.is_err());
        assert_eq!(proxy.next_synthetic_id, 29);
    }

    #[test]
    fn request_owned_proxy_restores_state_when_a_reference_is_retained() {
        let mut proxy = DraftProxy::new(Config::default());
        proxy.next_synthetic_id = 31;

        let retained = with_request_owned_proxy(&mut proxy, |shared| {
            shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .next_synthetic_id = 43;
            shared
        });

        assert_eq!(proxy.next_synthetic_id, 43);
        drop(retained);
    }
}
