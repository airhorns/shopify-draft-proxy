use super::*;

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
    pub(in crate::proxy) published_on_current_publication: Option<bool>,
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

fn collection_json(
    collection: &Value,
    products: Vec<CollectionProductEntry>,
    selections: &[SelectedField],
    shop_currency_code: &str,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "products" => Some(collection_products_connection_json(
            collection,
            products.clone(),
            selection,
            shop_currency_code,
        )),
        "hasProduct" => {
            let product_id = resolved_string_field(&selection.arguments, "id").unwrap_or_default();
            let has_product = products.iter().any(|entry| entry.product.id == product_id);
            Some(json!(has_product))
        }
        "productsCount" => Some(if collection_is_smart(collection) {
            selected_count_json(products.len(), &selection.selection)
        } else {
            collection
                .get("productsCount")
                .map(|count| selected_json(count, &selection.selection))
                .unwrap_or_else(|| {
                    let count = collection
                        .get("products")
                        .and_then(|connection| connection.get("nodes"))
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0);
                    selected_count_json(count, &selection.selection)
                })
        }),
        "ruleSet" => Some(collection.get("ruleSet").cloned().unwrap_or(Value::Null)),
        "sortOrder" => Some(
            collection
                .get("sortOrder")
                .cloned()
                .unwrap_or_else(|| json!("BEST_SELLING")),
        ),
        _ => collection.get(&selection.name).cloned(),
    })
}

