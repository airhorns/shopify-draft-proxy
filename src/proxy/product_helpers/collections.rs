use super::*;
use base64::Engine as _;

const COLLECTIONS_IDENTITY_HYDRATE_QUERY: &str = r#"
query CollectionsIdentityHydrate(
  $first: Int
  $after: String
  $last: Int
  $before: String
  $reverse: Boolean
  $sortKey: CollectionSortKeys
  $query: String
  $savedSearchId: ID
) {
  collections(
    first: $first
    after: $after
    last: $last
    before: $before
    reverse: $reverse
    sortKey: $sortKey
    query: $query
    savedSearchId: $savedSearchId
  ) {
    nodes {
      id
      title
      handle
      createdAt
      updatedAt
      sortOrder
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
          conditionObject {
            __typename
            ... on CollectionRuleMetafieldCondition {
              metafieldDefinition {
                id
                namespace
                key
              }
            }
          }
        }
      }
      productsCount {
        count
        precision
      }
    }
  }
}
"#;

pub(in crate::proxy) const STOREFRONT_COLLECTION_BASELINE_UPDATED_AT_FIELD: &str =
    "__storefrontBaselineUpdatedAt";
const COLLECTION_MEMBERSHIP_PAYLOAD_BASELINE_COUNT_FIELD: &str =
    "__collectionMembershipPayloadBaselineCount";

const COLLECTION_MEMBERSHIP_BASELINE_FIRST: usize = 11;
const COLLECTION_MEMBERSHIP_MAX_PROBE_ROWS: usize = 250;

impl CollectionMembershipState {
    fn baseline_membership(&self, product_id: &str) -> Option<bool> {
        self.known_membership
            .get(product_id)
            .copied()
            .or_else(|| {
                self.baseline_order
                    .iter()
                    .any(|id| id == product_id)
                    .then_some(true)
            })
            .or(self.baseline_complete.then_some(false))
    }

    fn effective_membership(&self, product_id: &str) -> Option<bool> {
        self.apply_deltas_to_membership(product_id, self.baseline_membership(product_id))
    }

    fn apply_deltas_to_membership(
        &self,
        product_id: &str,
        mut membership: Option<bool>,
    ) -> Option<bool> {
        for delta in &self.deltas {
            match delta {
                CollectionMembershipDelta::Add { product_id: id } if id == product_id => {
                    membership = Some(true);
                }
                CollectionMembershipDelta::Remove { product_id: id } if id == product_id => {
                    membership = Some(false);
                }
                _ => {}
            }
        }
        membership
    }

    fn effective_count(&self) -> Option<(u64, String)> {
        let mut count = self.baseline_count.or_else(|| {
            self.baseline_complete
                .then_some(self.baseline_order.len() as u64)
        })?;
        let changed_ids = self
            .deltas
            .iter()
            .map(|delta| match delta {
                CollectionMembershipDelta::Add { product_id }
                | CollectionMembershipDelta::Remove { product_id }
                | CollectionMembershipDelta::Move { product_id, .. } => product_id,
            })
            .collect::<BTreeSet<_>>();
        for product_id in changed_ids {
            let Some(before) = self.baseline_membership(product_id) else {
                continue;
            };
            let after = self.effective_membership(product_id).unwrap_or(before);
            match (before, after) {
                (false, true) => count = count.saturating_add(1),
                (true, false) => count = count.saturating_sub(1),
                _ => {}
            }
        }
        Some((
            count,
            self.baseline_precision
                .clone()
                .unwrap_or_else(|| "EXACT".to_string()),
        ))
    }

    fn effective_prefix_order(&self) -> Vec<String> {
        let mut order = self.baseline_order.clone();
        for delta in &self.deltas {
            match delta {
                CollectionMembershipDelta::Add { product_id } => {
                    if self.baseline_complete && !order.contains(product_id) {
                        order.push(product_id.clone());
                    }
                }
                CollectionMembershipDelta::Remove { product_id } => {
                    order.retain(|id| id != product_id);
                }
                CollectionMembershipDelta::Move {
                    product_id,
                    new_position,
                } => {
                    order.retain(|id| id != product_id);
                    if *new_position <= order.len() {
                        order.insert(*new_position, product_id.clone());
                    }
                }
            }
        }
        order
    }

    fn stage_add(&mut self, product_id: &str) -> bool {
        if self.effective_membership(product_id) == Some(true) {
            return false;
        }
        self.deltas.push(CollectionMembershipDelta::Add {
            product_id: product_id.to_string(),
        });
        true
    }

    fn stage_remove(&mut self, product_id: &str) -> bool {
        if self.effective_membership(product_id) != Some(true) {
            return false;
        }
        self.deltas.push(CollectionMembershipDelta::Remove {
            product_id: product_id.to_string(),
        });
        true
    }

    fn stage_move(&mut self, product_id: &str, new_position: usize) {
        self.deltas.push(CollectionMembershipDelta::Move {
            product_id: product_id.to_string(),
            new_position,
        });
    }
}

impl Store {
    pub(in crate::proxy) fn observe_collection_membership(&mut self, collection: &Value) {
        let Some(collection_id) = collection.get("id").and_then(Value::as_str) else {
            return;
        };
        let state = self
            .staged
            .collection_memberships
            .entry(collection_id.to_string())
            .or_default();
        if collection
            .get(OBSERVED_COLLECTION_BASELINE_FIELD)
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            state.upstream_baseline = true;
        }
        if let Some(count) = collection
            .pointer("/productsCount/count")
            .and_then(Value::as_u64)
        {
            state.baseline_count.get_or_insert(count);
            if state.baseline_precision.is_none() {
                state.baseline_precision = collection
                    .pointer("/productsCount/precision")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
        let connection = collection
            .get("manualProducts")
            .or_else(|| collection.get("products"));
        let Some(connection) = connection else {
            return;
        };
        let rows = observed_connection_rows(connection);
        let is_first_page = connection
            .pointer("/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            != Some(true);
        if is_first_page && rows.len() >= state.baseline_order.len() {
            state.baseline_order = rows
                .iter()
                .filter_map(|row| {
                    row.node
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
        }
        for row in rows {
            let Some(product_id) = row.node.get("id").and_then(Value::as_str) else {
                continue;
            };
            state.known_membership.insert(product_id.to_string(), true);
            if let Some(cursor) = row.cursor {
                state
                    .baseline_cursors
                    .insert(product_id.to_string(), cursor);
            }
        }
        if is_first_page
            && (!state.upstream_baseline
                || connection
                    .pointer("/pageInfo/hasNextPage")
                    .and_then(Value::as_bool)
                    == Some(false))
        {
            state.baseline_complete = true;
            state
                .baseline_count
                .get_or_insert(state.baseline_order.len() as u64);
        }
    }

    fn collection_membership(&self, collection_id: &str) -> Option<&CollectionMembershipState> {
        self.staged.collection_memberships.get(collection_id)
    }

    fn collection_membership_mut(&mut self, collection_id: &str) -> &mut CollectionMembershipState {
        self.staged
            .collection_memberships
            .entry(collection_id.to_string())
            .or_default()
    }
}

pub(in crate::proxy) fn collection_summary_json(collection: &Value) -> Value {
    json!({
        "id": collection.get("id").cloned().unwrap_or(Value::Null),
        "title": collection.get("title").cloned().unwrap_or(Value::Null),
        "handle": collection.get("handle").cloned().unwrap_or(Value::Null)
    })
}

pub(in crate::proxy) fn upsert_minimal_collection(
    collections: &mut Vec<Value>,
    collection: &Value,
) {
    let summary = collection_summary_json(collection);
    let Some(id) = summary.get("id").and_then(Value::as_str) else {
        return;
    };
    if let Some(existing) = collections
        .iter_mut()
        .find(|existing| existing.get("id").and_then(Value::as_str) == Some(id))
    {
        *existing = summary;
    } else {
        collections.push(summary);
    }
}

fn remove_minimal_collection(collections: &mut Vec<Value>, collection_id: &str) {
    collections
        .retain(|collection| collection.get("id").and_then(Value::as_str) != Some(collection_id));
}

#[derive(Clone)]
pub(in crate::proxy) struct CollectionProductEntry {
    pub(in crate::proxy) position: usize,
    pub(in crate::proxy) product: ProductRecord,
    pub(in crate::proxy) variants: Vec<ProductVariantRecord>,
}

#[derive(Clone, Copy)]
enum CollectionProductSortKey {
    BestSelling,
    Created,
    Id,
    Inventory,
    Manual,
    Price,
    Relevance,
    Title,
}

#[derive(Clone, Copy)]
struct CollectionProductSortPlan {
    key: CollectionProductSortKey,
    descending: bool,
}

pub(in crate::proxy) fn sort_collection_product_entries(
    collection: &Value,
    products: &mut [CollectionProductEntry],
    sort_key: Option<&str>,
    reverse: bool,
) {
    let sort_plan = collection_product_sort_plan(collection, sort_key);
    match sort_plan.key {
        CollectionProductSortKey::Manual => products.sort_by_key(|entry| entry.position),
        _ => products.sort_by(|left, right| {
            collection_product_sort_key(left, sort_plan.key)
                .cmp(&collection_product_sort_key(right, sort_plan.key))
                .then_with(|| {
                    collection_product_cursor(left).cmp(&collection_product_cursor(right))
                })
        }),
    }
    if sort_plan.descending ^ reverse {
        products.reverse();
    }
}

fn sorted_collection_product_entries(
    collection: &Value,
    mut products: Vec<CollectionProductEntry>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<CollectionProductEntry> {
    let sort_key = resolved_string_field(arguments, "sortKey");
    let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
    sort_collection_product_entries(collection, &mut products, sort_key.as_deref(), reverse);
    products
}

fn collection_product_sort_plan(
    collection: &Value,
    sort_key: Option<&str>,
) -> CollectionProductSortPlan {
    match sort_key.unwrap_or("COLLECTION_DEFAULT") {
        "BEST_SELLING" => CollectionProductSortPlan {
            key: CollectionProductSortKey::BestSelling,
            descending: true,
        },
        "CREATED" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Created,
            descending: false,
        },
        "ID" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Id,
            descending: false,
        },
        "INVENTORY" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Inventory,
            descending: false,
        },
        "MANUAL" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Manual,
            descending: false,
        },
        "PRICE" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Price,
            descending: false,
        },
        "RELEVANCE" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Relevance,
            descending: true,
        },
        "TITLE" => CollectionProductSortPlan {
            key: CollectionProductSortKey::Title,
            descending: false,
        },
        "COLLECTION_DEFAULT" => collection_default_product_sort_plan(collection),
        _ => collection_default_product_sort_plan(collection),
    }
}

fn collection_default_product_sort_plan(collection: &Value) -> CollectionProductSortPlan {
    match collection.get("sortOrder").and_then(Value::as_str) {
        Some("ALPHA_ASC") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Title,
            descending: false,
        },
        Some("ALPHA_DESC") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Title,
            descending: true,
        },
        Some("CREATED") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Created,
            descending: false,
        },
        Some("CREATED_DESC") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Created,
            descending: true,
        },
        Some("MANUAL") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Manual,
            descending: false,
        },
        Some("PRICE_ASC") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Price,
            descending: false,
        },
        Some("PRICE_DESC") => CollectionProductSortPlan {
            key: CollectionProductSortKey::Price,
            descending: true,
        },
        _ => CollectionProductSortPlan {
            key: CollectionProductSortKey::BestSelling,
            descending: true,
        },
    }
}

fn collection_product_sort_key(
    entry: &CollectionProductEntry,
    sort_key: CollectionProductSortKey,
) -> StagedSortKey {
    let primary = match sort_key {
        CollectionProductSortKey::BestSelling => collection_product_gid_tail_sort_value(entry),
        CollectionProductSortKey::Created => {
            StagedSortValue::String(entry.product.created_at.clone())
        }
        CollectionProductSortKey::Id => collection_product_gid_tail_sort_value(entry),
        CollectionProductSortKey::Inventory => StagedSortValue::I64(entry.product.total_inventory),
        CollectionProductSortKey::Manual => StagedSortValue::I64(entry.position as i64),
        CollectionProductSortKey::Price => collection_product_min_price_cents(entry)
            .map(StagedSortValue::I64)
            .unwrap_or(StagedSortValue::Null),
        CollectionProductSortKey::Title => {
            StagedSortValue::String(entry.product.title.to_ascii_lowercase())
        }
        CollectionProductSortKey::Relevance => collection_product_gid_tail_sort_value(entry),
    };
    vec![primary, collection_product_gid_tail_sort_value(entry)]
}

fn collection_product_gid_tail_sort_value(entry: &CollectionProductEntry) -> StagedSortValue {
    resource_id_tail_sort_value(Some(&entry.product.id))
}

fn collection_product_min_price_cents(entry: &CollectionProductEntry) -> Option<i64> {
    let prices: Box<dyn Iterator<Item = f64> + '_> = if entry.variants.is_empty() {
        Box::new(entry.product.variants.iter().filter_map(|variant| {
            variant
                .get("price")
                .and_then(Value::as_str)
                .and_then(parse_product_price)
        }))
    } else {
        Box::new(
            entry
                .variants
                .iter()
                .filter_map(|variant| parse_product_price(&variant.price)),
        )
    };
    prices
        .min_by(|left, right| left.total_cmp(right))
        .map(|price| (price * 100.0).round() as i64)
}

pub(in crate::proxy) fn collection_product_cursor(entry: &CollectionProductEntry) -> String {
    entry.product.id.clone()
}

pub(in crate::proxy) fn collection_products_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let (collection, entries) = collection_field_product_entries(proxy, invocation);
    if let Some(connection) = partial_collection_membership_connection(
        proxy,
        request,
        invocation,
        &collection,
        &arguments,
    ) {
        return Ok(connection);
    }
    if collection
        .get(OBSERVED_COLLECTION_BASELINE_FIELD)
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if let Some(connection) = collection.get("products") {
            return Ok(observed_collection_products_connection(
                proxy, connection, &arguments,
            ));
        }
    }
    let products = sorted_collection_product_entries(&collection, entries, &arguments);
    let (products, page_info) = connection_window(&products, &arguments, collection_product_cursor);
    Ok(typed_connection_value(
        &products,
        |entry| proxy.product_canonical_value(&entry.product),
        collection_product_cursor,
        page_info,
    ))
}

