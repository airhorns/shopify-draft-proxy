use super::search::{online_store_search_decision, online_store_sort_key};
use super::*;

impl DraftProxy {
    pub(in crate::proxy) fn hydrate_online_store_content_query_baselines(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        for (root, kind, hydrate_query) in ONLINE_STORE_COUNT_ROOTS {
            if fields.iter().any(|field| field.name == root)
                && kind.count_base(&self.store.staged).is_none()
            {
                self.hydrate_online_store_count_base(request, root, kind, hydrate_query);
            }
        }
    }

    pub(super) fn online_store_connection_value(
        &self,
        kind: OnlineStoreKind,
        field: &RootFieldSelection,
    ) -> Value {
        let query = resolved_string_field(&field.arguments, "query");
        let sort_key = resolved_string_field(&field.arguments, "sortKey");
        let mut records = self
            .online_store_records(kind)
            .into_iter()
            .filter(|record| {
                online_store_search_decision(kind, record, query.as_deref())
                    == StagedSearchDecision::Match
            })
            .collect::<Vec<_>>();

        if let Some(sort_key) = sort_key.as_deref() {
            records.sort_by(|left, right| {
                online_store_sort_key(kind, left, sort_key)
                    .cmp(&online_store_sort_key(kind, right, sort_key))
            });
        }

        if resolved_bool_field(&field.arguments, "reverse").unwrap_or(false) {
            records.reverse();
        }

        let (records, page_info) = connection_window(&records, &field.arguments, value_id_cursor);
        selected_json(
            &connection_json_with_cursor(records, |_, node| value_id_cursor(node), page_info),
            &field.selection,
        )
    }

    pub(super) fn online_store_count(&self, kind: OnlineStoreKind) -> usize {
        online_store_count_with_baseline(
            kind.count_base(&self.store.staged),
            kind.order(&self.store.staged),
            kind.deleted_ids(&self.store.staged),
        )
        .unwrap_or_else(|| self.online_store_records(kind).len())
    }

    fn hydrate_online_store_count_base(
        &mut self,
        request: &Request,
        root: &str,
        kind: OnlineStoreKind,
        hydrate_query: &str,
    ) {
        let response =
            self.upstream_post(request, json!({ "query": hydrate_query, "variables": {} }));
        if response.status >= 400 {
            return;
        }
        let Some(count) = response
            .body
            .get("data")
            .and_then(|data| data.get(root))
            .and_then(|value| value.get("count"))
            .and_then(Value::as_u64)
        else {
            return;
        };
        if let Some(count_base) = kind.count_base_mut(&mut self.store.staged) {
            *count_base = Some(count as usize);
        }
    }
}

fn online_store_count_with_baseline(
    baseline: Option<usize>,
    order: &[String],
    deleted_ids: &BTreeSet<String>,
) -> Option<usize> {
    let baseline = baseline?;
    let synthetic_staged = order
        .iter()
        .filter(|id| is_synthetic_gid(id))
        .filter(|id| !deleted_ids.contains(*id))
        .count();
    let deleted_baseline = deleted_ids
        .iter()
        .filter(|id| !is_synthetic_gid(id))
        .count();
    Some(baseline.saturating_sub(deleted_baseline) + synthetic_staged)
}
