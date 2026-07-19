use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn has_staged_url_redirects(&self) -> bool {
        !self.store.staged.url_redirects.is_empty()
    }

    pub(in crate::proxy) fn url_redirect_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "urlRedirect" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .url_redirects
                        .get(&id)
                        .map(|redirect| selected_json(redirect, &field.selection))
                        .unwrap_or(Value::Null)
                }
                "urlRedirects" => self.url_redirect_connection(field),
                "urlRedirectsCount" => {
                    let result = staged_connection_query(
                        self.url_redirect_records(),
                        &field.arguments,
                        url_redirect_search_decision,
                        url_redirect_sort_key,
                        value_id_cursor,
                    );
                    let count = if resolved_string_field(&field.arguments, "query")
                        .is_none_or(|query| query.trim().is_empty())
                    {
                        self.store
                            .staged
                            .url_redirects_count_base
                            .map(|base| base + self.synthetic_url_redirect_count())
                            .unwrap_or(result.total_count)
                    } else {
                        result.total_count
                    };
                    selected_count_json(count, &field.selection)
                }
                _ => Value::Null,
            })
        })
    }

    pub(in crate::proxy) fn observe_url_redirect_response(
        &mut self,
        body: &Value,
        fields: &[RootFieldSelection],
    ) {
        let Some(data) = body.get("data") else {
            return;
        };
        for field in fields {
            let Some(value) = data.get(&field.response_key) else {
                continue;
            };
            match field.name.as_str() {
                "urlRedirect" => self.observe_url_redirect_node(value),
                "urlRedirects" => {
                    self.observe_url_redirect_connection(value);
                    self.store.staged.url_redirects_baseline_loaded = true;
                }
                "urlRedirectsCount" => {
                    if resolved_string_field(&field.arguments, "query")
                        .is_none_or(|query| query.trim().is_empty())
                    {
                        if let Some(count) = value.get("count").and_then(Value::as_u64) {
                            self.store.staged.url_redirects_count_base = Some(count as usize);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn url_redirect_records(&self) -> Vec<Value> {
        let mut records = self
            .store
            .staged
            .url_redirect_order
            .iter()
            .filter_map(|id| self.store.staged.url_redirects.get(id))
            .cloned()
            .collect::<Vec<_>>();
        for (id, redirect) in &self.store.staged.url_redirects {
            if !self.store.staged.url_redirect_order.contains(id) {
                records.push(redirect.clone());
            }
        }
        records
    }

    fn synthetic_url_redirect_count(&self) -> usize {
        self.store
            .staged
            .url_redirects
            .keys()
            .filter(|id| is_synthetic_gid(id))
            .count()
    }

    fn observe_url_redirect_connection(&mut self, connection: &Value) {
        if let Some(nodes) = connection.get("nodes").and_then(Value::as_array) {
            for node in nodes {
                self.observe_url_redirect_node(node);
            }
        }
        if let Some(edges) = connection.get("edges").and_then(Value::as_array) {
            for edge in edges {
                if let Some(node) = edge.get("node") {
                    self.observe_url_redirect_node(node);
                }
            }
        }
    }

    fn observe_url_redirect_node(&mut self, node: &Value) {
        let Some(id) = node.get("id").and_then(Value::as_str) else {
            return;
        };
        if shopify_gid_resource_type(id) != Some("UrlRedirect")
            || self.store.staged.url_redirects.contains_key(id)
        {
            return;
        }
        self.store.staged.url_redirect_order.push(id.to_string());
        self.store
            .staged
            .url_redirects
            .insert(id.to_string(), node.clone());
    }

    fn url_redirect_connection(&self, field: &RootFieldSelection) -> Value {
        let result = staged_connection_query(
            self.url_redirect_records(),
            &field.arguments,
            url_redirect_search_decision,
            url_redirect_sort_key,
            value_id_cursor,
        );
        selected_typed_connection_with_page_info(
            &result.records,
            &field.selection,
            selected_json,
            value_id_cursor,
            result.page_info,
        )
    }

    pub(in crate::proxy) fn stage_url_redirect(&mut self, path: String, target: String) -> String {
        let id = self.next_proxy_synthetic_gid("UrlRedirect");
        let redirect = json!({
            "id": id,
            "path": path,
            "target": target
        });
        if !self.store.staged.url_redirects.contains_key(&id) {
            self.store.staged.url_redirect_order.push(id.clone());
        }
        self.store.staged.url_redirects.insert(id.clone(), redirect);
        id
    }
}

fn url_redirect_search_decision(redirect: &Value, query: Option<&str>) -> StagedSearchDecision {
    StagedSearchDecision::from_bool(
        query.is_none_or(|query| url_redirect_matches_query(redirect, query)),
    )
}

fn url_redirect_sort_key(redirect: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let field = match sort_key.unwrap_or("ID") {
        "PATH" => "path",
        "TARGET" => "target",
        _ => "id",
    };
    vec![StagedSortValue::String(
        redirect
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )]
}

fn url_redirect_matches_query(redirect: &Value, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if let Some(path) = query.strip_prefix("path:") {
        let path = path.trim().trim_matches('"').trim_matches('\'');
        return redirect.get("path").and_then(Value::as_str) == Some(path);
    }
    if let Some(target) = query.strip_prefix("target:") {
        let target = target.trim().trim_matches('"').trim_matches('\'');
        return redirect.get("target").and_then(Value::as_str) == Some(target);
    }
    redirect
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| path.contains(query))
        || redirect
            .get("target")
            .and_then(Value::as_str)
            .is_some_and(|target| target.contains(query))
}
