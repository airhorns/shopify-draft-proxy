//! Storefront request bridge for the independent executable Storefront schema.

use super::graphql_error_compat::shopify_storefront_engine_response;
use super::graphql_runtime::with_request_owned_proxy;
use super::storefront::{
    storefront_discovery_argument_error, storefront_request_context,
    StorefrontCustomerAuthLogDetails, STOREFRONT_CART_MUTATION_ROOTS,
    STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS,
};
use super::*;
use crate::admin_graphql::{
    apply_null_list_item_paths, FieldResolverInvocation, FieldResolverResult, ResolverValueSource,
    RootExecutionContext, RootFieldError, RootFieldExecutor, RootFieldInvocation, RootFieldResult,
};
use crate::graphql::{ParsedOperation, VariableDefinitionInfo};
use crate::resolver_registry::FieldResolverImplementation;
use crate::resolver_registry::GraphqlApiVersion;
use crate::storefront_graphql::{self, StorefrontApiVersion};
use graphql_parser::query::{parse_query, Definition, OperationDefinition};
use graphql_parser::Style;

#[derive(Debug, Clone)]
struct PreparedStorefrontRootCall {
    request: Request,
    query: String,
    variables: BTreeMap<String, ResolvedValue>,
    operation: ParsedOperation,
    operation_path: String,
    variable_definitions: BTreeMap<String, VariableDefinitionInfo>,
    field: RootFieldSelection,
}

#[derive(Debug, Clone)]
struct StorefrontCustomerAuthLogContext {
    variables: BTreeMap<String, ResolvedValue>,
    operation_type: OperationType,
    root_fields: Vec<String>,
    execution: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorefrontExecutionMode {
    LocalRead,
    LocalStage,
    Passthrough,
    SnapshotQuery,
    SnapshotMutation,
}

struct StorefrontRootExecutor {
    proxy: Arc<std::sync::Mutex<DraftProxy>>,
    version: StorefrontApiVersion,
    mode: StorefrontExecutionMode,
    calls: BTreeMap<String, PreparedStorefrontRootCall>,
    original_request: Request,
    logged: std::sync::Mutex<bool>,
    passthrough_response: Arc<std::sync::Mutex<Option<Response>>>,
}

impl StorefrontRootExecutor {
    fn customer_auth_log_context(&self) -> Option<StorefrontCustomerAuthLogContext> {
        if self.calls.is_empty() {
            return None;
        }
        let first = self.calls.values().next()?;
        let is_customer_auth_call =
            |call: &PreparedStorefrontRootCall| match call.operation.operation_type {
                OperationType::Query => call.field.name == "customer",
                OperationType::Mutation => {
                    STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS.contains(&call.field.name.as_str())
                }
                OperationType::Subscription => false,
            };
        if !self.calls.values().any(is_customer_auth_call) {
            return None;
        }
        let root_fields = self
            .calls
            .values()
            .map(|call| call.field.name.clone())
            .collect::<Vec<_>>();
        let execution = match first.operation.operation_type {
            OperationType::Mutation => "stage-locally",
            _ => "overlay-read",
        };
        Some(StorefrontCustomerAuthLogContext {
            variables: first.variables.clone(),
            operation_type: first.operation.operation_type,
            root_fields,
            execution,
        })
    }

