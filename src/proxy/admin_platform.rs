use super::*;

use crate::{admin_graphql::FieldResolverInvocation, resolver_registry::RootChildInvocation};

const TAXONOMY_CATEGORY_NODE_FIELDS: &[&str] = &[
    "ancestorIds",
    "childrenIds",
    "fullName",
    "id",
    "isArchived",
    "isLeaf",
    "isRoot",
    "level",
    "name",
    "parentId",
];

pub(in crate::proxy) fn admin_platform_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    let mut registrations = vec![FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "Taxonomy",
        "categories",
        taxonomy_categories_field,
    )];
    for (parent_type, fields) in [
        ("ApiVersion", &["displayName", "handle", "supported"][..]),
        (
            "TaxonomyCategoryConnection",
            &["edges", "nodes", "pageInfo"][..],
        ),
        ("TaxonomyCategoryEdge", &["cursor", "node"][..]),
    ] {
        registrations.extend(fields.iter().map(|field| {
            FieldResolverRegistration::property(ApiSurface::Admin, parent_type, field)
        }));
    }
    registrations
}

fn taxonomy_categories_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(proxy.taxonomy_categories_connection(invocation.api_version, &invocation.arguments))
}

impl DraftProxy {
    pub(crate) fn admin_platform_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        match invocation.root_name {
            "publicApiVersions" => self.admin_public_api_versions_root(invocation),
            "taxonomy" => self.admin_taxonomy_root(invocation),
            root_name => ResolverOutcome::error(format!(
                "Admin platform resolver does not own `{root_name}`"
            )),
        }
    }

    fn admin_public_api_versions_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || self.admin_public_api_versions_support_requested_paths(
                &invocation.requested_field_paths,
            )
        {
            return ResolverOutcome::value(Value::Array(
                self.store.base.admin_public_api_versions.clone(),
            ));
        }

        let result = self
            .cached_or_forward_upstream_graphql_result(invocation.request, invocation.response_key);
        if result.transport_succeeded {
            self.observe_admin_public_api_versions(
                &result.outcome.value,
                result.outcome.errors.is_empty(),
            );
        }
        result.outcome
    }

    fn admin_public_api_versions_support_requested_paths(
        &self,
        requested_paths: &BTreeSet<Vec<String>>,
    ) -> bool {
        if !self.store.base.admin_public_api_versions_observed {
            return false;
        }
        requested_paths.iter().all(|path| {
            path.first().is_none_or(|field| {
                field == "__typename"
                    || self
                        .store
                        .base
                        .admin_public_api_versions
                        .iter()
                        .all(|version| version.get(field).is_some())
            })
        })
    }

    fn observe_admin_public_api_versions(&mut self, value: &Value, complete: bool) {
        let Some(versions) = value.as_array() else {
            return;
        };
        let previous =
            complete.then(|| std::mem::take(&mut self.store.base.admin_public_api_versions));
        for (index, observed) in versions
            .iter()
            .filter(|value| value.is_object())
            .enumerate()
        {
            let existing_index = observed
                .get("handle")
                .and_then(Value::as_str)
                .and_then(|handle| {
                    previous
                        .as_ref()
                        .unwrap_or(&self.store.base.admin_public_api_versions)
                        .iter()
                        .position(|version| {
                            version.get("handle").and_then(Value::as_str) == Some(handle)
                        })
                })
                .or_else(|| {
                    let versions = previous
                        .as_ref()
                        .unwrap_or(&self.store.base.admin_public_api_versions);
                    (index < versions.len()).then_some(index)
                });
            if complete {
                let mut merged = existing_index
                    .and_then(|existing_index| previous.as_ref()?.get(existing_index))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                merge_observed_object(&mut merged, observed);
                self.store.base.admin_public_api_versions.push(merged);
            } else if let Some(existing_index) = existing_index {
                merge_observed_object(
                    &mut self.store.base.admin_public_api_versions[existing_index],
                    observed,
                );
            } else {
                self.store
                    .base
                    .admin_public_api_versions
                    .push(observed.clone());
            }
        }
        if complete {
            self.store.base.admin_public_api_versions_observed = true;
        }
    }

    fn admin_taxonomy_root(&mut self, invocation: RootInvocation<'_>) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || self.taxonomy_root_can_answer_locally(
                invocation.api_version.as_str(),
                &invocation.root_children,
                &invocation.requested_field_paths,
            )
        {
            return ResolverOutcome::value(json!({}));
        }

        let result = self
            .cached_or_forward_upstream_graphql_result(invocation.request, invocation.response_key);
        if result.transport_succeeded {
            let response = self.execution_session.upstream_query_response.clone();
            if let Some(response) = response {
                self.observe_taxonomy_root_response(
                    invocation.response_key,
                    invocation.api_version.as_str(),
                    &invocation.root_children,
                    &response.body,
                    result.outcome.errors.is_empty(),
                );
            }
        }
        result.outcome
    }

    fn taxonomy_root_can_answer_locally(
        &self,
        api_version: &str,
        children: &[RootChildInvocation],
        requested_paths: &BTreeSet<Vec<String>>,
    ) -> bool {
        children
            .iter()
            .filter(|child| child.name == "categories")
            .all(|child| {
                self.taxonomy_observed_connection(api_version, &child.arguments)
                    .or_else(|| {
                        self.taxonomy_complete_scope_connection(api_version, &child.arguments)
                    })
                    .is_some_and(|connection| {
                        taxonomy_connection_supports_paths(&connection, requested_paths)
                    })
            })
    }

    fn observe_taxonomy_root_response(
        &mut self,
        root_response_key: &str,
        api_version: &str,
        children: &[RootChildInvocation],
        response_body: &Value,
        cache_windows: bool,
    ) {
        let Some(taxonomy) = response_body
            .get("data")
            .and_then(|data| data.get(root_response_key))
            .and_then(Value::as_object)
        else {
            return;
        };
        for child in children.iter().filter(|child| child.name == "categories") {
            let Some(connection) = taxonomy
                .get(&child.response_key)
                .or_else(|| taxonomy.get(&child.name))
                .filter(|value| value.is_object())
            else {
                continue;
            };
            self.observe_taxonomy_connection(
                api_version,
                &child.arguments,
                connection,
                cache_windows,
            );
        }
    }

    fn observe_taxonomy_connection(
        &mut self,
        api_version: &str,
        arguments: &BTreeMap<String, Value>,
        connection: &Value,
        cache_window: bool,
    ) {
        for node in connection_nodes(connection) {
            self.observe_taxonomy_category(&node);
        }
        for row in observed_connection_rows(connection) {
            self.observe_taxonomy_category(&row.node);
        }

        if !cache_window {
            return;
        }
        self.store.base.taxonomy_connection_windows.insert(
            taxonomy_window_key(api_version, arguments),
            connection.clone(),
        );
        if taxonomy_connection_proves_complete_scope(arguments, connection) {
            let rows = observed_connection_rows(connection);
            if rows.iter().all(|row| row.cursor.is_some()) || rows.is_empty() {
                self.store.base.taxonomy_complete_scopes.insert(
                    taxonomy_scope_key(api_version, arguments),
                    connection.clone(),
                );
            }
        }
    }

    fn taxonomy_observed_connection(
        &self,
        api_version: &str,
        arguments: &BTreeMap<String, Value>,
    ) -> Option<Value> {
        self.store
            .base
            .taxonomy_connection_windows
            .get(&taxonomy_window_key(api_version, arguments))
            .map(|connection| self.taxonomy_connection_with_normalized_nodes(connection))
    }

    fn taxonomy_complete_scope_connection(
        &self,
        api_version: &str,
        arguments: &BTreeMap<String, Value>,
    ) -> Option<Value> {
        let complete = self
            .store
            .base
            .taxonomy_complete_scopes
            .get(&taxonomy_scope_key(api_version, arguments))?;
        let complete = self.taxonomy_connection_with_normalized_nodes(complete);
        let rows = observed_connection_rows(&complete);
        if rows.iter().any(|row| row.cursor.is_none()) {
            return None;
        }
        let resolved_arguments = resolved_arguments_from_json(arguments);
        for cursor_argument in ["after", "before"] {
            if let Some(ResolvedValue::String(cursor)) = resolved_arguments.get(cursor_argument) {
                if !rows
                    .iter()
                    .any(|row| row.cursor.as_deref() == Some(cursor.as_str()))
                {
                    return None;
                }
            }
        }
        let (window, page_info) = connection_window(&rows, &resolved_arguments, |row| {
            row.cursor.clone().unwrap_or_default()
        });
        let nodes = window
            .iter()
            .map(|row| row.node.clone())
            .collect::<Vec<_>>();
        let edges = window
            .into_iter()
            .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
            .collect::<Vec<_>>();
        Some(json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info }))
    }

    fn taxonomy_categories_connection(
        &self,
        api_version: &str,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        self.taxonomy_observed_connection(api_version, arguments)
            .or_else(|| self.taxonomy_complete_scope_connection(api_version, arguments))
            .unwrap_or_else(|| json!({ "nodes": [], "edges": [], "pageInfo": empty_page_info() }))
    }

    fn taxonomy_connection_with_normalized_nodes(&self, connection: &Value) -> Value {
        let mut connection = connection.clone();
        if let Some(nodes) = connection.get_mut("nodes").and_then(Value::as_array_mut) {
            for node in nodes {
                if let Some(id) = node.get("id").and_then(Value::as_str) {
                    if let Some(observed) = self.store.base.taxonomy_categories.get(id) {
                        *node = observed.clone();
                    }
                }
            }
        }
        if let Some(edges) = connection.get_mut("edges").and_then(Value::as_array_mut) {
            for edge in edges {
                let Some(id) = edge.pointer("/node/id").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(observed) = self.store.base.taxonomy_categories.get(id) {
                    edge["node"] = observed.clone();
                }
            }
        }
        connection
    }

    pub(in crate::proxy) fn observe_taxonomy_category(&mut self, observed: &Value) {
        let Some(id) = observed.get("id").and_then(Value::as_str) else {
            return;
        };
        if !is_shopify_gid_of_type(id, "TaxonomyCategory") {
            return;
        }
        let mut observed = observed.clone();
        if let Some(object) = observed.as_object_mut() {
            object
                .entry("__typename".to_string())
                .or_insert_with(|| json!("TaxonomyCategory"));
        }
        self.store.base.taxonomy_missing_category_ids.remove(id);
        if let Some(existing) = self.store.base.taxonomy_categories.records.get_mut(id) {
            merge_observed_object(existing, &observed);
        } else {
            self.store
                .base
                .taxonomy_categories
                .insert(id.to_string(), observed);
        }
    }

    pub(in crate::proxy) fn observe_taxonomy_node_root_value(
        &mut self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        value: &Value,
        authoritative_misses: bool,
    ) {
        let ids = match root_name {
            "node" => vec![resolved_string_field(arguments, "id").unwrap_or_default()],
            "nodes" => arguments
                .get("ids")
                .map(resolved_string_list)
                .unwrap_or_default(),
            _ => return,
        };
        let values = match root_name {
            "node" => vec![value],
            "nodes" => value
                .as_array()
                .map_or_else(Vec::new, |values| values.iter().collect()),
            _ => Vec::new(),
        };
        for (index, id) in ids.iter().enumerate() {
            if !is_shopify_gid_of_type(id, "TaxonomyCategory") {
                continue;
            }
            match values.get(index).copied() {
                Some(node) if node.is_object() => self.observe_taxonomy_category(node),
                Some(node) if node.is_null() && authoritative_misses => {
                    self.store
                        .base
                        .taxonomy_missing_category_ids
                        .insert(id.clone());
                }
                _ => {}
            }
        }
    }

    pub(in crate::proxy) fn taxonomy_category_node_value(&self, id: &str) -> Option<Value> {
        if self.store.base.taxonomy_missing_category_ids.contains(id) {
            return Some(Value::Null);
        }
        self.store
            .base
            .taxonomy_categories
            .get(id)
            .filter(|category| taxonomy_category_is_complete(category))
            .cloned()
    }
}

