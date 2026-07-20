use super::*;
use crate::proxy::graphql_runtime::serialize_resolved_value;

pub(in crate::proxy) fn localization_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    vec![
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "TranslatableResource",
            "translatableContent",
            translatable_resource_content_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "TranslatableResource",
            "translations",
            translatable_resource_translations_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "TranslatableResource",
            "nestedTranslatableResources",
            translatable_resource_nested_resources_field,
        ),
        FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "MarketLocalizableResource",
            "marketLocalizations",
            market_localizable_resource_localizations_field,
        ),
    ]
}

fn market_localizable_resource_localizations_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let resource_id = invocation
        .parent
        .get("resourceId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let market_id = resolved_string_field(&arguments, "marketId");
    Ok(
        proxy.market_localizable_resource(resource_id, market_id.as_deref())["marketLocalizations"]
            .clone(),
    )
}

fn translatable_resource_id(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> String {
    invocation
        .parent
        .get("resourceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn translatable_resource_content_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let resource_id = translatable_resource_id(invocation);
    Ok(Value::Array(
        proxy.localization_translatable_content(&resource_id),
    ))
}

fn translatable_resource_translations_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let locale = resolved_string_field(&arguments, "locale");
    let market_id = resolved_string_field(&arguments, "marketId");
    let outdated = resolved_bool_field(&arguments, "outdated");
    let resource_id = translatable_resource_id(invocation);
    Ok(Value::Array(proxy.localization_translations_for(
        &resource_id,
        locale.as_deref(),
        market_id.as_deref(),
        outdated,
    )))
}

fn translatable_resource_nested_resources_field(
    _proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    Ok(invocation
        .parent
        .get("nestedTranslatableResources")
        .map(|connection| seeded_connection_value(connection, &arguments))
        .unwrap_or_else(|| connection_json(Vec::new())))
}

/// Engine-coerced input for one localization mutation root. Selection and
/// transport metadata stay at the GraphQL executor boundary.
struct LocalizationMutationInput {
    name: String,
    arguments: BTreeMap<String, ResolvedValue>,
}

fn localization_mutation_target_ids(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    let mut ids = Vec::new();
    match root_name {
        "shopLocaleEnable" => {
            ids.extend(resolved_string_list_arg(arguments, "marketWebPresenceIds"));
        }
        "shopLocaleUpdate" => {
            let input = resolved_object_field(arguments, "shopLocale").unwrap_or_default();
            ids.extend(resolved_string_list_field_unsorted(
                &input,
                "marketWebPresenceIds",
            ));
        }
        "translationsRegister" => {
            for translation in resolved_list_arg(arguments, "translations") {
                if let Some(market_id) = resolved_object_string(&translation, "marketId") {
                    ids.push(market_id);
                }
            }
        }
        "translationsRemove" => {
            ids.extend(resolved_string_list_arg(arguments, "marketIds"));
        }
        _ => {}
    }
    ids.sort();
    ids.dedup();
    ids
}

fn localization_selected_scope(fields: &[SelectedField]) -> Value {
    let mut fields = fields
        .iter()
        .map(|field| {
            let arguments = field
                .arguments
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                .collect::<serde_json::Map<_, _>>();
            json!({
                "name": field.name,
                "arguments": arguments,
                "selection": localization_selected_scope(&field.selection)
            })
        })
        .collect::<Vec<_>>();
    fields.sort_by_key(Value::to_string);
    Value::Array(fields)
}

fn localization_scope_key(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    selected_fields: &[SelectedField],
) -> String {
    let arguments = arguments
        .iter()
        .map(|(name, value)| (name.clone(), resolved_value_json(value)))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "root": root_name,
        "arguments": arguments,
        "selection": localization_selected_scope(selected_fields)
    })
    .to_string()
}

fn localization_translation_identity(translation: &Value) -> String {
    json!({
        "resourceId": translation.get("resourceId").and_then(Value::as_str).unwrap_or_default(),
        "key": translation.get("key").and_then(Value::as_str).unwrap_or_default(),
        "locale": translation.get("locale").and_then(Value::as_str).unwrap_or_default(),
        "marketId": translation.pointer("/market/id").and_then(Value::as_str)
    })
    .to_string()
}

fn translatable_resource_id_from_value(value: &Value) -> Option<&str> {
    value.get("resourceId").and_then(Value::as_str)
}

fn shopify_gid_resource_type_from_identity(identity: &str) -> Option<String> {
    let identity: Value = serde_json::from_str(identity).ok()?;
    shopify_gid_resource_type(identity.get("resourceId")?.as_str()?)
        .map(|resource_type| resource_type.to_ascii_uppercase())
}