    fn record_execution_once(&self) -> Result<(), String> {
        let mut logged = self
            .logged
            .lock()
            .map_err(|_| "Storefront GraphQL log lock was poisoned".to_string())?;
        if *logged {
            return Ok(());
        }
        if let Some(context) = self.customer_auth_log_context() {
            self.proxy
                .lock()
                .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?
                .record_storefront_customer_auth_log_entry(
                    &self.original_request,
                    &context.variables,
                    context.operation_type,
                    &context.root_fields,
                    StorefrontCustomerAuthLogDetails {
                        status: "handled",
                        execution: context.execution,
                        notes: "Storefront customer-auth roots were resolved locally without Shopify writes or email delivery.",
                    },
                );
            *logged = true;
            return Ok(());
        }
        let (status, execution, notes) = match self.mode {
            StorefrontExecutionMode::LocalRead => (
                "handled",
                "overlay-read",
                "Storefront roots were resolved locally from shared proxy store state.",
            ),
            StorefrontExecutionMode::LocalStage => (
                "handled",
                "stage-locally",
                "Storefront roots were staged locally from shared proxy store state.",
            ),
            StorefrontExecutionMode::Passthrough => (
                "proxied",
                "passthrough",
                "Storefront API traffic was forwarded through the Storefront transport.",
            ),
            StorefrontExecutionMode::SnapshotQuery | StorefrontExecutionMode::SnapshotMutation => (
                "rejected",
                "passthrough",
                "Storefront API traffic had no implemented local snapshot resolver.",
            ),
        };
        self.proxy
            .lock()
            .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?
            .record_storefront_log_entry(&self.original_request, status, execution, notes);
        *logged = true;
        Ok(())
    }

    fn execute_local_root(
        &self,
        response_key: &str,
        root_name: &str,
        arguments: BTreeMap<String, Value>,
        requested_field_paths: BTreeSet<Vec<String>>,
    ) -> Result<RootFieldResult, String> {
        let mut call = self.calls.get(response_key).cloned().ok_or_else(|| {
            format!("No request-scoped Storefront resolver input was prepared for `{root_name}`")
        })?;
        call.field.arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        let mut proxy = self
            .proxy
            .lock()
            .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?;
        let registration = proxy
            .registry
            .registration_for_surface(
                ApiSurface::Storefront,
                call.operation.operation_type,
                root_name,
            )
            .cloned()
            .ok_or_else(|| format!("Storefront root `{root_name}` has no local registration"))?;
        let handler = registration.handler;
        let operation_roots = self
            .calls
            .values()
            .map(
                |prepared| crate::resolver_registry::OperationRootInvocation {
                    name: prepared.field.name.clone(),
                    response_key: prepared.field.response_key.clone(),
                    arguments: if prepared.field.response_key == response_key {
                        arguments.clone()
                    } else {
                        prepared
                            .field
                            .arguments
                            .iter()
                            .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                            .collect()
                    },
                },
            )
            .collect();
        let outcome = handler(
            &mut proxy,
            RootInvocation {
                api_surface: ApiSurface::Storefront,
                api_version: GraphqlApiVersion::Storefront(self.version),
                response_key,
                root_name,
                root_location: call.field.location,
                directives: call.field.directives.clone(),
                operation_path: &call.operation_path,
                operation_root_names: call.operation.root_fields.clone(),
                operation_roots,
                variable_definitions: &call.variable_definitions,
                raw_arguments: call.field.raw_arguments.clone(),
                arguments,
                requested_field_paths,
                upstream_value: None,
                request: &call.request,
                query: &call.query,
                variables: &call.variables,
                operation: &call.operation,
                mode: LocalResolverMode::from_execution(registration.execution),
            },
        );
        for draft in outcome.log_drafts {
            proxy.record_mutation_log_draft(&call.request, &call.query, &call.variables, draft);
        }
        Ok(RootFieldResult {
            value: outcome.value,
            errors: outcome.errors,
            value_source: outcome.value_source,
        })
    }

    fn execute_passthrough_root(
        &self,
        response_key: &str,
        root_name: &str,
    ) -> Result<RootFieldResult, String> {
        let mut cached = self
            .passthrough_response
            .lock()
            .map_err(|_| "Storefront GraphQL passthrough lock was poisoned".to_string())?;
        if cached.is_none() {
            let proxy = self
                .proxy
                .lock()
                .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?;
            *cached = Some((proxy.storefront_upstream_transport)(
                self.original_request.clone(),
            ));
        }
        Ok(storefront_root_result(
            cached
                .as_ref()
                .expect("Storefront passthrough response should be cached")
                .clone(),
            response_key,
            root_name,
        ))
    }