fn merge_observed_object(target: &mut Value, observed: &Value) {
    let (Some(target), Some(observed)) = (target.as_object_mut(), observed.as_object()) else {
        *target = observed.clone();
        return;
    };
    for (field, value) in observed {
        target.insert(field.clone(), value.clone());
    }
}

fn normalized_taxonomy_arguments(arguments: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    arguments
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn taxonomy_window_key(api_version: &str, arguments: &BTreeMap<String, Value>) -> String {
    format!(
        "{api_version}:{}",
        serde_json::to_string(&normalized_taxonomy_arguments(arguments))
            .expect("taxonomy arguments should serialize")
    )
}

fn taxonomy_scope_key(api_version: &str, arguments: &BTreeMap<String, Value>) -> String {
    let mut scope = normalized_taxonomy_arguments(arguments);
    for pagination_argument in ["after", "before", "first", "last"] {
        scope.remove(pagination_argument);
    }
    format!(
        "{api_version}:{}",
        serde_json::to_string(&scope).expect("taxonomy scope should serialize")
    )
}

fn taxonomy_connection_proves_complete_scope(
    arguments: &BTreeMap<String, Value>,
    connection: &Value,
) -> bool {
    !arguments
        .iter()
        .any(|(name, value)| matches!(name.as_str(), "after" | "before") && !value.is_null())
        && connection
            .pointer("/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            == Some(false)
        && connection
            .pointer("/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            == Some(false)
}

fn taxonomy_category_is_complete(category: &Value) -> bool {
    TAXONOMY_CATEGORY_NODE_FIELDS
        .iter()
        .all(|field| category.get(field).is_some())
}

fn taxonomy_connection_supports_paths(
    connection: &Value,
    requested_paths: &BTreeSet<Vec<String>>,
) -> bool {
    requested_paths.iter().all(|path| match path.as_slice() {
        [] => true,
        [.., field] if field == "__typename" => true,
        [categories] if categories == "categories" => true,
        [categories, field] if categories == "categories" => connection.get(field).is_some(),
        [categories, nodes, rest @ ..] if categories == "categories" && nodes == "nodes" => {
            connection
                .get("nodes")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().all(|value| value_has_path(value, rest)))
        }
        [categories, edges, rest @ ..] if categories == "categories" && edges == "edges" => {
            connection
                .get("edges")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().all(|value| value_has_path(value, rest)))
        }
        [categories, page_info, rest @ ..]
            if categories == "categories" && page_info == "pageInfo" =>
        {
            connection
                .get("pageInfo")
                .is_some_and(|value| value_has_path(value, rest))
        }
        _ => true,
    })
}

fn value_has_path(value: &Value, path: &[String]) -> bool {
    if path.is_empty() || path.first().is_some_and(|field| field == "__typename") {
        return true;
    }
    value
        .get(&path[0])
        .is_some_and(|child| value_has_path(child, &path[1..]))
}