fn collection_products_connection_json(
    collection: &Value,
    mut products: Vec<CollectionProductEntry>,
    selection: &SelectedField,
    shop_currency_code: &str,
) -> Value {
    let sort_key = resolved_string_field(&selection.arguments, "sortKey");
    let reverse = resolved_bool_field(&selection.arguments, "reverse").unwrap_or(false);
    sort_collection_product_entries(collection, &mut products, sort_key.as_deref(), reverse);

    selected_typed_connection_with_args(
        &products,
        &selection.arguments,
        &selection.selection,
        |entry, selections| {
            product_json_with_variants_and_currency_and_publication_context(
                &entry.product,
                &entry.variants,
                selections,
                shop_currency_code,
                entry.published_on_current_publication,
            )
        },
        collection_product_cursor,
    )
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

fn publication_products_connection_json(
    products: Vec<CollectionProductEntry>,
    selection: &SelectedField,
    shop_currency_code: &str,
) -> Value {
    selected_typed_connection_with_args(
        &products,
        &selection.arguments,
        &selection.selection,
        |entry, selections| {
            product_json_with_variants_and_currency_and_publication_context(
                &entry.product,
                &entry.variants,
                selections,
                shop_currency_code,
                entry.published_on_current_publication,
            )
        },
        collection_product_cursor,
    )
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
    /// In live-hybrid mode a `collection(id:)` read for a collection that was
    /// never staged locally must read through to upstream (the recorded
    /// cassette) rather than fabricate a `null`. Mirrors the location overlay
    /// read-through guard. Returns true only when there is a by-id collection
    /// field whose target is absent from local overlay state.
    pub(in crate::proxy) fn collection_read_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return false;
        }
        fields.iter().any(|field| {
            field.name == "collection"
                && resolved_string_field(&field.arguments, "id")
                    .map(|id| {
                        !id.is_empty()
                            && self.store.collection_by_id(&id).is_none()
                            // A locally-deleted (tombstoned) collection is served from
                            // local state for read-after-delete; only a genuinely
                            // unknown collection forwards to hydrate from upstream.
                            && !self.store.collection_is_deleted(&id)
                    })
                    .unwrap_or(false)
        })
    }

    pub(in crate::proxy) fn collection_identifier_read_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return false;
        }
        fields.iter().any(|field| match field.name.as_str() {
            "collectionByIdentifier" => {
                let Some(identifier) = resolved_object_field(&field.arguments, "identifier") else {
                    return false;
                };
                if let Some(id) = resolved_string_field(&identifier, "id")
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                {
                    return self.store.collection_by_id(&id).is_none()
                        && !self.store.collection_is_deleted(&id);
                }
                if let Some(handle) = resolved_string_field(&identifier, "handle")
                    .map(|handle| handle.trim().to_string())
                    .filter(|handle| !handle.is_empty())
                {
                    return self.store.collection_by_handle(&handle).is_none()
                        && !self.store.collection_handle_is_deleted(&handle);
                }
                false
            }
            "collectionByHandle" => resolved_string_field(&field.arguments, "handle")
                .map(|handle| handle.trim().to_string())
                .filter(|handle| !handle.is_empty())
                .map(|handle| {
                    self.store.collection_by_handle(&handle).is_none()
                        && !self.store.collection_handle_is_deleted(&handle)
                })
                .unwrap_or(false),
            _ => false,
        })
    }

    pub(in crate::proxy) fn collection_membership_downstream_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        self.collection_membership_downstream_read_data_with_upstream(fields, None)
    }

    pub(in crate::proxy) fn collection_membership_downstream_read_data_with_upstream(
        &self,
        fields: &[RootFieldSelection],
        upstream_data: Option<&Value>,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "collection" => self.collection_membership_value(field),
                "collectionByIdentifier" => self.collection_by_identifier_value(field),
                "collectionByHandle" => self.collection_by_handle_value(field),
                "collections" => self.collections_connection_field(field),
                "collectionsCount" => {
                    self.collections_count_field_with_upstream(field, upstream_data)
                }
                "product" => self.product_by_id_field(field),
                "job" => self.collection_job_read(field),
                _ => return None,
            })
        })
    }

    pub(in crate::proxy) fn hydrate_collections_for_read(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let response = (self.upstream_transport)(request.clone());
        if response.status < 400 {
            self.observe_collections_read_response(&response);
        }
        let upstream_data = response.body.get("data").cloned();
        self.hydrate_collection_identities_for_read(request, fields);
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

    pub(in crate::proxy) fn collections_connection_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let result = self.matching_collections_query(&field.arguments);
        let shop_currency_code = self.store.shop_currency_code();
        selected_typed_connection_with_page_info(
            &result.records,
            &field.selection,
            |collection, selections| {
                collection_json(
                    collection,
                    self.collection_product_entries(collection),
                    selections,
                    &shop_currency_code,
                )
            },
            value_id_cursor,
            result.page_info,
        )
    }

    pub(in crate::proxy) fn collections_count_field(&self, field: &RootFieldSelection) -> Value {
        self.collections_count_field_with_upstream(field, None)
    }

    fn collections_count_field_with_upstream(
        &self,
        field: &RootFieldSelection,
        upstream_data: Option<&Value>,
    ) -> Value {
        if let Some((base_count, precision)) = upstream_count_field(field, upstream_data) {
            let count =
                self.adjusted_collections_count_from_upstream(base_count, field, upstream_data);
            return selected_json(
                &count_with_limit_precision_from_upstream(count, &precision, &field.arguments),
                &field.selection,
            );
        }
        selected_json(
            &snapshot_count_with_limit_precision(
                self.matching_collections_query(&field.arguments)
                    .total_count,
                &field.arguments,
            ),
            &field.selection,
        )
    }

    fn adjusted_collections_count_from_upstream(
        &self,
        base_count: u64,
        field: &RootFieldSelection,
        upstream_data: Option<&Value>,
    ) -> usize {
        let mut count = usize::try_from(base_count).unwrap_or(usize::MAX);
        let query = resolved_string_field(&field.arguments, "query");
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

    fn hydrate_collection_identities_for_read(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        for field in fields
            .iter()
            .filter(|field| collection_field_needs_identity_hydration(field))
        {
            let response = self.upstream_post(
                request,
                json!({
                    "query": COLLECTIONS_IDENTITY_HYDRATE_QUERY,
                    "operationName": "CollectionsIdentityHydrate",
                    "variables": collection_identity_hydrate_variables(&field.arguments),
                }),
            );
            if response.status < 400 {
                self.observe_collections_read_response(&response);
            }
        }
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

    /// Render a multi-root publication read operation
    /// (`publication`/`channel`/`channels`/`publicationsCount`/
    /// `publishedProductsCount` plus any `product`/`collection` publication
    /// fields) entirely from local publication state.
    pub(in crate::proxy) fn publication_roots_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "publication" => self.publication_root_value(field),
                "channel" => self.channel_root_value(field),
                "channels" => self.channels_root_value(field),
                "publicationsCount" => count_object(self.store.staged.publications.len()),
                "publishedProductsCount" => {
                    let publication_id = resolved_string_field(&field.arguments, "publicationId");
                    count_object(
                        self.publication_resource_count(publication_id.as_deref(), "Product"),
                    )
                }
                "product" | "collection" => self.publishable_resource_root_value(field),
                _ => return None,
            })
        })
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

    fn publication_root_value(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        let Some(record) = self.store.staged.publications.get(&id) else {
            return Value::Null;
        };
        let product_entries = self.publication_product_entries(&id);
        let product_count = product_entries.len();
        let collection_count = self.publication_resource_count(Some(&id), "Collection");
        let shop_currency_code = self.store.shop_currency_code();
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "products" => Some(publication_products_connection_json(
                    product_entries.clone(),
                    selection,
                    &shop_currency_code,
                )),
                "includedProductsCount" | "publishedProductsCount" => {
                    Some(selected_count_json(product_count, &selection.selection))
                }
                "collectionsCount" => {
                    Some(selected_count_json(collection_count, &selection.selection))
                }
                "channel" => {
                    let mut channel = record.get("channel").cloned().unwrap_or(Value::Null);
                    if channel.is_object() {
                        channel["productsCount"] = count_object(product_count);
                    }
                    Some(nullable_selected_json(&channel, &selection.selection))
                }
                _ => record
                    .get(&selection.name)
                    .map(|value| nullable_selected_json(value, &selection.selection)),
            }
        })
    }

    fn publication_product_entries(&self, publication_id: &str) -> Vec<CollectionProductEntry> {
        self.publication_resource_ids(Some(publication_id), "Product")
            .into_iter()
            .enumerate()
            .filter_map(|(position, resource_id)| {
                let product = self.product_record_by_id(&resource_id)?.clone();
                let variants = self.store.product_variants_for_product(&product.id);
                let published_on_current_publication = Some(
                    self.store
                        .product_is_published_on_current_publication(&product),
                );
                Some(CollectionProductEntry {
                    position,
                    product,
                    variants,
                    published_on_current_publication,
                })
            })
            .collect()
    }

    fn channel_root_value(&self, field: &RootFieldSelection) -> Value {
        let Some(channel_id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        let Some((publication_id, record)) = self.publication_by_channel_id(&channel_id) else {
            return Value::Null;
        };
        let mut channel = record.get("channel").cloned().unwrap_or(Value::Null);
        if channel.is_object() {
            let product_count = self.publication_resource_count(Some(&publication_id), "Product");
            channel["productsCount"] = count_object(product_count);
        }
        selected_json(&channel, &field.selection)
    }

    fn channels_root_value(&self, field: &RootFieldSelection) -> Value {
        // Order channels by their publication's numeric id suffix so cursors and
        // node order are deterministic regardless of map key string ordering.
        let mut channels: Vec<Value> = self
            .store
            .staged
            .publications
            .values()
            .filter_map(|record| record.get("channel").cloned())
            .collect();
        channels.sort_by_key(|channel| {
            channel
                .get("id")
                .and_then(Value::as_str)
                .map(resource_id_path_tail)
                .and_then(|suffix| suffix.parse::<u64>().ok())
                .unwrap_or(u64::MAX)
        });
        let cursors: Vec<String> = channels
            .iter()
            .filter_map(|channel| channel.get("id").and_then(Value::as_str))
            .map(|id| format!("cursor:{id}"))
            .collect();
        let connection = json!({
            "nodes": channels,
            "pageInfo": connection_page_info(
                false,
                false,
                cursors.first().cloned(),
                cursors.last().cloned()
            )
        });
        selected_json(&connection, &field.selection)
    }

    fn publishable_resource_root_value(&self, field: &RootFieldSelection) -> Value {
        let Some(resource_id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        // The resource is unknown to the publication engine (never seeded or
        // published) -> null, mirroring Shopify's null for a missing node.
        if !self
            .store
            .staged
            .resource_publications
            .contains_key(&resource_id)
            && self.product_record_by_id(&resource_id).is_none()
        {
            return Value::Null;
        }
        self.publishable_resource_value(&resource_id, &field.selection)
    }

    /// Resolve the publication-membership fields requested on a publishable
    /// resource (product/collection): `publishedOnPublication(publicationId:)`,
    /// `resourcePublicationsCount`/`publicationCount`, `id`, `__typename`. Honors
    /// per-field arguments and inline-fragment type conditions, so it serves both
    /// the top-level `product`/`collection` reads and the `publishablePublish`
    /// payload's `publishable` selection.
    pub(in crate::proxy) fn publishable_resource_value(
        &self,
        resource_id: &str,
        selection: &[SelectedField],
    ) -> Value {
        let resource_type = shopify_gid_resource_type(resource_id).unwrap_or("");
        let pubs = self.resource_publication_set(resource_id);
        let live_pubs = if is_shopify_gid_of_type(resource_id, "Product")
            && self
                .product_record_by_id(resource_id)
                .map(|product| product.status != "ACTIVE")
                .unwrap_or(true)
        {
            BTreeSet::new()
        } else {
            pubs.clone()
        };
        let mut out = serde_json::Map::new();
        for sel in selection {
            if !selected_field_applies_to_type(resource_type, sel) {
                continue;
            }
            let value = match sel.name.as_str() {
                "id" => json!(resource_id),
                "__typename" => json!(resource_type),
                "title" => match resource_type {
                    "Product" => self
                        .product_record_by_id(resource_id)
                        .map(|product| json!(product.title))
                        .unwrap_or(Value::Null),
                    "Collection" => self
                        .store
                        .collection_by_id(resource_id)
                        .and_then(|collection| collection.get("title").cloned())
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                },
                "handle" => match resource_type {
                    "Product" => self
                        .product_record_by_id(resource_id)
                        .map(|product| json!(product.handle))
                        .unwrap_or(Value::Null),
                    "Collection" => self
                        .store
                        .collection_by_id(resource_id)
                        .and_then(|collection| collection.get("handle").cloned())
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                },
                "publishedOnPublication" => {
                    let publication_id = resolved_string_field(&sel.arguments, "publicationId");
                    json!(publication_id.map(|id| pubs.contains(&id)).unwrap_or(false))
                }
                "publishedOnCurrentPublication" => {
                    let current_publication_ids = self.store.current_publication_ids();
                    let published = current_publication_ids
                        .iter()
                        .any(|publication_id| live_pubs.contains(*publication_id));
                    json!(published)
                }
                "resourcePublicationsCount" | "publicationCount" | "availablePublicationsCount" => {
                    count_object(self.publishable_live_publication_count(resource_id, &pubs))
                }
                "resourcePublications" => staged_resource_publication_connection_json(
                    resource_id,
                    resource_type,
                    &live_pubs,
                    "ResourcePublication",
                    &sel.selection,
                ),
                "resourcePublicationsV2" => staged_resource_publication_connection_json(
                    resource_id,
                    resource_type,
                    &live_pubs,
                    "ResourcePublicationV2",
                    &sel.selection,
                ),
                _ => continue,
            };
            out.insert(sel.response_key.clone(), value);
        }
        Value::Object(out)
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
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(document) = parsed_document(query, variables) else {
            return json_error(400, "Unable to parse publishable mutation");
        };
        let operation_path = document.operation_path.clone();
        let Some(fields) = self.execution_root_fields(query, variables) else {
            return json_error(400, "Operation has no root field");
        };
        let publish = matches!(
            root_field,
            "publishablePublish" | "publishablePublishToCurrentChannel"
        );
        let to_current = matches!(
            root_field,
            "publishablePublishToCurrentChannel" | "publishableUnpublishToCurrentChannel"
        );
        let mut early_response = None;
        let data = root_payload_json(&fields, |field| {
            if early_response.is_some() {
                return None;
            }
            if field.name != root_field {
                return None;
            }
            if let Some(response) =
                publishable_empty_string_publication_error(query, &operation_path, field)
            {
                early_response = Some(response);
                return None;
            }
            let resource_id = resolved_string_field(&field.arguments, "id")?;
            let publishable_selection =
                selected_child_selection(&field.selection, "publishable").unwrap_or_default();
            if self
                .publishable_payload_resource_needs_hydration(&resource_id, &publishable_selection)
            {
                self.hydrate_publishable_payload_shop(&resource_id, request);
            }
            let mut user_errors = Vec::new();
            let resource_exists = self.publishable_resource_exists(&resource_id, request);
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
                    field.arguments.get("input"),
                    to_current,
                    self.store.has_known_publication_ids(),
                )
            {
                self.hydrate_publishable_payload_shop(&resource_id, request);
            }
            user_errors.extend(
                self.publishable_publication_input_errors(field.arguments.get("input"), to_current),
            );
            let current_channel_id = if resource_exists && to_current {
                self.resolve_current_channel_publication_id(request)
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
                if self.publishable_payload_resource_needs_hydration(
                    &resource_id,
                    &publishable_selection,
                ) {
                    self.hydrate_publishable_resource(&resource_id, request);
                }
                // Discover the resource's pre-existing publication membership
                // (e.g. the default Online Store) by reading upstream before
                // applying this publish, so counts reflect the real baseline.
                self.hydrate_publishable_resource(&resource_id, request);
                let publication_ids = if to_current {
                    current_channel_id.into_iter().collect::<Vec<_>>()
                } else {
                    publishable_input_publication_ids(&field.arguments)
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
                self.record_mutation_log_entry(request, query, variables, root_field, vec![]);
            }
            // When the payload selects `shop { publicationCount }` and the shop
            // baseline is not present in store state (the publication engine seeds
            // only `publications`, never the shop), hydrate it from the captured
            // upstream hydrate call — same as the non-engine publishable path.
            if selected_child_selection(&field.selection, "shop")
                .as_deref()
                .is_some_and(|selection| self.publishable_payload_shop_needs_hydration(selection))
            {
                self.hydrate_publishable_payload_shop(&resource_id, request);
            }
            let publishable_selection =
                selected_child_selection(&field.selection, "publishable").unwrap_or_default();
            let publishable = if user_errors.iter().any(|error| {
                error
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| code == "RESOURCE_DOES_NOT_EXIST")
            }) {
                Value::Null
            } else {
                self.publishable_resource_value(&resource_id, &publishable_selection)
            };
            let shop = self.store.effective_shop();
            let payload = selected_payload_json(&field.selection, |selection| {
                match selection.name.as_str() {
                    "publishable" => Some(publishable.clone()),
                    "shop" => Some(selected_json(&shop, &selection.selection)),
                    "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                    _ => None,
                }
            });
            Some(payload)
        });
        if let Some(response) = early_response {
            return response;
        }
        ok_json(json!({ "data": data }))
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

    pub(in crate::proxy) fn collection_membership_value(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.store
            .collection_by_id(&id)
            .map(|collection| {
                self.collection_json_with_publication_fields(collection, &field.selection)
            })
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn collection_by_identifier_value(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(identifier) = resolved_object_field(&field.arguments, "identifier") else {
            return Value::Null;
        };
        if let Some(id) = resolved_string_field(&identifier, "id") {
            return self.collection_by_id_value(&id, &field.selection);
        }
        if let Some(handle) = resolved_string_field(&identifier, "handle") {
            return self.collection_by_handle_selection_value(&handle, &field.selection);
        }
        Value::Null
    }

    pub(in crate::proxy) fn collection_by_handle_value(&self, field: &RootFieldSelection) -> Value {
        resolved_string_field(&field.arguments, "handle")
            .map(|handle| self.collection_by_handle_selection_value(&handle, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn collection_by_id_value(&self, id: &str, selection: &[SelectedField]) -> Value {
        if id.trim().is_empty() || self.store.collection_is_deleted(id) {
            return Value::Null;
        }
        self.store
            .collection_by_id(id)
            .map(|collection| self.collection_json_with_publication_fields(collection, selection))
            .unwrap_or(Value::Null)
    }

    fn collection_by_handle_selection_value(
        &self,
        handle: &str,
        selection: &[SelectedField],
    ) -> Value {
        let handle = handle.trim();
        if handle.is_empty() {
            return Value::Null;
        }
        self.store
            .collection_by_handle(handle)
            .map(|collection| self.collection_json_with_publication_fields(collection, selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn collection_json_with_publication_fields(
        &self,
        collection: &Value,
        selections: &[SelectedField],
    ) -> Value {
        self.collection_json_with_publication_fields_and_products(
            collection,
            selections,
            self.collection_product_entries(collection),
        )
    }

    fn collection_payload_json_with_publication_fields(
        &self,
        collection: &Value,
        selections: &[SelectedField],
    ) -> Value {
        self.collection_json_with_publication_fields_and_products(
            collection,
            selections,
            self.explicit_collection_product_entries(collection),
        )
    }

    fn collection_json_with_publication_fields_and_products(
        &self,
        collection: &Value,
        selections: &[SelectedField],
        products: Vec<CollectionProductEntry>,
    ) -> Value {
        let shop_currency_code = self.store.shop_currency_code();
        let mut value = collection_json(collection, products, selections, &shop_currency_code);
        let Some(fields) = value.as_object_mut() else {
            return value;
        };
        let collection_id = collection
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let publications = self.resource_publication_set(collection_id);
        for selection in selections {
            let computed = match selection.name.as_str() {
                "publishedOnPublication" => {
                    let publication_id =
                        resolved_string_field(&selection.arguments, "publicationId");
                    Some(json!(publication_id
                        .map(|id| publications.contains(&id))
                        .unwrap_or(false)))
                }
                "publishedOnCurrentPublication" => Some(json!(self
                    .store
                    .resource_is_published_on_current_publication(collection_id))),
                "resourcePublicationsCount" | "publicationCount" | "availablePublicationsCount" => {
                    Some(selected_count_json(
                        self.publishable_live_publication_count(collection_id, &publications),
                        &selection.selection,
                    ))
                }
                _ => None,
            };
            if let Some(computed) = computed {
                fields.insert(selection.response_key.clone(), computed);
            }
        }
        value
    }

    fn collection_job_read(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.store
            .staged
            .collection_jobs
            .get(&id)
            .map(|job| selected_json(job, &field.selection))
            .unwrap_or(Value::Null)
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
        let collection = self.observed_collection_for_staging(collection);
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

    pub(in crate::proxy) fn collection_mutation(
        &mut self,
        request: &Request,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "collectionCreate" => self.collection_create(query, variables),
            "collectionUpdate" => self.collection_update(query, variables),
            "collectionDelete" => self.collection_delete(request, query, variables),
            "collectionAddProducts" => self.collection_add_products(root_field, query, variables),
            "collectionAddProductsV2" => {
                self.collection_async_membership(root_field, query, variables, true)
            }
            "collectionRemoveProducts" => {
                self.collection_async_membership(root_field, query, variables, false)
            }
            "collectionReorderProducts" => self.collection_reorder_products(query, variables),
            _ => MutationOutcome::response(json_error(
                400,
                "No mutation dispatcher implemented for collection root",
            )),
        }
    }

    fn collection_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let arguments = self
            .execution_root_field(query, variables, "collectionCreate")
            .map(|field| field.arguments)
            .unwrap_or_default();
        let input = collection_input(&arguments).unwrap_or_default();
        if input.contains_key("id") {
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                "collectionCreate",
                None,
                None,
                vec![collection_user_error(
                    ["id"],
                    "id cannot be specified on collection creation",
                )],
            ));
        }
        if let Some(response) = self.collection_input_validation_response(
            query,
            variables,
            "collectionCreate",
            &input,
            true,
        ) {
            return MutationOutcome::response(response);
        }

        if input.contains_key("ruleSet") && collection_rule_set_rules_missing(&input) {
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                "collectionCreate",
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
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                "collectionCreate",
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

        MutationOutcome::staged(
            self.collection_payload_response(
                query,
                variables,
                "collectionCreate",
                Some(&payload_collection),
                None,
                Vec::new(),
            ),
            LogDraft::staged("collectionCreate", "products", vec![id]),
        )
    }

    fn collection_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let field = self.execution_root_field(query, variables, "collectionUpdate");
        let input = field
            .as_ref()
            .and_then(|field| collection_input(&field.arguments))
            .unwrap_or_default();
        let Some(id) = resolved_string_field(&input, "id").filter(|id| !id.trim().is_empty())
        else {
            return MutationOutcome::response(collection_update_missing_id_response(
                field.as_ref(),
            ));
        };
        self.hydrate_missing_collection_baseline(&id, &[]);
        let Some(existing) = self.store.collection_by_id(&id).cloned() else {
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                "collectionUpdate",
                None,
                None,
                vec![collection_user_error_null_field(
                    "Collection does not exist",
                )],
            ));
        };
        if let Some(response) = self.collection_input_validation_response(
            query,
            variables,
            "collectionUpdate",
            &input,
            false,
        ) {
            return MutationOutcome::response(response);
        }
        if input.contains_key("ruleSet") {
            if collection_rule_set_rules_empty(&input) {
                return MutationOutcome::response(self.collection_payload_response(
                    query,
                    variables,
                    "collectionUpdate",
                    None,
                    None,
                    vec![collection_user_error(
                        ["ruleSet", "rules"],
                        "Rules cannot be an empty set",
                    )],
                ));
            }
            if !collection_is_smart(&existing) {
                return MutationOutcome::response(self.collection_payload_response(
                    query,
                    variables,
                    "collectionUpdate",
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
            }
            if input.contains_key("ruleSet") {
                object.insert(
                    "ruleSet".to_string(),
                    resolved_object_field(&input, "ruleSet")
                        .map(collection_rule_set_json)
                        .unwrap_or(Value::Null),
                );
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

        MutationOutcome::staged(
            self.collection_payload_response(
                query,
                variables,
                "collectionUpdate",
                Some(&updated),
                job.as_ref().map(|(payload_job, _)| payload_job),
                Vec::new(),
            ),
            LogDraft::staged("collectionUpdate", "products", resource_ids),
        )
    }

    fn collection_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let arguments = self
            .execution_root_field(query, variables, "collectionDelete")
            .map(|field| field.arguments)
            .unwrap_or_default();
        let input = collection_input(&arguments).unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let (_, payload_selection) =
            self.execution_primary_root_response_selection(query, variables, || {
                "collectionDelete".to_string()
            });
        self.hydrate_payload_shop_identity_if_selected(request, &payload_selection);
        self.hydrate_missing_collection_baseline(&id, &[]);
        let deleted = self.store.delete_collection(&id);
        let response = self.collection_delete_response(
            query,
            variables,
            deleted.then_some(id.as_str()),
            if deleted {
                Vec::new()
            } else {
                vec![collection_user_error(["id"], "Collection does not exist")]
            },
        );
        if deleted {
            MutationOutcome::staged(
                response,
                LogDraft::staged("collectionDelete", "products", vec![id]),
            )
        } else {
            MutationOutcome::response(response)
        }
    }

    fn collection_add_products(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let arguments = self
            .execution_root_field(query, variables, root_field)
            .map(|field| field.arguments)
            .unwrap_or_default();
        let collection_id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let requested_product_ids = list_string_field(&arguments, "productIds");
        self.hydrate_missing_collection_baseline(&collection_id, &requested_product_ids);
        if let Some(response) = self.collection_membership_guard_response(
            query,
            variables,
            root_field,
            &collection_id,
            false,
        ) {
            return MutationOutcome::response(response);
        }
        let mut products = self.collection_products(&collection_id);
        let mut product_ids = products
            .iter()
            .map(|product| product.id.clone())
            .collect::<BTreeSet<_>>();
        for product_id in requested_product_ids {
            if product_ids.contains(&product_id) {
                continue;
            }
            if let Some(product) = self.store.product_by_id(&product_id).cloned() {
                product_ids.insert(product_id);
                products.push(product);
            }
        }
        let collection = self.replace_collection_products(&collection_id, products);
        MutationOutcome::staged(
            self.collection_payload_response(
                query,
                variables,
                root_field,
                collection.as_ref(),
                None,
                Vec::new(),
            ),
            LogDraft::staged(root_field, "products", vec![collection_id]),
        )
    }

    fn collection_async_membership(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        add: bool,
    ) -> MutationOutcome {
        let arguments = self
            .execution_root_field(query, variables, root_field)
            .map(|field| field.arguments)
            .unwrap_or_default();
        let collection_id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let product_ids = list_string_field(&arguments, "productIds");
        if product_ids.len() > COLLECTION_PRODUCT_IDS_LIMIT {
            return MutationOutcome::response(collection_product_ids_too_long_response(
                root_field,
                product_ids.len(),
            ));
        }
        self.hydrate_missing_collection_baseline(&collection_id, &product_ids);
        if let Some(response) = self.collection_membership_guard_response(
            query,
            variables,
            root_field,
            &collection_id,
            true,
        ) {
            return MutationOutcome::response(response);
        }
        let mut products = self.collection_products(&collection_id);
        if add {
            for product_id in &product_ids {
                if products.iter().any(|product| product.id == *product_id) {
                    continue;
                }
                if let Some(product) = self.store.product_by_id(product_id).cloned() {
                    products.push(product);
                }
            }
        } else {
            products.retain(|product| !product_ids.iter().any(|id| id == &product.id));
        }
        self.replace_collection_products(&collection_id, products);
        let job = self.stage_collection_job();
        let job_id = job
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let payload_job = collection_inline_job(&job);
        MutationOutcome::staged(
            self.collection_payload_response(
                query,
                variables,
                root_field,
                None,
                Some(&payload_job),
                Vec::new(),
            ),
            LogDraft::staged(root_field, "products", vec![collection_id, job_id]),
        )
    }

    fn collection_reorder_products(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let arguments = self
            .execution_root_field(query, variables, "collectionReorderProducts")
            .map(|field| field.arguments)
            .unwrap_or_default();
        let collection_id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let moves = resolved_object_list_field(&arguments, "moves");
        let move_product_ids = moves
            .iter()
            .filter_map(|move_input| {
                resolved_string_field(move_input, "id")
                    .or_else(|| resolved_string_field(move_input, "productId"))
            })
            .collect::<Vec<_>>();
        self.hydrate_missing_collection_baseline(&collection_id, &move_product_ids);
        if let Some(response) = self.collection_membership_guard_response(
            query,
            variables,
            "collectionReorderProducts",
            &collection_id,
            true,
        ) {
            return MutationOutcome::response(response);
        }
        self.hydrate_collection_reorder_sort_order(&collection_id);
        let manually_sorted =
            self.store
                .collection_by_id(&collection_id)
                .is_some_and(|collection| {
                    !collection_is_smart(collection)
                        && collection.get("sortOrder").and_then(Value::as_str) == Some("MANUAL")
                });
        if !manually_sorted {
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                "collectionReorderProducts",
                None,
                None,
                vec![collection_user_error(
                    ["id"],
                    "Can't reorder products unless collection is manually sorted",
                )],
            ));
        }
        let mut products = self.collection_products(&collection_id);
        for move_input in moves {
            let product_id = resolved_string_field(&move_input, "id")
                .or_else(|| resolved_string_field(&move_input, "productId"))
                .unwrap_or_default();
            let new_position = resolved_string_field(&move_input, "newPosition")
                .and_then(|value| value.parse::<usize>().ok())
                .or_else(|| {
                    resolved_int_field(&move_input, "newPosition")
                        .map(|value| value.max(0) as usize)
                })
                .unwrap_or(0);
            if let Some(index) = products.iter().position(|product| product.id == product_id) {
                let product = products.remove(index);
                products.insert(new_position.min(products.len()), product);
            }
        }
        self.replace_collection_products(&collection_id, products);
        let job = self.stage_collection_job();
        let job_id = job
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let payload_job = collection_inline_job(&job);
        MutationOutcome::staged(
            self.collection_payload_response(
                query,
                variables,
                "collectionReorderProducts",
                None,
                Some(&payload_job),
                Vec::new(),
            ),
            LogDraft::staged(
                "collectionReorderProducts",
                "products",
                vec![collection_id, job_id],
            ),
        )
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
    fn collection_input_validation_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        input: &BTreeMap<String, ResolvedValue>,
        title_required: bool,
    ) -> Option<Response> {
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
                return Some(collection_invalid_sort_order_response(
                    query,
                    input,
                    &sort_order,
                ));
            }
        }
        (!errors.is_empty()).then(|| {
            self.collection_payload_response(query, variables, root_field, None, None, errors)
        })
    }

    fn collection_membership_guard_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        collection_id: &str,
        job_payload: bool,
    ) -> Option<Response> {
        let Some(collection) = self.store.collection_by_id(collection_id) else {
            return Some(self.collection_payload_response(
                query,
                variables,
                root_field,
                None,
                None,
                vec![collection_user_error(["id"], "Collection does not exist")],
            ));
        };
        if root_field == "collectionAddProductsV2" && collection_is_smart(collection) {
            return Some(self.collection_payload_response(
                query,
                variables,
                root_field,
                None,
                job_payload.then_some(&Value::Null),
                vec![collection_user_error(
                    ["id"],
                    "Can't manually add products to a smart collection",
                )],
            ));
        }
        None
    }

    fn collection_payload_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        collection: Option<&Value>,
        job: Option<&Value>,
        user_errors: Vec<Value>,
    ) -> Response {
        let (response_key, payload_selection) =
            self.execution_primary_root_response_selection(query, variables, || {
                root_field.to_string()
            });
        let collection_selection =
            selected_child_selection(&payload_selection, "collection").unwrap_or_default();
        let job_selection = selected_child_selection(&payload_selection, "job").unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "collection" => Some(collection.map(|collection| self.collection_payload_json_with_publication_fields(collection, &collection_selection)).unwrap_or(Value::Null)),
                    "job" => Some(job.map(|job| selected_json(job, &job_selection)).unwrap_or(Value::Null)),
                    "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                    _ => None,
                })
            }
        }))
    }

    fn collection_delete_response(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        deleted_id: Option<&str>,
        user_errors: Vec<Value>,
    ) -> Response {
        let (response_key, payload_selection) =
            self.execution_primary_root_response_selection(query, variables, || {
                "collectionDelete".to_string()
            });
        let shop = self.store.effective_shop();
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "deletedCollectionId" => Some(deleted_id.map_or(Value::Null, |id| json!(id))),
                    "shop" => Some(selected_json(&shop, &selection.selection)),
                    "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
                    _ => None,
                })
            }
        }))
    }

    fn collection_products(&self, collection_id: &str) -> Vec<ProductRecord> {
        self.store
            .collection_by_id(collection_id)
            .and_then(|collection| collection.get("products"))
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|product| {
                product
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| self.store.product_by_id(id).cloned())
                    .or_else(|| product_state_from_json(product))
            })
            .collect()
    }

    pub(in crate::proxy) fn collection_product_entries(
        &self,
        collection: &Value,
    ) -> Vec<CollectionProductEntry> {
        if collection_is_smart(collection) {
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
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
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
                    .or_else(|| product_state_from_json(product))?;
                let variants = self.store.product_variants_for_product(&product.id);
                let published_on_current_publication = Some(
                    self.store
                        .product_is_published_on_current_publication(&product),
                );
                Some(CollectionProductEntry {
                    position,
                    product,
                    variants,
                    published_on_current_publication,
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
                let published_on_current_publication = Some(
                    self.store
                        .product_is_published_on_current_publication(&product),
                );
                collection_product_matches_rule_set(&product, &variants, rule_set).then_some(
                    CollectionProductEntry {
                        position,
                        product,
                        variants,
                        published_on_current_publication,
                    },
                )
            })
            .collect()
    }

    fn replace_collection_products(
        &mut self,
        collection_id: &str,
        products: Vec<ProductRecord>,
    ) -> Option<Value> {
        let mut collection = self.store.collection_by_id(collection_id)?.clone();
        apply_collection_products(&mut collection, &products);
        self.store.stage_collection(collection.clone());
        self.sync_collection_products(collection_id, products);
        Some(collection)
    }

    fn sync_collection_products(&mut self, collection_id: &str, products: Vec<ProductRecord>) {
        let Some(collection) = self.store.collection_by_id(collection_id).cloned() else {
            return;
        };
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

    fn hydrate_collection_reorder_sort_order(&mut self, collection_id: &str) {
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
                "query": COLLECTION_REORDER_PRODUCTS_COLLECTION_HYDRATE_QUERY,
                "operationName": "CollectionReorderProductsCollectionHydrate",
                "variables": { "id": collection_id }
            }),
        );
        if let Some(collection) = response.body.pointer("/data/collection") {
            self.stage_collection_from_observed_json(collection);
        }
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

