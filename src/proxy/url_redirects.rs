use super::*;

pub(in crate::proxy) fn url_redirect_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy>
{
    ["UrlRedirect", "UrlRedirectConnection", "UrlRedirectEdge"]
        .into_iter()
        .map(|parent_type| {
            FieldResolverTypePolicy::property_backed_ordinary_fields(
                ApiSurface::Admin,
                parent_type,
                "argument-bearing URL redirect field has no explicit canonical resolver",
            )
        })
        .collect()
}

impl DraftProxy {
    pub(crate) fn url_redirect_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode != ReadMode::Snapshot && !self.has_staged_url_redirects() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let value = match invocation.root_name {
            "urlRedirect" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                self.store
                    .staged
                    .url_redirects
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "urlRedirects" => self.url_redirect_connection_resolved_value(&arguments),
            "urlRedirectsCount" => {
                let result = staged_connection_query(
                    self.url_redirect_records(),
                    &arguments,
                    url_redirect_search_decision,
                    url_redirect_sort_key,
                    value_id_cursor,
                );
                count_object(result.total_count)
            }
            _ => Value::Null,
        };
        ResolverOutcome::value(value)
    }

    pub(in crate::proxy) fn has_staged_url_redirects(&self) -> bool {
        !self.store.staged.url_redirects.is_empty()
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

    pub(in crate::proxy) fn url_redirect_connection_value(
        &self,
        arguments: &BTreeMap<String, Value>,
    ) -> Value {
        let arguments = arguments
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_from_json(value)))
            .collect();
        self.url_redirect_connection_resolved_value(&arguments)
    }

    fn url_redirect_connection_resolved_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let result = staged_connection_query(
            self.url_redirect_records(),
            arguments,
            url_redirect_search_decision,
            url_redirect_sort_key,
            value_id_cursor,
        );
        connection_json_with_cursor(
            result.records,
            |_, record| value_id_cursor(record),
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