pub(in crate::proxy) fn collection_has_product_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let product_id = invocation
        .arguments
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let collection_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let is_smart = proxy
        .store
        .collection_by_id(&collection_id)
        .is_some_and(collection_is_smart);
    if let Some(state) = (!is_smart)
        .then(|| proxy.store.collection_membership(&collection_id))
        .flatten()
        .filter(|state| !state.deltas.is_empty())
        .cloned()
    {
        let baseline = state.baseline_membership(product_id).or_else(|| {
            proxy
                .cached_upstream_query_value_at_path(request, &invocation.path)
                .and_then(|value| value.as_bool())
        });
        if let Some(value) = state.apply_deltas_to_membership(product_id, baseline) {
            return Ok(json!(value));
        }
        if proxy.config.read_mode == ReadMode::LiveHybrid
            && proxy.hydrate_collection_membership_targets(
                request,
                &collection_id,
                &[product_id.to_string()],
                COLLECTION_MEMBERSHIP_BASELINE_FIRST,
            )
        {
            if let Some(value) = proxy
                .store
                .collection_membership(&collection_id)
                .and_then(|state| state.effective_membership(product_id))
            {
                return Ok(json!(value));
            }
        }
    }
    let (_, entries) = collection_field_product_entries(proxy, invocation);
    Ok(json!(entries
        .iter()
        .any(|entry| entry.product.id == product_id)))
}

pub(in crate::proxy) fn collection_products_count_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    if let Some(count) = invocation
        .parent
        .get(COLLECTION_MEMBERSHIP_PAYLOAD_BASELINE_COUNT_FIELD)
    {
        return Ok(count.clone());
    }
    let (collection, entries) = collection_field_product_entries(proxy, invocation);
    let collection_id = collection
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(mut state) = (!collection_is_smart(&collection))
        .then(|| proxy.store.collection_membership(collection_id))
        .flatten()
        .filter(|state| !state.deltas.is_empty())
        .cloned()
    {
        if state.baseline_count.is_none() {
            if let Some(upstream) =
                proxy.cached_upstream_query_value_at_path(request, &invocation.path)
            {
                if let Some(count) = upstream.get("count").and_then(Value::as_u64) {
                    state.baseline_count = Some(count);
                }
                if let Some(precision) = upstream.get("precision").and_then(Value::as_str) {
                    state.baseline_precision = Some(precision.to_string());
                }
            }
        }
        if let Some((count, precision)) = state.effective_count() {
            return Ok(count_object_with_precision(count, &precision));
        }
    }
    if collection
        .get(OBSERVED_COLLECTION_BASELINE_FIELD)
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if let Some(count) = collection.get("productsCount") {
            return Ok(count.clone());
        }
    }
    if collection_is_smart(&collection) {
        return Ok(count_object(entries.len()));
    }
    Ok(invocation
        .parent
        .get("productsCount")
        .cloned()
        .unwrap_or_else(|| {
            count_object(
                invocation
                    .parent
                    .get("products")
                    .and_then(|connection| connection.get("nodes"))
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0),
            )
        }))
}

fn partial_collection_membership_connection(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
    collection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if collection_is_smart(collection) {
        return None;
    }
    let collection_id = collection.get("id").and_then(Value::as_str)?;
    let state = proxy
        .store
        .collection_membership(collection_id)
        .filter(|state| !state.deltas.is_empty())?
        .clone();
    if state.baseline_complete && !state.upstream_baseline {
        return None;
    }
    let manual_order = resolved_string_field(arguments, "sortKey")
        .is_some_and(|sort_key| sort_key == "MANUAL")
        || (resolved_string_field(arguments, "sortKey")
            .is_none_or(|sort_key| sort_key == "COLLECTION_DEFAULT")
            && collection.get("sortOrder").and_then(Value::as_str) == Some("MANUAL"));
    let forward_first_window = !arguments.contains_key("after")
        && !arguments.contains_key("before")
        && !arguments.contains_key("last")
        && !resolved_bool_field(arguments, "reverse").unwrap_or(false);
    let prefix_rows = state
        .effective_prefix_order()
        .into_iter()
        .filter_map(|product_id| membership_row(proxy, &state, &product_id))
        .collect::<Vec<_>>();
    let use_prefix = manual_order
        && !prefix_rows.is_empty()
        && membership_prefix_covers_window(&state, &prefix_rows, arguments);

    if use_prefix {
        let (rows, mut page_info) = connection_window(&prefix_rows, arguments, |row| {
            row.cursor.clone().unwrap_or_default()
        });
        extend_prefix_page_info(&state, &prefix_rows, &rows, &mut page_info);
        return Some(membership_connection_value(proxy, rows, page_info));
    }

    let upstream_connection = proxy.cached_upstream_query_value_at_path(request, &invocation.path);
    if upstream_connection.is_none() {
        let mut available_rows = prefix_rows;
        if !manual_order {
            apply_partial_membership_deltas_to_rows(
                proxy,
                &state,
                &mut available_rows,
                false,
                false,
                true,
            );
            sort_partial_membership_rows(proxy, collection, arguments, &mut available_rows);
        }
        let (rows, mut page_info) = connection_window(&available_rows, arguments, |row| {
            row.cursor.clone().unwrap_or_default()
        });
        extend_prefix_page_info(&state, &available_rows, &rows, &mut page_info);
        return Some(membership_connection_value(proxy, rows, page_info));
    }
    let upstream_connection = upstream_connection?;
    let mut rows = observed_connection_rows(&upstream_connection);
    let source_has_next = upstream_connection
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let source_has_previous = upstream_connection
        .pointer("/pageInfo/hasPreviousPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    apply_partial_membership_deltas_to_rows(
        proxy,
        &state,
        &mut rows,
        manual_order,
        forward_first_window,
        manual_order && !source_has_next,
    );
    if !manual_order {
        sort_partial_membership_rows(proxy, collection, arguments, &mut rows);
    }
    let wanted = resolved_int_field(arguments, "first")
        .or_else(|| resolved_int_field(arguments, "last"))
        .map(|value| value.max(0) as usize);
    let backward = arguments.contains_key("last");
    let mut terminal_page_info = upstream_connection
        .get("pageInfo")
        .cloned()
        .unwrap_or_else(empty_page_info);
    if let Some(wanted) = wanted {
        let can_refill = if backward {
            source_has_previous
        } else {
            source_has_next
        };
        if rows.len() < wanted && can_refill {
            if let Some(extra) = proxy.hydrate_collection_membership_window(
                request,
                collection_id,
                arguments,
                &upstream_connection,
                wanted.saturating_sub(rows.len()) + state.deltas.len() + 1,
            ) {
                terminal_page_info = extra.get("pageInfo").cloned().unwrap_or(terminal_page_info);
                let mut extra_rows = observed_connection_rows(&extra);
                if backward {
                    extra_rows.extend(rows);
                    rows = extra_rows;
                } else {
                    rows.extend(extra_rows);
                }
                dedupe_membership_rows(&mut rows);
                apply_partial_membership_deltas_to_rows(
                    proxy,
                    &state,
                    &mut rows,
                    manual_order,
                    forward_first_window,
                    manual_order
                        && !terminal_page_info
                            .get("hasNextPage")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                );
                if !manual_order {
                    sort_partial_membership_rows(proxy, collection, arguments, &mut rows);
                }
            }
        }
    }
    let had_extra_rows = wanted.is_some_and(|wanted| rows.len() > wanted);
    if let Some(wanted) = wanted {
        if backward && rows.len() > wanted {
            rows.drain(..rows.len() - wanted);
        } else {
            rows.truncate(wanted);
        }
    }
    let terminal_has_next = terminal_page_info
        .get("hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(source_has_next);
    let terminal_has_previous = terminal_page_info
        .get("hasPreviousPage")
        .and_then(Value::as_bool)
        .unwrap_or(source_has_previous);
    let page_info = connection_page_info(
        if backward {
            source_has_next
        } else {
            had_extra_rows || terminal_has_next
        },
        if backward {
            had_extra_rows || terminal_has_previous
        } else {
            source_has_previous
        },
        rows.first().and_then(|row| row.cursor.clone()),
        rows.last().and_then(|row| row.cursor.clone()),
    );
    Some(membership_connection_value(proxy, rows, page_info))
}

fn membership_prefix_covers_window(
    state: &CollectionMembershipState,
    rows: &[ObservedConnectionRow],
    arguments: &BTreeMap<String, ResolvedValue>,
) -> bool {
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        return false;
    }
    let cursors = rows
        .iter()
        .filter_map(|row| row.cursor.as_deref())
        .collect::<BTreeSet<_>>();
    for cursor_name in ["after", "before"] {
        if let Some(cursor) = resolved_string_field(arguments, cursor_name) {
            if !cursors.contains(cursor.as_str()) {
                return false;
            }
        }
    }
    if arguments.contains_key("last")
        && !arguments.contains_key("before")
        && !state.baseline_complete
    {
        return false;
    }
    let wanted = resolved_int_field(arguments, "first")
        .or_else(|| resolved_int_field(arguments, "last"))
        .map(|value| value.max(0) as usize);
    let Some(wanted) = wanted else {
        return state.baseline_complete;
    };
    let (window, _) = connection_window(rows, arguments, |row| {
        row.cursor.clone().unwrap_or_default()
    });
    window.len() == wanted
        || state.baseline_complete
        || state
            .effective_count()
            .is_some_and(|(count, _)| count as usize <= rows.len())
}

fn extend_prefix_page_info(
    state: &CollectionMembershipState,
    prefix_rows: &[ObservedConnectionRow],
    window_rows: &[ObservedConnectionRow],
    page_info: &mut Value,
) {
    if state.baseline_complete || window_rows.is_empty() {
        return;
    }
    let reaches_observed_end = window_rows.last().and_then(|row| row.cursor.as_deref())
        == prefix_rows.last().and_then(|row| row.cursor.as_deref());
    if reaches_observed_end
        && state
            .effective_count()
            .is_some_and(|(count, _)| count as usize > prefix_rows.len())
    {
        page_info["hasNextPage"] = json!(true);
    }
}

fn membership_row(
    proxy: &DraftProxy,
    state: &CollectionMembershipState,
    product_id: &str,
) -> Option<ObservedConnectionRow> {
    let node = proxy
        .store
        .product_by_id(product_id)
        .map(|product| proxy.product_canonical_value(product))
        .unwrap_or_else(|| json!({ "id": product_id }));
    let cursor = state.baseline_cursors.get(product_id).cloned().or_else(|| {
        Some(if state.upstream_baseline {
            collection_membership_cursor(product_id)
        } else {
            product_id.to_string()
        })
    });
    Some(ObservedConnectionRow { cursor, node })
}

fn apply_partial_membership_deltas_to_rows(
    proxy: &DraftProxy,
    state: &CollectionMembershipState,
    rows: &mut Vec<ObservedConnectionRow>,
    manual_order: bool,
    allow_move_insertion: bool,
    include_manual_tail_additions: bool,
) {
    for delta in &state.deltas {
        match delta {
            CollectionMembershipDelta::Add { product_id } => {
                if (!manual_order || include_manual_tail_additions)
                    && !rows
                        .iter()
                        .any(|row| row.node.get("id").and_then(Value::as_str) == Some(product_id))
                {
                    if let Some(row) = membership_row(proxy, state, product_id) {
                        rows.push(row);
                    }
                }
            }
            CollectionMembershipDelta::Remove { product_id } => {
                rows.retain(|row| row.node.get("id").and_then(Value::as_str) != Some(product_id));
            }
            CollectionMembershipDelta::Move {
                product_id,
                new_position,
            } => {
                if !manual_order {
                    continue;
                }
                rows.retain(|row| row.node.get("id").and_then(Value::as_str) != Some(product_id));
                if allow_move_insertion && *new_position <= rows.len() {
                    if let Some(row) = membership_row(proxy, state, product_id) {
                        rows.insert(*new_position, row);
                    }
                }
            }
        }
    }
}

fn sort_partial_membership_rows(
    proxy: &DraftProxy,
    collection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
    rows: &mut [ObservedConnectionRow],
) {
    let sort_key = resolved_string_field(arguments, "sortKey");
    let sort_plan = collection_product_sort_plan(collection, sort_key.as_deref());
    let reverse = resolved_bool_field(arguments, "reverse").unwrap_or(false);
    rows.sort_by(|left, right| {
        partial_membership_row_sort_key(proxy, left, sort_plan.key)
            .cmp(&partial_membership_row_sort_key(
                proxy,
                right,
                sort_plan.key,
            ))
            .then_with(|| {
                left.node
                    .get("id")
                    .and_then(Value::as_str)
                    .cmp(&right.node.get("id").and_then(Value::as_str))
            })
    });
    if sort_plan.descending ^ reverse {
        rows.reverse();
    }
}

fn partial_membership_row_sort_key(
    proxy: &DraftProxy,
    row: &ObservedConnectionRow,
    sort_key: CollectionProductSortKey,
) -> StagedSortKey {
    let product = row
        .node
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.store.product_by_id(id).cloned())
        .or_else(|| product_state_from_json(&row.node))
        .unwrap_or_default();
    let entry = CollectionProductEntry {
        position: 0,
        variants: proxy.store.product_variants_for_product(&product.id),
        product,
    };
    collection_product_sort_key(&entry, sort_key)
}

fn dedupe_membership_rows(rows: &mut Vec<ObservedConnectionRow>) {
    let mut seen = BTreeSet::new();
    rows.retain(|row| {
        row.node
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(|id| seen.insert(id.to_string()))
    });
}

fn membership_connection_value(
    proxy: &DraftProxy,
    mut rows: Vec<ObservedConnectionRow>,
    page_info: Value,
) -> Value {
    for row in &mut rows {
        replace_observed_collection_product_node(proxy, &mut row.node);
    }
    let nodes = rows.iter().map(|row| row.node.clone()).collect::<Vec<_>>();
    let edges = rows
        .into_iter()
        .map(|row| json!({ "cursor": row.cursor, "node": row.node }))
        .collect::<Vec<_>>();
    json!({ "nodes": nodes, "edges": edges, "pageInfo": page_info })
}

fn collection_membership_cursor(product_id: &str) -> String {
    base64::engine::general_purpose::STANDARD
        .encode(json!({ "kind": "collection-membership", "productId": product_id }).to_string())
}

fn observed_collection_products_connection(
    proxy: &DraftProxy,
    connection: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let mut connection = seeded_connection_value(connection, arguments);
    if let Some(nodes) = connection.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            replace_observed_collection_product_node(proxy, node);
        }
    }
    if let Some(edges) = connection.get_mut("edges").and_then(Value::as_array_mut) {
        for edge in edges {
            if let Some(node) = edge.get_mut("node") {
                replace_observed_collection_product_node(proxy, node);
            }
        }
    }
    connection
}