    fn execute_snapshot_root(
        &self,
        response_key: &str,
        root_name: &str,
        arguments: BTreeMap<String, Value>,
        requested_field_paths: BTreeSet<Vec<String>>,
    ) -> Result<RootFieldResult, String> {
        let has_local_registration = self
            .proxy
            .lock()
            .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?
            .registry
            .registration_for_surface(ApiSurface::Storefront, OperationType::Query, root_name)
            .is_some();
        if has_local_registration {
            return self.execute_local_root(
                response_key,
                root_name,
                arguments,
                requested_field_paths,
            );
        }
        let call = self.calls.get(response_key).ok_or_else(|| {
            format!("No Storefront snapshot selection was prepared for `{root_name}`")
        })?;
        let value = self
            .proxy
            .lock()
            .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?
            .storefront_snapshot_root_value(&call.field, Some(self.version));
        Ok(RootFieldResult {
            value,
            errors: Vec::new(),
            value_source: ResolverValueSource::Local,
        })
    }
}

impl RootFieldExecutor for StorefrontRootExecutor {
    fn execute_root(&self, invocation: RootFieldInvocation) -> Result<RootFieldResult, String> {
        self.record_execution_once()?;
        let RootFieldInvocation {
            response_key,
            root_name,
            arguments,
            requested_field_paths,
        } = invocation;
        match self.mode {
            StorefrontExecutionMode::LocalRead | StorefrontExecutionMode::LocalStage => {
                self.execute_local_root(&response_key, &root_name, arguments, requested_field_paths)
            }
            StorefrontExecutionMode::Passthrough => {
                self.execute_passthrough_root(&response_key, &root_name)
            }
            StorefrontExecutionMode::SnapshotQuery => self.execute_snapshot_root(
                &response_key,
                &root_name,
                arguments,
                requested_field_paths,
            ),
            StorefrontExecutionMode::SnapshotMutation => Ok(RootFieldResult::default()),
        }
    }