fn upstream_count_field(
    field: &RootFieldSelection,
    upstream_data: Option<&Value>,
) -> Option<(u64, String)> {
    let value = upstream_data?.get(field.response_key.as_str())?;
    let count_key = field
        .selection
        .iter()
        .find(|selection| selection.name == "count")
        .map(|selection| selection.response_key.as_str())
        .unwrap_or("count");
    let precision_key = field
        .selection
        .iter()
        .find(|selection| selection.name == "precision")
        .map(|selection| selection.response_key.as_str())
        .unwrap_or("precision");
    let count = value
        .get(count_key)
        .or_else(|| value.get("count"))
        .and_then(Value::as_u64)?;
    let precision = value
        .get(precision_key)
        .or_else(|| value.get("precision"))
        .and_then(Value::as_str)
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

fn collection_field_needs_identity_hydration(field: &RootFieldSelection) -> bool {
    field.name == "collections"
        && collection_connection_selects_collection_nodes(&field.selection)
        && !collection_connection_selects_collection_node_id(&field.selection)
}

fn collection_connection_selects_collection_nodes(selections: &[SelectedField]) -> bool {
    selections
        .iter()
        .any(|selection| match selection.name.as_str() {
            "nodes" => true,
            "edges" => selection
                .selection
                .iter()
                .any(|edge_selection| edge_selection.name == "node"),
            _ => false,
        })
}

fn collection_connection_selects_collection_node_id(selections: &[SelectedField]) -> bool {
    selections
        .iter()
        .any(|selection| match selection.name.as_str() {
            "nodes" => selection
                .selection
                .iter()
                .any(|node_field| node_field.name == "id"),
            "edges" => selection
                .selection
                .iter()
                .filter(|edge_selection| edge_selection.name == "node")
                .any(|node_selection| {
                    node_selection
                        .selection
                        .iter()
                        .any(|node_field| node_field.name == "id")
                }),
            _ => false,
        })
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
                "condition": resolved_string_field(&rule, "condition").unwrap_or_default()
            }))
            .collect::<Vec<_>>()
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