fn replace_observed_collection_product_node(proxy: &DraftProxy, node: &mut Value) {
    let Some(product) = node
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.store.product_by_id(id))
    else {
        return;
    };
    *node = proxy.product_canonical_value(product);
}

fn collection_parent_id(invocation: &crate::admin_graphql::FieldResolverInvocation<'_>) -> String {
    invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(in crate::proxy) fn collection_published_on_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let publication_id = invocation
        .arguments
        .get("publicationId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!(proxy
        .resource_publication_set(&collection_parent_id(invocation))
        .contains(publication_id)))
}

pub(in crate::proxy) fn collection_published_on_current_publication_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(json!(proxy
        .store
        .resource_is_published_on_current_publication(
            &collection_parent_id(invocation)
        )))
}

pub(in crate::proxy) fn collection_publications_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let id = collection_parent_id(invocation);
    let publications = proxy.resource_publication_set(&id);
    Ok(count_object(
        proxy.publishable_live_publication_count(&id, &publications),
    ))
}

pub(in crate::proxy) fn collection_publication_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(json!(proxy
        .resource_publication_set(&collection_parent_id(invocation))
        .len()))
}

pub(in crate::proxy) fn publication_products_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let publication_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    let products = proxy.publication_product_entries(publication_id);
    let (products, page_info) = connection_window(&products, &arguments, collection_product_cursor);
    Ok(typed_connection_value(
        &products,
        |entry| proxy.product_canonical_value(&entry.product),
        collection_product_cursor,
        page_info,
    ))
}

pub(in crate::proxy) fn publication_product_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let publication_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(count_object(proxy.publication_resource_count(
        Some(publication_id),
        "Product",
    )))
}

pub(in crate::proxy) fn publication_collections_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let publication_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(count_object(proxy.publication_resource_count(
        Some(publication_id),
        "Collection",
    )))
}

pub(in crate::proxy) fn publication_channels_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let channel = invocation
        .parent
        .get("channel")
        .cloned()
        .filter(Value::is_object)
        .map(|channel| proxy.channel_canonical_value(channel));
    let arguments = resolved_arguments_from_json(&invocation.arguments);
    Ok(connection_value_with_args(
        channel.into_iter().collect(),
        &arguments,
        value_id_cursor,
    ))
}

pub(in crate::proxy) fn publication_channel_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(invocation
        .parent
        .get("channel")
        .cloned()
        .filter(Value::is_object)
        .map(|channel| proxy.channel_canonical_value(channel))
        .unwrap_or(Value::Null))
}

pub(in crate::proxy) fn channel_products_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let channel_id = invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let count = proxy
        .publication_by_channel_id(channel_id)
        .map(|(publication_id, _)| {
            proxy.publication_resource_count(Some(&publication_id), "Product")
        })
        .unwrap_or(0);
    Ok(count_object(count))
}

fn collection_field_value(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Value {
    if invocation.parent.get("products").is_some() {
        return invocation.parent.clone();
    }
    invocation
        .parent
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| proxy.store.collection_by_id(id))
        .cloned()
        .unwrap_or_else(|| invocation.parent.clone())
}

