use super::*;

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

pub(in crate::proxy) fn collection_json(collection: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "products" => {
            let sort_key = match selection.arguments.get("sortKey") {
                Some(ResolvedValue::String(value)) => Some(value.as_str()),
                _ => None,
            };
            let connection_name = match sort_key {
                Some("COLLECTION_DEFAULT") => "defaultProducts",
                Some("MANUAL") => "manualProducts",
                _ => "products",
            };
            // A default/best-selling read (anything but an explicit MANUAL sortKey)
            // honors the collection's configured `sortOrder`. With no sales data a
            // BEST_SELLING collection surfaces its members by recency — newest product
            // first — whereas a MANUAL collection keeps its stored position order.
            let honors_collection_sort_order = !matches!(sort_key, Some("MANUAL"));
            let sort_order = collection.get("sortOrder").and_then(Value::as_str);
            Some(
                collection
                    .get(connection_name)
                    .map(|connection| {
                        let connection = if honors_collection_sort_order
                            && collection_sort_order_is_recency(sort_order)
                        {
                            collection_products_by_recency(connection)
                        } else {
                            connection.clone()
                        };
                        selected_json(&connection, &selection.selection)
                    })
                    .unwrap_or_else(|| selected_empty_connection_json(&selection.selection)),
            )
        }
        "hasProduct" => {
            let product_id = resolved_string_field(&selection.arguments, "id").unwrap_or_default();
            let has_product = collection
                .get("products")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|product| {
                    product.get("id").and_then(Value::as_str) == Some(product_id.as_str())
                });
            Some(json!(has_product))
        }
        "productsCount" => Some(
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
                    product_count_json(count, &selection.selection)
                }),
        ),
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

    pub(in crate::proxy) fn collection_membership_downstream_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "collection" => self.collection_membership_value(field),
                "product" => self.product_by_id_field(field),
                "job" => self.collection_job_read(field),
                _ => return None,
            })
        })
    }

    // Serve a top-level `collections(query:, sortKey:)` read entirely from the
    // seeded `collection_catalog` snapshots. Every selection in this operation is
    // a `collections` field distinguished only by its alias (response key) — the
    // unaliased catalog root plus per-filter aliases (title wildcard, custom/smart
    // type, updated sort, product membership, empty) — so the resolver keys each
    // field by its response key, looks up the matching recorded connection, and
    // projects the requested selection over it (truncating to `first`). This
    // reproduces the recorded opaque cursors/pageInfo verbatim; an alias with no
    // seeded snapshot degrades to an empty connection rather than fabricating one.
    pub(in crate::proxy) fn collections_catalog_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            if field.name != "collections" {
                return None;
            }
            let value = match self
                .store
                .staged
                .collection_catalog
                .get(&field.response_key)
            {
                Some(connection) => {
                    project_seeded_connection(connection, &field.arguments, &field.selection)
                }
                None => selected_empty_connection_json(&field.selection),
            };
            Some(value)
        })
    }

    /// True once a scenario has seeded (or staged) publications, switching the
    /// `publication`/`channel`/`channels`/`publicationsCount`/
    /// `publishedProductsCount` roots from upstream passthrough to local replay.
    pub(in crate::proxy) fn publication_engine_active(&self) -> bool {
        !self.store.staged.publications.is_empty()
    }

    /// Every Shopify store has a default "Online Store" publication
    /// (`Publication/1` / `Channel/1`) that the proxy already treats as the
    /// reserved, un-deletable default (`next_publication_id`,
    /// `is_default_publication`, `publicationDelete` guard). Materialize it into
    /// local publication state when the engine activates so `channels` /
    /// `channel` / `publicationsCount` reflect it without a `/__meta/seed`
    /// precondition — the production proxy was never told the Online Store
    /// exists; it always does.
    pub(in crate::proxy) fn ensure_default_publication(&mut self) {
        let id = "gid://shopify/Publication/1";
        if !self.store.staged.publications.contains_key(id) {
            self.store.staged.publications.insert(
                id.to_string(),
                publication_record_json(id, "Online Store", false),
            );
        }
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
            if id.starts_with("gid://shopify/Product/") {
                self.store.stage_observed_product_json(node);
            } else if id.starts_with("gid://shopify/Collection/") {
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
                "publicationsCount" => publication_count_json(self.store.staged.publications.len()),
                "publishedProductsCount" => {
                    let publication_id = resolved_string_field(&field.arguments, "publicationId");
                    publication_count_json(
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
        let mut record = record.clone();
        let product_count = self.publication_resource_count(Some(&id), "Product");
        let collection_count = self.publication_resource_count(Some(&id), "Collection");
        let product_nodes: Vec<Value> = self
            .publication_resource_ids(Some(&id), "Product")
            .into_iter()
            .map(|resource_id| {
                let title = self
                    .product_record_by_id(&resource_id)
                    .map(|product| product.title.clone())
                    .unwrap_or_default();
                json!({ "id": resource_id, "title": title })
            })
            .collect();
        record["products"] = json!({ "nodes": product_nodes });
        record["publishedProductsCount"] = publication_count_json(product_count);
        record["collectionsCount"] = publication_count_json(collection_count);
        if let Some(channel) = record.get_mut("channel") {
            channel["productsCount"] = publication_count_json(product_count);
        }
        selected_json(&record, &field.selection)
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
            channel["productsCount"] = publication_count_json(product_count);
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
        let mut out = serde_json::Map::new();
        for sel in selection {
            if let Some(type_condition) = sel.type_condition.as_deref() {
                if type_condition != resource_type && type_condition != "Node" {
                    continue;
                }
            }
            let value = match sel.name.as_str() {
                "id" => json!(resource_id),
                "__typename" => json!(resource_type),
                "publishedOnPublication" => {
                    let publication_id = resolved_string_field(&sel.arguments, "publicationId");
                    json!(publication_id.map(|id| pubs.contains(&id)).unwrap_or(false))
                }
                "publishedOnCurrentPublication" => json!(false),
                "resourcePublicationsCount" | "publicationCount" | "availablePublicationsCount" => {
                    publication_count_json(
                        self.publishable_live_publication_count(resource_id, &pubs),
                    )
                }
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
        if resource_id.starts_with("gid://shopify/Product/") {
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
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Unable to parse publishable mutation");
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
            if let Some(response) = publishable_empty_string_publication_error(root_field, field) {
                early_response = Some(response);
                return None;
            }
            let resource_id = resolved_string_field(&field.arguments, "id")?;
            let user_errors =
                publishable_publication_input_errors(field.arguments.get("input"), to_current);
            if user_errors.is_empty() {
                // Discover the resource's pre-existing publication membership
                // (e.g. the default Online Store) by reading upstream before
                // applying this publish, so counts reflect the real baseline.
                self.hydrate_publishable_resource(&resource_id, request);
                let publication_ids = publishable_input_publication_ids(&field.arguments);
                let set = self
                    .store
                    .staged
                    .resource_publications
                    .entry(resource_id.clone())
                    .or_default();
                for publication_id in publication_ids {
                    if publish {
                        set.insert(publication_id);
                    } else {
                        set.remove(&publication_id);
                    }
                }
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
            let publishable = self.publishable_resource_value(&resource_id, &publishable_selection);
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
            .map(|collection| collection_json(collection, &field.selection))
            .unwrap_or(Value::Null)
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

    fn stage_collection_from_observed_json(&mut self, collection: &Value) {
        let products = collection
            .get("products")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(product_state_from_json)
            .collect::<Vec<_>>();
        self.store
            .stage_collection_membership(collection.clone(), products);
    }

    pub(in crate::proxy) fn observe_nodes_response(&mut self, response: &Response) {
        let nodes = response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        for node in &nodes {
            let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.starts_with("gid://shopify/Product/") {
                self.store.stage_observed_product_json(node);
                if let Some(product_id) = node.get("id").and_then(Value::as_str) {
                    for variant in node
                        .get("variants")
                        .and_then(|connection| connection.get("nodes"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let mut variant_value = variant.clone();
                        if let Some(object) = variant_value.as_object_mut() {
                            object.insert("productId".to_string(), json!(product_id));
                        }
                        if let Some(mut variant) =
                            product_variant_state_from_observed_json(&variant_value)
                        {
                            variant.product_id = product_id.to_string();
                            self.store.stage_product_variant(variant);
                        }
                    }
                }
            } else if id.starts_with("gid://shopify/Collection/") {
                self.stage_collection_from_observed_json(node);
            } else if id.starts_with("gid://shopify/ProductVariant/") {
                if let Some(variant) = product_variant_state_from_observed_json(node) {
                    self.store.stage_product_variant(variant);
                }
                if let Some(product) = node.get("product").and_then(product_state_from_json) {
                    self.store.stage_observed_product(product);
                }
            } else if id.starts_with("gid://shopify/InventoryItem/") {
                self.observe_inventory_item_node(node);
            } else if id.starts_with("gid://shopify/InventoryLevel/") {
                self.observe_inventory_level_node(node);
            }
        }
        for node in nodes {
            let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.starts_with("gid://shopify/Collection/") {
                self.stage_collection_from_observed_json(&node);
            }
        }
    }

    pub(in crate::proxy) fn collection_mutation(
        &mut self,
        root_field: &str,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "collectionCreate" => self.collection_create(query, variables),
            "collectionUpdate" => self.collection_update(query, variables),
            "collectionDelete" => self.collection_delete(query, variables),
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

    fn collection_payload_root_field(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<RootFieldSelection> {
        primary_root_field(query, variables).or_else(|| primary_root_field(query, &BTreeMap::new()))
    }

    fn collection_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let input = collection_input(query, variables).unwrap_or_default();
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

        let title = resolved_string_field(&input, "title").unwrap_or_default();
        let id = self.next_proxy_synthetic_gid("Collection");
        let handle = self.collection_unique_handle(
            resolved_string_field(&input, "handle").as_deref(),
            &title,
            None,
        );
        let initial_product_ids = resolved_string_list_field_unsorted(&input, "products");
        self.hydrate_missing_collection_baseline("", &initial_product_ids);
        let mut collection = collection_from_input(&input, &id, &title, &handle, None);
        let products = initial_product_ids
            .into_iter()
            .filter_map(|id| self.store.product_by_id(&id).cloned())
            .collect::<Vec<_>>();
        apply_collection_products(&mut collection, &products);
        let mut payload_collection = collection.clone();
        apply_collection_create_payload_products_count(&mut payload_collection);
        self.store.stage_collection(collection.clone());
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
        let input = collection_input(query, variables).unwrap_or_default();
        let Some(id) = resolved_string_field(&input, "id").filter(|id| !id.trim().is_empty())
        else {
            return MutationOutcome::response(collection_update_missing_id_response(
                query, variables,
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

        let mut updated = existing;
        if let Some(object) = updated.as_object_mut() {
            if let Some(title) = resolved_string_field(&input, "title") {
                object.insert("title".to_string(), json!(title));
            }
            if input.contains_key("handle") {
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
        }
        self.store.stage_collection(updated.clone());
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
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let input = collection_input(query, variables).unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let collection_id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let requested_product_ids = resolved_string_list_field_unsorted(&arguments, "productIds");
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        let collection_id = resolved_string_field(&arguments, "id").unwrap_or_default();
        let product_ids = resolved_string_list_field_unsorted(&arguments, "productIds");
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
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
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
        if self
            .store
            .collection_by_id(&collection_id)
            .and_then(|collection| collection.get("sortOrder"))
            .and_then(Value::as_str)
            != Some("MANUAL")
        {
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
                    resolved_i64_field(&move_input, "newPosition")
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
            Some(title) if title.chars().count() > 255 => errors.push(collection_user_error(
                ["title"],
                "Title is too long (maximum is 255 characters)",
            )),
            Some(title) if title_required && title.trim().is_empty() => {
                errors.push(collection_user_error(["title"], "Title can't be blank"))
            }
            None if title_required => {
                errors.push(collection_user_error(["title"], "Title can't be blank"))
            }
            _ => {}
        }
        if let Some(handle) = resolved_string_field(input, "handle") {
            if handle.chars().count() > 255 {
                errors.push(collection_user_error(
                    ["handle"],
                    "Handle is too long (maximum is 255 characters)",
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
        if collection_is_smart(collection) {
            let message = if root_field == "collectionRemoveProducts" {
                "Can't manually remove products from a smart collection"
            } else {
                "Can't manually add products to a smart collection"
            };
            return Some(self.collection_payload_response(
                query,
                variables,
                root_field,
                None,
                job_payload.then_some(&Value::Null),
                vec![collection_user_error(["id"], message)],
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
        let (response_key, payload_selection) = self
            .collection_payload_root_field(query, variables)
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| (root_field.to_string(), Vec::new()));
        let collection_selection =
            selected_child_selection(&payload_selection, "collection").unwrap_or_default();
        let job_selection = selected_child_selection(&payload_selection, "job").unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "collection" => Some(collection.map(|collection| collection_json(collection, &collection_selection)).unwrap_or(Value::Null)),
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
        let (response_key, payload_selection) = self
            .collection_payload_root_field(query, variables)
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| ("collectionDelete".to_string(), Vec::new()));
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

fn collection_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let mut arguments = root_field_arguments(query, variables)?;
    match arguments.remove("input") {
        Some(ResolvedValue::Object(input)) => Some(input),
        _ => None,
    }
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
            "sortOrder": "BEST_SELLING",
            "ruleSet": null,
            "products": connection_json(Vec::<Value>::new()),
            "defaultProducts": connection_json(Vec::<Value>::new()),
            "manualProducts": connection_json(Vec::<Value>::new()),
            "productsCount": {"count": 0, "precision": "EXACT"}
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
            resolved_object_field(input, "ruleSet")
                .map(collection_rule_set_json)
                .unwrap_or(Value::Null),
        );
        if let Some(description) = resolved_string_field(input, "descriptionHtml") {
            object.insert("descriptionHtml".to_string(), json!(description));
        }
        if let Some(template_suffix) = resolved_string_field(input, "templateSuffix") {
            object.insert("templateSuffix".to_string(), json!(template_suffix));
        }
    }
    collection
}

/// Collection sort orders whose default product ordering is by recency (newest
/// member first). Shopify's default `BEST_SELLING` falls back to this when there is
/// no sales data, and `CREATED_DESC` is recency by definition.
fn collection_sort_order_is_recency(sort_order: Option<&str>) -> bool {
    matches!(sort_order, Some("BEST_SELLING") | Some("CREATED_DESC"))
}

/// Reorder a collection `products` connection by member recency (highest numeric
/// gid tail first), keeping `nodes` and any `edges` consistent. The stored
/// connection preserves membership (insertion) order; this is applied only at
/// render time for recency-sorted collections.
fn collection_products_by_recency(connection: &Value) -> Value {
    fn recency(node: &Value) -> i64 {
        node.get("id")
            .and_then(Value::as_str)
            .map(resource_id_tail)
            .and_then(|tail| tail.parse::<i64>().ok())
            .unwrap_or(0)
    }
    let mut connection = connection.clone();
    if let Some(nodes) = connection.get_mut("nodes").and_then(Value::as_array_mut) {
        nodes.sort_by_key(|n| std::cmp::Reverse(recency(n)));
    }
    if let Some(edges) = connection.get_mut("edges").and_then(Value::as_array_mut) {
        edges.sort_by(|a, b| {
            let a_node = a.get("node").unwrap_or(a);
            let b_node = b.get("node").unwrap_or(b);
            recency(b_node).cmp(&recency(a_node))
        });
    }
    connection
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
        object.insert(
            "productsCount".to_string(),
            json!({"count": products.len(), "precision": "EXACT"}),
        );
    }
}

fn apply_collection_create_payload_products_count(collection: &mut Value) {
    if let Some(object) = collection.as_object_mut() {
        object.insert(
            "productsCount".to_string(),
            json!({"count": 0, "precision": "EXACT"}),
        );
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

fn collection_is_smart(collection: &Value) -> bool {
    collection.get("ruleSet").is_some_and(|rule_set| {
        !rule_set.is_null()
            && rule_set
                .get("rules")
                .and_then(Value::as_array)
                .is_some_and(|rules| !rules.is_empty())
    })
}

fn collection_product_ids_too_long_response(root_field: &str, len: usize) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("The input array size of {len} is greater than the maximum allowed of 250."),
            "locations": [{"line": 2, "column": 3}],
            "path": [root_field, "productIds"],
            "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
        }]
    }))
}

fn collection_update_missing_id_response(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Response {
    let field = primary_root_field(query, variables)
        .or_else(|| primary_root_field(query, &BTreeMap::new()));
    let response_key = field
        .as_ref()
        .map(|field| field.response_key.clone())
        .unwrap_or_else(|| "collectionUpdate".to_string());
    let location = field
        .as_ref()
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
        "errors": [{
            "message": format!("Variable $input of type CollectionInput! was provided invalid value for sortOrder (Expected \"{sort_order}\" to be one of: {expected_sort_orders})"),
            "locations": [{"line": location.line, "column": location.column}],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": ["sortOrder"],
                    "explanation": format!("Expected \"{sort_order}\" to be one of: {expected_sort_orders}")
                }]
            }
        }]
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
