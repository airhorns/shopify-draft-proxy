//! Request-scoped execution bridge between the versioned GraphQL engine and
//! domain-owned root resolvers.

use super::graphql_error_compat::{
    directive_variable_mismatch_error, product_create_argument_arity_error,
    required_variable_error, shopify_engine_response, shopify_root_id_errors,
};
use super::*;
use crate::admin_graphql::{
    self, AdminApiVersion, RootExecutionContext, RootFieldError, RootFieldExecutor,
    RootFieldInvocation, RootFieldResult,
};
use crate::graphql::{DirectiveSelection, ParsedDocument, VariableDefinitionInfo};
use crate::resolver_registry::{LocalResolverMode, ResolverHandler, RootResolverContext};

struct ProxyRootExecutor {
    proxy: Arc<std::sync::Mutex<DraftProxy>>,
    root_calls: BTreeMap<String, PreparedRootCall>,
    root_locations: BTreeMap<String, SourceLocation>,
    discount_preflight: Option<(Request, Vec<RootFieldSelection>)>,
    discount_preflight_done: std::sync::Mutex<bool>,
    grouped_local_request: Option<Request>,
    grouped_local_fields: Option<Vec<RootFieldSelection>>,
    grouped_local_response: std::sync::Mutex<Option<Response>>,
    full_passthrough_request: Option<Request>,
    full_passthrough_response: Arc<std::sync::Mutex<Option<Response>>>,
    reject_mixed_mutation: bool,
    resolved_responses: Arc<std::sync::Mutex<BTreeMap<String, Response>>>,
}

#[derive(Debug, Clone)]
struct PreparedRootCall {
    request: Request,
    query: String,
    variables: BTreeMap<String, ResolvedValue>,
    operation: crate::graphql::ParsedOperation,
    field: RootFieldSelection,
}

pub(crate) fn resolver_handler_for_domain(domain: CapabilityDomain) -> ResolverHandler {
    match domain {
        CapabilityDomain::Products => DraftProxy::resolve_products_graphql,
        CapabilityDomain::Orders => DraftProxy::resolve_orders_graphql,
        CapabilityDomain::ShippingFulfillments => DraftProxy::resolve_shipping_fulfillments_graphql,
        CapabilityDomain::Customers => DraftProxy::resolve_customers_graphql,
        CapabilityDomain::B2b => DraftProxy::resolve_b2b_graphql,
        CapabilityDomain::SavedSearches => DraftProxy::resolve_saved_searches_graphql,
        CapabilityDomain::OnlineStore => DraftProxy::resolve_online_store_graphql,
        CapabilityDomain::Metaobjects => DraftProxy::resolve_metaobjects_graphql,
        CapabilityDomain::BulkOperations => DraftProxy::resolve_bulk_operations_graphql,
        CapabilityDomain::Discounts => DraftProxy::resolve_discounts_graphql,
        CapabilityDomain::GiftCards => DraftProxy::resolve_gift_cards_graphql,
        CapabilityDomain::AdminPlatform => DraftProxy::resolve_admin_platform_graphql,
        CapabilityDomain::Apps => DraftProxy::resolve_apps_graphql,
        CapabilityDomain::Media => DraftProxy::resolve_media_graphql,
        CapabilityDomain::StoreProperties => DraftProxy::resolve_store_properties_graphql,
        CapabilityDomain::Events => DraftProxy::resolve_events_graphql,
        CapabilityDomain::Functions => DraftProxy::resolve_functions_graphql,
        CapabilityDomain::Payments => DraftProxy::resolve_payments_graphql,
        CapabilityDomain::Marketing => DraftProxy::resolve_marketing_graphql,
        CapabilityDomain::Privacy => DraftProxy::resolve_privacy_graphql,
        CapabilityDomain::Segments => DraftProxy::resolve_segments_graphql,
        CapabilityDomain::Webhooks => DraftProxy::resolve_webhooks_graphql,
        CapabilityDomain::Localization => DraftProxy::resolve_localization_graphql,
        CapabilityDomain::Markets => DraftProxy::resolve_markets_graphql,
        CapabilityDomain::Metafields => DraftProxy::resolve_metafields_graphql,
        CapabilityDomain::Unknown => {
            panic!("unknown GraphQL capabilities cannot register local resolvers")
        }
    }
}