    fn execute_field(
        &self,
        invocation: FieldResolverInvocation<'_>,
    ) -> Result<FieldResolverResult, String> {
        let mut proxy = self
            .proxy
            .lock()
            .map_err(|_| "Storefront GraphQL proxy state lock was poisoned".to_string())?;
        let implementation = proxy.registry.field_implementation(
            invocation.api_surface,
            invocation.api_version,
            &invocation.parent_type,
            &invocation.field_name,
        );
        match implementation {
            FieldResolverImplementation::PropertyBacked => Ok(FieldResolverResult::PropertyBacked),
            FieldResolverImplementation::Explicit(handler) => {
                let value = handler(&mut proxy, &self.original_request, &invocation)?;
                Ok(FieldResolverResult::Resolved(value))
            }
            FieldResolverImplementation::DeliberatelyUnsupported(reason) => Ok(
                FieldResolverResult::DeliberatelyUnsupported(reason.to_string()),
            ),
        }
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn execute_storefront_graphql(&mut self, request: &Request) -> Response {
        self.execution_session = ExecutionSession::default();
        let Some(graphql_request) = parse_graphql_request_body(&request.body) else {
            return json_error(400, "Expected JSON body with a string `query`");
        };
        let Some(version) = StorefrontApiVersion::from_route(&request.path) else {
            return self.execute_legacy_storefront_graphql(request, &graphql_request);
        };
        self.execution_session = ExecutionSession::storefront(version);
        let schema = match storefront_graphql::schema(version) {
            Ok(schema) => schema,
            Err(error) => {
                return json_error(
                    500,
                    &format!("Could not initialize Storefront GraphQL {version}: {error}"),
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
            let calls = document
                .root_fields
                .iter()
                .map(|field| {
                    (
                        field.response_key.clone(),
                        PreparedStorefrontRootCall {
                            request: request.clone(),
                            query: query.to_string(),
                            variables: variables.clone(),
                            operation: ParsedOperation {
                                operation_type: document.operation_type,
                                root_fields: vec![field.name.clone()],
                            },
                            operation_path: document.operation_path.clone(),
                            variable_definitions: document.variable_definitions.clone(),
                            field: field.clone(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            Some((document, variables, calls))
        });
        let operation_type = prepared
            .as_ref()
            .map(|(document, _, _)| document.operation_type);
        let capabilities = prepared
            .as_ref()
            .map(|(document, _, _)| {
                document
                    .root_fields
                    .iter()
                    .map(|field| {
                        self.registry.resolve_for_surface(
                            ApiSurface::Storefront,
                            document.operation_type,
                            &field.name,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some((_, variables, _)) = prepared.as_ref() {
            let context = storefront_request_context(
                selected_query.as_deref().unwrap_or(&graphql_request.query),
                variables,
            );
            if context.invalid_buyer_token(self) {
                return ok_json(json!({
                    "errors": [{ "message": "The token provided is not valid" }]
                }));
            }
        }
        if let Some((field, (message, extensions))) =
            prepared.as_ref().and_then(|(document, _, _)| {
                document.root_fields.iter().find_map(|field| {
                    storefront_discovery_argument_error(field).map(|error| (field, error))
                })
            })
        {
            self.record_storefront_log_entry(
                request,
                "rejected",
                "overlay-read",
                "Storefront discovery arguments failed captured Shopify validation.",
            );
            let error = json!({
                "message": message,
                "path": [field.response_key],
                "extensions": extensions
            });
            if field.name == "predictiveSearch" {
                let mut data = serde_json::Map::new();
                data.insert(field.response_key.clone(), Value::Null);
                return ok_json(json!({ "data": Value::Object(data), "errors": [error] }));
            }
            return ok_json(json!({ "errors": [error] }));
        }
        let all_local = !capabilities.is_empty()
            && capabilities.iter().all(|capability| {
                capability.api_surface == ApiSurface::Storefront
                    && capability.domain == CapabilityDomain::Storefront
                    && capability.execution != CapabilityExecution::Passthrough
            })
            && prepared
                .as_ref()
                .is_some_and(|(document, _, _)| match document.operation_type {
                    OperationType::Query => document.root_fields.iter().all(|field| {
                        self.storefront_query_root_is_local(&field.name, &field.arguments)
                    }),
                    OperationType::Mutation => document
                        .root_fields
                        .iter()
                        .all(|field| self.storefront_mutation_root_is_local(&field.name)),
                    OperationType::Subscription => false,
                });
        let has_customer_auth_mutation = prepared.as_ref().is_some_and(|(document, _, _)| {
            document.operation_type == OperationType::Mutation
                && document.root_fields.iter().any(|field| {
                    STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS.contains(&field.name.as_str())
                })
        });
        let has_cart_mutation = prepared.as_ref().is_some_and(|(document, _, _)| {
            document.operation_type == OperationType::Mutation
                && document
                    .root_fields
                    .iter()
                    .any(|field| STOREFRONT_CART_MUTATION_ROOTS.contains(&field.name.as_str()))
        });
        if (has_customer_auth_mutation || has_cart_mutation) && !all_local {
            let (document, variables, _) = prepared
                .as_ref()
                .expect("prepared document should exist for mixed local Storefront rejection");
            if has_customer_auth_mutation {
                self.record_storefront_customer_auth_log_entry(
                    request,
                    variables,
                    document.operation_type,
                    &document
                        .root_fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>(),
                    StorefrontCustomerAuthLogDetails {
                        status: "rejected",
                        execution: "stage-locally",
                        notes: "Storefront customer-auth mutations cannot be mixed with unsupported Storefront roots.",
                    },
                );
            } else {
                self.record_storefront_log_entry(
                    request,
                    "rejected",
                    "stage-locally",
                    "Storefront cart mutations cannot be mixed with unsupported Storefront roots.",
                );
            }
            let message = if has_customer_auth_mutation {
                "Storefront customer-auth mutations cannot be mixed with unsupported Storefront roots"
            } else {
                "Storefront cart mutations cannot be mixed with unsupported Storefront roots"
            };
            return json_error(400, message);
        }
        let mode = match (self.config.read_mode.clone(), operation_type, all_local) {
            (ReadMode::Snapshot, Some(OperationType::Mutation), true) => {
                StorefrontExecutionMode::LocalStage
            }
            (ReadMode::Snapshot, Some(OperationType::Mutation), false) => {
                StorefrontExecutionMode::SnapshotMutation
            }
            (ReadMode::Snapshot, _, true) => StorefrontExecutionMode::LocalRead,
            (ReadMode::Snapshot, _, false) => StorefrontExecutionMode::SnapshotQuery,
            (ReadMode::LiveHybrid, Some(OperationType::Query), true) => {
                StorefrontExecutionMode::LocalRead
            }
            (ReadMode::LiveHybrid, Some(OperationType::Mutation), true) => {
                StorefrontExecutionMode::LocalStage
            }
            _ => StorefrontExecutionMode::Passthrough,
        };

        let engine_query = storefront_engine_query(&graphql_request.query);
        let engine_variables = resolved_variables_json(&graphql_request.variables);
        let engine_operation_name = graphql_request.operation_name;
        let calls = prepared
            .as_ref()
            .map(|(_, _, calls)| calls.clone())
            .unwrap_or_default();
        let passthrough_response = Arc::new(std::sync::Mutex::new(None));
        let passthrough_for_executor = Arc::clone(&passthrough_response);
        let null_list_item_paths = Arc::new(std::sync::Mutex::new(Vec::new()));
        let null_list_item_paths_for_engine = Arc::clone(&null_list_item_paths);
        let engine_response = with_request_owned_proxy(self, move |shared_proxy| {
            let executor: Arc<dyn RootFieldExecutor> = Arc::new(StorefrontRootExecutor {
                proxy: shared_proxy,
                version,
                mode,
                calls,
                original_request: request.clone(),
                logged: std::sync::Mutex::new(false),
                passthrough_response: passthrough_for_executor,
            });
            let mut engine_request = async_graphql::Request::new(engine_query)
                .variables(async_graphql::Variables::from_json(engine_variables))
                .data(RootExecutionContext::with_null_list_item_paths(
                    executor,
                    null_list_item_paths_for_engine,
                ));
            if let Some(operation_name) = engine_operation_name {
                engine_request = engine_request.operation_name(operation_name);
            }
            futures_executor::block_on(schema.execute(engine_request))
        });

        if let Some(response) = passthrough_response
            .lock()
            .ok()
            .and_then(|response| response.clone())
        {
            return response;
        }
        let validation_failed = engine_response
            .errors
            .iter()
            .any(|error| error.path.is_empty());
        if mode == StorefrontExecutionMode::SnapshotMutation && !validation_failed {
            self.record_storefront_log_entry(
                request,
                "rejected",
                "passthrough",
                "Storefront API mutations are not locally implemented in snapshot mode.",
            );
            return json_error(
                501,
                "Storefront API mutations are not locally implemented in snapshot mode",
            );
        }

        let mut body = shopify_storefront_engine_response(
            engine_response,
            prepared.as_ref().map(|(document, _, _)| document),
            selected_query.as_deref().unwrap_or(&graphql_request.query),
        );
        if let Ok(paths) = null_list_item_paths.lock() {
            apply_null_list_item_paths(&mut body, &paths);
        }
        ok_json(body)
    }

    fn execute_legacy_storefront_graphql(
        &mut self,
        request: &Request,
        graphql_request: &GraphqlRequestBody,
    ) -> Response {
        if self.config.read_mode != ReadMode::Snapshot {
            self.record_storefront_log_entry(
                request,
                "proxied",
                "passthrough",
                "Storefront route has no captured executable schema for this version.",
            );
            return (self.storefront_upstream_transport)(request.clone());
        }
        let variables = variables_with_operation_defaults(
            &graphql_request.query,
            &graphql_request.variables,
            None,
        )
        .unwrap_or_else(|_| graphql_request.variables.clone());
        self.storefront_snapshot_graphql_response(&graphql_request.query, &variables, None)
    }
}

fn storefront_root_result(
    response: Response,
    response_key: &str,
    root_name: &str,
) -> RootFieldResult {
    let value = response
        .body
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get(response_key))
        .cloned()
        .unwrap_or(Value::Null);
    let mut errors = response
        .body
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|error| {
            let path = error.get("path").and_then(Value::as_array);
            if path
                .and_then(|path| path.first())
                .and_then(Value::as_str)
                .is_some_and(|root| root != response_key)
            {
                return None;
            }
            Some(RootFieldError {
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Storefront GraphQL root resolver failed")
                    .to_string(),
                extensions: error
                    .get("extensions")
                    .and_then(Value::as_object)
                    .map(|extensions| {
                        extensions
                            .iter()
                            .map(|(name, value)| (name.clone(), value.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                path: path.map(|path| {
                    path.iter()
                        .skip(1)
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
                }),
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
        .collect::<Vec<_>>();
    if errors.is_empty() && response.status >= 400 {
        errors.push(RootFieldError {
            message: format!(
                "Storefront GraphQL root `{root_name}` failed with status {}",
                response.status
            ),
            extensions: BTreeMap::new(),
            path: Some(Vec::new()),
            locations: Vec::new(),
        });
    }
    RootFieldResult {
        value,
        errors,
        value_source: ResolverValueSource::Upstream,
    }
}

/// `async-graphql` dynamic schemas cannot register executable custom directive
/// definitions. Storefront's `@inContext` values are consumed by the domain
/// resolver from the original document, so remove that one directive from the
/// engine-only copy and remove variables that were used only by it. Every
/// other directive remains under normal engine validation.
fn storefront_engine_query(query: &str) -> String {
    let Ok(mut document) = parse_query::<String>(query) else {
        return query.to_string();
    };
    for definition in &mut document.definitions {
        let Definition::Operation(operation) = definition else {
            continue;
        };
        match operation {
            OperationDefinition::SelectionSet(_) => {}
            OperationDefinition::Query(query) => query
                .directives
                .retain(|directive| directive.name != "inContext"),
            OperationDefinition::Mutation(mutation) => mutation
                .directives
                .retain(|directive| directive.name != "inContext"),
            OperationDefinition::Subscription(subscription) => subscription
                .directives
                .retain(|directive| directive.name != "inContext"),
        }
    }
    let without_context = document.format(&Style::default());
    for definition in &mut document.definitions {
        let Definition::Operation(operation) = definition else {
            continue;
        };
        let definitions = match operation {
            OperationDefinition::SelectionSet(_) => continue,
            OperationDefinition::Query(query) => &mut query.variable_definitions,
            OperationDefinition::Mutation(mutation) => &mut mutation.variable_definitions,
            OperationDefinition::Subscription(subscription) => {
                &mut subscription.variable_definitions
            }
        };
        definitions
            .retain(|definition| variable_occurrences(&without_context, &definition.name) > 1);
    }
    document.format(&Style::default())
}

fn variable_occurrences(query: &str, variable_name: &str) -> usize {
    let needle = format!("${variable_name}");
    query
        .match_indices(&needle)
        .filter(|(index, _)| {
            query
                .as_bytes()
                .get(index + needle.len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::storefront_engine_query;

    #[test]
    fn engine_copy_removes_in_context_and_its_exclusive_variables() {
        let query = r#"
            query Context($country: CountryCode, $language: LanguageCode, $include: Boolean!)
            @inContext(country: $country, language: $language) {
              shop { name @include(if: $include) }
            }
        "#;
        let engine_query = storefront_engine_query(query);
        assert!(!engine_query.contains("@inContext"));
        assert!(!engine_query.contains("$country"));
        assert!(!engine_query.contains("$language"));
        assert!(engine_query.contains("$include"));
        assert!(engine_query.contains("@include"));
    }
}
