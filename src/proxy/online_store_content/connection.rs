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
        self.online_store_connection_from_records(
            kind,
            self.online_store_records(kind),
            &field.arguments,
            &field.selection,
        )
    }

    pub(super) fn online_store_connection_from_records(
        &self,
        kind: OnlineStoreKind,
        records: Vec<Value>,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let indexed_records = records.into_iter().enumerate().collect::<Vec<_>>();
        let result = staged_connection_query(
            indexed_records,
            arguments,
            |(_, record), query| online_store_search_decision(kind, record, query),
            |(index, record), sort_key| {
                sort_key
                    .map(|sort_key| online_store_sort_key(kind, record, sort_key))
                    .unwrap_or_else(|| vec![StagedSortValue::I64(*index as i64)])
            },
            |(_, record)| value_id_cursor(record),
        );

        selected_typed_connection_with_page_info(
            &result.records,
            selection,
            |(_, record), selection| self.online_store_selected_record(kind, record, selection),
            |(_, record)| value_id_cursor(record),
            result.page_info,
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