fn collection_field_product_entries(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> (Value, Vec<CollectionProductEntry>) {
    let has_payload_membership = invocation.parent.get("products").is_some();
    let collection = collection_field_value(proxy, invocation);
    let entries = if has_payload_membership {
        proxy.explicit_collection_product_entries(&collection)
    } else {
        proxy.collection_product_entries(&collection)
    };
    (collection, entries)
}

pub(in crate::proxy) fn collection_passthrough_hydration_ids(
    root_field: &str,
    response: &Response,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Vec<String> {
    match root_field {
        "collectionAddProducts" => {
            let mut ids = collection_product_ids_from_response(
                response,
                "/data/collectionAddProducts/collection",
            );
            ids.reverse();
            if let Some(collection_id) = response
                .body
                .pointer("/data/collectionAddProducts/collection/id")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                ids.insert(0, collection_id);
            }
            ids
        }
        "collectionCreate" => {
            collection_product_ids_from_response(response, "/data/collectionCreate/collection")
        }
        "collectionReorderProducts" => {
            // The async reorder response carries no collection/product nodes, so the
            // hydration set is derived from the mutation input: the target collection
            // plus every moved product. (Previously this returned the live-capture
            // store's ids verbatim, which only hydrated the right nodes for that one
            // recording.)
            let mut ids = Vec::new();
            if let Some(collection_id) = resolved_string_field(variables, "id") {
                ids.push(collection_id);
            }
            for move_input in resolved_object_list_field(variables, "moves") {
                if let Some(product_id) = resolved_string_field(&move_input, "id") {
                    ids.push(product_id);
                }
            }
            ids
        }
        _ => Vec::new(),
    }
}

fn collection_product_ids_from_response(response: &Response, path: &str) -> Vec<String> {
    response
        .body
        .pointer(path)
        .and_then(|collection| collection.get("products"))
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|product| {
            product
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn merge_observed_collection_into_local(local: &Value, observed: &Value) -> Value {
    shallow_merged_object(observed.clone(), local.clone())
}

impl DraftProxy {
    pub(crate) fn collection_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let identifier = BTreeMap::from([("id".to_string(), ResolvedValue::String(id.clone()))]);
        self.hydrate_collection_identifier_if_needed(&invocation, &identifier, None);
        let value = self.collection_canonical_value_by_id(&id);
        ResolverOutcome::value(
            if value.is_null() && self.owner_has_metafield_local_effects(&id) {
                json!({ "__typename": "Collection", "id": id })
            } else {
                value
            },
        )
    }

    pub(crate) fn collections_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::Live
            || (self.config.read_mode != ReadMode::Snapshot && !self.store.has_collection_state())
        {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        if self.config.read_mode == ReadMode::LiveHybrid {
            self.hydrate_collections_for_read(invocation.request, Some(&arguments));
        }
        let result = self.matching_collections_query(&arguments);
        ResolverOutcome::value(typed_connection_value(
            &result.records,
            |collection| self.collection_canonical_value(collection),
            value_id_cursor,
            result.page_info,
        ))
    }

    pub(crate) fn collections_count_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::Live
            || (self.config.read_mode != ReadMode::Snapshot && !self.store.has_collection_state())
        {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        if self.config.read_mode == ReadMode::LiveHybrid {
            let upstream_data = self.hydrate_collections_for_read(invocation.request, None);
            if let Some((base_count, precision)) =
                upstream_count_value(invocation.response_key, upstream_data.as_ref())
            {
                let count = self.adjusted_collections_count_from_upstream(
                    base_count,
                    &arguments,
                    upstream_data.as_ref(),
                );
                return ResolverOutcome::value(count_with_limit_precision_from_upstream(
                    count, &precision, &arguments,
                ));
            }
        }
        ResolverOutcome::value(snapshot_count_with_limit_precision(
            self.matching_collections_query(&arguments).total_count,
            &arguments,
        ))
    }

    pub(crate) fn collection_by_identifier_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let identifier = resolved_object_field(&arguments, "identifier").unwrap_or_default();
        self.hydrate_collection_identifier_if_needed(&invocation, &identifier, None);
        let value = if let Some(id) = resolved_string_field(&identifier, "id") {
            self.collection_canonical_value_by_id(&id)
        } else if let Some(handle) = resolved_string_field(&identifier, "handle") {
            self.collection_canonical_value_by_handle(&handle)
        } else {
            Value::Null
        };
        ResolverOutcome::value(value)
    }

    pub(crate) fn collection_by_handle_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let handle = invocation
            .arguments
            .get("handle")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        self.hydrate_collection_identifier_if_needed(&invocation, &BTreeMap::new(), Some(&handle));
        ResolverOutcome::value(self.collection_canonical_value_by_handle(&handle))
    }

    fn hydrate_collection_identifier_if_needed(
        &mut self,
        invocation: &RootInvocation<'_>,
        identifier: &BTreeMap<String, ResolvedValue>,
        direct_handle: Option<&str>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let id = resolved_string_field(identifier, "id")
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        let handle = direct_handle
            .map(str::trim)
            .filter(|handle| !handle.is_empty())
            .map(str::to_string)
            .or_else(|| {
                resolved_string_field(identifier, "handle")
                    .map(|handle| handle.trim().to_string())
                    .filter(|handle| !handle.is_empty())
            });
        let needs_upstream = id.as_deref().is_some_and(|id| {
            self.store.collection_by_id(id).is_none()
                && !self.store.collection_is_deleted(id)
                && !self
                    .execution_session
                    .owner_metafield_hydrated_ids
                    .contains(id)
        }) || handle.as_deref().is_some_and(|handle| {
            self.store.collection_by_handle(handle).is_none()
                && !self.store.collection_handle_is_deleted(handle)
        });
        if !needs_upstream {
            return;
        }
        let response = (self.upstream_transport)(invocation.request.clone());
        if response.status < 400 {
            self.observe_collections_read_response(&response);
        }
    }

    pub(in crate::proxy) fn collection_canonical_value_by_id(&self, id: &str) -> Value {
        let id = id.trim();
        if id.is_empty() || self.store.collection_is_deleted(id) {
            return Value::Null;
        }
        self.store
            .collection_by_id(id)
            .map(|collection| self.collection_canonical_value(collection))
            .unwrap_or(Value::Null)
    }

    fn collection_canonical_value_by_handle(&self, handle: &str) -> Value {
        let handle = handle.trim();
        if handle.is_empty() {
            return Value::Null;
        }
        self.store
            .collection_by_handle(handle)
            .map(|collection| self.collection_canonical_value(collection))
            .unwrap_or(Value::Null)
    }

    fn collection_canonical_value(&self, collection: &Value) -> Value {
        let mut value = collection.clone();
        self.populate_collection_rule_condition_objects(&mut value);
        if let Some(object) = value.as_object_mut() {
            object.insert("__typename".to_string(), json!("Collection"));
            object.remove("products");
            object.remove("defaultProducts");
            object.remove("manualProducts");
            object.entry("ruleSet".to_string()).or_insert(Value::Null);
            object
                .entry("sortOrder".to_string())
                .or_insert_with(|| json!("BEST_SELLING"));
        }
        value
    }

    pub(in crate::proxy) fn automated_collection_uses_metafield_definition(
        &self,
        definition_id: &str,
    ) -> bool {
        !definition_id.is_empty()
            && self.store.staged.collections.values().any(|collection| {
                collection
                    .get("ruleSet")
                    .and_then(|rule_set| rule_set.get("rules"))
                    .and_then(Value::as_array)
                    .is_some_and(|rules| {
                        rules.iter().any(|rule| {
                            rule.get("column").and_then(Value::as_str)
                                == Some("PRODUCT_METAFIELD_DEFINITION")
                                && collection_rule_metafield_definition_id(rule)
                                    == Some(definition_id)
                        })
                    })
            })
    }

    fn populate_collection_rule_condition_objects(&self, collection: &mut Value) {
        let Some(rules) = collection
            .get_mut("ruleSet")
            .and_then(|rule_set| rule_set.get_mut("rules"))
            .and_then(Value::as_array_mut)
        else {
            return;
        };
        for rule in rules {
            if !matches!(
                rule.get("column").and_then(Value::as_str),
                Some("PRODUCT_METAFIELD_DEFINITION" | "VARIANT_METAFIELD_DEFINITION")
            ) {
                continue;
            }
            let Some(definition_id) = collection_rule_metafield_definition_id(rule) else {
                continue;
            };
            let Some((_, definition)) = self.effective_metafield_definition_by_id(definition_id)
            else {
                continue;
            };
            rule["conditionObject"] = json!({
                "__typename": "CollectionRuleMetafieldCondition",
                "metafieldDefinition": definition
            });
        }
    }

    pub(in crate::proxy) fn hydrate_collections_for_read(
        &mut self,
        request: &Request,
        identity_arguments: Option<&BTreeMap<String, ResolvedValue>>,
    ) -> Option<Value> {
        let response = (self.upstream_transport)(request.clone());
        if response.status < 400 {
            self.observe_collections_read_response(&response);
        }
        let upstream_data = response.body.get("data").cloned();
        if let Some(arguments) = identity_arguments {
            let response = self.upstream_post(
                request,
                json!({
                    "query": COLLECTIONS_IDENTITY_HYDRATE_QUERY,
                    "operationName": "CollectionsIdentityHydrate",
                    "variables": collection_identity_hydrate_variables(arguments),
                }),
            );
            if response.status < 400 {
                self.observe_collections_read_response(&response);
            }
        }
        upstream_data
    }

    pub(in crate::proxy) fn observe_collections_read_response(&mut self, response: &Response) {
        self.observe_collection_value(&response.body["data"]);
    }

    fn observe_collection_value(&mut self, value: &Value) {
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            if is_shopify_gid_of_type(id, "Collection")
                && !self.store.collection_is_deleted(id)
                && !value
                    .get("handle")
                    .and_then(Value::as_str)
                    .is_some_and(|handle| self.store.collection_handle_is_deleted(handle))
                && self.store.collection_by_id(id).is_none()
            {
                self.stage_collection_from_observed_json(value);
            }
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    self.observe_collection_value(value);
                }
            }
            Value::Object(object) => {
                for value in object.values() {
                    self.observe_collection_value(value);
                }
            }
            _ => {}
        }
    }

    pub(in crate::proxy) fn matching_collections_query(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> StagedConnectionResult<Value> {
        staged_connection_query(
            self.store
                .staged
                .collections
                .values()
                .cloned()
                .collect::<Vec<_>>(),
            arguments,
            |collection, query| self.collection_search_decision(collection, query),
            collection_staged_sort_key,
            value_id_cursor,
        )
    }

    fn adjusted_collections_count_from_upstream(
        &self,
        base_count: u64,
        arguments: &BTreeMap<String, ResolvedValue>,
        upstream_data: Option<&Value>,
    ) -> usize {
        let mut count = usize::try_from(base_count).unwrap_or(usize::MAX);
        let query = resolved_string_field(arguments, "query");
        let query = query.as_deref();
        let upstream_identities = upstream_collection_identities(upstream_data);
        if query.map(str::trim).is_none_or(str::is_empty) {
            for id in &self.store.staged.collections.tombstones {
                if !is_synthetic_gid(id) {
                    count = count.saturating_sub(1);
                }
            }
        }
        for (id, collection) in self.store.staged.collections.iter() {
            if self.collection_search_decision(collection, query) != StagedSearchDecision::Match {
                continue;
            }
            if is_synthetic_gid(id) && !upstream_identities.contains_collection_identity(collection)
            {
                count = count.saturating_add(1);
            }
        }
        count
    }

    pub(in crate::proxy) fn collection_search_decision(
        &self,
        collection: &Value,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return StagedSearchDecision::Match;
        };
        StagedSearchDecision::from_bool(self.collection_matches_search_query(collection, query))
    }

    fn collection_matches_search_query(&self, collection: &Value, query: &str) -> bool {
        let terms = collection_search_terms(query);
        if terms.is_empty() {
            return true;
        }
        terms.into_iter().all(|term| {
            if term.eq_ignore_ascii_case("AND") {
                return true;
            }
            let (negated, term) = term
                .strip_prefix('-')
                .map(|stripped| (true, stripped))
                .unwrap_or((false, term.as_str()));
            let matched = collection_matches_search_term(self, collection, term);
            if negated {
                !matched
            } else {
                matched
            }
        })
    }

    /// True once a scenario has seeded (or staged) publications, switching the
    /// `publication`/`channel`/`channels`/`publicationsCount`/
    /// `publishedProductsCount` roots from upstream passthrough to local replay.
    pub(in crate::proxy) fn publication_engine_active(&self) -> bool {
        !self.store.staged.publications.is_empty()
    }

    pub(crate) fn publication_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if !self.publication_engine_active() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ResolverOutcome::value(
            self.store
                .staged
                .publications
                .get(id)
                .map(|record| self.publication_canonical_value(record))
                .unwrap_or(Value::Null),
        )
    }

    pub(crate) fn channel_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if !self.publication_engine_active() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ResolverOutcome::value(
            self.publication_by_channel_id(id)
                .and_then(|(_, record)| record.get("channel").cloned())
                .map(|channel| self.channel_canonical_value(channel))
                .unwrap_or(Value::Null),
        )
    }

    pub(crate) fn channels_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if !self.publication_engine_active() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let mut channels = self
            .store
            .staged
            .publications
            .values()
            .filter_map(|record| record.get("channel").cloned())
            .map(|channel| self.channel_canonical_value(channel))
            .collect::<Vec<_>>();
        channels.sort_by_key(|channel| {
            channel
                .get("id")
                .and_then(Value::as_str)
                .map(resource_id_path_tail)
                .and_then(|suffix| suffix.parse::<u64>().ok())
                .unwrap_or(u64::MAX)
        });
        ResolverOutcome::value(connection_value_with_args(
            channels,
            &arguments,
            value_id_cursor,
        ))
    }

    pub(crate) fn publications_count_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if !self.publication_engine_active() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        ResolverOutcome::value(snapshot_count_with_limit_precision(
            self.store.staged.publications.len(),
            &arguments,
        ))
    }

    pub(crate) fn published_products_count_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if !self.publication_engine_active() {
            return self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
        }
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let publication_id = resolved_string_field(&arguments, "publicationId");
        ResolverOutcome::value(snapshot_count_with_limit_precision(
            self.publication_resource_count(publication_id.as_deref(), "Product"),
            &arguments,
        ))
    }

    fn publication_canonical_value(&self, record: &Value) -> Value {
        let mut value = record.clone();
        if let Some(object) = value.as_object_mut() {
            object.insert("__typename".to_string(), json!("Publication"));
            if let Some(channel) = object.get("channel").cloned() {
                object.insert("channel".to_string(), self.channel_canonical_value(channel));
            }
        }
        value
    }

    fn channel_canonical_value(&self, mut channel: Value) -> Value {
        if let Some(object) = channel.as_object_mut() {
            object.insert("__typename".to_string(), json!("Channel"));
        }
        channel
    }

    /// Discover a publishable resource's pre-existing publication membership by
    /// reading it upstream, the first time the local publication engine
    /// publishes a resource it has never seen. Stages the resource's
    /// title/status and the set of publications it is already published on (e.g.
    /// the default Online Store) into local state, so `resourcePublicationsCount`
    /// / `publicationCount` / the publication's `products` reflect the real
    /// baseline instead of one injected via `/__meta/seed`. No-op once the
    /// resource is known to the engine, outside LiveHybrid, or for an empty id.
    pub(in crate::proxy) fn hydrate_publishable_resource(
        &mut self,
        resource_id: &str,
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid || resource_id.is_empty() {
            return;
        }
        if self
            .store
            .staged
            .resource_publications
            .contains_key(resource_id)
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PUBLICATION_RESOURCE_HYDRATE_QUERY,
                "operationName": "PublicationResourceHydrate",
                "variables": { "ids": [resource_id] },
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let nodes = response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for node in &nodes {
            let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            if is_shopify_gid_of_type(&id, "Product") {
                self.store.stage_observed_product_json(node);
            } else if is_shopify_gid_of_type(&id, "Collection") {
                self.stage_collection_from_observed_json(node);
            }
            // Mark the resource as known to the engine (so re-hydration does not
            // re-fire) and fold in its observed publication membership.
            let set = self
                .store
                .staged
                .resource_publications
                .entry(id)
                .or_default();
            for entry in node
                .pointer("/resourcePublications/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(pid) = entry.pointer("/publication/id").and_then(Value::as_str) {
                    set.insert(pid.to_string());
                }
            }
        }
    }

    /// The set of publication gids a resource (product/collection) is published on.
    fn resource_publication_set(&self, resource_id: &str) -> BTreeSet<String> {
        self.store
            .staged
            .resource_publications
            .get(resource_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::proxy) fn publishable_resource_exists(
        &mut self,
        resource_id: &str,
        request: &Request,
    ) -> bool {
        if resource_id.is_empty() {
            return false;
        }
        let resource_type = shopify_gid_resource_type(resource_id);
        let known = match resource_type {
            Some("Product") => {
                self.product_record_by_id(resource_id).is_some()
                    || self
                        .store
                        .staged
                        .resource_publications
                        .contains_key(resource_id)
            }
            Some("Collection") => {
                self.store.collection_by_id(resource_id).is_some()
                    || self
                        .store
                        .staged
                        .resource_publications
                        .contains_key(resource_id)
            }
            _ => false,
        };
        if known || self.config.read_mode != ReadMode::LiveHybrid {
            return known;
        }

        self.hydrate_publishable_resource(resource_id, request);
        match resource_type {
            Some("Product") => {
                self.product_record_by_id(resource_id).is_some()
                    || self
                        .store
                        .staged
                        .resource_publications
                        .contains_key(resource_id)
            }
            Some("Collection") => {
                self.store.collection_by_id(resource_id).is_some()
                    || self
                        .store
                        .staged
                        .resource_publications
                        .contains_key(resource_id)
            }
            _ => false,
        }
    }

    pub(in crate::proxy) fn current_channel_publication_id(&self) -> Option<String> {
        if self.store.staged.current_channel_publication_resolved {
            return self.store.staged.current_channel_publication_id.clone();
        }
        None
    }

    pub(in crate::proxy) fn resolve_current_channel_publication_id(
        &mut self,
        request: &Request,
    ) -> Option<String> {
        if self.store.staged.current_channel_publication_resolved {
            return self.store.staged.current_channel_publication_id.clone();
        }
        if self.config.read_mode != ReadMode::LiveHybrid {
            self.store.staged.current_channel_publication_resolved = true;
            self.store.staged.current_channel_publication_id = None;
            return self.current_channel_publication_id();
        }

        let response = self.upstream_post(
            request,
            json!({
                "query": CURRENT_APP_PUBLICATION_HYDRATE_QUERY,
                "operationName": "StorePropertiesCurrentAppPublicationHydrate",
                "variables": {},
            }),
        );
        self.store.staged.current_channel_publication_resolved = true;
        self.store.staged.current_channel_publication_id = if (200..300).contains(&response.status)
        {
            response
                .body
                .pointer("/data/currentAppInstallation/publication/id")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        self.store.staged.current_channel_publication_id.clone()
    }

    /// Count resources of the given gid type published on `publication_id` (or on
    /// any publication when `publication_id` is `None`).
    fn publication_resource_count(
        &self,
        publication_id: Option<&str>,
        resource_type: &str,
    ) -> usize {
        self.publication_resource_ids(publication_id, resource_type)
            .len()
    }

    /// The gids of resources of the given type published on `publication_id` (or
    /// on any publication when `publication_id` is `None`), in stable id order.
    fn publication_resource_ids(
        &self,
        publication_id: Option<&str>,
        resource_type: &str,
    ) -> Vec<String> {
        let needle = format!("/{resource_type}/");
        self.store
            .staged
            .resource_publications
            .iter()
            .filter(|(resource_id, pubs)| {
                resource_id.contains(&needle)
                    && match publication_id {
                        Some(pid) => pubs.contains(pid),
                        None => !pubs.is_empty(),
                    }
            })
            .map(|(resource_id, _)| resource_id.clone())
            .collect()
    }

    fn publication_by_channel_id(&self, channel_id: &str) -> Option<(String, Value)> {
        self.store
            .staged
            .publications
            .iter()
            .find_map(|(id, record)| {
                let matches = record
                    .get("channel")
                    .and_then(|channel| channel.get("id"))
                    .and_then(Value::as_str)
                    == Some(channel_id);
                matches.then(|| (id.clone(), record.clone()))
            })
    }

    fn publication_product_entries(&self, publication_id: &str) -> Vec<CollectionProductEntry> {
        self.publication_resource_ids(Some(publication_id), "Product")
            .into_iter()
            .enumerate()
            .filter_map(|(position, resource_id)| {
                let product = self.product_record_by_id(&resource_id)?.clone();
                let variants = self.store.product_variants_for_product(&product.id);
                Some(CollectionProductEntry {
                    position,
                    product,
                    variants,
                })
            })
            .collect()
    }

    pub(in crate::proxy) fn publishable_resource_canonical_value(
        &self,
        resource_id: &str,
    ) -> Value {
        match shopify_gid_resource_type(resource_id) {
            Some("Product") => self
                .product_record_by_id(resource_id)
                .map(|product| self.product_canonical_value(product))
                .unwrap_or(Value::Null),
            Some("Collection") => self.collection_canonical_value_by_id(resource_id),
            _ => Value::Null,
        }
    }

    /// The publication count Shopify reports for a publishable resource's
    /// `resourcePublicationsCount`/`availablePublicationsCount` fields. These
    /// default to `onlyPublished: true`, so they count only publications on which
    /// the resource is actually live. A product that is not `ACTIVE` (e.g. a
    /// `DRAFT`) is never live on any publication regardless of its membership, so
    /// its count is 0 even immediately after a `publishablePublish`. Collections
    /// and other resources have no draft state, so their count is the membership
    /// size.
    fn publishable_live_publication_count(
        &self,
        resource_id: &str,
        pubs: &BTreeSet<String>,
    ) -> usize {
        if is_shopify_gid_of_type(resource_id, "Product") {
            let active = self
                .product_record_by_id(resource_id)
                .map(|product| product.status == "ACTIVE")
                .unwrap_or(false);
            if !active {
                return 0;
            }
        }
        pubs.len()
    }

    /// Stage a `publishablePublish`/`publishableUnpublish` against the local
    /// publication engine: add (or remove) the target publications to the
    /// resource's membership and render the payload from local state.
    pub(in crate::proxy) fn publishable_publish_with_publications(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let publish = matches!(
            invocation.root_name,
            "publishablePublish" | "publishablePublishToCurrentChannel"
        );
        let to_current = matches!(
            invocation.root_name,
            "publishablePublishToCurrentChannel" | "publishableUnpublishToCurrentChannel"
        );
        if let Some(error) = publishable_empty_string_publication_error(
            invocation.query,
            invocation.operation_path,
            invocation.response_key,
            invocation.root_location,
            invocation.variable_definitions,
            &invocation.raw_arguments,
            arguments,
        ) {
            return graphql_error_outcome(vec![error], invocation.response_key);
        }
        let Some(resource_id) = resolved_string_field(arguments, "id") else {
            return ResolverOutcome::value(Value::Null);
        };
        let requests_publishable_details = invocation.requested_field_paths.iter().any(|path| {
            matches!(
                path.as_slice(),
                [parent, field, ..]
                    if parent == "publishable" && matches!(field.as_str(), "title" | "handle")
            )
        });
        if requests_publishable_details
            && is_shopify_gid_of_type(&resource_id, "Collection")
            && self.store.collection_by_id(&resource_id).is_none()
        {
            let _ = self.hydrate_publishable_payload_shop(&resource_id, invocation.request);
        }
        let mut user_errors = Vec::new();
        let resource_exists = self.publishable_resource_exists(&resource_id, invocation.request);
        if !resource_exists {
            user_errors.push(user_error_omit_code(
                ["id"],
                "Resource does not exist",
                Some("RESOURCE_DOES_NOT_EXIST"),
            ));
        }
        if resource_exists
            && is_shopify_gid_of_type(&resource_id, "Product")
            && publishable_input_needs_publication_catalog_hydration(
                arguments.get("input"),
                to_current,
                self.store.has_known_publication_ids(),
            )
        {
            let _ = self.hydrate_publishable_payload_shop(&resource_id, invocation.request);
        }
        user_errors
            .extend(self.publishable_publication_input_errors(arguments.get("input"), to_current));
        let current_channel_id = if resource_exists && to_current {
            self.resolve_current_channel_publication_id(invocation.request)
        } else {
            None
        };
        if resource_exists && to_current && current_channel_id.is_none() {
            user_errors.push(user_error_omit_code(
                ["id"],
                "Channel does not exist",
                Some("CHANNEL_DOES_NOT_EXIST"),
            ));
        }
        if user_errors.is_empty() {
            if requests_publishable_details
                && is_shopify_gid_of_type(&resource_id, "Collection")
                && self.store.collection_by_id(&resource_id).is_none()
            {
                self.hydrate_publishable_resource(&resource_id, invocation.request);
            }
            // Discover the resource's pre-existing publication membership
            // before applying this publish, so counts reflect the real baseline.
            self.hydrate_publishable_resource(&resource_id, invocation.request);
            let publication_ids = if to_current {
                current_channel_id.into_iter().collect::<Vec<_>>()
            } else {
                publishable_input_publication_ids(arguments)
            };
            let published_at = self.next_product_timestamp();
            let set = self
                .store
                .staged
                .resource_publications
                .entry(resource_id.clone())
                .or_default();
            for publication_id in &publication_ids {
                if publish {
                    set.insert(publication_id.clone());
                } else {
                    set.remove(publication_id);
                }
            }
            self.sync_product_publication_entries(
                &resource_id,
                &publication_ids,
                publish,
                &published_at,
            );
        }
        let requests_shop = invocation
            .requested_field_paths
            .iter()
            .any(|path| path.first().is_some_and(|field| field == "shop"));
        if requests_shop && self.store.base.publication_count.is_none() {
            let _ = self.hydrate_publishable_payload_shop(&resource_id, invocation.request);
        }
        let publishable = if user_errors.iter().any(|error| {
            error
                .get("code")
                .and_then(Value::as_str)
                .is_some_and(|code| code == "RESOURCE_DOES_NOT_EXIST")
        }) {
            Value::Null
        } else {
            self.publishable_resource_canonical_value(&resource_id)
        };
        let success = user_errors.is_empty();
        let payload = json!({
            "publishable": publishable,
            "shop": self.store.effective_shop(),
            "userErrors": user_errors,
        });
        let outcome = ResolverOutcome::value(payload);
        if success {
            outcome.with_log_draft(LogDraft::staged(
                invocation.root_name,
                "store_properties",
                Vec::new(),
            ))
        } else {
            outcome
        }
    }

    pub(in crate::proxy) fn observe_collection_passthrough_response(
        &mut self,
        response: &Response,
    ) {
        if response.status >= 400 {
            return;
        }
        self.observe_nodes_response(response);
        if let Some(product) = response
            .body
            .pointer("/data/productVariantsBulkDelete/product")
            .and_then(product_state_from_json)
        {
            self.store.stage_observed_product(product);
        }
        if let Some(collection) = response
            .body
            .pointer("/data/collectionAddProducts/collection")
        {
            self.stage_collection_from_observed_json(collection);
        }
        if let Some(collection) = response.body.pointer("/data/collectionCreate/collection") {
            self.stage_collection_from_observed_json(collection);
        }
    }

    pub(in crate::proxy) fn hydrate_product_nodes_for_observation(&mut self, ids: Vec<String>) {
        let path = self
            .log_entries
            .last()
            .and_then(|entry| entry.get("path"))
            .and_then(Value::as_str)
            .unwrap_or("/admin/api/2025-01/graphql.json")
            .to_string();
        self.hydrate_product_nodes_for_observation_with_request(
            &Request {
                method: "POST".to_string(),
                path,
                headers: BTreeMap::new(),
                body: String::new(),
            },
            ids,
        );
    }

    pub(in crate::proxy) fn hydrate_product_nodes_for_observation_with_request(
        &mut self,
        request: &Request,
        ids: Vec<String>,
    ) {
        if ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY,
                "operationName": "ProductsHydrateNodes",
                "variables": { "ids": ids }
            }),
        );
        self.observe_nodes_response(&response);
    }

    pub(in crate::proxy) fn hydrate_product_set_target_by_id_with_request(
        &mut self,
        request: &Request,
        id: &str,
    ) {
        if id.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PRODUCT_SET_TARGET_HYDRATE_BY_ID_QUERY,
                "operationName": "ProductSetTargetHydrateById",
                "variables": { "ids": [id] }
            }),
        );
        self.observe_nodes_response(&response);
    }

    pub(in crate::proxy) fn hydrate_product_set_target_by_handle_with_request(
        &mut self,
        request: &Request,
        handle: &str,
    ) {
        if handle.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": PRODUCT_SET_TARGET_HYDRATE_BY_HANDLE_QUERY,
                "operationName": "ProductSetTargetHydrateByHandle",
                "variables": { "handle": handle }
            }),
        );
        self.observe_nodes_response(&response);
    }

    /// Forward the options-aware product hydrate (selecting the option/optionValue
    /// graph that the generic observation query omits) and observe it, so a cold
    /// productOptionsReorder resolves the real owning product + option graph from
    /// upstream instead of relying on seeded state.
    pub(in crate::proxy) fn hydrate_product_options_owner(&mut self, product_id: &str) {
        if product_id.is_empty() {
            return;
        }
        let path = self
            .log_entries
            .last()
            .and_then(|entry| entry.get("path"))
            .and_then(Value::as_str)
            .unwrap_or("/admin/api/2025-01/graphql.json")
            .to_string();
        let response = self.upstream_post(
            &Request {
                method: "POST".to_string(),
                path,
                headers: BTreeMap::new(),
                body: String::new(),
            },
            json!({
                "query": PRODUCT_OPTIONS_HYDRATE_NODES_QUERY,
                "operationName": "ProductOptionsHydrateNodes",
                "variables": { "ids": [product_id] }
            }),
        );
        self.observe_nodes_response(&response);
    }

    pub(in crate::proxy) fn stage_collection_from_observed_json(&mut self, collection: &Value) {
        if collection
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| self.store.collection_is_deleted(id))
        {
            return;
        }
        if collection
            .get("handle")
            .and_then(Value::as_str)
            .is_some_and(|handle| self.store.collection_handle_is_deleted(handle))
        {
            return;
        }
        let mut collection = self.observed_collection_for_staging(collection);
        if !collection
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(is_synthetic_gid)
        {
            if let Some(object) = collection.as_object_mut() {
                object.insert(OBSERVED_COLLECTION_BASELINE_FIELD.to_string(), json!(true));
            }
        }
        let products = collection
            .get("products")
            .map(connection_nodes)
            .into_iter()
            .flatten()
            .filter_map(|product| product_state_from_json(&product))
            .collect::<Vec<_>>();
        self.store.stage_collection_membership(collection, products);
    }

    fn observed_collection_for_staging(&self, collection: &Value) -> Value {
        let Some(observed_id) = collection.get("id").and_then(Value::as_str) else {
            return collection.clone();
        };
        if is_synthetic_gid(observed_id) {
            return collection.clone();
        }
        let Some(observed_handle) = collection
            .get("handle")
            .and_then(Value::as_str)
            .filter(|handle| !handle.is_empty())
        else {
            return collection.clone();
        };
        let Some((_, local_collection)) =
            self.store.staged.collections.iter().find(|(id, staged)| {
                id.as_str() != observed_id
                    && is_synthetic_gid(id)
                    && staged.get("handle").and_then(Value::as_str) == Some(observed_handle)
            })
        else {
            return collection.clone();
        };
        merge_observed_collection_into_local(local_collection, collection)
    }

    pub(crate) fn collection_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        match invocation.root_name {
            "collectionCreate" => self.collection_create(&invocation, &arguments),
            "collectionUpdate" => self.collection_update(&invocation, &arguments),
            "collectionDelete" => self.collection_delete(&arguments),
            "collectionAddProducts" => self.collection_add_products(&invocation, &arguments),
            "collectionAddProductsV2" => {
                self.collection_async_membership(&invocation, &arguments, true)
            }
            "collectionRemoveProducts" => {
                self.collection_async_membership(&invocation, &arguments, false)
            }
            "collectionReorderProducts" => {
                self.collection_reorder_products(&invocation, &arguments)
            }
            root => ResolverOutcome::error(format!(
                "No mutation dispatcher implemented for collection root `{root}`"
            )),
        }
    }

    fn collection_create(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let input = collection_input(arguments).unwrap_or_default();
        if input.contains_key("id") {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_user_error(
                    ["id"],
                    "id cannot be specified on collection creation",
                )],
            ));
        }
        let validation_errors =
            match self.collection_input_validation(invocation.query, &input, true) {
                Ok(errors) => errors,
                Err(errors) => return graphql_error_outcome(errors, invocation.response_key),
            };
        if !validation_errors.is_empty() {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                validation_errors,
            ));
        }

        if input.contains_key("ruleSet") && collection_rule_set_rules_missing(&input) {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_user_error(
                    ["ruleSet", "rules"],
                    "Rules cannot be an empty set",
                )],
            ));
        }

        let initial_product_ids = list_string_field(&input, "products");
        self.hydrate_missing_collection_baseline("", &initial_product_ids);
        let product_errors =
            collection_initial_product_user_errors(&self.store, &initial_product_ids);
        if !product_errors.is_empty() {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                product_errors,
            ));
        }

        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("Collection");
        let handle = self.collection_unique_handle(
            resolved_string_field(&input, "handle").as_deref(),
            &title,
            None,
        );
        let timestamp = self.next_product_timestamp();
        let mut collection = collection_from_input(&input, &id, &title, &handle, None);
        apply_collection_timestamps(&mut collection, &timestamp, &timestamp);
        let products = initial_product_ids
            .into_iter()
            .filter_map(|id| self.store.product_by_id(&id).cloned())
            .collect::<Vec<_>>();
        apply_collection_products(&mut collection, &products);
        let mut payload_collection = collection.clone();
        apply_collection_create_payload_products_count(&mut payload_collection);
        self.store.stage_collection(collection.clone());
        self.stage_owner_metafields_from_input(&id, &input);
        self.sync_collection_products(&id, products);

        ResolverOutcome::value(self.collection_payload_value(
            Some(&payload_collection),
            None,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged("collectionCreate", "products", vec![id]))
    }

    fn collection_update(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let input = collection_input(arguments).unwrap_or_default();
        let Some(id) = resolved_string_field(&input, "id").filter(|id| !id.trim().is_empty())
        else {
            return graphql_error_outcome(
                vec![collection_update_missing_id_error(
                    invocation.response_key,
                    invocation.root_location,
                )],
                invocation.response_key,
            );
        };
        self.hydrate_missing_collection_baseline(&id, &[]);
        let Some(existing) = self.store.collection_by_id(&id).cloned() else {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_user_error_null_field(
                    "Collection does not exist",
                )],
            ));
        };
        let validation_errors =
            match self.collection_input_validation(invocation.query, &input, false) {
                Ok(errors) => errors,
                Err(errors) => return graphql_error_outcome(errors, invocation.response_key),
            };
        if !validation_errors.is_empty() {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                validation_errors,
            ));
        }
        if input.contains_key("ruleSet") {
            if collection_rule_set_rules_empty(&input) {
                return ResolverOutcome::value(self.collection_payload_value(
                    None,
                    None,
                    vec![collection_user_error(
                        ["ruleSet", "rules"],
                        "Rules cannot be an empty set",
                    )],
                ));
            }
            if !collection_is_smart(&existing) {
                return ResolverOutcome::value(self.collection_payload_value(
                    None,
                    None,
                    vec![collection_user_error(
                        ["id"],
                        "Cannot update rule set of a custom collection",
                    )],
                ));
            }
        }

        let current_updated_at = existing
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                existing
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            })
            .to_string();
        let next_updated_at = self.next_product_updated_at(&current_updated_at);
        let mut updated = existing;
        if let Some(object) = updated.as_object_mut() {
            object
                .entry(STOREFRONT_COLLECTION_BASELINE_UPDATED_AT_FIELD.to_string())
                .or_insert_with(|| json!(current_updated_at));
            if let Some(title) = resolved_string_field(&input, "title") {
                object.insert("title".to_string(), json!(title));
            }
            if input.contains_key("handle") {
                let previous_handle = object
                    .get("handle")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let title = object
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let handle = self.collection_unique_handle(
                    resolved_string_field(&input, "handle").as_deref(),
                    &title,
                    Some(&id),
                );
                if previous_handle.as_deref() != Some(handle.as_str()) {
                    if let Some(previous_handle) = previous_handle {
                        self.store.delete_collection_handle(&previous_handle);
                    }
                }
                object.insert("handle".to_string(), json!(handle));
            }
            if let Some(sort_order) = resolved_string_field(&input, "sortOrder") {
                object.insert("sortOrder".to_string(), json!(sort_order));
                object.remove(OBSERVED_COLLECTION_BASELINE_FIELD);
            }
            if input.contains_key("ruleSet") {
                object.insert(
                    "ruleSet".to_string(),
                    resolved_object_field(&input, "ruleSet")
                        .map(collection_rule_set_json)
                        .unwrap_or(Value::Null),
                );
                object.remove(OBSERVED_COLLECTION_BASELINE_FIELD);
            }
            if let Some(description) = resolved_string_field(&input, "descriptionHtml") {
                object.insert("descriptionHtml".to_string(), json!(description));
            }
            if let Some(template_suffix) = resolved_string_field(&input, "templateSuffix") {
                object.insert("templateSuffix".to_string(), json!(template_suffix));
            }
            if input.contains_key("image") {
                object.insert(
                    "image".to_string(),
                    collection_image_from_input(&input).unwrap_or(Value::Null),
                );
            }
            if input.contains_key("seo") {
                object.insert(
                    "seo".to_string(),
                    collection_seo_from_input(&input).unwrap_or(Value::Null),
                );
            }
            object
                .entry("createdAt".to_string())
                .or_insert_with(|| json!(default_product_timestamp()));
            object.insert("updatedAt".to_string(), json!(next_updated_at));
        }
        self.store.stage_collection(updated.clone());
        self.stage_owner_metafields_from_input(&id, &input);
        self.refresh_collection_summary_on_products(&id);
        let job = input.contains_key("ruleSet").then(|| {
            let staged_job = self.stage_collection_job();
            let job_id = staged_job
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let payload_job = collection_inline_job(&staged_job);
            (payload_job, job_id)
        });
        let resource_ids = job
            .as_ref()
            .map(|(_, job_id)| vec![id.clone(), job_id.clone()])
            .unwrap_or_else(|| vec![id.clone()]);

        ResolverOutcome::value(self.collection_payload_value(
            Some(&updated),
            job.as_ref().map(|(payload_job, _)| payload_job),
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(
            "collectionUpdate",
            "products",
            resource_ids,
        ))
    }

    fn collection_delete(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let input = collection_input(arguments).unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        self.hydrate_missing_collection_baseline(&id, &[]);
        let deleted = self.store.delete_collection(&id);
        let payload = self.collection_delete_payload_value(
            deleted.then_some(id.as_str()),
            if deleted {
                Vec::new()
            } else {
                vec![collection_user_error(["id"], "Collection does not exist")]
            },
        );
        if deleted {
            ResolverOutcome::value(payload).with_log_draft(LogDraft::staged(
                "collectionDelete",
                "products",
                vec![id],
            ))
        } else {
            ResolverOutcome::value(payload)
        }
    }

    fn collection_add_products(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let root_field = invocation.root_name;
        let collection_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let requested_product_ids = list_string_field(arguments, "productIds");
        let baseline_first = COLLECTION_MEMBERSHIP_BASELINE_FIRST
            .saturating_add(requested_product_ids.len())
            .min(COLLECTION_MEMBERSHIP_MAX_PROBE_ROWS);
        if !self.hydrate_collection_membership_targets(
            invocation.request,
            &collection_id,
            &requested_product_ids,
            baseline_first,
        ) {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_user_error(
                    ["productIds"],
                    "Collection membership could not be resolved",
                )],
            ));
        }
        if let Some(errors) = self.collection_membership_guard_errors(root_field, &collection_id) {
            return ResolverOutcome::value(self.collection_payload_value(None, None, errors));
        }
        let payload_baseline_count = self
            .store
            .collection_membership(&collection_id)
            .and_then(CollectionMembershipState::effective_count)
            .map(|(count, precision)| count_object_with_precision(count, &precision));
        for product_id in &requested_product_ids {
            if self.store.product_by_id(product_id).is_some() {
                self.stage_collection_membership_add(&collection_id, product_id);
            }
        }
        let mut collection = self.refresh_collection_membership_projection(&collection_id);
        if let (Some(collection), Some(payload_baseline_count)) =
            (collection.as_mut(), payload_baseline_count)
        {
            if let Some(object) = collection.as_object_mut() {
                object.insert(
                    COLLECTION_MEMBERSHIP_PAYLOAD_BASELINE_COUNT_FIELD.to_string(),
                    payload_baseline_count,
                );
            }
        }
        ResolverOutcome::value(self.collection_payload_value(collection.as_ref(), None, Vec::new()))
            .with_log_draft(LogDraft::staged(
                root_field,
                "products",
                vec![collection_id],
            ))
    }

    fn collection_async_membership(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
        add: bool,
    ) -> ResolverOutcome<Value> {
        let root_field = invocation.root_name;
        let collection_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let product_ids = list_string_field(arguments, "productIds");
        if product_ids.len() > COLLECTION_PRODUCT_IDS_LIMIT {
            return graphql_error_outcome(
                vec![collection_product_ids_too_long_error(
                    root_field,
                    product_ids.len(),
                )],
                invocation.response_key,
            );
        }
        let baseline_first = COLLECTION_MEMBERSHIP_BASELINE_FIRST
            .saturating_add(product_ids.len())
            .min(COLLECTION_MEMBERSHIP_MAX_PROBE_ROWS);
        if !self.hydrate_collection_membership_targets(
            invocation.request,
            &collection_id,
            &product_ids,
            baseline_first,
        ) {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_user_error(
                    ["productIds"],
                    "Collection membership could not be resolved",
                )],
            ));
        }
        if let Some(errors) = self.collection_membership_guard_errors(root_field, &collection_id) {
            return ResolverOutcome::value(self.collection_payload_value(None, None, errors));
        }
        if add {
            for product_id in &product_ids {
                if self.store.product_by_id(product_id).is_some() {
                    self.stage_collection_membership_add(&collection_id, product_id);
                }
            }
        } else {
            for product_id in &product_ids {
                self.stage_collection_membership_remove(&collection_id, product_id);
            }
        }
        self.refresh_collection_membership_projection(&collection_id);
        let job = self.stage_collection_job();
        let job_id = job
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let payload_job = collection_inline_job(&job);
        ResolverOutcome::value(self.collection_payload_value(None, Some(&payload_job), Vec::new()))
            .with_log_draft(LogDraft::staged(
                root_field,
                "products",
                vec![collection_id, job_id],
            ))
    }

    fn collection_reorder_products(
        &mut self,
        invocation: &RootInvocation<'_>,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let root_field = invocation.root_name;
        let collection_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let moves = resolved_object_list_field(arguments, "moves");
        let move_product_ids = moves
            .iter()
            .filter_map(|move_input| {
                resolved_string_field(move_input, "id")
                    .or_else(|| resolved_string_field(move_input, "productId"))
            })
            .collect::<Vec<_>>();
        let parsed_moves = moves
            .iter()
            .map(|move_input| {
                let product_id = resolved_string_field(move_input, "id")
                    .or_else(|| resolved_string_field(move_input, "productId"))
                    .unwrap_or_default();
                let new_position = resolved_string_field(move_input, "newPosition")
                    .and_then(|value| value.parse::<usize>().ok())
                    .or_else(|| {
                        resolved_int_field(move_input, "newPosition")
                            .map(|value| value.max(0) as usize)
                    })
                    .unwrap_or(0);
                (product_id, new_position)
            })
            .collect::<Vec<_>>();
        let baseline_first = COLLECTION_MEMBERSHIP_BASELINE_FIRST
            .saturating_add(move_product_ids.len())
            .max(
                parsed_moves
                    .iter()
                    .map(|(_, new_position)| new_position.saturating_add(1))
                    .max()
                    .unwrap_or(0),
            )
            .min(COLLECTION_MEMBERSHIP_MAX_PROBE_ROWS);
        let membership_resolved = self.hydrate_collection_membership_targets(
            invocation.request,
            &collection_id,
            &move_product_ids,
            baseline_first,
        );
        self.hydrate_collection_reorder_sort_order(invocation.request, &collection_id);
        if let Some(errors) = self.collection_membership_guard_errors(root_field, &collection_id) {
            return ResolverOutcome::value(self.collection_payload_value(None, None, errors));
        }
        let manually_sorted =
            self.store
                .collection_by_id(&collection_id)
                .is_some_and(|collection| {
                    !collection_is_smart(collection)
                        && collection.get("sortOrder").and_then(Value::as_str) == Some("MANUAL")
                });
        if !manually_sorted {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_user_error(
                    ["id"],
                    "Can't reorder products unless collection is manually sorted",
                )],
            ));
        }
        let membership_resolved = membership_resolved
            || move_product_ids.iter().all(|product_id| {
                self.store.product_by_id(product_id).is_some()
                    && self
                        .store
                        .collection_membership(&collection_id)
                        .and_then(|state| state.effective_membership(product_id))
                        .is_some()
            });
        if !membership_resolved {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_reorder_user_error(
                    ["moves"],
                    "Collection membership could not be resolved",
                    "INVALID_MOVE",
                )],
            ));
        }
        let invalid_move = parsed_moves.iter().find(|(product_id, _)| {
            self.store
                .collection_membership(&collection_id)
                .and_then(|state| state.effective_membership(product_id))
                != Some(true)
        });
        if invalid_move.is_some() {
            return ResolverOutcome::value(self.collection_payload_value(
                None,
                None,
                vec![collection_reorder_user_error(
                    ["moves"],
                    "The move is invalid",
                    "INVALID_MOVE",
                )],
            ));
        }
        for (product_id, new_position) in parsed_moves {
            self.store
                .collection_membership_mut(&collection_id)
                .stage_move(&product_id, new_position);
        }
        self.refresh_collection_membership_projection(&collection_id);
        let job = self.stage_collection_job();
        let job_id = job
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let payload_job = collection_inline_job(&job);
        ResolverOutcome::value(self.collection_payload_value(None, Some(&payload_job), Vec::new()))
            .with_log_draft(LogDraft::staged(
                root_field,
                "products",
                vec![collection_id, job_id],
            ))
    }
}