fn collection_product_ids_too_long_response(root_field: &str, len: usize) -> Response {
    ok_json(json!({
        "errors": [max_input_size_exceeded_error(
            vec![root_field.to_string(), "productIds".to_string()],
            len,
            250,
            Some(json!([{ "line": 2, "column": 3 }])),
        )]
    }))
}

fn collection_update_missing_id_response(field: Option<&RootFieldSelection>) -> Response {
    let response_key = field
        .map(|field| field.response_key.clone())
        .unwrap_or_else(|| "collectionUpdate".to_string());
    let location = field
        .map(|field| field.location)
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    ok_json(json!({
        "errors": [{
            "message": "id must be specified on collectionUpdate",
            "locations": [{"line": location.line, "column": location.column}],
            "extensions": {"code": "BAD_REQUEST"},
            "path": [response_key.clone()]
        }],
        "data": {
            response_key: Value::Null
        }
    }))
}

fn collection_invalid_sort_order_response(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
    sort_order: &str,
) -> Response {
    let expected_sort_orders = collection_sort_orders_message();
    let location = variable_definition_info(query, "input")
        .map(|definition| definition.location)
        .or_else(|| parsed_document(query, &BTreeMap::new()).map(|document| document.location))
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    ok_json(json!({
        "errors": [invalid_variable_error_envelope(
            format!("Variable $input of type CollectionInput! was provided invalid value for sortOrder (Expected \"{sort_order}\" to be one of: {expected_sort_orders})"),
            location,
            resolved_value_json(&ResolvedValue::Object(input.clone())),
            json!([{
                "path": ["sortOrder"],
                "explanation": format!("Expected \"{sort_order}\" to be one of: {expected_sort_orders}")
            }]),
        )]
    }))
}

fn collection_sort_orders_message() -> String {
    COLLECTION_SORT_ORDERS.join(", ")
}

fn collection_user_error<const N: usize>(field: [&str; N], message: &str) -> Value {
    user_error_omit_code(field, message, None)
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