/// Temporarily make this proxy available to `'static` GraphQL resolver data
/// without risking replacement of the caller-owned instance. The normal path
/// moves the proxy back out of the request Arc. Exceptional paths clone the
/// latest guarded value before resuming the unwind, so `self` is never left as
/// the fresh placeholder used during execution.
fn with_request_owned_proxy<T>(
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
    restored_proxy.engine_root_fields = None;
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
                *cached = Some(proxy.execute_passthrough_graphql(request));
            }
            cached
                .as_ref()
                .expect("passthrough response should be cached")
                .clone()
        } else if let Some(request) = &self.grouped_local_request {
            let mut cached = self
                .grouped_local_response
                .lock()
                .map_err(|_| "Admin GraphQL grouped response lock was poisoned".to_string())?;
            if cached.is_none() {
                let mut proxy = self
                    .proxy
                    .lock()
                    .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
                proxy.engine_root_fields = self.grouped_local_fields.clone();
                *cached = Some(proxy.resolve_prevalidated_graphql_root(request));
                proxy.engine_root_fields = None;
            }
            cached
                .as_ref()
                .expect("grouped local response should be cached")
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
            let mut proxy = self
                .proxy
                .lock()
                .map_err(|_| "Admin GraphQL proxy state lock was poisoned".to_string())?;
            proxy.engine_root_fields = Some(vec![call.field.clone()]);
            let response = proxy.resolve_prevalidated_graphql_root_call(&call);
            proxy.engine_root_fields = None;
            response
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
        self.engine_root_fields
            .clone()
            .or_else(|| root_fields(query, variables))
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

    pub(in crate::proxy) fn finalize_mutation_outcome(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        outcome: MutationOutcome,
    ) -> Response {
        for draft in outcome.log_drafts {
            self.record_mutation_log_draft(request, query, variables, draft);
        }
        outcome.response
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
            "operationName": null,
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

    pub(in crate::proxy) fn unimplemented_resolver_response(
        mode: LocalResolverMode,
        root_field: &str,
    ) -> Response {
        unimplemented_root_response(mode.registry_name(), root_field)
    }

    /// Execute an Admin GraphQL request through the captured versioned schema.
    /// Domain code is reached only through root field resolvers; the GraphQL
    /// engine owns the executable language and response projection.
    pub(in crate::proxy) fn execute_graphql(&mut self, request: &Request) -> Response {
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
                                "query": field_query,
                                "variables": resolved_variables_json(&variables)
                            })
                            .to_string(),
                        }
                    };
                    (
                        field.response_key.clone(),
                        PreparedRootCall {
                            request: field_request,
                            // Domain execution receives the caller's selected operation for
                            // diagnostics and mutation logging. Only `request` carries the
                            // isolated transport document used when this root must go upstream.
                            query: query.to_string(),
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
        let grouped_local_request = prepared.as_ref().and_then(|(document, variables, _)| {
            selected_query.as_deref().and_then(|query| {
                let owner_metafields = document.operation_type == OperationType::Query
                    && self.should_route_owner_metafields_read(query, variables);
                let grouped_domain_read = document.operation_type == OperationType::Query
                    && document.root_fields.len() > 1
                    && capabilities.first().is_some_and(|first| {
                        first.domain != CapabilityDomain::Unknown
                            && first.execution == CapabilityExecution::OverlayRead
                            && capabilities.iter().all(|capability| capability == first)
                    });
                let grouped_media_saved_search_read =
                    document.operation_type == OperationType::Query
                        && document.root_fields.len() > 1
                        && document.root_fields.iter().all(|field| {
                            matches!(field.name.as_str(), "files" | "fileSavedSearches")
                        });
                let grouped_localization_markets_read = document.operation_type
                    == OperationType::Query
                    && document.root_fields.len() > 1
                    && capabilities.first().is_some_and(|capability| {
                        capability.domain == CapabilityDomain::Localization
                            && capability.execution == CapabilityExecution::OverlayRead
                    })
                    && document
                        .root_fields
                        .iter()
                        .any(|field| field.name == "markets")
                    && capabilities.iter().all(|capability| {
                        capability.execution == CapabilityExecution::OverlayRead
                            && matches!(
                                capability.domain,
                                CapabilityDomain::Localization | CapabilityDomain::Markets
                            )
                    });
                let grouped_admin_platform_read = document.operation_type == OperationType::Query
                    && document.root_fields.len() > 1
                    && capabilities
                        .iter()
                        .any(|capability| capability.domain == CapabilityDomain::AdminPlatform)
                    && capabilities.iter().all(|capability| {
                        matches!(
                            capability.domain,
                            CapabilityDomain::AdminPlatform | CapabilityDomain::Unknown
                        )
                    });
                let grouped_product_helper_read = document.operation_type == OperationType::Query
                    && document.root_fields.len() > 1
                    && capabilities
                        .iter()
                        .any(|capability| capability.domain == CapabilityDomain::Products)
                    && capabilities.iter().all(|capability| {
                        matches!(
                            capability.domain,
                            CapabilityDomain::Products
                                | CapabilityDomain::SavedSearches
                                | CapabilityDomain::Unknown
                        )
                    });
                let live_events = self.config.read_mode == ReadMode::LiveHybrid
                    && !capabilities.is_empty()
                    && capabilities
                        .iter()
                        .all(|capability| capability.domain == CapabilityDomain::Events);
                (owner_metafields
                    || grouped_domain_read
                    || grouped_media_saved_search_read
                    || grouped_localization_markets_read
                    || grouped_admin_platform_read
                    || grouped_product_helper_read
                    || live_events)
                    .then(|| request.clone())
            })
        });
        let grouped_local_fields = grouped_local_request.as_ref().and_then(|_| {
            prepared
                .as_ref()
                .map(|(document, _, _)| document.root_fields.clone())
        });
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
        let (engine_response, resolved_responses, full_passthrough_response, log_start) =
            with_request_owned_proxy(self, move |shared_proxy| {
                let log_start = {
                    let mut proxy = shared_proxy
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let log_start = proxy.log_entries.len();
                    if operation_type == Some(OperationType::Mutation) && has_local_root {
                        proxy.engine_mutation_log_start = Some(log_start);
                    }
                    log_start
                };
                let resolved_responses = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
                let full_passthrough_response = Arc::new(std::sync::Mutex::new(None));
                let root_executor: Arc<dyn RootFieldExecutor> = Arc::new(ProxyRootExecutor {
                    proxy: Arc::clone(&shared_proxy),
                    root_calls,
                    root_locations,
                    discount_preflight,
                    discount_preflight_done: std::sync::Mutex::new(false),
                    grouped_local_request,
                    grouped_local_fields,
                    grouped_local_response: std::sync::Mutex::new(None),
                    full_passthrough_request: all_passthrough.then(|| request.clone()),
                    full_passthrough_response: Arc::clone(&full_passthrough_response),
                    reject_mixed_mutation,
                    resolved_responses: Arc::clone(&resolved_responses),
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
                let full_passthrough_response = full_passthrough_response
                    .lock()
                    .ok()
                    .and_then(|response| response.clone());
                (
                    engine_response,
                    resolved_responses,
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
        } else if engine_response.errors.iter().any(|error| {
            (error.message.contains("expected \"FieldValue::WithType\"")
                && (error.message.contains("invalid value for interface")
                    || error.message.contains("invalid value for union")))
                || error
                    .message
                    .contains("\"null\" is not of the expected type")
        }) {
            // async-graphql's dynamic API cannot represent a null list element
            // whose item type is an interface/union: `FieldValue::NULL` is
            // rejected because abstract values normally require `with_type`.
            // The request has already passed full engine validation. Preserve
            // the correctly projected resolver payload for this narrow library
            // limitation so `nodes(ids:)` can retain null placeholders.
            let mut body = merge_resolved_root_responses(&resolved_responses);
            if let Some((document, _, _)) = prepared.as_ref() {
                strip_unselected_typenames_from_response(&mut body, document);
            }
            body
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
        strip_cloud_webhook_callback_urls(&mut body);
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
                status: response.status,
                headers: response.headers.clone(),
                body,
            };
        }
        ok_json(body)
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
        self.resolve_registered_graphql(request, None)
    }

    pub(in crate::proxy) fn resolve_prevalidated_graphql_root(
        &mut self,
        request: &Request,
    ) -> Response {
        self.resolve_registered_graphql(request, None)
    }

    pub(in crate::proxy) fn resolve_nested_graphql_request(
        &mut self,
        request: &Request,
    ) -> Response {
        let outer_fields = self.engine_root_fields.take();
        let response = self.resolve_registered_graphql(request, None);
        self.engine_root_fields = outer_fields;
        response
    }

    fn resolve_prevalidated_graphql_root_call(&mut self, call: &PreparedRootCall) -> Response {
        self.resolve_registered_graphql(&call.request, Some(call))
    }

    fn resolve_registered_graphql(
        &mut self,
        request: &Request,
        prepared: Option<&PreparedRootCall>,
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
            let Some(root_field) = operation.primary_root_field().map(str::to_string) else {
                return ok_json(json!({ "data": {} }));
            };
            (request, query, variables, operation, root_field)
        };
        let root_field = root_field_name.as_str();

        if operation.root_fields.len() > 1
            && operation.operation_type == OperationType::Query
            && self.should_route_owner_metafields_read(&query, &variables)
        {
            return self.owner_metafields_read(request, &query, &variables);
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
        (registration.handler)(
            self,
            RootResolverContext {
                request,
                query: &query,
                variables: &variables,
                operation: &operation,
                root_name: root_field,
                mode: LocalResolverMode::from_execution(registration.execution),
            },
        )
    }
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

fn merge_resolved_root_responses(responses: &BTreeMap<String, Response>) -> Value {
    let mut data = serde_json::Map::new();
    let mut errors = Vec::new();
    for response in responses.values() {
        if let Some(response_data) = response.body.get("data").and_then(Value::as_object) {
            data.extend(response_data.clone());
        }
        if let Some(response_errors) = response.body.get("errors").and_then(Value::as_array) {
            errors.extend(response_errors.iter().cloned());
        }
    }
    let mut body = serde_json::Map::new();
    body.insert("data".to_string(), Value::Object(data));
    if !errors.is_empty() {
        body.insert("errors".to_string(), Value::Array(errors));
    }
    Value::Object(body)
}

fn strip_unselected_typenames_from_response(body: &mut Value, document: &ParsedDocument) {
    let Some(data) = body.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    for field in &document.root_fields {
        if let Some(value) = data.get_mut(&field.response_key) {
            strip_unselected_typenames(value, &field.selection);
        }
    }
}

fn strip_unselected_typenames(value: &mut Value, selection: &[SelectedField]) {
    if let Some(values) = value.as_array_mut() {
        for value in values {
            strip_unselected_typenames(value, selection);
        }
        return;
    }
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if !selection.iter().any(|field| field.name == "__typename") {
        object.remove("__typename");
    }
    for field in selection {
        if field.selection.is_empty() {
            continue;
        }
        if let Some(value) = object.get_mut(&field.response_key) {
            strip_unselected_typenames(value, &field.selection);
        }
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
    use super::{
        expand_bare_store_credit_origin_selections, resolver_handler_for_domain,
        with_request_owned_proxy,
    };
    use crate::operation_registry::{default_registry, CapabilityDomain};
    use crate::proxy::{Config, DraftProxy};
    use crate::resolver_registry::{ResolverHandler, ResolverRegistry};

    #[test]
    fn every_registered_domain_uses_one_distinct_callback() {
        let registry = ResolverRegistry::new(default_registry());
        let mut domain_handlers: Vec<(CapabilityDomain, ResolverHandler)> = Vec::new();

        for registration in registry.local_resolvers() {
            assert!(std::ptr::fn_addr_eq(
                registration.handler,
                resolver_handler_for_domain(registration.domain),
            ));
            if let Some((_, handler)) = domain_handlers
                .iter()
                .find(|(domain, _)| *domain == registration.domain)
            {
                assert!(std::ptr::fn_addr_eq(*handler, registration.handler));
                continue;
            }
            for (other_domain, other_handler) in &domain_handlers {
                assert!(
                    !std::ptr::fn_addr_eq(*other_handler, registration.handler),
                    "{other_domain:?} and {:?} unexpectedly share a resolver callback",
                    registration.domain
                );
            }
            domain_handlers.push((registration.domain, registration.handler));
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