const COLLECTION_PRODUCT_IDS_LIMIT: usize = 250;
const COLLECTION_SORT_ORDERS: &[&str] = &[
    "ALPHA_ASC",
    "ALPHA_DESC",
    "BEST_SELLING",
    "CREATED",
    "CREATED_DESC",
    "MANUAL",
    "PRICE_ASC",
    "PRICE_DESC",
];

impl DraftProxy {
    fn collection_input_validation(
        &self,
        query: &str,
        input: &BTreeMap<String, ResolvedValue>,
        title_required: bool,
    ) -> Result<Vec<Value>, Vec<Value>> {
        let mut errors = Vec::new();
        match resolved_string_field(input, "title") {
            Some(title) if title.chars().count() > 255 => errors.push(user_error_omit_code(
                ["title"],
                &too_long_message("Title", 255),
                None,
            )),
            Some(title) if title_required && title.trim().is_empty() => errors.push(
                user_error_omit_code(["title"], &blank_message("Title"), None),
            ),
            None if title_required => errors.push(user_error_omit_code(
                ["title"],
                &blank_message("Title"),
                None,
            )),
            _ => {}
        }
        if let Some(handle) = resolved_string_field(input, "handle") {
            if handle.chars().count() > 255 {
                errors.push(user_error_omit_code(
                    ["handle"],
                    &too_long_message("Handle", 255),
                    None,
                ));
            }
        }
        if let Some(sort_order) = resolved_string_field(input, "sortOrder") {
            if !COLLECTION_SORT_ORDERS.contains(&sort_order.as_str()) {
                return Err(vec![collection_invalid_sort_order_error(
                    query,
                    input,
                    &sort_order,
                )]);
            }
        }
        Ok(errors)
    }