fn localization_by_ids_scope_accounts_for_all(
    arguments: &BTreeMap<String, ResolvedValue>,
    connection: &Value,
) -> bool {
    if arguments.contains_key("after")
        || arguments.contains_key("before")
        || arguments.contains_key("last")
        || connection
            .pointer("/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || connection
            .pointer("/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return false;
    }
    let unique_ids = resolved_string_list_arg(arguments, "resourceIds")
        .into_iter()
        .collect::<BTreeSet<_>>()
        .len();
    resolved_int_field(arguments, "first")
        .is_none_or(|first| first >= 0 && first as usize >= unique_ids)
}

struct LocalizationMutationPreflight {
    scope_key: String,
    operation_name: &'static str,
    query: String,
    variables: Value,
    queries_available_locales: bool,
    queries_shop_locales: bool,
    queries_resource: bool,
    queries_nodes: bool,
}

const LOCALIZATION_AVAILABLE_LOCALES_CATALOG_SCOPE: &str =
    "localization:availableLocales:authoritative-catalog";
const LOCALIZATION_SHOP_LOCALES_CATALOG_SCOPE: &str =
    "localization:shopLocales:authoritative-catalog";

fn localization_mutation_locales(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<(String, Option<String>)> {
    let mut scopes = BTreeSet::new();
    match root_name {
        "translationsRegister" => {
            for translation in resolved_list_arg(arguments, "translations") {
                let locale = resolved_object_string(&translation, "locale").unwrap_or_default();
                if !locale.is_empty() {
                    scopes.insert((locale, resolved_object_string(&translation, "marketId")));
                }
            }
        }
        "translationsRemove" => {
            let locales = resolved_string_list_arg(arguments, "locales");
            let market_ids = resolved_string_list_arg(arguments, "marketIds");
            for locale in locales {
                if market_ids.is_empty() {
                    scopes.insert((locale, None));
                } else {
                    for market_id in &market_ids {
                        scopes.insert((locale.clone(), Some(market_id.clone())));
                    }
                }
            }
        }
        _ => {}
    }
    scopes.into_iter().collect()
}

fn localization_mutation_preflight_plan(
    proxy: &DraftProxy,
    input: &LocalizationMutationInput,
) -> Option<LocalizationMutationPreflight> {
    let ids = localization_mutation_target_ids(&input.name, &input.arguments)
        .into_iter()
        .filter(|id| {
            (is_shopify_gid_of_type(id, "Market") && !proxy.market_exists(id))
                || (is_shopify_gid_of_type(id, "MarketWebPresence")
                    && !proxy.market_web_presence_exists(id))
        })
        .collect::<Vec<_>>();
    let resource_id = resolved_string_field(&input.arguments, "resourceId");
    let translation_scopes = localization_mutation_locales(&input.name, &input.arguments);
    let locale_mutation = matches!(
        input.name.as_str(),
        "shopLocaleEnable" | "shopLocaleUpdate" | "shopLocaleDisable"
    );
    let queries_available_locales = locale_mutation
        && !proxy
            .store
            .staged
            .localization_complete_scopes
            .contains(LOCALIZATION_AVAILABLE_LOCALES_CATALOG_SCOPE);
    let queries_shop_locales = (locale_mutation
        || translation_scopes
            .iter()
            .any(|(locale, _)| !proxy.localization_shop_locale_added(locale)))
        && !proxy
            .store
            .staged
            .localization_complete_scopes
            .contains(LOCALIZATION_SHOP_LOCALES_CATALOG_SCOPE);
    let resource_is_local = resource_id
        .as_deref()
        .is_some_and(|id| proxy.localization_resource_is_local_authoritative(id));
    let resource_is_confirmed_missing = resource_id.as_deref().is_some_and(|id| {
        proxy
            .store
            .staged
            .missing_translatable_resource_ids
            .contains(id)
    });
    let queries_resource =
        resource_id.is_some() && !resource_is_local && !resource_is_confirmed_missing;
    let scope_key = json!({
        "mutationPrerequisites": input.name,
        "resourceId": resource_id,
        "ids": ids,
        "translationScopes": translation_scopes,
        "availableLocales": queries_available_locales,
        "shopLocales": queries_shop_locales,
        "resource": queries_resource
    })
    .to_string();
    if proxy
        .store
        .staged
        .localization_complete_scopes
        .contains(&scope_key)
    {
        return None;
    }

    if !ids.is_empty() && !queries_available_locales && !queries_shop_locales && !queries_resource {
        return Some(LocalizationMutationPreflight {
            scope_key,
            operation_name: "LocalizationMutationTargetsHydrate",
            query: LOCALIZATION_MUTATION_TARGETS_HYDRATE_QUERY.to_string(),
            variables: json!({ "ids": ids }),
            queries_available_locales,
            queries_shop_locales,
            queries_resource,
            queries_nodes: true,
        });
    }

    let mut variables = serde_json::Map::new();
    let mut variable_definitions = Vec::new();
    let mut selections = Vec::new();
    if queries_available_locales {
        selections.push("availableLocales { isoCode name }".to_string());
    }
    if queries_shop_locales {
        selections.push("shopLocales { locale name primary published marketWebPresences { id subfolderSuffix } }".to_string());
    }
    if queries_resource {
        let resource_id = resource_id.expect("resource preflight has an id");
        variables.insert("resourceId".to_string(), json!(resource_id));
        variable_definitions.push("$resourceId: ID!".to_string());
        let mut translation_fields = Vec::new();
        for (index, (locale, market_id)) in translation_scopes.iter().enumerate() {
            let locale_variable = format!("locale{index}");
            variable_definitions.push(format!("${locale_variable}: String!"));
            variables.insert(locale_variable.clone(), json!(locale));
            let market_argument = market_id.as_ref().map_or_else(String::new, |market_id| {
                let market_variable = format!("market{index}");
                variable_definitions.push(format!("${market_variable}: ID!"));
                variables.insert(market_variable.clone(), json!(market_id));
                format!(", marketId: ${market_variable}")
            });
            translation_fields.push(format!(
                "translations{index}: translations(locale: ${locale_variable}{market_argument}) {{ key value locale outdated updatedAt market {{ id }} }}"
            ));
        }
        selections.push(format!(
            "translatableResource(resourceId: $resourceId) {{ resourceId translatableContent {{ key value digest locale type }} {} }}",
            translation_fields.join(" ")
        ));
    }
    if !ids.is_empty() {
        variables.insert("ids".to_string(), json!(ids));
        variable_definitions.push("$ids: [ID!]!".to_string());
        selections.push(localization_prerequisite_nodes_selection().to_string());
    }
    if selections.is_empty() {
        return None;
    }
    let definitions = if variable_definitions.is_empty() {
        String::new()
    } else {
        format!("({})", variable_definitions.join(", "))
    };
    Some(LocalizationMutationPreflight {
        scope_key,
        operation_name: "LocalizationMutationPrerequisites",
        query: format!(
            "query LocalizationMutationPrerequisites{definitions} {{ {} }}",
            selections.join(" ")
        ),
        variables: Value::Object(variables),
        queries_available_locales,
        queries_shop_locales,
        queries_resource,
        queries_nodes: !ids.is_empty(),
    })
}

fn localization_prerequisite_nodes_selection() -> &'static str {
    "nodes(ids: $ids) { __typename ... on Market { id name handle status type } ... on MarketWebPresence { id subfolderSuffix domain { id host url sslEnabled } rootUrls { locale url } defaultLocale { locale name primary published } alternateLocales { locale name primary published } markets(first: 250) { nodes { id name handle status type } } } }"
}

fn localization_connection_refill_query(
    root_name: &str,
    arguments: &BTreeMap<String, ResolvedValue>,
    selected_fields: &[SelectedField],
    backward: bool,
    count: i64,
    boundary_cursor: &str,
) -> (String, Value) {
    let mut definitions = vec![
        "$count: Int!".to_string(),
        "$cursor: String!".to_string(),
        "$reverse: Boolean!".to_string(),
    ];
    let mut variables = serde_json::Map::from_iter([
        ("count".to_string(), json!(count)),
        ("cursor".to_string(), json!(boundary_cursor)),
        (
            "reverse".to_string(),
            json!(resolved_bool_field(arguments, "reverse").unwrap_or(false)),
        ),
    ]);
    let mut root_arguments = Vec::new();
    match root_name {
        "translatableResources" => {
            definitions.push("$resourceType: TranslatableResourceType!".to_string());
            variables.insert(
                "resourceType".to_string(),
                json!(resolved_string_field(arguments, "resourceType")
                    .unwrap_or_else(|| "PRODUCT".to_string())),
            );
            root_arguments.push("resourceType: $resourceType".to_string());
        }
        "translatableResourcesByIds" => {
            definitions.push("$resourceIds: [ID!]!".to_string());
            let mut seen = BTreeSet::new();
            let resource_ids = resolved_string_list_arg(arguments, "resourceIds")
                .into_iter()
                .filter(|id| seen.insert(id.clone()))
                .collect::<Vec<_>>();
            variables.insert("resourceIds".to_string(), json!(resource_ids));
            root_arguments.push("resourceIds: $resourceIds".to_string());
        }
        _ => unreachable!("only translatable connections can refill"),
    }
    if backward {
        root_arguments.push("last: $count".to_string());
        root_arguments.push("before: $cursor".to_string());
    } else {
        root_arguments.push("first: $count".to_string());
        root_arguments.push("after: $cursor".to_string());
    }
    root_arguments.push("reverse: $reverse".to_string());

    let mut translation_arguments = BTreeMap::<String, BTreeMap<String, ResolvedValue>>::new();
    collect_localization_translation_selections(selected_fields, &mut translation_arguments);
    let translation_fields = translation_arguments
        .into_values()
        .enumerate()
        .map(|(index, arguments)| {
            format!(
                "translationsRefill{index}: translations{} {{ key value locale outdated updatedAt market {{ id }} }}",
                localization_resolved_arguments(&arguments)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let query = format!(
        "query LocalizationTranslatableConnectionRefill({}) {{ refill: {root_name}({}) {{ edges {{ cursor node {{ resourceId translatableContent {{ key value digest locale type }} {translation_fields} }} }} pageInfo {{ hasNextPage hasPreviousPage startCursor endCursor }} }} }}",
        definitions.join(", "),
        root_arguments.join(", ")
    );
    (query, Value::Object(variables))
}

fn collect_localization_translation_selections(
    fields: &[SelectedField],
    arguments: &mut BTreeMap<String, BTreeMap<String, ResolvedValue>>,
) {
    for field in fields {
        if field.name == "translations" {
            let key = field
                .arguments
                .iter()
                .map(|(name, value)| format!("{name}:{}", serialize_resolved_value(value)))
                .collect::<Vec<_>>()
                .join(",");
            arguments
                .entry(key)
                .or_insert_with(|| field.arguments.clone());
        }
        collect_localization_translation_selections(&field.selection, arguments);
    }
}

fn localization_resolved_arguments(arguments: &BTreeMap<String, ResolvedValue>) -> String {
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

fn combine_localization_connection_pages(
    observed: &Value,
    refill: &Value,
    backward: bool,
) -> Value {
    let mut rows = if backward {
        observed_connection_rows(refill)
    } else {
        observed_connection_rows(observed)
    };
    let trailing = if backward {
        observed_connection_rows(observed)
    } else {
        observed_connection_rows(refill)
    };
    let mut seen = rows
        .iter()
        .filter_map(|row| translatable_resource_id_from_value(&row.node))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    rows.extend(trailing.into_iter().filter(|row| {
        translatable_resource_id_from_value(&row.node).is_some_and(|id| seen.insert(id.to_string()))
    }));
    let observed_page_info = observed
        .get("pageInfo")
        .cloned()
        .unwrap_or_else(empty_page_info);
    let refill_page_info = refill
        .get("pageInfo")
        .cloned()
        .unwrap_or_else(empty_page_info);
    let has_next = if backward {
        observed_page_info["hasNextPage"].as_bool().unwrap_or(false)
    } else {
        refill_page_info["hasNextPage"].as_bool().unwrap_or(false)
    };
    let has_previous = if backward {
        refill_page_info["hasPreviousPage"]
            .as_bool()
            .unwrap_or(false)
    } else {
        observed_page_info["hasPreviousPage"]
            .as_bool()
            .unwrap_or(false)
    };
    let start_cursor = if backward {
        refill_page_info.get("startCursor")
    } else {
        observed_page_info.get("startCursor")
    }
    .and_then(Value::as_str)
    .map(str::to_string)
    .or_else(|| rows.first().and_then(|row| row.cursor.clone()));
    let end_cursor = if backward {
        observed_page_info.get("endCursor")
    } else {
        refill_page_info.get("endCursor")
    }
    .and_then(Value::as_str)
    .map(str::to_string)
    .or_else(|| rows.last().and_then(|row| row.cursor.clone()));
    let nodes = rows.iter().map(|row| row.node.clone()).collect::<Vec<_>>();
    let edges = rows
        .into_iter()
        .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
        .collect::<Vec<_>>();
    json!({
        "nodes": nodes,
        "edges": edges,
        "pageInfo": connection_page_info(
            has_next,
            has_previous,
            start_cursor,
            end_cursor
        )
    })
}

impl DraftProxy {
    pub(crate) fn localization_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            root_name,
            operation_roots,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let selected_fields = self
            .execution_session
            .upstream_query_selections
            .get(response_key)
            .cloned()
            .unwrap_or_default();
        let scope_key = localization_scope_key(root_name, &arguments, &selected_fields);
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.localization_should_fetch_upstream(root_name, &arguments, &scope_key)
        {
            // A localization document commonly selects several same-domain
            // roots. Hydrate from one request-scoped execution of the complete
            // document so sibling resolvers do not each consume the same
            // upstream call and so aliases are observed together.
            let result = self.cached_or_forward_upstream_graphql_result(request, response_key);
            if result.transport_succeeded && result.outcome.errors.is_empty() {
                self.observe_localization_read(
                    root_name,
                    response_key,
                    &arguments,
                    &scope_key,
                    &result.data,
                );
                self.hydrate_localization_from_upstream(&json!({ "data": result.data }));
                if self.localization_has_relevant_overlay(root_name, &arguments) {
                    if matches!(
                        root_name,
                        "translatableResources" | "translatableResourcesByIds"
                    ) {
                        self.refill_localization_connection_if_needed(
                            root_name,
                            &arguments,
                            &selected_fields,
                            &scope_key,
                            request,
                        );
                    }
                    return ResolverOutcome::value(self.localization_query_value(
                        root_name,
                        response_key,
                        &arguments,
                        request,
                        false,
                        Some(&scope_key),
                    ));
                }
            }
            if self.localization_operation_has_local_overlay(&operation_roots) {
                return ResolverOutcome::value(self.localization_query_value(
                    root_name,
                    response_key,
                    &arguments,
                    request,
                    false,
                    Some(&scope_key),
                ));
            }
            return result.outcome;
        }
        ResolverOutcome::value(self.localization_query_value(
            root_name,
            response_key,
            &arguments,
            request,
            true,
            Some(&scope_key),
        ))
    }

    pub(crate) fn localization_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            response_key,
            arguments,
            request,
            root_name,
            ..
        } = invocation;
        let input = LocalizationMutationInput {
            name: root_name.to_string(),
            arguments: resolved_arguments_from_json(&arguments),
        };
        self.localization_mutation_preflight(&input, request);
        let result = self.localization_mutation_value(&input);
        let mut outcome = ResolverOutcome::value(result.value);
        if result.staged {
            outcome = outcome.with_log_draft(LogDraft::staged(
                root_name,
                "localization",
                vec![response_key.to_string()],
            ));
        }
        outcome
    }

    fn localization_query_value(
        &mut self,
        root_name: &str,
        response_key: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        hydrate_missing_markets: bool,
        scope_key: Option<&str>,
    ) -> Value {
        match root_name {
            "availableLocales" => scope_key
                .and_then(|scope| {
                    self.store
                        .staged
                        .localization_observed_read_values
                        .get(scope)
                        .cloned()
                })
                .unwrap_or_else(|| Value::Array(self.localization_available_locales())),
            "shopLocales" => {
                if !self.localization_has_relevant_overlay(root_name, arguments) {
                    if let Some(observed) = scope_key.and_then(|scope| {
                        self.store
                            .staged
                            .localization_observed_read_values
                            .get(scope)
                    }) {
                        return observed.clone();
                    }
                }
                // Shopify's schema default is `published: false`, where false means
                // "do not restrict to published locales" rather than "only return
                // unpublished locales". Only true activates the filter.
                let published_filter =
                    resolved_bool_field(arguments, "published").filter(|published| *published);
                Value::Array(self.localization_shop_locales(published_filter))
            }
            "translatableResource" => {
                let resource_id =
                    resolved_string_field(arguments, "resourceId").unwrap_or_default();
                if !self.localization_translatable_resource_exists(&resource_id) {
                    Value::Null
                } else {
                    self.localization_translatable_resource_value(&resource_id)
                }
            }
            "translatableResources" => {
                self.localization_translatable_resources_connection(arguments, scope_key)
            }
            "translatableResourcesByIds" => {
                self.localization_translatable_resources_by_ids_connection(arguments, scope_key)
            }
            "markets" => self.localization_markets_connection_with_hydration(
                arguments,
                response_key,
                request,
                hydrate_missing_markets,
            ),
            _ => Value::Null,
        }
    }

    fn localization_mutation_value(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        match input.name.as_str() {
            "shopLocaleEnable" => self.shop_locale_enable_response(input),
            "shopLocaleUpdate" => self.shop_locale_update_response(input),
            "shopLocaleDisable" => self.shop_locale_disable_response(input),
            "translationsRegister" => self.localization_register_response(input),
            "translationsRemove" => self.localization_remove_response(input),
            _ => LocalMutationResult::no_stage(Value::Null),
        }
    }

    fn localization_mutation_preflight(
        &mut self,
        input: &LocalizationMutationInput,
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let Some(plan) = localization_mutation_preflight_plan(self, input) else {
            return;
        };
        let response = self.upstream_post(
            request,
            json!({
                "query": plan.query,
                "operationName": plan.operation_name,
                "variables": plan.variables
            }),
        );
        if (200..300).contains(&response.status) && response.body.get("errors").is_none() {
            let data = response.body.get("data").unwrap_or(&Value::Null);
            let available_locales_complete = !plan.queries_available_locales
                || data.get("availableLocales").is_some_and(Value::is_array);
            let shop_locales_complete =
                !plan.queries_shop_locales || data.get("shopLocales").is_some_and(Value::is_array);
            let resource_complete = !plan.queries_resource
                || data
                    .get("translatableResource")
                    .is_some_and(|value| value.is_object() || value.is_null());
            let nodes_complete =
                !plan.queries_nodes || data.get("nodes").is_some_and(Value::is_array);
            if available_locales_complete
                && shop_locales_complete
                && resource_complete
                && nodes_complete
            {
                self.store
                    .staged
                    .localization_complete_scopes
                    .insert(plan.scope_key);
            }
            if plan.queries_available_locales && available_locales_complete {
                self.store
                    .staged
                    .localization_complete_scopes
                    .insert(LOCALIZATION_AVAILABLE_LOCALES_CATALOG_SCOPE.to_string());
            }
            if plan.queries_shop_locales && shop_locales_complete {
                self.store
                    .staged
                    .localization_complete_scopes
                    .insert(LOCALIZATION_SHOP_LOCALES_CATALOG_SCOPE.to_string());
            }
            self.hydrate_markets_from_upstream(&response.body);
            self.hydrate_localization_from_upstream(&response.body);
            if plan.queries_resource {
                let resource_id = resolved_string_field(&input.arguments, "resourceId")
                    .expect("resource preflight has an id");
                match response.body.pointer("/data/translatableResource") {
                    Some(resource) if resource.is_object() => {
                        self.observe_translatable_resource(resource);
                    }
                    Some(resource) if resource.is_null() => {
                        self.store
                            .staged
                            .missing_translatable_resource_ids
                            .insert(resource_id);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(in crate::proxy) fn localization_available_locales(&self) -> Vec<Value> {
        let mut locales = self.store.base.available_locales.clone();
        locales.extend(self.store.staged.observed_available_locales.clone());
        locales
            .iter()
            .map(|(iso_code, name)| {
                json!({
                    "isoCode": iso_code,
                    "name": name
                })
            })
            .collect()
    }

    pub(in crate::proxy) fn localization_available_locale_name(
        &self,
        locale: &str,
    ) -> Option<&str> {
        self.store
            .staged
            .observed_available_locales
            .get(locale)
            .or_else(|| self.store.base.available_locales.get(locale))
            .map(String::as_str)
    }

    pub(in crate::proxy) fn localization_shop_locales(
        &self,
        published_filter: Option<bool>,
    ) -> Vec<Value> {
        let mut by_code: BTreeMap<String, Value> = BTreeMap::new();
        for locale in self.store.base.shop_locales.values() {
            if let Some(code) = locale["locale"].as_str() {
                by_code.insert(code.to_string(), locale.clone());
            }
        }
        for locale in self.store.staged.observed_shop_locales.values() {
            if let Some(code) = locale["locale"].as_str() {
                by_code
                    .entry(code.to_string())
                    .and_modify(|existing| {
                        *existing = shallow_merged_object(existing.clone(), locale.clone());
                    })
                    .or_insert_with(|| locale.clone());
            }
        }
        for locale in self.store.staged.shop_locales.values() {
            if let Some(code) = locale["locale"].as_str() {
                by_code
                    .entry(code.to_string())
                    .and_modify(|existing| {
                        *existing = shallow_merged_object(existing.clone(), locale.clone());
                    })
                    .or_insert_with(|| locale.clone());
            }
        }
        for locale in &self.store.staged.deleted_shop_locale_codes {
            by_code.remove(locale);
        }
        let mut locales = by_code.into_values().collect::<Vec<_>>();
        locales.sort_by_key(|locale| locale["locale"].as_str().unwrap_or_default().to_string());
        if let Some(published) = published_filter {
            locales.retain(|locale| locale["published"].as_bool() == Some(published));
        }
        locales
    }

    fn shop_locale_enable_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let locale =
            resolved_string_field(&input.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let primary_locale = self.localization_primary_locale();
        if locale == primary_locale {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                PRIMARY_LOCALE_CHANGE_MESSAGE,
            ))
        } else if self.localization_available_locale_name(&locale).is_none() {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                "Locale is invalid",
            ))
        } else if self.localization_shop_locale_added(&locale) {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                "Locale has already been taken",
            ))
        } else if self
            .localization_shop_locales(None)
            .iter()
            .filter(|locale| !locale["primary"].as_bool().unwrap_or(false))
            .count()
            >= 20
        {
            LocalMutationResult::no_stage(payload_user_error(
                "shopLocale",
                user_error_omit_code(Value::Null, &format!(
                        "Your store has reached its 20 language limit. To add {}, delete one of your other languages.",
                        self.localization_available_locale_name(&locale).unwrap_or(locale.as_str())
                    ), None),
            ))
        } else {
            let name = self
                .localization_available_locale_name(&locale)
                .unwrap_or(locale.as_str());
            let mut record = shop_locale_record(&locale, name, false, &primary_locale);
            let target_web_presence_ids = self.known_market_web_presence_ids(
                resolved_string_list_arg(&input.arguments, "marketWebPresenceIds"),
            );
            record["marketWebPresences"] = Value::Array(
                target_web_presence_ids
                    .iter()
                    .map(|id| {
                        let default_locale = self.market_web_presence_default_locale(id);
                        shop_locale_market_web_presence_record(id, &default_locale)
                    })
                    .collect(),
            );
            self.store
                .staged
                .shop_locales
                .insert(locale.clone(), record.clone());
            self.store.staged.deleted_shop_locale_codes.remove(&locale);
            self.sync_web_presence_locales(&locale, &target_web_presence_ids, false);
            LocalMutationResult::staged(json!({ "shopLocale": record, "userErrors": [] }))
        }
    }

    fn shop_locale_update_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let locale =
            resolved_string_field(&input.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let shop_locale = resolved_object_field(&input.arguments, "shopLocale").unwrap_or_default();
        let published = resolved_bool_field(&shop_locale, "published");
        let market_web_presence_ids = list_string_field(&shop_locale, "marketWebPresenceIds");
        let primary_locale = self.localization_primary_locale();

        if locale == primary_locale && published.is_some() {
            return LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                PRIMARY_LOCALE_CHANGE_MESSAGE,
            ));
        }

        let locale_exists = self.localization_shop_locale_added(&locale);
        if !locale_exists && published.is_some() {
            return LocalMutationResult::no_stage(shop_locale_payload_error(
                "shopLocale",
                "The locale doesn't exist.",
            ));
        }

        let mut record = self
            .store
            .staged
            .shop_locales
            .get(&locale)
            .cloned()
            .or_else(|| {
                self.store
                    .staged
                    .observed_shop_locales
                    .get(&locale)
                    .cloned()
            })
            .or_else(|| self.store.base.shop_locales.get(&locale).cloned())
            .unwrap_or_else(|| {
                let name = self
                    .localization_available_locale_name(&locale)
                    .unwrap_or(locale.as_str());
                shop_locale_record(&locale, name, false, &primary_locale)
            });
        if let Some(published) = published {
            record["published"] = json!(published);
        }
        if shop_locale.contains_key("marketWebPresenceIds") {
            let target_web_presence_ids =
                self.known_market_web_presence_ids(market_web_presence_ids);
            record["marketWebPresences"] = Value::Array(
                target_web_presence_ids
                    .iter()
                    .map(|id| {
                        let default_locale = self.market_web_presence_default_locale(id);
                        shop_locale_market_web_presence_record(id, &default_locale)
                    })
                    .collect(),
            );
            self.sync_web_presence_locales(&locale, &target_web_presence_ids, true);
        }
        let staged = locale != primary_locale;
        if staged {
            self.store
                .staged
                .shop_locales
                .insert(locale.clone(), record.clone());
            self.store.staged.deleted_shop_locale_codes.remove(&locale);
        }
        if staged {
            LocalMutationResult::staged(json!({ "shopLocale": record, "userErrors": [] }))
        } else {
            LocalMutationResult::no_stage(json!({ "shopLocale": record, "userErrors": [] }))
        }
    }

    fn known_market_web_presence_ids(&self, ids: Vec<String>) -> Vec<String> {
        ids.into_iter()
            .filter(|id| self.market_web_presence_exists(id))
            .collect()
    }

    fn market_web_presence_exists(&self, id: &str) -> bool {
        self.store.staged.web_presences.contains_key(id)
            || self.localization_shop_locales(None).iter().any(|locale| {
                locale["marketWebPresences"]
                    .as_array()
                    .is_some_and(|presences| {
                        presences
                            .iter()
                            .any(|presence| presence["id"].as_str() == Some(id))
                    })
            })
    }

    fn market_web_presence_default_locale(&self, id: &str) -> String {
        self.store
            .staged
            .web_presences
            .get(id)
            .and_then(|presence| presence.pointer("/defaultLocale/locale"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.localization_primary_locale())
    }

    fn shop_locale_disable_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let locale =
            resolved_string_field(&input.arguments, "locale").unwrap_or_else(|| "fr".to_string());
        let primary_locale = self.localization_primary_locale();
        if locale == primary_locale {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "locale",
                PRIMARY_LOCALE_CHANGE_MESSAGE,
            ))
        } else if !self.localization_shop_locale_added(&locale) {
            LocalMutationResult::no_stage(shop_locale_payload_error(
                "locale",
                "The locale doesn't exist.",
            ))
        } else {
            self.store.staged.shop_locales.remove(&locale);
            self.store
                .staged
                .deleted_shop_locale_codes
                .insert(locale.clone());
            let translations = self
                .store
                .staged
                .observed_localization_translations
                .iter()
                .chain(self.store.staged.localization_translations.iter())
                .filter(|translation| translation["locale"] == json!(locale))
                .cloned()
                .collect::<Vec<_>>();
            for translation in translations {
                self.store
                    .staged
                    .localization_translation_tombstones
                    .insert(localization_translation_identity(&translation));
            }
            self.store
                .staged
                .localization_translations
                .retain(|translation| translation["locale"] != json!(locale));
            self.store.staged.localization_dirty = true;
            LocalMutationResult::staged(json!({ "locale": locale, "userErrors": [] }))
        }
    }

    /// Forward a market-localization mutation preflight on a cold store so the
    /// target resource's content (valid keys/digests), the shop's markets, and any
    /// existing localizations are staged before the register/remove is validated and
    /// applied. Gated like the web-presence preflight: once any markets-domain record
    /// is staged (including markets observed from a read carrying a `markets` field),
    /// the baseline is already known and the preflight is skipped. The cassette
    /// matches the sentinel query plus the mutation's own variables exactly; an
    /// unmatched preflight (a capture recorded with a different preflight form, or
    /// none) returns an error body and is ignored.
    pub(in crate::proxy) fn market_localization_mutation_preflight(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) {
        self.cold_markets_preflight(
            MARKET_LOCALIZATION_PREFLIGHT_QUERY,
            market_localization_preflight_variables(variables),
            request,
            Self::hydrate_markets_from_upstream,
        );
    }
    /// Project a market-localizable resource from observed/staged state: the
    /// `marketLocalizableContent` recorded when the resource was first read (empty
    /// when never observed), plus the staged `marketLocalizations` for this resource
    /// filtered to the read's `marketId` argument. No field metadata is fabricated.
    pub(in crate::proxy) fn market_localizable_resource(
        &self,
        resource_id: &str,
        market_filter: Option<&str>,
    ) -> Value {
        let content = self
            .store
            .staged
            .localization_resources
            .get(resource_id)
            .cloned()
            .unwrap_or_else(|| json!([]));
        let localizations = self
            .store
            .staged
            .localization_translations
            .iter()
            .filter(|translation| translation["resourceId"].as_str() == Some(resource_id))
            .filter(|translation| match market_filter {
                Some(market_id) => translation["market"]["id"].as_str() == Some(market_id),
                None => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        json!({
            "resourceId": resource_id,
            "marketLocalizableContent": content,
            "marketLocalizations": localizations
        })
    }

    pub(in crate::proxy) fn market_localizable_resources_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let resource_type = resolved_string_field(arguments, "resourceType");
        let records = self
            .market_localizable_resource_ids()
            .into_iter()
            .filter(|resource_id| {
                resource_type.as_deref().is_none_or(|resource_type| {
                    localization_resource_type_matches(resource_id, resource_type)
                })
            })
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .iter()
                .map(|resource_id| self.market_localizable_resource(resource_id, None))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    pub(in crate::proxy) fn market_localizable_resources_by_ids_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let records = resolved_string_list_arg(arguments, "resourceIds")
            .into_iter()
            .filter(|resource_id| self.market_localizable_resource_exists(resource_id))
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .iter()
                .map(|resource_id| self.market_localizable_resource(resource_id, None))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    fn market_localizable_resource_ids(&self) -> Vec<String> {
        let mut ids = self
            .store
            .staged
            .localization_resources
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        ids.extend(
            self.store
                .staged
                .localization_translations
                .iter()
                .filter_map(|translation| {
                    translation["resourceId"]
                        .as_str()
                        .filter(|resource_id| !resource_id.is_empty())
                        .map(ToString::to_string)
                }),
        );
        ids.into_iter().collect()
    }

    pub(in crate::proxy) fn has_market_localizable_resource_state(&self) -> bool {
        !self.market_localizable_resource_ids().is_empty()
    }

    pub(in crate::proxy) fn market_localizable_resources_by_ids_should_fetch_upstream(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let resource_ids = resolved_string_list_arg(arguments, "resourceIds");
        resource_ids.is_empty()
            || resource_ids.iter().any(|resource_id| {
                !self
                    .store
                    .staged
                    .localization_resources
                    .contains_key(resource_id)
            })
    }

    pub(super) fn market_localizable_resource_exists(&self, resource_id: &str) -> bool {
        !resource_id.is_empty()
            && (self
                .store
                .staged
                .localization_resources
                .contains_key(resource_id)
                || self
                    .store
                    .staged
                    .localization_translations
                    .iter()
                    .any(|translation| {
                        translation["resourceId"].as_str() == Some(resource_id)
                            && !translation["market"].is_null()
                    }))
    }

    pub(in crate::proxy) fn market_localization_mutation_value(
        &mut self,
        field: &MarketsRootInput,
    ) -> Value {
        match field.name.as_str() {
            "marketLocalizationsRegister" => self.market_localizations_register_response(field),
            "marketLocalizationsRemove" => self.market_localizations_remove_response(field),
            _ => Value::Null,
        }
    }

    pub(in crate::proxy) fn market_localizations_register_response(
        &mut self,
        field: &MarketsRootInput,
    ) -> Value {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        let localizations = resolved_list_arg(&field.arguments, "marketLocalizations");
        // 1. Per-mutation key cap fires before resource existence (matches live Shopify).
        if localizations.len() > 100 {
            return selected_market_localization_error(
                field,
                vec!["resourceId"],
                "TOO_MANY_KEYS_FOR_RESOURCE",
                "Too many keys for resource - maximum 100 per mutation",
            );
        }
        // 2. The resource must have been observed (cold read / mutation preflight).
        let Some(content) = self
            .store
            .staged
            .localization_resources
            .get(&resource_id)
            .cloned()
        else {
            return selected_market_localization_error(
                field,
                vec!["resourceId"],
                "RESOURCE_NOT_FOUND",
                &format!("Resource {resource_id} does not exist"),
            );
        };

        let mut staged_inputs = Vec::new();
        for (index, input) in localizations.iter().enumerate() {
            let field_index = index.to_string();
            let market_id = resolved_object_string(input, "marketId").unwrap_or_default();
            if market_id.is_empty() || !self.market_exists(&market_id) {
                return selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "marketId"],
                    "MARKET_DOES_NOT_EXIST",
                    "The market does not exist",
                );
            }
            let key = resolved_object_string(input, "key").unwrap_or_default();
            // 3. The key must be one of the resource's localizable content keys.
            let Some(content_entry) = content.as_array().and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry["key"].as_str() == Some(key.as_str()))
            }) else {
                return selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "key"],
                    "INVALID_KEY_FOR_MODEL",
                    &format!("Key {key} is not a valid market localizable field"),
                );
            };
            // 4. The supplied digest must match the resource's current content digest.
            let expected_digest = content_entry["digest"].as_str();
            if resolved_object_string(input, "marketLocalizableContentDigest").as_deref()
                != expected_digest
            {
                return selected_market_localization_error(
                    field,
                    vec![
                        "marketLocalizations",
                        &field_index,
                        "marketLocalizableContentDigest",
                    ],
                    "INVALID_MARKET_LOCALIZABLE_CONTENT",
                    "The provided content digest does not match the latest resource content",
                );
            }
            // 5. The localized value must not be blank.
            if resolved_object_string(input, "value").as_deref() == Some("") {
                return selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "value"],
                    "FAILS_RESOURCE_VALIDATION",
                    "Value can't be blank",
                );
            }
            // 6. Shopify exposes definition-backed money metafields as a
            // `value` market-localizable field, but rejects JSON money payloads
            // during register with a resource-validation error.
            if market_localizable_content_is_money_metafield(content_entry) {
                return selected_market_localization_error(
                    field,
                    vec!["marketLocalizations", &field_index, "value"],
                    "FAILS_RESOURCE_VALIDATION",
                    "Market Localizable content is invalid",
                );
            }
            staged_inputs.push((market_id, input));
        }

        let updated_at = if staged_inputs.is_empty() {
            None
        } else {
            Some(self.next_mutation_timestamp())
        };
        let staged = staged_inputs
            .into_iter()
            .map(|(market_id, input)| {
                self.market_localization_staged_record(
                    &resource_id,
                    &market_id,
                    input,
                    updated_at.as_deref().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();

        for record in &staged {
            let resource_id = record["resourceId"].as_str().unwrap_or_default();
            let key = record["key"].as_str().unwrap_or_default();
            let market_id = record["market"]["id"].as_str().unwrap_or_default();
            self.store
                .staged
                .localization_translations
                .retain(|existing| {
                    existing["resourceId"].as_str() != Some(resource_id)
                        || existing["key"].as_str() != Some(key)
                        || existing["market"]["id"].as_str() != Some(market_id)
                });
            self.store
                .staged
                .localization_translations
                .push(record.clone());
        }

        json!({ "marketLocalizations": staged, "userErrors": [] })
    }

    /// Build a staged market-localization record with the live market name resolved
    /// from staged markets and the successful mutation's clock timestamp.
    fn market_localization_staged_record(
        &self,
        resource_id: &str,
        market_id: &str,
        input: &ResolvedValue,
        updated_at: &str,
    ) -> Value {
        let value = resolved_object_string(input, "value").unwrap_or_default();
        let key = resolved_object_string(input, "key").unwrap_or_default();
        let market_name = self
            .store
            .staged
            .markets
            .get(market_id)
            .and_then(|market| market.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        json!({
            "resourceId": resource_id,
            "key": key,
            "value": value,
            "updatedAt": updated_at,
            "outdated": false,
            "market": { "id": market_id, "name": market_name }
        })
    }

    pub(in crate::proxy) fn market_localizations_remove_response(
        &mut self,
        field: &MarketsRootInput,
    ) -> Value {
        let resource_id = resolved_string_field(&field.arguments, "resourceId").unwrap_or_default();
        if !self
            .store
            .staged
            .localization_resources
            .contains_key(&resource_id)
        {
            return selected_market_localization_error(
                field,
                vec!["resourceId"],
                "RESOURCE_NOT_FOUND",
                &format!("Resource {resource_id} does not exist"),
            );
        }
        let keys = resolved_string_list_arg(&field.arguments, "marketLocalizationKeys");
        let market_ids = resolved_string_list_arg(&field.arguments, "marketIds");
        if keys.is_empty() {
            return payload_error("marketLocalizations", vec![]);
        }

        let mut removed = Vec::new();
        self.store
            .staged
            .localization_translations
            .retain(|translation| {
                let matches_resource =
                    translation["resourceId"].as_str() == Some(resource_id.as_str());
                let matches_key = translation["key"]
                    .as_str()
                    .is_some_and(|key| keys.iter().any(|candidate| candidate == key));
                let matches_market = market_ids.is_empty()
                    || translation["market"]["id"]
                        .as_str()
                        .is_some_and(|id| market_ids.iter().any(|candidate| candidate == id));
                let should_remove = matches_resource && matches_key && matches_market;
                if should_remove {
                    removed.push(translation.clone());
                }
                !should_remove
            });
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        json!({ "marketLocalizations": removed, "userErrors": [] })
    }

    fn localization_register_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let resource_id = resolved_string_field(&input.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translation_mutation_resource_exists(&resource_id) {
            return LocalMutationResult::no_stage(translation_payload_error(
                &format!("Resource {resource_id} does not exist"),
                "RESOURCE_NOT_FOUND",
            ));
        }

        let translations = resolved_list_arg(&input.arguments, "translations");
        if translations.is_empty() {
            return LocalMutationResult::no_stage(json!({ "translations": [], "userErrors": [] }));
        }
        if translations.len() > 100 {
            return LocalMutationResult::no_stage(translation_payload_error(
                "Too many keys for resource - maximum 100 per mutation",
                "TOO_MANY_KEYS_FOR_RESOURCE",
            ));
        }
        let mut staged = Vec::new();
        let mut user_errors = Vec::new();
        let primary_locale = self.localization_primary_locale();
        for (index, translation_input) in translations.iter().enumerate() {
            let field_index = index.to_string();
            let locale = resolved_object_string(translation_input, "locale")
                .unwrap_or_else(|| "fr".to_string());
            let market_id = resolved_object_string(translation_input, "marketId");
            if matches!(market_id.as_deref(), Some(id) if !self.market_exists(id)) {
                user_errors.push(user_error(
                    json!(["translations", field_index, "marketId"]),
                    "The market corresponding to the `marketId` argument doesn't exist",
                    Some("MARKET_DOES_NOT_EXIST"),
                ));
                continue;
            }
            if locale == primary_locale {
                user_errors.push(user_error(
                    json!(["translations", field_index, "locale"]),
                    "Locale cannot be the same as the shop's primary locale",
                    Some("INVALID_LOCALE_FOR_SHOP"),
                ));
                continue;
            }
            if !self.localization_shop_locale_added(&locale) {
                user_errors.push(user_error(
                    json!(["translations", field_index, "locale"]),
                    "Locale is not a valid locale for the shop",
                    Some("INVALID_LOCALE_FOR_SHOP"),
                ));
                continue;
            }
            if resolved_object_string(translation_input, "value").as_deref() == Some("") {
                user_errors.push(user_error(
                    json!(["translations", field_index, "value"]),
                    "Value can't be blank",
                    Some("FAILS_RESOURCE_VALIDATION"),
                ));
                continue;
            }
            let key = resolved_object_string(translation_input, "key").unwrap_or_default();
            if self.localization_resource_has_modeled_translation_keys(&resource_id)
                && !self.localization_translation_key_is_valid(&resource_id, &key)
            {
                user_errors.push(user_error(
                    json!(["translations", field_index, "key"]),
                    &format!("Key {key} is not a valid translatable field"),
                    Some("INVALID_KEY_FOR_MODEL"),
                ));
                continue;
            }
            let value = resolved_object_string(translation_input, "value").unwrap_or_default();
            if market_id.is_some()
                && self
                    .localization_shop_level_translation_value(&resource_id, &key, &locale)
                    .is_some_and(|base_value| base_value == value)
            {
                user_errors.push(user_error(
                    json!(["translations", field_index, "value"]),
                    "Value cannot match original content",
                    Some("FAILS_RESOURCE_VALIDATION"),
                ));
                continue;
            }
            if let Some(supplied_digest) =
                resolved_object_string(translation_input, "translatableContentDigest")
            {
                let digest_invalid = self
                    .localization_source_content_value(&resource_id, &key)
                    .is_some_and(|value| localization_content_digest(&value) != supplied_digest);
                if digest_invalid {
                    user_errors.push(user_error(
                        json!(["translations", field_index, "translatableContentDigest"]),
                        "Translatable content hash is invalid",
                        Some("INVALID_TRANSLATABLE_CONTENT"),
                    ));
                    continue;
                }
            }
            if market_id.is_some()
                && !self.localization_translation_key_is_market_customizable(&resource_id, &key)
            {
                user_errors.push(user_error(
                    json!(["translations", field_index, "key"]),
                    &format!(
                        "Key {key} cannot be customized for a market; it can only be translated."
                    ),
                    Some("RESOURCE_NOT_MARKET_CUSTOMIZABLE"),
                ));
                continue;
            }

            let mut translation = translation_from_input(translation_input);
            translation["resourceId"] = json!(resource_id);
            if translation["key"] == json!("handle") {
                let original_value = translation["value"].as_str().unwrap_or_default();
                if original_value.chars().count() > 255 {
                    user_errors.push(user_error(json!(["translations", field_index, "value"]), "Value fails validation on resource: [\"Handle is too long (maximum is 255 characters)\"]", Some("FAILS_RESOURCE_VALIDATION")));
                    continue;
                }
                translation["value"] = json!(normalize_localized_handle(original_value));
            }
            staged.push(translation);
        }

        if !staged.is_empty() {
            let updated_at = self.next_mutation_timestamp();
            for translation in &mut staged {
                translation["updatedAt"] = json!(updated_at.clone());
            }
        }

        for translation in &staged {
            let identity = localization_translation_identity(translation);
            self.store
                .staged
                .localization_translations
                .retain(|existing| localization_translation_identity(existing) != identity);
            self.store
                .staged
                .localization_translation_tombstones
                .remove(&identity);
            self.store
                .staged
                .localization_translations
                .push(translation.clone());
        }

        let value = json!({ "translations": staged, "userErrors": user_errors });
        if value["translations"]
            .as_array()
            .is_some_and(|translations| !translations.is_empty())
        {
            LocalMutationResult::staged(value)
        } else {
            LocalMutationResult::no_stage(value)
        }
    }

    fn localization_remove_response(
        &mut self,
        input: &LocalizationMutationInput,
    ) -> LocalMutationResult {
        let resource_id = resolved_string_field(&input.arguments, "resourceId").unwrap_or_default();
        if !self.localization_translation_mutation_resource_exists(&resource_id) {
            return LocalMutationResult::no_stage(translation_payload_error(
                &format!("Resource {resource_id} does not exist"),
                "RESOURCE_NOT_FOUND",
            ));
        }
        let keys = resolved_string_list_arg(&input.arguments, "translationKeys");
        let market_ids = resolved_string_list_arg(&input.arguments, "marketIds");
        let locales = resolved_string_list_arg(&input.arguments, "locales");
        if keys.is_empty() || locales.is_empty() {
            return LocalMutationResult::no_stage(payload_error("translations", vec![]));
        }
        let effective = self.localization_translations_for(&resource_id, None, None, None);
        let mut removed = Vec::new();
        for translation in effective {
            let key_matches = keys.iter().any(|key| translation["key"] == json!(key));
            let locale_matches = locales
                .iter()
                .any(|locale| translation["locale"] == json!(locale));
            let market_matches = if market_ids.is_empty() {
                translation["market"].is_null()
            } else {
                market_ids
                    .iter()
                    .any(|id| translation["market"]["id"] == json!(id))
            };
            if key_matches && locale_matches && market_matches {
                removed.push(translation);
            }
        }
        for translation in &removed {
            let identity = localization_translation_identity(translation);
            self.store
                .staged
                .localization_translation_tombstones
                .insert(identity.clone());
            self.store
                .staged
                .localization_translations
                .retain(|existing| localization_translation_identity(existing) != identity);
        }
        let staged = !removed.is_empty();
        if staged {
            self.store.staged.localization_dirty = true;
        }
        let removed = if removed.is_empty() {
            Value::Null
        } else {
            Value::Array(removed)
        };
        let value = json!({ "translations": removed, "userErrors": [] });
        if staged {
            LocalMutationResult::staged(value)
        } else {
            LocalMutationResult::no_stage(value)
        }
    }

    pub(in crate::proxy) fn localization_translatable_resource_value(
        &self,
        resource_id: &str,
    ) -> Value {
        let mut value = self
            .store
            .staged
            .observed_translatable_resources
            .get(resource_id)
            .cloned()
            .unwrap_or_else(|| json!({"resourceId": resource_id}));
        value["resourceId"] = json!(resource_id);
        if value.get("nestedTranslatableResources").is_none() {
            let nested_resources = match shopify_gid_resource_type(resource_id) {
                Some("Product") => self
                    .store
                    .product_by_id(resource_id)
                    .and_then(|product| product.extra_fields.get("nestedTranslatableResources")),
                Some("Collection") => self
                    .store
                    .collection_by_id(resource_id)
                    .and_then(|collection| collection.get("nestedTranslatableResources")),
                _ => None,
            };
            if let Some(nested_resources) = nested_resources {
                value["nestedTranslatableResources"] = nested_resources.clone();
            }
        }
        value
    }

    pub(in crate::proxy) fn localization_translatable_resources_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        scope_key: Option<&str>,
    ) -> Value {
        let resource_type = resolved_string_field(arguments, "resourceType")
            .unwrap_or_else(|| "PRODUCT".to_string());
        if let Some(observed) = scope_key.and_then(|scope| {
            self.store
                .staged
                .localization_observed_read_values
                .get(scope)
        }) {
            if !self.localization_catalog_has_overlay(&resource_type, None) {
                return observed.clone();
            }
            return self.localization_translatable_connection_overlay(
                observed,
                arguments,
                &resource_type,
            );
        }
        let records = self
            .localization_translatable_resource_ids()
            .into_iter()
            .filter(|id| localization_resource_type_matches(id, &resource_type))
            .collect::<Vec<_>>();
        connection_value_with_args(
            records
                .into_iter()
                .map(|id| self.localization_translatable_resource_value(&id))
                .collect(),
            arguments,
            |resource| {
                resource
                    .get("resourceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            },
        )
    }

    pub(in crate::proxy) fn localization_translatable_resources_by_ids_connection(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        scope_key: Option<&str>,
    ) -> Value {
        let selected_ids = resolved_string_list_arg(arguments, "resourceIds")
            .into_iter()
            .collect::<BTreeSet<_>>();
        let resource_types = selected_ids
            .iter()
            .filter_map(|id| shopify_gid_resource_type(id))
            .map(|kind| kind.to_ascii_uppercase())
            .collect::<BTreeSet<_>>();
        let has_overlay = resource_types
            .iter()
            .any(|kind| self.localization_catalog_has_overlay(kind, Some(&selected_ids)));
        if !has_overlay {
            if let Some(observed) = scope_key.and_then(|scope| {
                self.store
                    .staged
                    .localization_observed_read_values
                    .get(scope)
            }) {
                return observed.clone();
            }
        }
        let observed_rows = scope_key
            .and_then(|scope| {
                self.store
                    .staged
                    .localization_observed_read_values
                    .get(scope)
            })
            .map(observed_connection_rows)
            .unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut records = observed_rows
            .iter()
            .filter_map(|row| translatable_resource_id_from_value(&row.node))
            .filter(|id| selected_ids.contains(*id))
            .filter(|id| seen.insert((*id).to_string()))
            .filter(|id| self.localization_translatable_resource_exists(id))
            .map(str::to_string)
            .collect::<Vec<_>>();
        records.extend(
            resolved_string_list_arg(arguments, "resourceIds")
                .into_iter()
                .filter(|id| seen.insert(id.clone()))
                .filter(|id| self.localization_translatable_resource_exists(id)),
        );
        if observed_rows.is_empty() && resolved_bool_field(arguments, "reverse").unwrap_or(false) {
            records.reverse();
        }
        let mut observed_cursors = BTreeMap::new();
        for row in observed_rows {
            let Some(resource_id) = translatable_resource_id_from_value(&row.node) else {
                continue;
            };
            observed_cursors
                .entry(resource_id.to_string())
                .and_modify(|existing: &mut Option<String>| {
                    if existing.is_none() && row.cursor.is_some() {
                        *existing = row.cursor.clone();
                    }
                })
                .or_insert(row.cursor);
        }
        let values = records
            .into_iter()
            .map(|id| {
                (
                    observed_cursors
                        .get(&id)
                        .cloned()
                        .flatten()
                        .unwrap_or_else(|| id.clone()),
                    self.localization_translatable_resource_value(&id),
                )
            })
            .collect::<Vec<_>>();
        let (values, page_info) =
            connection_window(values.as_slice(), arguments, |row| row.0.clone());
        let nodes = values
            .iter()
            .map(|(_, node)| node.clone())
            .collect::<Vec<_>>();
        let edges = values
            .into_iter()
            .map(|(cursor, node)| json!({ "cursor": cursor, "node": node }))
            .collect::<Vec<_>>();
        json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info })
    }

    fn localization_translatable_connection_overlay(
        &self,
        observed: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        resource_type: &str,
    ) -> Value {
        let mut rows = observed_connection_rows(observed)
            .into_iter()
            .filter_map(|row| {
                let id = translatable_resource_id_from_value(&row.node)?.to_string();
                if !localization_resource_type_matches(&id, resource_type)
                    || self.store.products.staged.tombstones.contains(&id)
                    || self.store.staged.collections.tombstones.contains(&id)
                {
                    return None;
                }
                Some(ObservedConnectionRow {
                    cursor: row.cursor,
                    node: self.localization_translatable_resource_value(&id),
                })
            })
            .collect::<Vec<_>>();
        let mut seen = rows
            .iter()
            .filter_map(|row| translatable_resource_id_from_value(&row.node))
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let mut local_rows = self
            .localization_translatable_resource_ids()
            .into_iter()
            .filter(|id| is_synthetic_gid(id))
            .filter(|id| localization_resource_type_matches(id, resource_type))
            .filter(|id| seen.insert(id.clone()))
            .map(|id| ObservedConnectionRow {
                cursor: Some(id.clone()),
                node: self.localization_translatable_resource_value(&id),
            })
            .collect::<Vec<_>>();
        local_rows.sort_by_key(|row| {
            translatable_resource_id_from_value(&row.node)
                .unwrap_or_default()
                .to_string()
        });
        let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
        if reverse {
            local_rows.reverse();
            local_rows.extend(rows);
            rows = local_rows;
        } else if !connection_has_next_page(observed) {
            rows.extend(local_rows);
        }

        let original_len = rows.len();
        if let Some(first) = resolved_int_field(arguments, "first").filter(|first| *first >= 0) {
            rows.truncate(first as usize);
        }
        if let Some(last) = resolved_int_field(arguments, "last").filter(|last| *last >= 0) {
            let keep = last as usize;
            if rows.len() > keep {
                rows = rows.split_off(rows.len() - keep);
            }
        }
        let upstream_page_info = observed
            .get("pageInfo")
            .cloned()
            .unwrap_or_else(empty_page_info);
        let start_cursor = rows.first().and_then(|row| row.cursor.clone()).or_else(|| {
            upstream_page_info
                .get("startCursor")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        let end_cursor = rows.last().and_then(|row| row.cursor.clone()).or_else(|| {
            upstream_page_info
                .get("endCursor")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        let has_next = upstream_page_info["hasNextPage"].as_bool().unwrap_or(false)
            || rows.len() < original_len;
        let has_previous = upstream_page_info["hasPreviousPage"]
            .as_bool()
            .unwrap_or(false);
        let nodes = rows.iter().map(|row| row.node.clone()).collect::<Vec<_>>();
        let edges = rows
            .into_iter()
            .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
            .collect::<Vec<_>>();
        json!({
            "nodes": nodes,
            "edges": edges,
            "pageInfo": connection_page_info(
                has_next,
                has_previous,
                start_cursor,
                end_cursor
            )
        })
    }

    /// A staged tombstone can remove a row from an otherwise partial upstream
    /// page. Fetch at most the missing window plus the relevant staged removals
    /// from the adjacent opaque cursor, then merge that bounded page into the
    /// exact observed scope. This never walks the complete translatable catalog.
    fn refill_localization_connection_if_needed(
        &mut self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        selected_fields: &[SelectedField],
        scope_key: &str,
        request: &Request,
    ) {
        let Some(requested) = resolved_int_field(arguments, "first")
            .or_else(|| resolved_int_field(arguments, "last"))
            .filter(|requested| *requested > 0)
            .map(|requested| requested as usize)
        else {
            return;
        };
        let Some(observed) = self
            .store
            .staged
            .localization_observed_read_values
            .get(scope_key)
            .cloned()
        else {
            return;
        };
        let projected = match root_name {
            "translatableResources" => {
                let resource_type = resolved_string_field(arguments, "resourceType")
                    .unwrap_or_else(|| "PRODUCT".to_string());
                self.localization_translatable_connection_overlay(
                    &observed,
                    arguments,
                    &resource_type,
                )
            }
            "translatableResourcesByIds" => self
                .localization_translatable_resources_by_ids_connection(arguments, Some(scope_key)),
            _ => return,
        };
        let projected_len = observed_connection_rows(&projected).len();
        if projected_len >= requested {
            return;
        }
        let backward = arguments.contains_key("last");
        let more_available = if backward {
            observed
                .pointer("/pageInfo/hasPreviousPage")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        } else {
            connection_has_next_page(&observed)
        };
        if !more_available {
            return;
        }
        let boundary_name = if backward { "startCursor" } else { "endCursor" };
        let Some(boundary_cursor) = observed
            .pointer(&format!("/pageInfo/{boundary_name}"))
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let staged_removal_count = self.localization_connection_removal_count(root_name, arguments);
        let refill_count = requested
            .saturating_sub(projected_len)
            .saturating_add(staged_removal_count)
            .max(1)
            .min(i64::MAX as usize) as i64;
        let (query, variables) = localization_connection_refill_query(
            root_name,
            arguments,
            selected_fields,
            backward,
            refill_count,
            &boundary_cursor,
        );
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": "LocalizationTranslatableConnectionRefill",
                "variables": variables
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return;
        }
        let Some(refill) = response
            .body
            .pointer("/data/refill")
            .filter(|value| value.is_object())
        else {
            return;
        };
        let refill = refill.clone();
        for row in observed_connection_rows(&refill) {
            self.observe_translatable_resource(&row.node);
        }
        let combined = combine_localization_connection_pages(&observed, &refill, backward);
        self.store
            .staged
            .localization_observed_read_values
            .insert(scope_key.to_string(), combined);
    }

    fn localization_connection_removal_count(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> usize {
        let selected_ids = (root_name == "translatableResourcesByIds").then(|| {
            resolved_string_list_arg(arguments, "resourceIds")
                .into_iter()
                .collect::<BTreeSet<_>>()
        });
        let resource_type =
            resolved_string_field(arguments, "resourceType").map(|kind| kind.to_ascii_uppercase());
        self.store
            .products
            .staged
            .tombstones
            .iter()
            .chain(self.store.staged.collections.tombstones.iter())
            .filter(|id| {
                selected_ids
                    .as_ref()
                    .is_none_or(|selected| selected.contains(id.as_str()))
                    && resource_type
                        .as_ref()
                        .is_none_or(|kind| localization_resource_type_matches(id, kind))
            })
            .count()
    }

    fn localization_markets_connection_with_hydration(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
        request: &Request,
        hydrate_if_missing: bool,
    ) -> Value {
        let mut records = self
            .store
            .staged
            .markets
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if records.is_empty() && hydrate_if_missing {
            records = self.hydrate_localization_markets(arguments, response_key, request);
        }
        staged_connection_value_with_args(
            records,
            arguments,
            market_search_decision,
            market_sort_key,
            Value::clone,
            value_id_cursor,
        )
    }

    fn hydrate_localization_markets(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        response_key: &str,
        request: &Request,
    ) -> Vec<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return Vec::new();
        }
        let first = resolved_int_field(arguments, "first").unwrap_or(50).max(0);
        if first == 0 {
            return Vec::new();
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query LocalizationMarketsHydrate($first: Int!) { markets(first: $first) { nodes { id name handle status type } } }",
                "operationName": "LocalizationMarketsHydrate",
                "variables": { "first": first }
            }),
        );
        self.stage_observed_localization_source_data(&response.body["data"]);
        if response.status >= 400 {
            return self.hydrate_localization_markets_from_original_request(response_key, request);
        }
        let records = response.body["data"]["markets"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if records.is_empty() && response.body["data"]["markets"].is_null() {
            return self.hydrate_localization_markets_from_original_request(response_key, request);
        }
        self.stage_observed_localization_markets(&records);
        records
    }

    fn hydrate_localization_markets_from_original_request(
        &mut self,
        response_key: &str,
        request: &Request,
    ) -> Vec<Value> {
        let response = (self.upstream_transport)(request.clone());
        self.stage_observed_localization_source_data(&response.body["data"]);
        if response.status >= 400 {
            return Vec::new();
        }
        let market_connection = &response.body["data"][response_key];
        let mut records = market_connection["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if records.is_empty() {
            records = market_connection["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|edge| edge.get("node").cloned())
                .collect();
        }
        self.stage_observed_localization_markets(&records);
        records
    }

    fn stage_observed_localization_source_data(&mut self, data: &Value) {
        let Some(data) = data.as_object() else {
            return;
        };
        for value in data.values() {
            if let Some(locales) = value.as_array() {
                self.stage_observed_shop_locales(locales);
            }
            if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
                self.stage_observed_localization_markets(nodes);
            }
        }
    }

    fn stage_observed_shop_locales(&mut self, locales: &[Value]) {
        for locale in locales {
            let Some(locale_code) = locale.get("locale").and_then(Value::as_str) else {
                continue;
            };
            if !locale.get("name").is_some_and(Value::is_string)
                || !locale.get("primary").is_some_and(Value::is_boolean)
                || !locale.get("published").is_some_and(Value::is_boolean)
            {
                continue;
            }
            self.store
                .staged
                .observed_shop_locales
                .entry(locale_code.to_string())
                .and_modify(|existing| {
                    *existing = shallow_merged_object(existing.clone(), locale.clone());
                })
                .or_insert_with(|| locale.clone());
        }
    }

    fn stage_observed_localization_markets(&mut self, records: &[Value]) {
        for market in records {
            let Some(id) = market.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !is_shopify_gid_of_type(id, "Market")
                || !market.get("name").is_some_and(Value::is_string)
                || !market.get("handle").is_some_and(Value::is_string)
                || !market.get("status").is_some_and(Value::is_string)
            {
                continue;
            }
            self.store
                .staged
                .markets
                .insert(id.to_string(), market.clone());
        }
    }

    /// Record a market-localizable resource observed upstream: index its
    /// `marketLocalizableContent` by `resourceId` (existence + valid keys/digests)
    /// and stage any pre-existing `marketLocalizations` so read-after-write filtering
    /// reflects Shopify's prior state for an arbitrary backend.
    pub(in crate::proxy) fn stage_observed_market_localizable_resource(
        &mut self,
        resource: &Value,
    ) {
        let Some(resource_id) = resource.get("resourceId").and_then(Value::as_str) else {
            return;
        };
        if let Some(content) = resource
            .get("marketLocalizableContent")
            .filter(|content| content.is_array())
        {
            self.store
                .staged
                .localization_resources
                .insert(resource_id.to_string(), content.clone());
        }
        let Some(localizations) = resource
            .get("marketLocalizations")
            .and_then(Value::as_array)
            .filter(|localizations| !localizations.is_empty())
        else {
            return;
        };
        for localization in localizations {
            let key = localization.get("key").and_then(Value::as_str);
            let market_id = localization
                .get("market")
                .and_then(|market| market.get("id"))
                .and_then(Value::as_str);
            self.store
                .staged
                .localization_translations
                .retain(|existing| {
                    existing["resourceId"].as_str() != Some(resource_id)
                        || existing["key"].as_str() != key
                        || existing["market"]["id"].as_str() != market_id
                });
            let mut record = serde_json::Map::new();
            record.insert("resourceId".to_string(), json!(resource_id));
            for field in ["key", "value", "updatedAt", "outdated", "market"] {
                if let Some(value) = localization.get(field) {
                    record.insert(field.to_string(), value.clone());
                }
            }
            self.store
                .staged
                .localization_translations
                .push(Value::Object(record));
        }
    }

    pub(in crate::proxy) fn localization_should_fetch_upstream(
        &self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        scope_key: &str,
    ) -> bool {
        if !matches!(
            root_field,
            "availableLocales"
                | "shopLocales"
                | "translatableResource"
                | "translatableResources"
                | "translatableResourcesByIds"
        ) {
            return false;
        }
        if self
            .store
            .staged
            .localization_complete_scopes
            .contains(scope_key)
        {
            return false;
        }
        if root_field == "translatableResource" {
            let resource_id = resolved_string_field(arguments, "resourceId").unwrap_or_default();
            if self
                .store
                .staged
                .missing_translatable_resource_ids
                .contains(&resource_id)
                || self.localization_resource_is_local_authoritative(&resource_id)
            {
                return false;
            }
        }
        if root_field == "translatableResourcesByIds"
            && resolved_string_list_arg(arguments, "resourceIds")
                .iter()
                .all(|resource_id| {
                    self.store
                        .staged
                        .missing_translatable_resource_ids
                        .contains(resource_id)
                        || self.localization_resource_is_local_authoritative(resource_id)
                })
        {
            return false;
        }
        true
    }

    fn localization_resource_is_local_authoritative(&self, resource_id: &str) -> bool {
        let product_tombstone = self.store.products.staged.tombstones.contains(resource_id);
        let collection_tombstone = self
            .store
            .staged
            .collections
            .tombstones
            .contains(resource_id);
        product_tombstone
            || collection_tombstone
            || (is_synthetic_gid(resource_id)
                && match shopify_gid_resource_type(resource_id) {
                    Some("Product") => self.store.has_product(resource_id),
                    Some("Collection") => self.store.collection_by_id(resource_id).is_some(),
                    _ => false,
                })
    }

    fn localization_has_relevant_overlay(
        &self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        match root_field {
            "availableLocales" => false,
            "shopLocales" => {
                !self.store.staged.shop_locales.is_empty()
                    || !self.store.staged.deleted_shop_locale_codes.is_empty()
            }
            "translatableResource" => {
                let resource_id =
                    resolved_string_field(arguments, "resourceId").unwrap_or_default();
                self.localization_resource_is_local_authoritative(&resource_id)
                    || self
                        .store
                        .staged
                        .localization_translations
                        .iter()
                        .any(|translation| {
                            translation["resourceId"].as_str() == Some(resource_id.as_str())
                        })
                    || self
                        .store
                        .staged
                        .localization_translation_tombstones
                        .iter()
                        .any(|identity| identity.contains(&resource_id))
            }
            "translatableResources" => {
                let resource_type = resolved_string_field(arguments, "resourceType")
                    .unwrap_or_else(|| "PRODUCT".to_string());
                self.localization_catalog_has_overlay(&resource_type, None)
            }
            "translatableResourcesByIds" => {
                let ids = resolved_string_list_arg(arguments, "resourceIds");
                ids.iter().any(|id| {
                    self.localization_resource_is_local_authoritative(id)
                        || self
                            .store
                            .staged
                            .localization_translations
                            .iter()
                            .any(|translation| {
                                translation["resourceId"].as_str() == Some(id.as_str())
                            })
                        || self
                            .store
                            .staged
                            .localization_translation_tombstones
                            .iter()
                            .any(|identity| identity.contains(id))
                })
            }
            _ => false,
        }
    }

    pub(in crate::proxy) fn localization_operation_has_local_overlay(
        &self,
        roots: &[crate::resolver_registry::OperationRootInvocation],
    ) -> bool {
        roots.iter().any(|root| {
            matches!(
                root.name.as_str(),
                "shopLocales"
                    | "translatableResource"
                    | "translatableResources"
                    | "translatableResourcesByIds"
            ) && self.localization_has_relevant_overlay(
                &root.name,
                &resolved_arguments_from_json(&root.arguments),
            )
        })
    }

    fn localization_catalog_has_overlay(
        &self,
        resource_type: &str,
        selected_ids: Option<&BTreeSet<String>>,
    ) -> bool {
        let selected = |id: &str| selected_ids.is_none_or(|ids| ids.contains(id));
        let matching =
            |id: &str| selected(id) && localization_resource_type_matches(id, resource_type);
        self.store
            .products
            .staged
            .records
            .keys()
            .chain(self.store.products.staged.tombstones.iter())
            .any(|id| matching(id))
            || self
                .store
                .staged
                .collections
                .records
                .keys()
                .chain(self.store.staged.collections.tombstones.iter())
                .any(|id| matching(id))
            || self
                .store
                .staged
                .localization_translations
                .iter()
                .filter_map(|translation| translation["resourceId"].as_str())
                .any(matching)
            || self
                .store
                .staged
                .localization_translation_tombstones
                .iter()
                .any(|identity| {
                    shopify_gid_resource_type_from_identity(identity)
                        .is_some_and(|kind| kind == resource_type)
                })
    }

    fn observe_localization_read(
        &mut self,
        root_field: &str,
        response_key: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        scope_key: &str,
        data: &Value,
    ) {
        let value = data.get(response_key).cloned().unwrap_or(Value::Null);
        self.store
            .staged
            .localization_complete_scopes
            .insert(scope_key.to_string());
        self.store
            .staged
            .localization_observed_read_values
            .insert(scope_key.to_string(), value.clone());

        match root_field {
            "availableLocales" => self.observe_available_locales(&value),
            "shopLocales" => self.observe_shop_locales(&value),
            "translatableResource" => {
                let resource_id =
                    resolved_string_field(arguments, "resourceId").unwrap_or_default();
                if value.is_null() {
                    self.store
                        .staged
                        .missing_translatable_resource_ids
                        .insert(resource_id);
                } else {
                    self.observe_translatable_resource(&value);
                }
            }
            "translatableResources" | "translatableResourcesByIds" => {
                let rows = observed_connection_rows(&value);
                let observed_ids = rows
                    .iter()
                    .filter_map(|row| translatable_resource_id_from_value(&row.node))
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>();
                for row in rows {
                    self.observe_translatable_resource(&row.node);
                }
                if root_field == "translatableResourcesByIds"
                    && localization_by_ids_scope_accounts_for_all(arguments, &value)
                {
                    for id in resolved_string_list_arg(arguments, "resourceIds") {
                        if !observed_ids.contains(&id)
                            && !self.localization_resource_is_local_authoritative(&id)
                        {
                            self.store
                                .staged
                                .missing_translatable_resource_ids
                                .insert(id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn observe_available_locales(&mut self, value: &Value) {
        for locale in value.as_array().into_iter().flatten() {
            let (Some(code), Some(name)) = (
                locale.get("isoCode").and_then(Value::as_str),
                locale.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            self.store
                .staged
                .observed_available_locales
                .insert(code.to_string(), name.to_string());
        }
    }

    fn observe_shop_locales(&mut self, value: &Value) {
        for locale in value.as_array().into_iter().flatten() {
            let Some(code) = locale.get("locale").and_then(Value::as_str) else {
                continue;
            };
            self.store
                .staged
                .observed_shop_locales
                .entry(code.to_string())
                .and_modify(|existing| {
                    *existing = shallow_merged_object(existing.clone(), locale.clone());
                })
                .or_insert_with(|| locale.clone());
        }
    }

    fn observe_translatable_resource(&mut self, resource: &Value) {
        let Some(resource_id) = translatable_resource_id_from_value(resource) else {
            return;
        };
        self.store
            .staged
            .missing_translatable_resource_ids
            .remove(resource_id);
        self.store
            .staged
            .observed_translatable_resources
            .entry(resource_id.to_string())
            .and_modify(|existing| {
                *existing = shallow_merged_object(existing.clone(), resource.clone());
            })
            .or_insert_with(|| resource.clone());

        let Some(object) = resource.as_object() else {
            return;
        };
        for (field_name, value) in object {
            if !field_name.starts_with("translations") {
                continue;
            }
            for translation in value.as_array().into_iter().flatten() {
                let mut translation = translation.clone();
                translation["resourceId"] = json!(resource_id);
                let identity = localization_translation_identity(&translation);
                self.store
                    .staged
                    .observed_localization_translations
                    .retain(|existing| localization_translation_identity(existing) != identity);
                self.store
                    .staged
                    .observed_localization_translations
                    .push(translation);
            }
        }
    }

    /// Observe reusable localization rows without treating any one response as
    /// completeness for unrelated roots or arguments.
    pub(in crate::proxy) fn hydrate_localization_from_upstream(&mut self, body: &Value) {
        let Some(data) = body.get("data").and_then(Value::as_object) else {
            return;
        };
        // Scan every top-level field (queries alias fields freely, e.g.
        // `allShopLocales: shopLocales`, `single: translatableResource`).
        let mut resources: Vec<Value> = Vec::new();
        for value in data.values() {
            // shopLocales / availableLocales arrays.
            if let Some(items) = value.as_array() {
                if items
                    .iter()
                    .any(|item| item.get("isoCode").and_then(Value::as_str).is_some())
                {
                    self.observe_available_locales(value);
                } else if items.iter().any(|item| item.get("locale").is_some()) {
                    self.observe_shop_locales(value);
                }
            }
            // translatableResource (single) or a connection of resources.
            if value.get("resourceId").and_then(Value::as_str).is_some() {
                resources.push(value.clone());
            }
            if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
                resources.extend(
                    nodes
                        .iter()
                        .filter(|node| node.get("resourceId").and_then(Value::as_str).is_some())
                        .cloned(),
                );
            }
        }
        for resource in &resources {
            self.observe_translatable_resource(resource);
        }
    }

    fn localization_shop_locale_added(&self, locale: &str) -> bool {
        !self.store.staged.deleted_shop_locale_codes.contains(locale)
            && (self.store.base.shop_locales.contains_key(locale)
                || self.store.staged.observed_shop_locales.contains_key(locale)
                || self.store.staged.shop_locales.contains_key(locale))
    }

    pub(in crate::proxy) fn localization_translatable_resource_ids(&self) -> Vec<String> {
        let mut ids = self
            .store
            .staged
            .localization_translations
            .iter()
            .filter_map(|translation| translation["resourceId"].as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        ids.extend(
            self.store
                .staged
                .observed_translatable_resources
                .keys()
                .cloned(),
        );
        ids.extend(self.store.products().into_iter().map(|product| product.id));
        ids.extend(self.store.base.localization_product_ids.iter().cloned());
        ids.extend(
            self.store
                .staged
                .collections
                .iter()
                .map(|(id, _)| id.clone()),
        );
        ids.sort();
        ids.dedup();
        ids
    }

    pub(in crate::proxy) fn localization_translations_for(
        &self,
        resource_id: &str,
        locale: Option<&str>,
        market_id: Option<&str>,
        outdated: Option<bool>,
    ) -> Vec<Value> {
        let translations = self
            .store
            .staged
            .observed_localization_translations
            .iter()
            .chain(self.store.staged.localization_translations.iter())
            .filter(|translation| translation["resourceId"].as_str() == Some(resource_id))
            .filter(|translation| {
                locale.is_none_or(|locale| translation["locale"].as_str() == Some(locale))
            })
            .filter(|translation| match market_id {
                Some(market_id) => translation["market"]["id"].as_str() == Some(market_id),
                None => true,
            })
            .filter(|translation| {
                outdated.is_none_or(|outdated| {
                    translation["outdated"].as_bool().unwrap_or(false) == outdated
                })
            })
            .filter(|translation| {
                !self
                    .store
                    .staged
                    .localization_translation_tombstones
                    .contains(&localization_translation_identity(translation))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut positions = BTreeMap::new();
        let mut merged = Vec::new();
        for translation in translations {
            let identity = localization_translation_identity(&translation);
            if let Some(index) = positions.get(&identity).copied() {
                merged[index] = translation;
            } else {
                positions.insert(identity, merged.len());
                merged.push(translation);
            }
        }
        merged
    }

    pub(in crate::proxy) fn localization_translatable_resource_exists(
        &self,
        resource_id: &str,
    ) -> bool {
        if resource_id.is_empty() {
            return false;
        }
        if self
            .store
            .staged
            .missing_translatable_resource_ids
            .contains(resource_id)
            || self.store.products.staged.tombstones.contains(resource_id)
            || self
                .store
                .staged
                .collections
                .tombstones
                .contains(resource_id)
        {
            return false;
        }
        if self
            .store
            .staged
            .observed_translatable_resources
            .contains_key(resource_id)
        {
            return true;
        }
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => self.store.has_localization_product(resource_id),
            Some("Collection") => self.store.collection_by_id(resource_id).is_some(),
            Some(_) => false,
            _ => false,
        }
    }

    /// Mutations must reject resource IDs the proxy cannot resolve locally, while
    /// read roots still keep Shopify-like empty placeholders for unmodeled types.
    fn localization_translation_mutation_resource_exists(&self, resource_id: &str) -> bool {
        if resource_id.is_empty() {
            return false;
        }
        if self
            .store
            .staged
            .missing_translatable_resource_ids
            .contains(resource_id)
        {
            return false;
        }
        if self
            .store
            .staged
            .observed_translatable_resources
            .contains_key(resource_id)
        {
            return true;
        }
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => self.store.has_localization_product(resource_id),
            Some("Collection") => self.store.collection_by_id(resource_id).is_some(),
            Some("PackingSlipTemplate") => true,
            _ => false,
        }
    }

    fn localization_translatable_content(&self, resource_id: &str) -> Vec<Value> {
        if let Some(content) = self
            .store
            .staged
            .observed_translatable_resources
            .get(resource_id)
            .and_then(|resource| resource.get("translatableContent"))
            .and_then(Value::as_array)
        {
            return content.clone();
        }
        let locale = self.localization_primary_locale();
        if is_shopify_gid_of_type(resource_id, "Product") {
            return self
                .store
                .product_staged_or_base(resource_id)
                .map(|product| localization_product_translatable_content(&product, &locale))
                .unwrap_or_default();
        }
        if is_shopify_gid_of_type(resource_id, "Collection") {
            return self
                .store
                .collection_by_id(resource_id)
                .map(|collection| localization_collection_translatable_content(collection, &locale))
                .unwrap_or_default();
        }
        Vec::new()
    }

    pub(in crate::proxy) fn localization_primary_locale(&self) -> String {
        self.localization_shop_locales(None)
            .into_iter()
            .find(|locale| locale.get("primary").and_then(Value::as_bool) == Some(true))
            .and_then(|locale| {
                locale
                    .get("locale")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "en".to_string())
    }

    /// The current source-content value for a translatable resource field, when the
    /// proxy holds authoritative local state for it. Translatable content digests are
    /// `sha256(value)` of the source string (verified against live Shopify captures),
    /// so this lets the register path reject stale/incorrect `translatableContentDigest`
    /// inputs exactly like Shopify. Returns `None` for resources whose source content
    /// the proxy hasn't observed (hydrated-only ids), in which case digest validation
    /// is skipped — matching Shopify's captured "content not found -> no digest error" behavior.
    fn localization_source_content_value(&self, resource_id: &str, key: &str) -> Option<String> {
        if let Some(value) = self
            .store
            .staged
            .observed_translatable_resources
            .get(resource_id)
            .and_then(|resource| resource.get("translatableContent"))
            .and_then(Value::as_array)
            .and_then(|content| {
                content
                    .iter()
                    .find(|entry| entry.get("key").and_then(Value::as_str) == Some(key))
            })
            .and_then(|entry| entry.get("value"))
            .and_then(Value::as_str)
        {
            return Some(value.to_string());
        }
        if is_shopify_gid_of_type(resource_id, "Product") {
            let product = self.store.product_staged_or_base(resource_id)?;
            let value = match key {
                "title" => product.title.clone(),
                "body_html" => product.description_html.clone(),
                "handle" => product.handle.clone(),
                "product_type" => product.product_type.clone(),
                "meta_title" => product.seo_title.clone(),
                "meta_description" => product.seo_description.clone(),
                _ => return None,
            };
            return Some(value);
        }
        if is_shopify_gid_of_type(resource_id, "Collection") {
            let collection = self.store.collection_by_id(resource_id)?;
            let value = match key {
                "title" => collection
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "body_html" => collection
                    .get("descriptionHtml")
                    .or_else(|| collection.get("bodyHtml"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "handle" => collection
                    .get("handle")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "meta_title" => collection
                    .pointer("/seo/title")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "meta_description" => collection
                    .pointer("/seo/description")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                _ => return None,
            };
            return Some(value.to_string());
        }
        None
    }

    fn localization_shop_level_translation_value(
        &self,
        resource_id: &str,
        key: &str,
        locale: &str,
    ) -> Option<String> {
        self.localization_translations_for(resource_id, Some(locale), None, None)
            .into_iter()
            .rev()
            .find(|translation| {
                translation["key"].as_str() == Some(key) && translation["market"].is_null()
            })
            .and_then(|translation| translation["value"].as_str().map(ToString::to_string))
    }

    fn localization_resource_has_modeled_translation_keys(&self, resource_id: &str) -> bool {
        self.store
            .staged
            .observed_translatable_resources
            .get(resource_id)
            .and_then(|resource| resource.get("translatableContent"))
            .is_some_and(Value::is_array)
            || is_shopify_gid_of_type(resource_id, "Product")
            || (is_shopify_gid_of_type(resource_id, "Collection")
                && self.store.collection_by_id(resource_id).is_some())
    }

    fn localization_translation_key_is_valid(&self, resource_id: &str, key: &str) -> bool {
        if let Some(content) = self
            .store
            .staged
            .observed_translatable_resources
            .get(resource_id)
            .and_then(|resource| resource.get("translatableContent"))
            .and_then(Value::as_array)
        {
            return content
                .iter()
                .any(|entry| entry.get("key").and_then(Value::as_str) == Some(key));
        }
        if is_shopify_gid_of_type(resource_id, "Product") {
            return matches!(
                key,
                "title"
                    | "body_html"
                    | "handle"
                    | "product_type"
                    | "meta_title"
                    | "meta_description"
            );
        }
        if is_shopify_gid_of_type(resource_id, "Collection") {
            return matches!(
                key,
                "title" | "body_html" | "handle" | "meta_title" | "meta_description"
            );
        }
        false
    }

    fn localization_translation_key_is_market_customizable(
        &self,
        resource_id: &str,
        key: &str,
    ) -> bool {
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => matches!(key, "title" | "body_html" | "product_type"),
            Some("Collection") => matches!(key, "title" | "body_html"),
            _ => false,
        }
    }

    /// Mirror Shopify's web-presence ↔ alternate-locale sync. When a non-primary
    /// locale is associated with one or more market web presences via
    /// `shopLocaleEnable`/`shopLocaleUpdate`, Shopify reflects it on the
    /// `MarketWebPresence` itself: every target presence gains the locale in
    /// `alternateLocales` (unpublished) plus a matching `rootUrls` entry. On an
    /// update the association is authoritative, so non-target presences lose the
    /// locale (`replace = true`); enable only adds. The downstream `webPresences`
    /// read is served from `staged.web_presences`, so the staged records are
    /// mutated in place. Modeled from captured localization mutation behavior.
    fn sync_web_presence_locales(&mut self, locale: &str, target_ids: &[String], replace: bool) {
        let primary_locale = self.localization_primary_locale();
        if locale == primary_locale {
            return;
        }
        let name = self
            .localization_available_locale_name(locale)
            .map(str::to_string);
        for (id, record) in self.store.staged.web_presences.iter_mut() {
            if target_ids.iter().any(|target| target == id) {
                web_presence_add_locale(record, locale, name.as_deref());
            } else if replace {
                web_presence_remove_locale(record, locale);
            }
        }
    }
}