    fn collection_membership_guard_errors(
        &self,
        root_field: &str,
        collection_id: &str,
    ) -> Option<Vec<Value>> {
        let Some(collection) = self.store.collection_by_id(collection_id) else {
            return Some(vec![collection_user_error(
                ["id"],
                "Collection does not exist",
            )]);
        };
        if root_field == "collectionAddProductsV2" && collection_is_smart(collection) {
            return Some(vec![collection_user_error(
                ["id"],
                "Can't manually add products to a smart collection",
            )]);
        }
        None
    }

    fn collection_payload_value(
        &self,
        collection: Option<&Value>,
        job: Option<&Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        let collection = collection.map(|collection| {
            let mut collection = collection.clone();
            self.populate_collection_rule_condition_objects(&mut collection);
            if let Some(object) = collection.as_object_mut() {
                // Preserve the mutation's explicit membership source. The
                // Collection field resolver still owns arguments, windowing,
                // and projection, while ordinary reads derive smart membership
                // from the normalized store.
                object.remove("defaultProducts");
                object.remove("manualProducts");
            }
            collection
        });
        json!({
            "collection": collection.unwrap_or(Value::Null),
            "job": job.cloned().unwrap_or(Value::Null),
            "userErrors": user_errors,
        })
    }

    fn collection_delete_payload_value(
        &self,
        deleted_id: Option<&str>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "deletedCollectionId": deleted_id.map_or(Value::Null, |id| json!(id)),
            "shop": Value::Null,
            "userErrors": user_errors,
        })
    }

    pub(in crate::proxy) fn collection_product_entries(
        &self,
        collection: &Value,
    ) -> Vec<CollectionProductEntry> {
        let has_observed_baseline = collection
            .get(OBSERVED_COLLECTION_BASELINE_FIELD)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if collection_is_smart(collection) && !has_observed_baseline {
            return self.smart_collection_product_entries(collection);
        }
        self.explicit_collection_product_entries(collection)
    }

    fn explicit_collection_product_entries(
        &self,
        collection: &Value,
    ) -> Vec<CollectionProductEntry> {
        collection
            .get("products")
            .map(connection_nodes)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .filter_map(|(position, product)| {
                let id = product.get("id").and_then(Value::as_str)?;
                if self.store.product_is_tombstoned(id) {
                    return None;
                }
                let product = product
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| self.store.product_by_id(id).cloned())
                    .or_else(|| product_state_from_json(&product))?;
                let variants = self.store.product_variants_for_product(&product.id);
                Some(CollectionProductEntry {
                    position,
                    product,
                    variants,
                })
            })
            .collect()
    }

    fn smart_collection_product_entries(&self, collection: &Value) -> Vec<CollectionProductEntry> {
        let Some(rule_set) = collection.get("ruleSet") else {
            return Vec::new();
        };
        self.store
            .products()
            .into_iter()
            .enumerate()
            .filter_map(|(position, product)| {
                let variants = self.store.product_variants_for_product(&product.id);
                collection_product_matches_rule_set(&product, &variants, rule_set).then_some(
                    CollectionProductEntry {
                        position,
                        product,
                        variants,
                    },
                )
            })
            .collect()
    }

    fn sync_collection_products(&mut self, collection_id: &str, products: Vec<ProductRecord>) {
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
        self.store.observe_collection_membership(&collection);
        let product_ids = products
            .iter()
            .map(|product| product.id.clone())
            .collect::<BTreeSet<_>>();
        for mut product in self.store.products() {
            if product_ids.contains(&product.id) {
                upsert_minimal_collection(&mut product.collections, &collection);
                self.store.stage_product(product);
            } else if product
                .collections
                .iter()
                .any(|existing| existing.get("id").and_then(Value::as_str) == Some(collection_id))
            {
                remove_minimal_collection(&mut product.collections, collection_id);
                self.store.stage_product(product);
            }
        }
    }

    fn refresh_collection_summary_on_products(&mut self, collection_id: &str) {
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
        for mut product in self.store.products() {
            if product
                .collections
                .iter()
                .any(|existing| existing.get("id").and_then(Value::as_str) == Some(collection_id))
            {
                upsert_minimal_collection(&mut product.collections, &collection);
                self.store.stage_product(product);
            }
        }
    }

    fn hydrate_collection_membership_targets(
        &mut self,
        request: &Request,
        collection_id: &str,
        product_ids: &[String],
        first: usize,
    ) -> bool {
        if collection_id.is_empty() {
            return true;
        }
        if self.config.read_mode != ReadMode::LiveHybrid {
            return true;
        }
        if self.store.collection_membership(collection_id).is_none() {
            if let Some(collection) = self.store.collection_by_id(collection_id).cloned() {
                self.store.observe_collection_membership(&collection);
            }
        }
        let mut unprobed_product_ids = product_ids
            .iter()
            .filter(|product_id| {
                let target_is_known = self.store.product_by_id(product_id).is_some()
                    || self
                        .store
                        .collection_membership(collection_id)
                        .is_some_and(|state| state.probed_product_ids.contains(*product_id));
                let membership_is_known = self
                    .store
                    .collection_membership(collection_id)
                    .and_then(|state| state.effective_membership(product_id))
                    .is_some();
                !target_is_known || !membership_is_known
            })
            .cloned()
            .collect::<Vec<_>>();
        unprobed_product_ids.sort();
        unprobed_product_ids.dedup();
        let needs_baseline = self.store.collection_by_id(collection_id).is_none()
            || self
                .store
                .collection_membership(collection_id)
                .is_none_or(|state| state.baseline_order.len() < first && !state.baseline_complete);
        if !needs_baseline && unprobed_product_ids.is_empty() {
            return true;
        }
        let has_local_authoritative_baseline = self
            .store
            .collection_membership(collection_id)
            .is_some_and(|state| !state.upstream_baseline && state.baseline_complete);
        if has_local_authoritative_baseline && !unprobed_product_ids.is_empty() {
            self.hydrate_product_nodes_for_observation_with_request(
                request,
                unprobed_product_ids.clone(),
            );
            let state = self.store.collection_membership_mut(collection_id);
            state
                .probed_product_ids
                .extend(unprobed_product_ids.iter().cloned());
            return true;
        }

        let response = self.upstream_post(
            request,
            json!({
                "query": COLLECTION_MEMBERSHIP_TARGETS_HYDRATE_QUERY,
                "operationName": "CollectionMembershipTargetsHydrate",
                "variables": {
                    "collectionId": collection_id,
                    "productIds": unprobed_product_ids,
                    "collectionQuery": format!("id:{}", resource_id_tail(collection_id)),
                    "first": first,
                }
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return self.hydrate_collection_membership_targets_from_legacy_observation(
                request,
                collection_id,
                &unprobed_product_ids,
            );
        }
        let Some(collection) = response.body.pointer("/data/collection") else {
            return self.hydrate_collection_membership_targets_from_legacy_observation(
                request,
                collection_id,
                &unprobed_product_ids,
            );
        };
        let Some(target_nodes) = response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .filter(|nodes| nodes.len() == unprobed_product_ids.len())
        else {
            return self.hydrate_collection_membership_targets_from_legacy_observation(
                request,
                collection_id,
                &unprobed_product_ids,
            );
        };
        if self.stage_collection_membership_target_observation(
            collection_id,
            &unprobed_product_ids,
            collection,
            target_nodes,
            true,
        ) {
            return true;
        }
        self.hydrate_collection_membership_targets_from_legacy_observation(
            request,
            collection_id,
            &unprobed_product_ids,
        )
    }

    fn hydrate_collection_membership_targets_from_legacy_observation(
        &mut self,
        request: &Request,
        collection_id: &str,
        product_ids: &[String],
    ) -> bool {
        let mut ids = Vec::with_capacity(product_ids.len() + 1);
        ids.push(collection_id.to_string());
        ids.extend(product_ids.iter().cloned());
        let response = self.upstream_post(
            request,
            json!({
                "query": PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY,
                "operationName": "ProductsHydrateNodes",
                "variables": { "ids": ids }
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return false;
        }
        let Some(nodes) = response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .filter(|nodes| nodes.len() == product_ids.len() + 1)
        else {
            return false;
        };
        self.stage_collection_membership_target_observation(
            collection_id,
            product_ids,
            &nodes[0],
            &nodes[1..],
            false,
        )
    }

    fn hydrate_collection_reorder_sort_order(&mut self, request: &Request, collection_id: &str) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || collection_id.is_empty()
            || self
                .store
                .collection_by_id(collection_id)
                .and_then(|collection| collection.get("sortOrder"))
                .and_then(Value::as_str)
                .is_some()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": COLLECTION_REORDER_PRODUCTS_COLLECTION_HYDRATE_QUERY,
                "operationName": "CollectionReorderProductsCollectionHydrate",
                "variables": { "id": collection_id }
            }),
        );
        if (200..300).contains(&response.status) && response.body.get("errors").is_none() {
            if let Some(collection) = response.body.pointer("/data/collection") {
                self.stage_collection_from_observed_json(collection);
            }
        }
    }

    fn stage_collection_membership_target_observation(
        &mut self,
        collection_id: &str,
        product_ids: &[String],
        collection: &Value,
        target_nodes: &[Value],
        exact_target_filter: bool,
    ) -> bool {
        if collection.get("id").and_then(Value::as_str) != Some(collection_id)
            || target_nodes.len() != product_ids.len()
        {
            return false;
        }
        let collection_connection = collection
            .get("manualProducts")
            .or_else(|| collection.get("products"));
        let collection_members = collection_connection
            .map(connection_nodes)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|product| {
                product
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<BTreeSet<_>>();
        let collection_window_complete = collection_connection.is_some_and(|connection| {
            connection
                .pointer("/pageInfo/hasNextPage")
                .and_then(Value::as_bool)
                != Some(true)
                && connection
                    .pointer("/pageInfo/hasPreviousPage")
                    .and_then(Value::as_bool)
                    != Some(true)
        });
        let mut observed_membership = BTreeMap::<String, bool>::new();
        let mut observed_products = Vec::new();
        for (index, requested_id) in product_ids.iter().enumerate() {
            let Some(node) = target_nodes
                .iter()
                .find(|node| node.get("id").and_then(Value::as_str) == Some(requested_id))
                .or_else(|| target_nodes.get(index).filter(|node| node.is_null()))
            else {
                return false;
            };
            if node.is_null() {
                observed_membership.insert(requested_id.clone(), false);
                continue;
            }
            let Some(product_id) = node.get("id").and_then(Value::as_str) else {
                return false;
            };
            if product_id != requested_id {
                return false;
            }
            let target_collections = node.get("collections");
            let is_member = target_collections
                .map(connection_nodes)
                .unwrap_or_default()
                .iter()
                .any(|collection| {
                    collection.get("id").and_then(Value::as_str) == Some(collection_id)
                })
                || collection_members.contains(product_id);
            let target_window_complete = target_collections.is_some_and(|connection| {
                connection
                    .pointer("/pageInfo/hasNextPage")
                    .and_then(Value::as_bool)
                    != Some(true)
                    && connection
                        .pointer("/pageInfo/hasPreviousPage")
                        .and_then(Value::as_bool)
                        != Some(true)
            });
            if !is_member
                && !exact_target_filter
                && !target_window_complete
                && !collection_window_complete
            {
                return false;
            }
            observed_membership.insert(product_id.to_string(), is_member);
            observed_products.push(node.clone());
        }
        self.stage_collection_from_observed_json(collection);
        for product in &observed_products {
            self.store.stage_observed_product_json(product);
        }
        let state = self.store.collection_membership_mut(collection_id);
        for product_id in product_ids {
            state.probed_product_ids.insert(product_id.clone());
            state.known_membership.insert(
                product_id.clone(),
                observed_membership
                    .get(product_id)
                    .copied()
                    .unwrap_or(false),
            );
        }
        self.refresh_collection_membership_reverse_links(collection_id);
        true
    }

    fn hydrate_collection_membership_window(
        &self,
        request: &Request,
        collection_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        observed_connection: &Value,
        refill_size: usize,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid || refill_size == 0 {
            return None;
        }
        let backward = arguments.contains_key("last");
        let after = if backward {
            arguments
                .get("after")
                .map(resolved_value_json)
                .unwrap_or(Value::Null)
        } else {
            observed_connection
                .pointer("/pageInfo/endCursor")
                .cloned()
                .unwrap_or(Value::Null)
        };
        let before = if backward {
            observed_connection
                .pointer("/pageInfo/startCursor")
                .cloned()
                .unwrap_or(Value::Null)
        } else {
            arguments
                .get("before")
                .map(resolved_value_json)
                .unwrap_or(Value::Null)
        };
        let response = self.upstream_post(
            request,
            json!({
                "query": COLLECTION_MEMBERSHIP_WINDOW_HYDRATE_QUERY,
                "operationName": "CollectionMembershipWindowHydrate",
                "variables": {
                    "id": collection_id,
                    "first": (!backward).then_some(refill_size),
                    "after": after,
                    "last": backward.then_some(refill_size),
                    "before": before,
                    "reverse": resolved_bool_field(arguments, "reverse").unwrap_or(false),
                    "sortKey": resolved_string_field(arguments, "sortKey"),
                }
            }),
        );
        if !(200..300).contains(&response.status) || response.body.get("errors").is_some() {
            return None;
        }
        response.body.pointer("/data/collection/products").cloned()
    }

    fn stage_collection_membership_add(&mut self, collection_id: &str, product_id: &str) {
        let changed = self
            .store
            .collection_membership_mut(collection_id)
            .stage_add(product_id);
        if !changed {
            return;
        }
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        upsert_minimal_collection(&mut product.collections, &collection);
        self.store.stage_product(product);
    }

    fn stage_collection_membership_remove(&mut self, collection_id: &str, product_id: &str) {
        let changed = self
            .store
            .collection_membership_mut(collection_id)
            .stage_remove(product_id);
        if !changed {
            return;
        }
        let Some(mut product) = self.store.product_by_id(product_id).cloned() else {
            return;
        };
        remove_minimal_collection(&mut product.collections, collection_id);
        self.store.stage_product(product);
    }

    fn refresh_collection_membership_reverse_links(&mut self, collection_id: &str) {
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
        let Some(state) = self.store.collection_membership(collection_id).cloned() else {
            return;
        };
        let touched = state
            .deltas
            .iter()
            .map(|delta| match delta {
                CollectionMembershipDelta::Add { product_id }
                | CollectionMembershipDelta::Remove { product_id }
                | CollectionMembershipDelta::Move { product_id, .. } => product_id.clone(),
            })
            .collect::<BTreeSet<_>>();
        for product_id in touched {
            let Some(mut product) = self.store.product_by_id(&product_id).cloned() else {
                continue;
            };
            if state.effective_membership(&product_id) == Some(true) {
                upsert_minimal_collection(&mut product.collections, &collection);
            } else {
                remove_minimal_collection(&mut product.collections, collection_id);
            }
            self.store.stage_product(product);
        }
    }

    fn refresh_collection_membership_projection(&mut self, collection_id: &str) -> Option<Value> {
        let state = self.store.collection_membership(collection_id)?.clone();
        let mut collection = self.store.collection_by_id(collection_id)?.clone();
        if state.baseline_complete {
            let products = state
                .effective_prefix_order()
                .into_iter()
                .filter_map(|product_id| self.store.product_by_id(&product_id).cloned())
                .collect::<Vec<_>>();
            apply_collection_products(&mut collection, &products);
        } else if let Some((count, precision)) = state.effective_count() {
            if let Some(object) = collection.as_object_mut() {
                object.insert(
                    "productsCount".to_string(),
                    count_object_with_precision(count, &precision),
                );
            }
        }
        self.store.stage_collection(collection.clone());
        Some(collection)
    }

    fn hydrate_missing_collection_baseline(&mut self, collection_id: &str, product_ids: &[String]) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut ids = Vec::new();
        if !collection_id.is_empty() && self.store.collection_by_id(collection_id).is_none() {
            ids.push(collection_id.to_string());
        }
        ids.extend(
            product_ids
                .iter()
                .filter(|id| self.store.product_by_id(id).is_none())
                .cloned(),
        );
        ids.sort();
        ids.dedup();
        self.hydrate_product_nodes_for_observation(ids);
    }

    fn stage_collection_job(&mut self) -> Value {
        let job = json!({
            "__typename": "Job",
            "id": self.next_proxy_synthetic_gid("Job"),
            "done": true,
            "query": { "__typename": "QueryRoot" }
        });
        if let Some(id) = job.get("id").and_then(Value::as_str) {
            self.store
                .staged
                .collection_jobs
                .insert(id.to_string(), job.clone());
        }
        job
    }

    fn collection_unique_handle(
        &self,
        requested_handle: Option<&str>,
        title: &str,
        current_id: Option<&str>,
    ) -> String {
        let requested = requested_handle
            .filter(|handle| !handle.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| slugify_handle(title));
        let base = strip_numeric_suffix(&requested);
        let mut candidate = requested;
        let mut suffix = 1;
        while self.collection_handle_exists(&candidate, current_id) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        candidate
    }

    fn collection_handle_exists(&self, handle: &str, current_id: Option<&str>) -> bool {
        self.store
            .staged
            .collections
            .iter()
            .any(|(id, collection)| {
                Some(id.as_str()) != current_id
                    && collection.get("handle").and_then(Value::as_str) == Some(handle)
            })
    }
}

fn upstream_count_value(
    response_key: &str,
    upstream_data: Option<&Value>,
) -> Option<(u64, String)> {
    let value = upstream_data?.get(response_key)?;
    let count = value
        .get("count")
        .and_then(Value::as_u64)
        .or_else(|| value.as_object()?.values().find_map(Value::as_u64))?;
    let precision = value
        .get("precision")
        .and_then(Value::as_str)
        .or_else(|| {
            value.as_object()?.values().find_map(|value| {
                value
                    .as_str()
                    .filter(|value| matches!(*value, "EXACT" | "AT_LEAST"))
            })
        })
        .unwrap_or("EXACT")
        .to_string();
    Some((count, precision))
}

#[derive(Default)]
struct UpstreamCollectionIdentities {
    ids: BTreeSet<String>,
    handles: BTreeSet<String>,
}

impl UpstreamCollectionIdentities {
    fn contains_collection_identity(&self, collection: &Value) -> bool {
        collection
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| self.ids.contains(id))
            || collection
                .get("handle")
                .and_then(Value::as_str)
                .and_then(normalized_collection_handle)
                .is_some_and(|handle| self.handles.contains(&handle))
    }
}

fn upstream_collection_identities(upstream_data: Option<&Value>) -> UpstreamCollectionIdentities {
    let mut identities = UpstreamCollectionIdentities::default();
    if let Some(upstream_data) = upstream_data {
        collect_upstream_collection_identities(upstream_data, &mut identities);
    }
    identities
}

fn collect_upstream_collection_identities(
    value: &Value,
    identities: &mut UpstreamCollectionIdentities,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_upstream_collection_identities(value, identities);
            }
        }
        Value::Object(object) => {
            if object
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| is_shopify_gid_of_type(id, "Collection"))
            {
                if let Some(id) = object.get("id").and_then(Value::as_str) {
                    identities.ids.insert(id.to_string());
                }
                if let Some(handle) = object
                    .get("handle")
                    .and_then(Value::as_str)
                    .and_then(normalized_collection_handle)
                {
                    identities.handles.insert(handle);
                }
            }
            for value in object.values() {
                collect_upstream_collection_identities(value, identities);
            }
        }
        _ => {}
    }
}

fn collection_identity_hydrate_variables(arguments: &BTreeMap<String, ResolvedValue>) -> Value {
    let mut variables = serde_json::Map::new();
    for name in [
        "first",
        "after",
        "last",
        "before",
        "reverse",
        "sortKey",
        "query",
        "savedSearchId",
    ] {
        if let Some(value) = arguments.get(name) {
            variables.insert(name.to_string(), resolved_value_json(value));
        }
    }
    Value::Object(variables)
}

fn collection_input(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match arguments.get("input") {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

fn collection_string_field(collection: &Value, field: &str) -> String {
    collection
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn collection_handle_matches(actual: &str, query_value: &str) -> bool {
    let actual = actual.to_ascii_lowercase();
    let query_value = query_value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if query_value.is_empty() {
        return true;
    }
    if let Some(prefix) = query_value.strip_suffix('*') {
        return actual.starts_with(prefix);
    }
    actual == query_value
}

fn collection_normalized_sort_string(value: &str) -> StagedSortValue {
    StagedSortValue::String(value.to_ascii_lowercase())
}

pub(in crate::proxy) fn collection_staged_sort_key(
    collection: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let id = collection_string_field(collection, "id");
    let primary = match sort_key.unwrap_or("ID") {
        "TITLE" => collection_normalized_sort_string(&collection_string_field(collection, "title")),
        "UPDATED_AT" => StagedSortValue::String(collection_string_field(collection, "updatedAt")),
        "ID" | "RELEVANCE" => resource_id_tail_sort_value(Some(&id)),
        _ => resource_id_tail_sort_value(Some(&id)),
    };
    vec![primary, resource_id_tail_sort_value(Some(&id))]
}

fn collection_search_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in query.chars() {
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | ')' | ' ' | '\t' | '\n' | '\r' if !current.is_empty() => {
                terms.push(std::mem::take(&mut current));
            }
            '(' | ')' | ' ' | '\t' | '\n' | '\r' => {}
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}

fn collection_matches_search_term(proxy: &DraftProxy, collection: &Value, term: &str) -> bool {
    let term = term.trim();
    if term.is_empty() {
        return true;
    }
    if let Some((field, value)) = term.split_once(':') {
        let field = field.to_ascii_lowercase();
        let value = value.trim_matches('"').trim_matches('\'');
        if field.is_empty() || value.is_empty() {
            return false;
        }
        return match field.as_str() {
            "id" => collection_id_matches(collection, value),
            "title" => {
                product_search_string_matches(&collection_string_field(collection, "title"), value)
            }
            "handle" => {
                collection_handle_matches(&collection_string_field(collection, "handle"), value)
            }
            "created_at" => {
                product_matches_date_query(&collection_string_field(collection, "createdAt"), value)
            }
            "updated_at" => {
                product_matches_date_query(&collection_string_field(collection, "updatedAt"), value)
            }
            "collection_type" => collection_matches_type(collection, value),
            "published_status" => collection_matches_published_status(proxy, collection, value),
            "product_id" => collection_matches_product_id(collection, value),
            _ => false,
        };
    }
    product_search_string_matches(&collection_string_field(collection, "title"), term)
        || product_search_string_matches(&collection_string_field(collection, "handle"), term)
}

fn collection_id_matches(collection: &Value, value: &str) -> bool {
    let id = collection_string_field(collection, "id");
    id == value || resource_id_tail(&id) == value
}

fn collection_matches_type(collection: &Value, value: &str) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "custom" => !collection_is_smart(collection),
        "smart" => collection_is_smart(collection),
        _ => false,
    }
}

fn collection_matches_published_status(
    proxy: &DraftProxy,
    collection: &Value,
    value: &str,
) -> bool {
    let id = collection_string_field(collection, "id");
    let published = !proxy.resource_publication_set(&id).is_empty();
    match value.to_ascii_lowercase().as_str() {
        "published" => published,
        "unpublished" => !published,
        "any" => true,
        _ => false,
    }
}

fn collection_matches_product_id(collection: &Value, value: &str) -> bool {
    collection
        .get("products")
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|product| {
            let id = product
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            id == value || resource_id_tail(id) == value
        })
}

fn collection_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    id: &str,
    title: &str,
    handle: &str,
    existing: Option<&Value>,
) -> Value {
    let mut collection = existing.cloned().unwrap_or_else(|| {
        json!({
            "id": id,
            "title": title,
            "handle": handle,
            "createdAt": default_product_timestamp(),
            "updatedAt": default_product_timestamp(),
            "sortOrder": "BEST_SELLING",
            "ruleSet": null,
            "products": connection_json(Vec::<Value>::new()),
            "defaultProducts": connection_json(Vec::<Value>::new()),
            "manualProducts": connection_json(Vec::<Value>::new()),
            "productsCount": count_object(0)
        })
    });
    if let Some(object) = collection.as_object_mut() {
        object.insert("id".to_string(), json!(id));
        object.insert("title".to_string(), json!(title));
        object.insert("handle".to_string(), json!(handle));
        object.insert(
            "sortOrder".to_string(),
            json!(resolved_string_field(input, "sortOrder")
                .unwrap_or_else(|| "BEST_SELLING".to_string())),
        );
        object.insert(
            "ruleSet".to_string(),
            collection_create_rule_set_json(input).unwrap_or(Value::Null),
        );
        if let Some(description) = resolved_string_field(input, "descriptionHtml") {
            object.insert("descriptionHtml".to_string(), json!(description));
        }
        if let Some(template_suffix) = resolved_string_field(input, "templateSuffix") {
            object.insert("templateSuffix".to_string(), json!(template_suffix));
        }
        if input.contains_key("image") {
            object.insert(
                "image".to_string(),
                collection_image_from_input(input).unwrap_or(Value::Null),
            );
        }
        if input.contains_key("seo") {
            object.insert(
                "seo".to_string(),
                collection_seo_from_input(input).unwrap_or(Value::Null),
            );
        }
    }
    collection
}

fn collection_image_from_input(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let image = resolved_object_field(input, "image")?;
    let url = resolved_string_field(&image, "src").unwrap_or_default();
    Some(json!({
        "url": url,
        "src": url,
        "originalSrc": url,
        "altText": resolved_string_field(&image, "altText")
    }))
}

fn collection_seo_from_input(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let seo = resolved_object_field(input, "seo")?;
    Some(json!({
        "title": resolved_string_field(&seo, "title"),
        "description": resolved_string_field(&seo, "description")
    }))
}

fn apply_collection_timestamps(collection: &mut Value, created_at: &str, updated_at: &str) {
    if let Some(object) = collection.as_object_mut() {
        object.insert("createdAt".to_string(), json!(created_at));
        object.insert("updatedAt".to_string(), json!(updated_at));
    }
}

fn collection_initial_product_user_errors(store: &Store, product_ids: &[String]) -> Vec<Value> {
    product_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| store.product_by_id(id).is_none())
        .map(|(index, _)| {
            user_error_omit_code(
                vec!["products".to_string(), index.to_string()],
                "Product does not exist",
                None,
            )
        })
        .collect()
}

fn collection_create_rule_set_json(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let rule_set = resolved_object_field(input, "ruleSet")?;
    (!resolved_object_list_field(&rule_set, "rules").is_empty())
        .then(|| collection_rule_set_json(rule_set))
}

fn apply_collection_products(collection: &mut Value, products: &[ProductRecord]) {
    let product_nodes = products
        .iter()
        .map(product_summary_json)
        .collect::<Vec<_>>();
    if let Some(object) = collection.as_object_mut() {
        object.remove(OBSERVED_COLLECTION_BASELINE_FIELD);
        object.insert(
            "products".to_string(),
            connection_json(product_nodes.clone()),
        );
        object.insert(
            "defaultProducts".to_string(),
            connection_json(product_nodes.clone()),
        );
        object.insert("manualProducts".to_string(), connection_json(product_nodes));
        object.insert("productsCount".to_string(), count_object(products.len()));
    }
}

fn apply_collection_create_payload_products_count(collection: &mut Value) {
    if let Some(object) = collection.as_object_mut() {
        object.insert("productsCount".to_string(), count_object(0));
    }
}

fn collection_inline_job(job: &Value) -> Value {
    json!({
        "__typename": "Job",
        "id": job.get("id").cloned().unwrap_or(Value::Null),
        "done": false,
        "query": Value::Null
    })
}

fn collection_rule_set_json(input: BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "appliedDisjunctively": resolved_bool_field(&input, "appliedDisjunctively").unwrap_or(false),
        "rules": resolved_object_list_field(&input, "rules")
            .into_iter()
            .map(|rule| json!({
                "column": resolved_string_field(&rule, "column").unwrap_or_default(),
                "relation": resolved_string_field(&rule, "relation").unwrap_or_default(),
                "condition": resolved_string_field(&rule, "condition").unwrap_or_default(),
                "conditionObjectId": resolved_string_field(&rule, "conditionObjectId")
            }))
            .collect::<Vec<_>>()
    })
}

fn collection_rule_metafield_definition_id(rule: &Value) -> Option<&str> {
    rule.get("conditionObjectId")
        .and_then(Value::as_str)
        .or_else(|| {
            rule.get("conditionObject")
                .and_then(|condition_object| condition_object.get("metafieldDefinition"))
                .and_then(|definition| definition.get("id"))
                .and_then(Value::as_str)
        })
}

fn collection_rule_set_rules_empty(input: &BTreeMap<String, ResolvedValue>) -> bool {
    resolved_object_field(input, "ruleSet")
        .map(|rule_set| resolved_object_list_field(&rule_set, "rules").is_empty())
        .unwrap_or(false)
}

fn collection_rule_set_rules_missing(input: &BTreeMap<String, ResolvedValue>) -> bool {
    resolved_object_field(input, "ruleSet")
        .map(|rule_set| {
            !rule_set.contains_key("rules")
                || matches!(rule_set.get("rules"), Some(ResolvedValue::Null))
        })
        .unwrap_or(false)
}

fn collection_is_smart(collection: &Value) -> bool {
    collection.get("ruleSet").is_some_and(|rule_set| {
        !rule_set.is_null()
            && rule_set
                .get("rules")
                .and_then(Value::as_array)
                .is_some_and(|rules| !rules.is_empty())
    })
}

fn collection_product_matches_rule_set(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    rule_set: &Value,
) -> bool {
    let rules = rule_set
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rules.is_empty() {
        return false;
    }
    let applied_disjunctively = rule_set
        .get("appliedDisjunctively")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if applied_disjunctively {
        rules
            .iter()
            .any(|rule| collection_product_matches_rule(product, variants, rule))
    } else {
        rules
            .iter()
            .all(|rule| collection_product_matches_rule(product, variants, rule))
    }
}

fn collection_product_matches_rule(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    rule: &Value,
) -> bool {
    let column = rule
        .get("column")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let relation = rule
        .get("relation")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let condition = rule
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match column {
        "TITLE" => {
            collection_rule_string_values_match([product.title.as_str()], relation, condition)
        }
        "TYPE" | "PRODUCT_TYPE" => collection_rule_string_values_match(
            [product.product_type.as_str()],
            relation,
            condition,
        ),
        "VENDOR" => {
            collection_rule_string_values_match([product.vendor.as_str()], relation, condition)
        }
        "TAG" => collection_rule_string_values_match(
            product.tags.iter().map(String::as_str),
            relation,
            condition,
        ),
        "VARIANT_PRICE" => {
            collection_rule_variant_price_matches(product, variants, relation, condition)
        }
        _ => false,
    }
}

fn collection_rule_string_values_match<I>(values: I, relation: &str, condition: &str) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let values = values
        .into_iter()
        .map(|value| collection_rule_normalized_string(value.as_ref()))
        .collect::<Vec<_>>();
    let condition = collection_rule_normalized_string(condition);
    let has_value = values.iter().any(|value| !value.is_empty());
    match relation {
        "EQUALS" => values.iter().any(|value| value == &condition),
        "NOT_EQUALS" => has_value && values.iter().all(|value| value != &condition),
        "CONTAINS" => values.iter().any(|value| value.contains(&condition)),
        "NOT_CONTAINS" => has_value && values.iter().all(|value| !value.contains(&condition)),
        "STARTS_WITH" => values.iter().any(|value| value.starts_with(&condition)),
        "ENDS_WITH" => values.iter().any(|value| value.ends_with(&condition)),
        "IS_SET" => has_value,
        "IS_NOT_SET" => !has_value,
        _ => false,
    }
}

fn collection_rule_normalized_string(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
}

fn collection_rule_variant_price_matches(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    relation: &str,
    condition: &str,
) -> bool {
    let prices = collection_rule_variant_prices(product, variants);
    match relation {
        "IS_SET" => !prices.is_empty(),
        "IS_NOT_SET" => prices.is_empty(),
        _ => {
            let Some(condition) = collection_rule_price_cents(condition) else {
                return false;
            };
            match relation {
                "EQUALS" => prices.contains(&condition),
                "NOT_EQUALS" => {
                    !prices.is_empty() && prices.iter().all(|price| *price != condition)
                }
                "GREATER_THAN" => prices.iter().any(|price| *price > condition),
                "LESS_THAN" => prices.iter().any(|price| *price < condition),
                "GREATER_THAN_OR_EQUAL_TO" | "GREATER_THAN_OR_EQUAL" => {
                    prices.iter().any(|price| *price >= condition)
                }
                "LESS_THAN_OR_EQUAL_TO" | "LESS_THAN_OR_EQUAL" => {
                    prices.iter().any(|price| *price <= condition)
                }
                _ => false,
            }
        }
    }
}

fn collection_rule_variant_prices(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Vec<i64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| collection_rule_price_cents(&variant.price))
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| {
            variant
                .get("price")
                .and_then(Value::as_str)
                .and_then(collection_rule_price_cents)
        })
        .collect()
}

fn collection_rule_price_cents(value: &str) -> Option<i64> {
    parse_product_price(value).map(|price| (price * 100.0).round() as i64)
}

fn collection_product_ids_too_long_error(root_field: &str, len: usize) -> Value {
    max_input_size_exceeded_error(
        vec![root_field.to_string(), "productIds".to_string()],
        len,
        250,
        Some(json!([{ "line": 2, "column": 3 }])),
    )
}

fn collection_update_missing_id_error(response_key: &str, location: SourceLocation) -> Value {
    json!({
        "message": "id must be specified on collectionUpdate",
        "locations": [{"line": location.line, "column": location.column}],
        "extensions": {"code": "BAD_REQUEST"},
        "path": [response_key]
    })
}

fn collection_invalid_sort_order_error(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
    sort_order: &str,
) -> Value {
    let expected_sort_orders = collection_sort_orders_message();
    let location = variable_definition_info(query, "input")
        .map(|definition| definition.location)
        .or_else(|| parsed_document(query, &BTreeMap::new()).map(|document| document.location))
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    invalid_variable_error_envelope(
        format!("Variable $input of type CollectionInput! was provided invalid value for sortOrder (Expected \"{sort_order}\" to be one of: {expected_sort_orders})"),
        location,
        resolved_value_json(&ResolvedValue::Object(input.clone())),
        json!([{
            "path": ["sortOrder"],
            "explanation": format!("Expected \"{sort_order}\" to be one of: {expected_sort_orders}")
        }]),
    )
}

fn collection_sort_orders_message() -> String {
    COLLECTION_SORT_ORDERS.join(", ")
}

fn collection_user_error<const N: usize>(field: [&str; N], message: &str) -> Value {
    user_error_omit_code(field, message, None)
}

fn collection_reorder_user_error<const N: usize>(
    field: [&str; N],
    message: &str,
    code: &str,
) -> Value {
    user_error_omit_code(field, message, Some(code))
}

fn collection_user_error_null_field(message: &str) -> Value {
    user_error_omit_code(Value::Null, message, None)
}

fn strip_numeric_suffix(handle: &str) -> String {
    let Some((base, suffix)) = handle.rsplit_once('-') else {
        return handle.to_string();
    };
    if suffix.chars().all(|ch| ch.is_ascii_digit()) && !base.is_empty() {
        base.to_string()
    } else {
        handle.to_string()
    }
}
