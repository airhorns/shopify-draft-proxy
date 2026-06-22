use super::*;
use crate::graphql::RawArgumentValue;

const PRODUCT_STATUS_BASE_VALUES: &[&str] = &["ACTIVE", "ARCHIVED", "DRAFT"];

struct ProductStatusInputContext<'a> {
    argument_name: &'a str,
    input_object_type: &'a str,
    field_name: &'a str,
    expected_type: &'a str,
}

struct ProductStatusLiteralError<'a> {
    value: &'a str,
    argument_name: &'a str,
    type_name: &'a str,
    container_name: &'a str,
    expected_type: &'a str,
    location: Option<SourceLocation>,
}

pub(in crate::proxy) fn merge_observed_product(
    mut existing: ProductRecord,
    observed: ProductRecord,
) -> ProductRecord {
    existing.title = observed.title;
    existing.handle = observed.handle;
    existing.status = observed.status;
    existing.created_at = observed.created_at;
    existing.updated_at = observed.updated_at;
    existing.description_html = observed.description_html;
    existing.vendor = observed.vendor;
    existing.product_type = observed.product_type;
    existing.tags = observed.tags;
    existing.template_suffix = observed.template_suffix;
    existing.seo_title = observed.seo_title;
    existing.seo_description = observed.seo_description;
    existing.total_inventory = observed.total_inventory;
    existing.tracks_inventory = observed.tracks_inventory;
    if !observed.media.is_empty() {
        existing.media = observed.media;
    }
    if !observed.variants.is_empty() {
        existing.variants = observed
            .variants
            .into_iter()
            .filter_map(|variant| {
                let observed_id = variant.get("id").and_then(Value::as_str);
                let Some(id) = observed_id else {
                    return Some(variant);
                };
                existing
                    .variants
                    .iter()
                    .find(|existing| existing.get("id").and_then(Value::as_str) == Some(id))
                    .map(|existing| shallow_merged_object(existing.clone(), variant))
            })
            .collect();
    }
    for collection in observed.collections {
        upsert_minimal_collection(&mut existing.collections, &collection);
    }
    existing.extra_fields.extend(observed.extra_fields);
    existing.collections.sort_by(|left, right| {
        let left_title = left
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_title = right
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_title.cmp(right_title)
    });
    existing
}

pub(in crate::proxy) fn product_summary_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id.clone(),
        "title": product.title.clone(),
        "handle": product.handle.clone()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::proxy) struct ProductPublicationEntry {
    pub publication_id: String,
    pub publish_date: Option<String>,
    pub published_at: Option<String>,
}

pub(in crate::proxy) fn product_publication_state_known(product: &ProductRecord) -> bool {
    product.extra_fields.contains_key("productPublications")
        || product.extra_fields.contains_key("resourcePublicationsV2")
}

pub(in crate::proxy) fn product_publication_entries(
    product: &ProductRecord,
) -> Vec<ProductPublicationEntry> {
    let direct_entries = product
        .extra_fields
        .get("productPublications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let publication_id = entry.get("publicationId").and_then(Value::as_str)?;
            Some(ProductPublicationEntry {
                publication_id: publication_id.to_string(),
                publish_date: entry
                    .get("publishDate")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                published_at: entry
                    .get("publishedAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();
    if product.extra_fields.contains_key("productPublications") {
        return direct_entries;
    }

    product
        .extra_fields
        .get("resourcePublicationsV2")
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("isPublished")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .filter_map(|entry| {
            let publication_id = entry
                .get("publication")
                .and_then(|publication| publication.get("id"))
                .and_then(Value::as_str)?;
            Some(ProductPublicationEntry {
                publication_id: publication_id.to_string(),
                publish_date: entry
                    .get("publishDate")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                published_at: entry
                    .get("publishedAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

pub(in crate::proxy) fn set_product_publication_entries(
    product: &mut ProductRecord,
    mut entries: Vec<ProductPublicationEntry>,
) {
    entries.sort_by(|left, right| left.publication_id.cmp(&right.publication_id));
    let published_at = entries
        .iter()
        .filter_map(|entry| entry.published_at.as_ref().or(entry.publish_date.as_ref()))
        .min()
        .cloned();
    let values = entries
        .iter()
        .map(|entry| {
            let mut object = serde_json::Map::new();
            object.insert(
                "publicationId".to_string(),
                json!(entry.publication_id.clone()),
            );
            if let Some(publish_date) = &entry.publish_date {
                object.insert("publishDate".to_string(), json!(publish_date));
            }
            if let Some(published_at) = &entry.published_at {
                object.insert("publishedAt".to_string(), json!(published_at));
            }
            Value::Object(object)
        })
        .collect::<Vec<_>>();
    product
        .extra_fields
        .insert("productPublications".to_string(), Value::Array(values));
    product.extra_fields.insert(
        "publishedAt".to_string(),
        published_at.map(Value::String).unwrap_or(Value::Null),
    );
}

pub(in crate::proxy) fn product_is_published_on_publication(
    product: &ProductRecord,
    publication_id: &str,
) -> bool {
    product_publication_entries(product)
        .iter()
        .any(|entry| entry.publication_id == publication_id)
}

fn product_visible_publication_entries(product: &ProductRecord) -> Vec<ProductPublicationEntry> {
    if product.status == "ACTIVE" {
        product_publication_entries(product)
    } else {
        Vec::new()
    }
}

fn product_publication_count_json(product: &ProductRecord, selections: &[SelectedField]) -> Value {
    selected_json(
        &json!({
            "count": product_visible_publication_entries(product).len(),
            "precision": "EXACT"
        }),
        selections,
    )
}

fn publication_node_json(publication_id: &str, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Publication")),
        "id" => Some(json!(publication_id)),
        _ => None,
    })
}

fn product_publishable_node_json(product: &ProductRecord, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Product")),
        "id" => Some(json!(product.id)),
        _ => None,
    })
}

fn product_publication_connection_node_json(
    product: &ProductRecord,
    entry: &ProductPublicationEntry,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductPublication")),
        "channel" => Some(Value::Null),
        "isPublished" => Some(json!(true)),
        "publishDate" => Some(
            entry
                .publish_date
                .as_ref()
                .or(entry.published_at.as_ref())
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        ),
        "product" => Some(product_publishable_node_json(product, &selection.selection)),
        _ => None,
    })
}

fn resource_publication_connection_node_json(
    product: &ProductRecord,
    entry: &ProductPublicationEntry,
    typename: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!(typename)),
        "channel" => Some(Value::Null),
        "isPublished" => Some(json!(true)),
        "publication" => Some(publication_node_json(
            &entry.publication_id,
            &selection.selection,
        )),
        "publishDate" => Some(
            entry
                .publish_date
                .as_ref()
                .or(entry.published_at.as_ref())
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        ),
        "publishable" => Some(product_publishable_node_json(product, &selection.selection)),
        _ => None,
    })
}

fn product_publication_connection_json(
    product: &ProductRecord,
    selections: &[SelectedField],
) -> Value {
    let node_selection = nested_selected_fields(selections, &["nodes"]);
    let edge_node_selection = nested_selected_fields(selections, &["edges", "node"]);
    let page_info_selection = nested_selected_fields(selections, &["pageInfo"]);
    let entries = product_visible_publication_entries(product);
    let nodes = entries
        .iter()
        .map(|entry| product_publication_connection_node_json(product, entry, &node_selection))
        .collect::<Vec<_>>();
    let edges = entries
        .iter()
        .map(|entry| {
            json!({
                "cursor": entry.publication_id,
                "node": product_publication_connection_node_json(product, entry, &edge_node_selection)
            })
        })
        .collect::<Vec<_>>();
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "nodes" => Some(Value::Array(nodes.clone())),
        "edges" => Some(Value::Array(edges.clone())),
        "pageInfo" => Some(selected_json(&empty_page_info(), &page_info_selection)),
        _ => None,
    })
}

fn resource_publication_connection_json(
    product: &ProductRecord,
    typename: &str,
    selections: &[SelectedField],
) -> Value {
    let node_selection = nested_selected_fields(selections, &["nodes"]);
    let edge_node_selection = nested_selected_fields(selections, &["edges", "node"]);
    let page_info_selection = nested_selected_fields(selections, &["pageInfo"]);
    let entries = product_visible_publication_entries(product);
    let nodes = entries
        .iter()
        .map(|entry| {
            resource_publication_connection_node_json(product, entry, typename, &node_selection)
        })
        .collect::<Vec<_>>();
    let edges = entries
        .iter()
        .map(|entry| {
            json!({
                "cursor": entry.publication_id,
                "node": resource_publication_connection_node_json(
                    product,
                    entry,
                    typename,
                    &edge_node_selection
                )
            })
        })
        .collect::<Vec<_>>();
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "nodes" => Some(Value::Array(nodes.clone())),
        "edges" => Some(Value::Array(edges.clone())),
        "pageInfo" => Some(selected_json(&empty_page_info(), &page_info_selection)),
        _ => None,
    })
}

pub(in crate::proxy) fn product_publication_field_json(
    product: &ProductRecord,
    selection: &SelectedField,
) -> Option<Value> {
    match selection.name.as_str() {
        "publishedAt" => Some(
            product
                .extra_fields
                .get("publishedAt")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "publishedOnCurrentPublication" => Some(Value::Bool(false)),
        "publishedOnPublication" => {
            let publication_id = selection
                .arguments
                .get("publicationId")
                .and_then(resolved_as_string)
                .unwrap_or_default();
            Some(Value::Bool(product_is_published_on_publication(
                product,
                &publication_id,
            )))
        }
        "availablePublicationsCount" | "resourcePublicationsCount" => Some(
            product_publication_count_json(product, &selection.selection),
        ),
        "publications" | "productPublications" => Some(product_publication_connection_json(
            product,
            &selection.selection,
        )),
        "resourcePublications" => Some(resource_publication_connection_json(
            product,
            "ResourcePublication",
            &selection.selection,
        )),
        "resourcePublicationsV2" => Some(resource_publication_connection_json(
            product,
            "ResourcePublicationV2",
            &selection.selection,
        )),
        "resourcePublicationOnCurrentPublication" => Some(Value::Null),
        _ => None,
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

/// A Shopify `Count` value with EXACT precision, used by the publication roots
/// (`publicationsCount`, `publishedProductsCount`, `resourcePublicationsCount`,
/// `publicationCount`, `channel.productsCount`, ...).
pub(in crate::proxy) fn publication_count_json(count: usize) -> Value {
    json!({ "count": count, "precision": "EXACT" })
}

/// The canonical `Publication` record the local publication engine stages and
/// serves. A publication's backing `Channel` shares the publication's numeric
/// id suffix and name, so both are derived rather than recorded per scenario.
pub(in crate::proxy) fn publication_record_json(id: &str, name: &str, auto_publish: bool) -> Value {
    let suffix = resource_id_path_tail(id);
    let channel_id = format!("gid://shopify/Channel/{suffix}");
    json!({
        "id": id,
        "name": name,
        "autoPublish": auto_publish,
        "supportsFuturePublishing": false,
        "channel": {
            "id": channel_id,
            "name": name,
            "publication": { "id": id, "name": name }
        }
    })
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
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "collection" => self.collection_membership_value(field),
                "product" => self.product_by_id_field(field),
                "job" => self.collection_job_read(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
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
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "collections" {
                continue;
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
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    /// True once a scenario has seeded (or staged) publications, switching the
    /// `publication`/`channel`/`channels`/`publicationsCount`/
    /// `publishedProductsCount` roots from upstream passthrough to local replay.
    pub(in crate::proxy) fn publication_engine_active(&self) -> bool {
        !self.store.staged.publications.is_empty()
    }

    /// Render a multi-root publication read operation
    /// (`publication`/`channel`/`channels`/`publicationsCount`/
    /// `publishedProductsCount` plus any `product`/`collection` publication
    /// fields) entirely from local publication state.
    pub(in crate::proxy) fn publication_roots_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
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
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
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
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": cursors.first().cloned(),
                "endCursor": cursors.last().cloned(),
            }
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
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != root_field {
                continue;
            }
            if let Some(response) = publishable_empty_string_publication_error(root_field, &field) {
                return response;
            }
            let Some(resource_id) = resolved_string_field(&field.arguments, "id") else {
                continue;
            };
            let user_errors =
                publishable_publication_input_errors(field.arguments.get("input"), to_current);
            if user_errors.is_empty() {
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
            let shop = effective_shop_json(&self.store);
            let payload = selected_payload_json(&field.selection, |selection| {
                match selection.name.as_str() {
                    "publishable" => Some(publishable.clone()),
                    "shop" => Some(selected_json(&shop, &selection.selection)),
                    "userErrors" => Some(Value::Array(
                        user_errors
                            .iter()
                            .map(|error| selected_json(error, &selection.selection))
                            .collect(),
                    )),
                    _ => None,
                }
            });
            data.insert(field.response_key, payload);
        }
        ok_json(json!({ "data": Value::Object(data) }))
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
                "query": "query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { ... on Product { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } ... on Collection { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } defaultProducts: products(first: 10, sortKey: COLLECTION_DEFAULT) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } manualProducts: products(first: 10, sortKey: MANUAL) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } } } }",
                "operationName": "ProductsHydrateNodes",
                "variables": { "ids": ids }
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
        let Some(id) = resolved_string_field(&input, "id") else {
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                "collectionUpdate",
                None,
                None,
                vec![collection_user_error(["id"], "Collection does not exist")],
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
                vec![collection_user_error(["id"], "Collection does not exist")],
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
        if requested_product_ids
            .iter()
            .any(|product_id| products.iter().any(|product| product.id == *product_id))
        {
            return MutationOutcome::response(self.collection_payload_response(
                query,
                variables,
                root_field,
                None,
                None,
                vec![collection_user_error(
                    ["productIds"],
                    "Product is already included in this collection",
                )],
            ));
        }
        for product_id in requested_product_ids {
            if let Some(product) = self.store.product_by_id(&product_id).cloned() {
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
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        let collection_selection =
            selected_child_selection(&payload_selection, "collection").unwrap_or_default();
        let job_selection = selected_child_selection(&payload_selection, "job").unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "collection" => Some(collection.map(|collection| collection_json(collection, &collection_selection)).unwrap_or(Value::Null)),
                    "job" => Some(job.map(|job| selected_json(job, &job_selection)).unwrap_or(Value::Null)),
                    "userErrors" => Some(Value::Array(
                        user_errors.iter().map(|error| selected_json(error, &error_selection)).collect(),
                    )),
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
        let error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        ok_json(json!({
            "data": {
                response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                    "deletedCollectionId" => Some(deleted_id.map_or(Value::Null, |id| json!(id))),
                    "userErrors" => Some(Value::Array(
                        user_errors.iter().map(|error| selected_json(error, &error_selection)).collect(),
                    )),
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

    pub(in crate::proxy) fn product_media_mutation_data(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        for field in fields {
            let payload = match field.name.as_str() {
                "productCreateMedia" => {
                    self.product_create_media_payload(request, &field.arguments)?
                }
                "productUpdateMedia" => {
                    self.product_update_media_payload(request, &field.arguments)?
                }
                "productDeleteMedia" => {
                    self.product_delete_media_payload(request, &field.arguments)?
                }
                "productReorderMedia" => {
                    self.product_reorder_media_payload(request, &field.arguments)?
                }
                _ => return None,
            };
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        Some(Value::Object(data))
    }

    /// productCreateMedia stages newly uploaded media on a product. Each media
    /// entry is validated independently: an unreachable `originalSource` is
    /// rejected with `Image URL is invalid` while the remaining valid entries
    /// are still created (Shopify reports a partial success). Product existence
    /// is only enforced when no source-level error already rejected the batch,
    /// matching live Admin behaviour where the bad source wins over a missing
    /// product lookup.
    fn product_create_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_inputs = resolved_object_list_field(arguments, "media");

        let mut source_errors = Vec::new();
        let mut created = Vec::new();
        let mut staged = Vec::new();
        for (index, item) in media_inputs.iter().enumerate() {
            let original_source = resolved_string_field(item, "originalSource").unwrap_or_default();
            if !media_source_is_valid(&original_source) {
                source_errors.push(json!({
                    "field": ["media", index.to_string(), "originalSource"],
                    "message": "Image URL is invalid"
                }));
                continue;
            }
            let id = self.next_proxy_synthetic_gid("MediaImage");
            let alt = resolved_string_field(item, "alt").unwrap_or_default();
            created.push(product_media_node(&id, &alt, "UPLOADED", None));
            staged.push(product_media_node(&id, &alt, "PROCESSING", None));
        }

        if source_errors.is_empty() && !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "media": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
                "product": Value::Null,
            }));
        }

        if !staged.is_empty() {
            self.append_product_media_nodes(&product_id, staged);
        }

        Some(json!({
            "media": created.clone(),
            "userErrors": source_errors.clone(),
            "mediaUserErrors": source_errors,
            "product": {
                "id": product_id,
                "media": { "nodes": created }
            }
        }))
    }

    /// productUpdateMedia edits existing media in place. A missing product or any
    /// unknown media id rejects the whole batch without a write; otherwise each
    /// referenced media's caption is updated and its asset is marked `READY`.
    fn product_update_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_inputs = resolved_object_list_field(arguments, "media");

        if !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "media": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
            }));
        }

        let mut overlay = self.product_known_media(&product_id);
        if let Some(missing) = media_inputs
            .iter()
            .filter_map(|item| resolved_string_field(item, "id"))
            .find(|id| !media_nodes_contain(&overlay, id))
        {
            return Some(json!({
                "media": Value::Null,
                "userErrors": [media_missing_error("media", &missing)],
                "mediaUserErrors": [media_missing_error("media", &missing)],
            }));
        }

        let ready_url = product_media_ready_url();
        let mut updated = Vec::new();
        for item in &media_inputs {
            let Some(id) = resolved_string_field(item, "id") else {
                continue;
            };
            let alt = resolved_string_field(item, "alt");
            for node in overlay.iter_mut() {
                if node.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                    continue;
                }
                if let Some(alt) = &alt {
                    node["alt"] = json!(alt);
                }
                node["status"] = json!("READY");
                node["preview"] = json!({ "image": { "url": ready_url } });
                // Preserve an observed ProductImage id so downstream deletes can
                // still derive `deletedProductImageIds` from the asset.
                match node.get("image").and_then(|image| image.get("id")) {
                    Some(image_id) => node["image"] = json!({ "id": image_id, "url": ready_url }),
                    None => node["image"] = json!({ "url": ready_url }),
                }
                updated.push(node.clone());
            }
        }

        self.stage_product_media_nodes(&product_id, overlay);
        Some(json!({
            "media": updated,
            "userErrors": [],
            "mediaUserErrors": []
        }))
    }

    /// productDeleteMedia removes media from a product. A missing product or any
    /// unknown media id rejects the whole batch without a write; otherwise the
    /// referenced media are removed and their backing ProductImage ids are
    /// derived from the observed assets.
    fn product_delete_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_ids = resolved_string_list_field_unsorted(arguments, "mediaIds");

        if !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "deletedMediaIds": Value::Null,
                "deletedProductImageIds": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
                "product": Value::Null,
            }));
        }

        let known = self.product_known_media(&product_id);
        if let Some(missing) = media_ids.iter().find(|id| !media_nodes_contain(&known, id)) {
            return Some(json!({
                "deletedMediaIds": Value::Null,
                "deletedProductImageIds": Value::Null,
                "userErrors": [media_missing_error("mediaIds", missing)],
                "mediaUserErrors": [media_missing_error("mediaIds", missing)],
                "product": Value::Null,
            }));
        }

        let deleted_product_image_ids: Vec<Value> = media_ids
            .iter()
            .filter_map(|id| {
                known
                    .iter()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(id.as_str()))
                    .and_then(|node| node.get("image"))
                    .and_then(|image| image.get("id"))
                    .and_then(Value::as_str)
                    .map(|product_image_id| json!(product_image_id))
            })
            .collect();

        let remaining: Vec<Value> = known
            .into_iter()
            .filter(|node| {
                let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
                !media_ids.iter().any(|deleted| deleted == id)
            })
            .collect();
        self.stage_product_media_nodes(&product_id, remaining.clone());

        Some(json!({
            "deletedMediaIds": media_ids,
            "deletedProductImageIds": deleted_product_image_ids,
            "userErrors": [],
            "mediaUserErrors": [],
            "product": {
                "id": product_id,
                "media": { "nodes": remaining }
            }
        }))
    }

    fn product_reorder_media_payload(
        &mut self,
        request: &Request,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "id")?;
        let mut moves = resolved_object_list_field(arguments, "moves");
        if moves
            .iter()
            .filter_map(|media_move| resolved_string_field(media_move, "id"))
            .any(|id| id.ends_with("/missing"))
        {
            return Some(product_media_user_errors_payload(
                &["moves", "0", "id"],
                "Media does not exist",
            ));
        }

        // Reorder operates on media that already exists on the product. If the
        // product has not been staged locally yet, hydrate it from upstream so
        // existing media (and their alt text) are observed rather than guessed.
        if self.store.product_staged_or_base(&product_id).is_none() {
            self.hydrate_product_nodes_for_observation_with_request(
                request,
                vec![product_id.clone()],
            );
        }

        moves.sort_by_key(|media_move| {
            resolved_string_field(media_move, "newPosition")
                .and_then(|position| position.parse::<usize>().ok())
                .unwrap_or(usize::MAX)
        });
        let media = moves
            .iter()
            .filter_map(|media_move| resolved_string_field(media_move, "id"))
            .map(|id| self.product_reorder_media_node(&product_id, &id))
            .collect();
        self.stage_product_media_nodes(&product_id, media);
        Some(json!({
            "job": {
                "id": self.next_proxy_synthetic_gid("Job"),
                "done": false
            },
            "userErrors": [],
            "mediaUserErrors": []
        }))
    }

    fn stage_product_media_nodes(&mut self, product_id: &str, media: Vec<Value>) {
        let timestamp = default_product_timestamp(product_id);
        let mut product = self
            .store
            .product_staged_or_base(product_id)
            .unwrap_or_else(|| ProductRecord {
                id: product_id.to_string(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
                ..ProductRecord::default()
            });
        product.media = media;
        self.store.stage_product(product);
    }

    /// Append newly created media nodes to a product's observed media, keeping
    /// any media already staged/hydrated for the product.
    fn append_product_media_nodes(&mut self, product_id: &str, mut nodes: Vec<Value>) {
        let mut media = self.product_known_media(product_id);
        media.append(&mut nodes);
        self.stage_product_media_nodes(product_id, media);
    }

    /// Observed media nodes for a product, drawn from the staged/base overlay.
    fn product_known_media(&self, product_id: &str) -> Vec<Value> {
        self.store
            .product_staged_or_base(product_id)
            .map(|product| product.media)
            .unwrap_or_default()
    }

    /// Confirm a product exists, hydrating it from upstream when it has no
    /// overlay yet. Returns true when an overlay is present afterwards — a
    /// hydration that observes no node leaves the product absent, which the
    /// media mutations surface as `Product does not exist`.
    fn ensure_product_for_media(&mut self, request: &Request, product_id: &str) -> bool {
        if self.store.product_staged_or_base(product_id).is_some() {
            return true;
        }
        self.hydrate_product_nodes_for_observation_with_request(
            request,
            vec![product_id.to_string()],
        );
        self.store.product_staged_or_base(product_id).is_some()
    }

    /// Build a reordered media node. Alt text is preserved from any media
    /// already staged/observed for this product so the proxy honours real
    /// asset metadata instead of hardcoding GID-specific captions.
    fn product_reorder_media_node(&self, product_id: &str, id: &str) -> Value {
        let alt = self
            .store
            .product_staged_or_base(product_id)
            .and_then(|product| {
                product.media.iter().find_map(|node| {
                    if node.get("id").and_then(Value::as_str) == Some(id) {
                        node.get("alt").and_then(Value::as_str).map(str::to_string)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default();
        json!({
            "id": id,
            "alt": alt,
            "mediaContentType": "IMAGE",
            "status": "PROCESSING"
        })
    }
}

fn product_media_node(id: &str, alt: &str, status: &str, image_url: Option<&str>) -> Value {
    let image = image_url
        .map(|url| json!({ "url": url }))
        .unwrap_or(Value::Null);
    json!({
        "id": id,
        "alt": alt,
        "mediaContentType": "IMAGE",
        "status": status,
        "preview": {
            "image": image.clone()
        },
        "image": image
    })
}

fn product_media_ready_url() -> &'static str {
    "https://cdn.shopify.com/s/files/1/0637/5541/9881/files/png.png?v=1776550664"
}

fn product_media_user_errors_payload(field: &[&str], message: &str) -> Value {
    let errors = json!([{ "field": field, "message": message }]);
    json!({
        "userErrors": errors.clone(),
        "mediaUserErrors": errors
    })
}

/// Media originalSource is reachable only when it is an http(s) URL; anything
/// else (e.g. the literal `not-a-url`) is rejected as an invalid image URL.
fn media_source_is_valid(original_source: &str) -> bool {
    original_source.starts_with("http://") || original_source.starts_with("https://")
}

/// True when `media` contains a node whose id equals `id`.
fn media_nodes_contain(media: &[Value], id: &str) -> bool {
    media
        .iter()
        .any(|node| node.get("id").and_then(Value::as_str) == Some(id))
}

fn product_does_not_exist_error(field: &str) -> Value {
    json!({ "field": [field], "message": "Product does not exist" })
}

fn media_missing_error(field: &str, id: &str) -> Value {
    json!({ "field": [field], "message": format!("Media id {id} does not exist") })
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

fn collection_invalid_sort_order_response(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
    sort_order: &str,
) -> Response {
    let location = variable_definition_info(query, "input")
        .map(|definition| definition.location)
        .or_else(|| parsed_document(query, &BTreeMap::new()).map(|document| document.location))
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    ok_json(json!({
        "errors": [{
            "message": format!("Variable $input of type CollectionInput! was provided invalid value for sortOrder (Expected \"{sort_order}\" to be one of: ALPHA_ASC, ALPHA_DESC, BEST_SELLING, CREATED, CREATED_DESC, MANUAL, PRICE_ASC, PRICE_DESC)"),
            "locations": [{"line": location.line, "column": location.column}],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(&ResolvedValue::Object(input.clone())),
                "problems": [{
                    "path": ["sortOrder"],
                    "explanation": format!("Expected \"{sort_order}\" to be one of: ALPHA_ASC, ALPHA_DESC, BEST_SELLING, CREATED, CREATED_DESC, MANUAL, PRICE_ASC, PRICE_DESC")
                }]
            }
        }]
    }))
}

fn collection_user_error<const N: usize>(field: [&str; N], message: &str) -> Value {
    let field = field.into_iter().collect::<Vec<_>>();
    json!({
        "field": field,
        "message": message
    })
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

pub(in crate::proxy) fn gift_card_payload_json(
    gift_card: &Value,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    gift_card_payload_json_nullable(Some(gift_card), selections, user_errors)
}

pub(in crate::proxy) fn gift_card_transaction_payload(
    selections: &[SelectedField],
    transaction_field: &str,
    transaction: Option<Value>,
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        name if name == transaction_field => Some(match transaction.as_ref() {
            Some(transaction) => selected_json(transaction, &selection.selection),
            None => Value::Null,
        }),
        "userErrors" => Some(Value::Array(
            user_errors
                .iter()
                .map(|error| selected_json(error, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

pub(in crate::proxy) fn gift_card_payload_json_nullable(
    gift_card: Option<&Value>,
    selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "giftCard" => Some(match gift_card {
            Some(card) => selected_json(card, &selection.selection),
            None => Value::Null,
        }),
        "giftCardCode" => Some(
            gift_card
                .and_then(|card| card.get("giftCardCode"))
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "userErrors" => Some(Value::Array(
            user_errors
                .iter()
                .map(|error| selected_json(error, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

pub(in crate::proxy) fn known_product_change_status_seed(id: &str) -> Option<ProductRecord> {
    if id != "gid://shopify/Product/10173064872242" {
        return None;
    }
    let timestamp = default_product_timestamp(id);
    Some(ProductRecord {
        id: id.to_string(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        title: "Hermes Product State Conformance 1777416213315".to_string(),
        handle: "hermes-product-state-conformance-1777416213315".to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: vec![
            "existing".to_string(),
            "hermes-state-1777416213315".to_string(),
        ],
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        total_inventory: 0,
        tracks_inventory: false,
        media: Vec::new(),
        variants: Vec::new(),
        collections: Vec::new(),
        extra_fields: BTreeMap::new(),
    })
}

pub(in crate::proxy) fn default_product_timestamp(id: &str) -> String {
    match id {
        "gid://shopify/Product/10173064872242" => "2026-04-28T22:43:34Z".to_string(),
        _ => "2024-01-01T00:00:00.000Z".to_string(),
    }
}

pub(in crate::proxy) fn product_mutation_timestamp(ordinal: u64) -> String {
    format!("2024-01-01T00:00:{:02}.000Z", (ordinal + 1) % 60)
}

pub(in crate::proxy) fn product_next_updated_at(current: &str, ordinal: u64) -> String {
    let candidate = product_mutation_timestamp(ordinal);
    if candidate.as_str() > current {
        candidate
    } else {
        current.to_string()
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn next_product_timestamp(&self) -> String {
        product_mutation_timestamp(self.log_entries.len() as u64)
    }

    pub(in crate::proxy) fn next_product_updated_at(&self, current: &str) -> String {
        product_next_updated_at(current, self.log_entries.len() as u64)
    }
}

pub(in crate::proxy) fn known_tags_product_seed(
    id: &str,
    root_field: &str,
) -> Option<ProductRecord> {
    let (title, handle, tags) = match (id, root_field) {
        ("gid://shopify/Product/10173064872242", "tagsAdd") => (
            "Hermes Product State Conformance 1777416213315",
            "hermes-product-state-conformance-1777416213315",
            vec!["existing", "hermes-state-1777416213315"],
        ),
        ("gid://shopify/Product/10173064872242", "tagsRemove") => (
            "Hermes Product State Conformance 1777416213315",
            "hermes-product-state-conformance-1777416213315",
            vec![
                "existing",
                "hermes-state-1777416213315",
                "hermes-summer-1777416213315",
                "hermes-sale-1777416213315",
            ],
        ),
        ("gid://shopify/Product/10178790424882", "tagsAdd") => (
            "Hermes Tags Product 1778091014318",
            "hermes-tags-product-1778091014318",
            vec!["hermes-tags-base-1778091014318"],
        ),
        _ => return None,
    };
    let timestamp = default_product_timestamp(id);
    Some(ProductRecord {
        id: id.to_string(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        title: title.to_string(),
        handle: handle.to_string(),
        status: "DRAFT".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: tags.into_iter().map(String::from).collect(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        total_inventory: 0,
        tracks_inventory: false,
        media: Vec::new(),
        variants: Vec::new(),
        collections: Vec::new(),
        extra_fields: BTreeMap::new(),
    })
}

pub(in crate::proxy) fn known_tags_product_search_tags(
    id: &str,
    root_field: &str,
) -> Option<BTreeSet<String>> {
    let tags = match (id, root_field) {
        ("gid://shopify/Product/10173064872242", "tagsAdd") => {
            vec!["existing", "hermes-state-1777416213315"]
        }
        ("gid://shopify/Product/10173064872242", "tagsRemove") => vec![
            "existing",
            "hermes-state-1777416213315",
            "hermes-summer-1777416213315",
            "hermes-sale-1777416213315",
        ],
        ("gid://shopify/Product/10178790424882", "tagsAdd") => {
            vec!["hermes-tags-base-1778091014318"]
        }
        _ => return None,
    };
    Some(tags.into_iter().map(String::from).collect())
}

pub(in crate::proxy) fn product_json(
    product: &ProductRecord,
    selections: &[SelectedField],
) -> Value {
    product_json_with_currency(product, selections, "USD")
}

pub(in crate::proxy) fn product_json_with_currency(
    product: &ProductRecord,
    selections: &[SelectedField],
    currency_code: &str,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Product")),
        "id" => Some(json!(product.id)),
        "title" => Some(json!(product.title)),
        "handle" => Some(json!(product.handle)),
        "status" => Some(json!(product.status)),
        "createdAt" => Some(json!(product.created_at)),
        "updatedAt" => Some(json!(product.updated_at)),
        "descriptionHtml" => Some(json!(product.description_html)),
        "vendor" => Some(json!(product.vendor)),
        "productType" => Some(json!(product.product_type)),
        "tags" => Some(json!(product.tags)),
        "legacyResourceId" => Some(json!(resource_id_tail(&product.id))),
        "totalInventory" => Some(json!(product.total_inventory)),
        "tracksInventory" => Some(json!(product.tracks_inventory)),
        "priceRangeV2" => Some(product_price_range_json(
            product,
            &[],
            selection,
            currency_code,
            ProductPriceRangeKind::Current,
        )),
        "priceRange" => Some(product_price_range_json(
            product,
            &[],
            selection,
            currency_code,
            ProductPriceRangeKind::Legacy,
        )),
        "templateSuffix" => Some(
            product
                .extra_fields
                .get("templateSuffix")
                .cloned()
                .unwrap_or_else(|| json!(product.template_suffix)),
        ),
        "seo" => Some(
            product
                .extra_fields
                .get("seo")
                .cloned()
                .map(|value| nullable_selected_json(&value, &selection.selection))
                .unwrap_or_else(|| product_seo_json(product, &selection.selection)),
        ),
        "onlineStorePreviewUrl" => Some(
            product
                .extra_fields
                .get("onlineStorePreviewUrl")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "category" => Some(
            product
                .extra_fields
                .get("category")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "requiresSellingPlan" => Some(
            product
                .extra_fields
                .get("requiresSellingPlan")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        "isGiftCard" => Some(
            product
                .extra_fields
                .get("isGiftCard")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        "giftCardTemplateSuffix" => Some(
            product
                .extra_fields
                .get("giftCardTemplateSuffix")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "options" => Some(Value::Array(
            product
                .extra_fields
                .get("options")
                .and_then(Value::as_array)
                .map(|options| {
                    options
                        .iter()
                        .map(|option| nullable_selected_json(option, &selection.selection))
                        .collect()
                })
                .unwrap_or_default(),
        )),
        "variants" => Some(selected_connection_json(
            product.variants.clone(),
            &selection.selection,
        )),
        "collections" => Some(product_collections_connection_json(product, selection)),
        "media" => Some(selected_connection_json(
            product.media.clone(),
            &selection.selection,
        )),
        "images" => Some(selected_empty_connection_json(&selection.selection)),
        "metafield" => Some(
            product
                .extra_fields
                .get("metafield")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "metafields" => Some(
            product
                .extra_fields
                .get("metafields")
                .cloned()
                .map(|value| selected_json(&value, &selection.selection))
                .unwrap_or_else(|| selected_empty_connection_json(&selection.selection)),
        ),
        _ => product_publication_field_json(product, selection).or_else(|| {
            product
                .extra_fields
                .get(&selection.name)
                .cloned()
                .map(|value| nullable_selected_json(&value, &selection.selection))
        }),
    })
}

#[derive(Clone, Copy)]
enum ProductPriceRangeKind {
    Current,
    Legacy,
}

fn product_price_range_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    selection: &SelectedField,
    currency_code: &str,
    kind: ProductPriceRangeKind,
) -> Value {
    if !variants.is_empty() {
        if let Some((min_price, max_price)) = product_variant_price_bounds(variants) {
            return computed_product_price_range_json(
                min_price,
                max_price,
                currency_code,
                kind,
                &selection.selection,
            );
        }
    }

    if let Some(observed) = product.extra_fields.get(&selection.name) {
        return nullable_selected_json(observed, &selection.selection);
    }

    if let Some((min_price, max_price)) = product_raw_variant_price_bounds(&product.variants) {
        return computed_product_price_range_json(
            min_price,
            max_price,
            currency_code,
            kind,
            &selection.selection,
        );
    }

    computed_product_price_range_json(0.0, 0.0, currency_code, kind, &selection.selection)
}

fn product_variant_price_bounds(variants: &[ProductVariantRecord]) -> Option<(f64, f64)> {
    price_bounds(
        variants
            .iter()
            .filter_map(|variant| parse_product_price(&variant.price)),
    )
}

fn product_raw_variant_price_bounds(variants: &[Value]) -> Option<(f64, f64)> {
    price_bounds(variants.iter().filter_map(|variant| {
        variant
            .get("price")
            .and_then(Value::as_str)
            .and_then(parse_product_price)
    }))
}

fn price_bounds<I>(prices: I) -> Option<(f64, f64)>
where
    I: IntoIterator<Item = f64>,
{
    let mut iter = prices.into_iter();
    let first = iter.next()?;
    let mut min_price = first;
    let mut max_price = first;
    for price in iter {
        if price < min_price {
            min_price = price;
        }
        if price > max_price {
            max_price = price;
        }
    }
    Some((min_price, max_price))
}

fn parse_product_price(price: impl AsRef<str>) -> Option<f64> {
    price.as_ref().trim().parse::<f64>().ok()
}

fn computed_product_price_range_json(
    min_price: f64,
    max_price: f64,
    currency_code: &str,
    kind: ProductPriceRangeKind,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!(match kind {
            ProductPriceRangeKind::Current => "ProductPriceRangeV2",
            ProductPriceRangeKind::Legacy => "ProductPriceRange",
        })),
        "minVariantPrice" => Some(selected_json(
            &product_price_range_money(min_price, currency_code, kind),
            &selection.selection,
        )),
        "maxVariantPrice" => Some(selected_json(
            &product_price_range_money(max_price, currency_code, kind),
            &selection.selection,
        )),
        _ => None,
    })
}

fn product_price_range_money(
    price: f64,
    currency_code: &str,
    kind: ProductPriceRangeKind,
) -> Value {
    let amount = match kind {
        ProductPriceRangeKind::Current => price,
        ProductPriceRangeKind::Legacy => price * 100.0,
    };
    json!({
        "__typename": "MoneyV2",
        "amount": normalize_money_amount(&format!("{amount:.2}")),
        "currencyCode": currency_code
    })
}

fn product_collections_connection_json(
    product: &ProductRecord,
    selection: &SelectedField,
) -> Value {
    let mut collections = product.collections.clone();
    if selection.arguments.get("sortKey") == Some(&ResolvedValue::String("TITLE".to_string())) {
        collections.sort_by(|left, right| {
            let left_title = left
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let right_title = right
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            left_title.cmp(right_title)
        });
    }
    if selection.arguments.get("reverse") == Some(&ResolvedValue::Bool(true)) {
        collections.reverse();
    }
    selected_connection_json_with_args(
        collections,
        &selection.arguments,
        &selection.selection,
        value_id_cursor,
    )
}

/// `Product.hasOnlyDefaultVariant` is true exactly when the product has a single variant
/// carrying Shopify's implicit default option (`Title: Default Title`).
pub(in crate::proxy) fn product_has_only_default_variant(
    variants: &[ProductVariantRecord],
) -> bool {
    match variants {
        [variant] => {
            variant.selected_options.len() == 1
                && variant.selected_options[0].name == "Title"
                && variant.selected_options[0].value == "Default Title"
        }
        _ => false,
    }
}

/// `Product.hasOutOfStockVariants` is true when any inventory-tracked variant has a
/// non-positive available quantity. `inventory_quantity` mirrors the variant's total
/// available stock (kept in sync by the inventory mutation handlers), so it is the
/// available figure to test; untracked variants never count as out of stock.
pub(in crate::proxy) fn product_has_out_of_stock_variants(
    variants: &[ProductVariantRecord],
) -> bool {
    variants
        .iter()
        .filter(|variant| variant.inventory_item.tracked)
        .any(|variant| variant.inventory_quantity <= 0)
}

pub(in crate::proxy) fn product_json_with_variants(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    selections: &[SelectedField],
) -> Value {
    product_json_with_variants_and_currency(product, variants, selections, "USD")
}

pub(in crate::proxy) fn product_json_with_variants_and_currency(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    selections: &[SelectedField],
    currency_code: &str,
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("Product")),
        "id" => Some(json!(product.id)),
        "title" => Some(json!(product.title)),
        "handle" => Some(json!(product.handle)),
        "status" => Some(json!(product.status)),
        "createdAt" => Some(json!(product.created_at)),
        "updatedAt" => Some(json!(product.updated_at)),
        "descriptionHtml" => Some(json!(product.description_html)),
        "vendor" => Some(json!(product.vendor)),
        "productType" => Some(json!(product.product_type)),
        "tags" => Some(json!(product.tags)),
        "legacyResourceId" => Some(json!(resource_id_tail(&product.id))),
        // `Product.totalInventory` is a denormalized aggregate Shopify maintains lazily:
        // variant/inventory-item quantities update immediately, but the product total can
        // lag (notably after a 2025-01 `inventoryAdjustQuantities`, and for non-`available`
        // quantity changes). The mutation handlers recompute and store it on the product
        // record (`sync_product_total_inventory`) following the route-version contract, so
        // the read path renders the stored value rather than recomputing live.
        "totalInventory" => Some(json!(product.total_inventory)),
        "tracksInventory" => Some(if variants.is_empty() {
            json!(product.tracks_inventory)
        } else {
            json!(variants
                .iter()
                .any(|variant| variant.inventory_item.tracked))
        }),
        // Recomputed live from the effective variants: unlike `totalInventory`, Shopify
        // keeps these structural aggregates in step with the current variant set.
        "hasOnlyDefaultVariant" => Some(if variants.is_empty() {
            product
                .extra_fields
                .get("hasOnlyDefaultVariant")
                .cloned()
                .unwrap_or(Value::Bool(true))
        } else {
            json!(product_has_only_default_variant(variants))
        }),
        "hasOutOfStockVariants" => Some(if variants.is_empty() {
            product
                .extra_fields
                .get("hasOutOfStockVariants")
                .cloned()
                .unwrap_or(Value::Bool(false))
        } else {
            json!(product_has_out_of_stock_variants(variants))
        }),
        "totalVariants" => Some(if variants.is_empty() {
            product
                .extra_fields
                .get("totalVariants")
                .cloned()
                .unwrap_or_else(|| json!(product.variants.len()))
        } else {
            json!(variants.len())
        }),
        "priceRangeV2" => Some(product_price_range_json(
            product,
            variants,
            selection,
            currency_code,
            ProductPriceRangeKind::Current,
        )),
        "priceRange" => Some(product_price_range_json(
            product,
            variants,
            selection,
            currency_code,
            ProductPriceRangeKind::Legacy,
        )),
        "templateSuffix" => Some(
            product
                .extra_fields
                .get("templateSuffix")
                .cloned()
                .unwrap_or_else(|| json!(product.template_suffix)),
        ),
        "seo" => Some(
            product
                .extra_fields
                .get("seo")
                .cloned()
                .map(|value| nullable_selected_json(&value, &selection.selection))
                .unwrap_or_else(|| product_seo_json(product, &selection.selection)),
        ),
        "onlineStorePreviewUrl" => Some(
            product
                .extra_fields
                .get("onlineStorePreviewUrl")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "category" => Some(
            product
                .extra_fields
                .get("category")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "requiresSellingPlan" => Some(
            product
                .extra_fields
                .get("requiresSellingPlan")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        "isGiftCard" => Some(
            product
                .extra_fields
                .get("isGiftCard")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        ),
        "giftCardTemplateSuffix" => Some(
            product
                .extra_fields
                .get("giftCardTemplateSuffix")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "options" => Some(Value::Array(
            product
                .extra_fields
                .get("options")
                .and_then(Value::as_array)
                .map(|options| {
                    options
                        .iter()
                        .map(|option| nullable_selected_json(option, &selection.selection))
                        .collect()
                })
                .unwrap_or_default(),
        )),
        "variants" => Some(if variants.is_empty() {
            selected_connection_json(product.variants.clone(), &selection.selection)
        } else {
            product_variant_connection_with_fallback_json(
                variants,
                &product.variants,
                &selection.arguments,
                &selection.selection,
            )
        }),
        "collections" => Some(product_collections_connection_json(product, selection)),
        "media" => Some(selected_connection_json(
            product.media.clone(),
            &selection.selection,
        )),
        "images" => Some(selected_empty_connection_json(&selection.selection)),
        "metafield" => Some(
            product
                .extra_fields
                .get("metafield")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "metafields" => Some(
            product
                .extra_fields
                .get("metafields")
                .cloned()
                .map(|value| selected_json(&value, &selection.selection))
                .unwrap_or_else(|| selected_empty_connection_json(&selection.selection)),
        ),
        _ => product_publication_field_json(product, selection).or_else(|| {
            product
                .extra_fields
                .get(&selection.name)
                .cloned()
                .map(|value| nullable_selected_json(&value, &selection.selection))
        }),
    })
}

pub(in crate::proxy) fn product_variant_connection_with_fallback_json(
    variants: &[ProductVariantRecord],
    fallback_variants: &[Value],
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let (variant_records, page_info) =
        connection_window(variants, arguments, |variant| variant.id.clone());
    let variant_nodes = variant_records
        .iter()
        .map(product_variant_state_json)
        .collect::<Vec<_>>();
    let variant_ids = variant_records
        .iter()
        .map(|variant| variant.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut nodes = Vec::new();
    for fallback in fallback_variants {
        let fallback_id = fallback.get("id").and_then(Value::as_str);
        if fallback_id.is_some_and(|id| variant_ids.contains(id)) {
            continue;
        }
        nodes.push(fallback.clone());
    }
    nodes.extend(variant_nodes);
    selected_json(
        &connection_json_with_cursor(nodes, |_, node| value_id_cursor(node), page_info),
        selections,
    )
}

pub(in crate::proxy) fn product_variant_json_without_parent(
    variant: &ProductVariantRecord,
    selections: &[SelectedField],
) -> Value {
    product_variant_json(variant, None, selections)
}

pub(in crate::proxy) fn product_variant_json(
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductVariant")),
        "id" => Some(json!(variant.id)),
        "title" => Some(json!(variant.title)),
        // Shopify returns `null` (not an empty string) for a variant with no SKU.
        "sku" => Some(if variant.sku.is_empty() {
            Value::Null
        } else {
            json!(variant.sku)
        }),
        "barcode" => Some(match &variant.barcode {
            Some(value) => json!(value),
            None => Value::Null,
        }),
        "price" => Some(json!(variant.price)),
        "compareAtPrice" => Some(match &variant.compare_at_price {
            Some(value) => json!(value),
            None => Value::Null,
        }),
        "taxable" => Some(json!(variant.taxable)),
        "inventoryPolicy" => Some(json!(variant.inventory_policy)),
        "inventoryQuantity" => Some(json!(variant.inventory_quantity)),
        "selectedOptions" => Some(Value::Array(
            variant
                .selected_options
                .iter()
                .map(|option| {
                    selected_json(
                        &json!({ "name": option.name, "value": option.value }),
                        &selection.selection,
                    )
                })
                .collect(),
        )),
        "inventoryItem" => Some(product_variant_inventory_item_json(
            variant,
            product,
            &selection.selection,
        )),
        "product" => Some(match product {
            Some(product) => product_json_with_variants(product, &[], &selection.selection),
            None => variant
                .extra_fields
                .get("product")
                .map(|value| product_variant_extra_field_json(value, &selection.selection))
                .unwrap_or(Value::Null),
        }),
        // A variant's `media` is the subset of the owning product's media library
        // that has been attached to the variant (via productVariantAppendMedia),
        // rendered in attachment order.
        "media" => Some(selected_connection_json(
            variant_attached_media_nodes(variant, product),
            &selection.selection,
        )),
        _ => variant
            .extra_fields
            .get(&selection.name)
            .map(|value| product_variant_extra_field_json(value, &selection.selection)),
    })
}

/// Resolve a variant's attached `media_ids` against its owning product's media
/// library, preserving attachment order. Falls back to any media nodes stashed
/// in `extra_fields` when the product (library) is not available in this render
/// context.
pub(in crate::proxy) fn variant_attached_media_nodes(
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
) -> Vec<Value> {
    match product {
        Some(product) => variant
            .media_ids
            .iter()
            .filter_map(|media_id| {
                product
                    .media
                    .iter()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(media_id.as_str()))
                    .cloned()
            })
            .collect(),
        None => Vec::new(),
    }
}

pub(in crate::proxy) fn product_variant_inventory_item_json(
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("InventoryItem")),
        "id" => Some(json!(variant.inventory_item.id)),
        "tracked" => Some(json!(variant.inventory_item.tracked)),
        "requiresShipping" => Some(json!(variant.inventory_item.requires_shipping)),
        // Render the inventory item's backreference variant with its owning product so
        // `inventoryItem(id:).variant.product` resolves rather than returning null.
        "variant" => Some(product_variant_json(variant, product, &selection.selection)),
        _ => variant
            .inventory_item
            .extra_fields
            .get(&selection.name)
            .map(|value| product_variant_extra_field_json(value, &selection.selection)),
    })
}

pub(in crate::proxy) fn observed_product_variant_inventory_item_json(
    product: &ProductRecord,
    variant: &Value,
    selections: &[SelectedField],
) -> Option<Value> {
    let inventory_item = variant.get("inventoryItem")?;
    Some(selected_payload_json(
        selections,
        |selection| match selection.name.as_str() {
            "__typename" => Some(json!("InventoryItem")),
            "variant" => Some(observed_product_variant_json(
                product,
                variant,
                &selection.selection,
            )),
            _ => inventory_item
                .get(&selection.name)
                .map(|value| product_variant_extra_field_json(value, &selection.selection)),
        },
    ))
}

fn observed_product_variant_json(
    product: &ProductRecord,
    variant: &Value,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductVariant")),
        "product" => Some(product_json_with_variants(
            product,
            &[],
            &selection.selection,
        )),
        _ => variant
            .get(&selection.name)
            .map(|value| product_variant_extra_field_json(value, &selection.selection)),
    })
}

pub(in crate::proxy) fn product_variant_extra_field_json(
    value: &Value,
    selections: &[SelectedField],
) -> Value {
    if selections.is_empty() || value.is_null() {
        value.clone()
    } else if let Some(values) = value.as_array() {
        Value::Array(
            values
                .iter()
                .map(|item| selected_json(item, selections))
                .collect(),
        )
    } else {
        selected_json(value, selections)
    }
}

pub(in crate::proxy) fn product_seo_json(
    product: &ProductRecord,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "title" => Some(json!(product.seo_title)),
        "description" => Some(json!(product.seo_description)),
        _ => None,
    })
}

pub(in crate::proxy) fn product_tag_query_value(query: &str) -> Option<&str> {
    query
        .strip_prefix("tag:")
        .map(|tag| tag.strip_suffix(" OR").unwrap_or(tag))
}

pub(in crate::proxy) fn product_sku_query_value(query: &str) -> Option<&str> {
    product_search_term_value(query, "sku:")
}

pub(in crate::proxy) fn product_matches_sku_query(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    query: &str,
) -> bool {
    let Some(sku) = product_sku_query_value(query) else {
        return true;
    };
    variants.iter().any(|variant| variant.sku == sku)
        || product
            .variants
            .iter()
            .any(|variant| variant.get("sku").and_then(Value::as_str) == Some(sku))
}

pub(in crate::proxy) fn product_variant_state_from_observed_json(
    value: &Value,
) -> Option<ProductVariantRecord> {
    let product_id = value
        .get("productId")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("product")
                .and_then(|product| product.get("id"))
                .and_then(Value::as_str)
        })?
        .to_string();
    let id = value.get("id")?.as_str()?.to_string();
    let inventory_item = value.get("inventoryItem");
    Some(ProductVariantRecord {
        id: id.clone(),
        product_id,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sku: value
            .get("sku")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        barcode: value
            .get("barcode")
            .and_then(Value::as_str)
            .map(str::to_string),
        price: value
            .get("price")
            .and_then(Value::as_str)
            .unwrap_or("0.00")
            .to_string(),
        compare_at_price: value
            .get("compareAtPrice")
            .and_then(Value::as_str)
            .map(str::to_string),
        taxable: value
            .get("taxable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        inventory_policy: value
            .get("inventoryPolicy")
            .and_then(Value::as_str)
            .unwrap_or("DENY")
            .to_string(),
        inventory_quantity: value
            .get("inventoryQuantity")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        selected_options: value
            .get("selectedOptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(ProductVariantSelectedOption {
                    name: option.get("name")?.as_str()?.to_string(),
                    value: option.get("value")?.as_str()?.to_string(),
                })
            })
            .collect(),
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!("gid://shopify/InventoryItem/{}", resource_id_tail(&id))
                }),
            tracked: inventory_item
                .and_then(|item| item.get("tracked"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            requires_shipping: inventory_item
                .and_then(|item| item.get("requiresShipping"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: inventory_item
                .map(|inventory_item| {
                    product_variant_state_extra_fields(
                        inventory_item,
                        &["id", "tracked", "requiresShipping"],
                    )
                })
                .unwrap_or_default(),
        },
        media_ids: variant_media_ids_from_json(value),
        extra_fields: product_variant_state_extra_fields(
            value,
            &[
                "id",
                "productId",
                "product",
                "title",
                "sku",
                "barcode",
                "price",
                "compareAtPrice",
                "taxable",
                "inventoryPolicy",
                "inventoryQuantity",
                "selectedOptions",
                "inventoryItem",
            ],
        ),
    })
}

fn product_search_term_value<'a>(query: &'a str, prefix: &str) -> Option<&'a str> {
    query
        .split_ascii_whitespace()
        .find_map(|term| term.strip_prefix(prefix))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
}

pub(in crate::proxy) fn product_state_map_json(
    products: &BTreeMap<String, ProductRecord>,
) -> Value {
    Value::Object(
        products
            .iter()
            .map(|(id, product)| (id.clone(), product_state_json(product)))
            .collect(),
    )
}

pub(in crate::proxy) fn product_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, ProductRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            product_state_from_json(value).map(|product| (id.clone(), product))
        })
        .collect()
}

pub(in crate::proxy) fn product_state_from_json(value: &Value) -> Option<ProductRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let created_at = value
        .get("createdAt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| default_product_timestamp(&id));
    let updated_at = value
        .get("updatedAt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| created_at.clone());
    let mut extra_fields = product_extra_fields_from_json(value);
    if let Some(state_extra_fields) = value.get("extraFields").and_then(Value::as_object) {
        for (key, observed) in state_extra_fields {
            extra_fields.insert(key.clone(), observed.clone());
        }
    }
    Some(ProductRecord {
        id,
        created_at,
        updated_at,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        handle: value
            .get("handle")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("ACTIVE")
            .to_string(),
        description_html: value
            .get("descriptionHtml")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        vendor: value
            .get("vendor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        product_type: value
            .get("productType")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        tags: value
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|tag| tag.as_str().map(str::to_string))
            .collect(),
        template_suffix: value
            .get("templateSuffix")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seo_title: value
            .get("seo")
            .and_then(|seo| seo.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seo_description: value
            .get("seo")
            .and_then(|seo| seo.get("description"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        total_inventory: value
            .get("totalInventory")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        tracks_inventory: value
            .get("tracksInventory")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        variants: value
            .get("variants")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        media: value
            .get("media")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        collections: value
            .get("collections")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        extra_fields,
    })
}

pub(in crate::proxy) fn product_extra_fields_from_json(value: &Value) -> BTreeMap<String, Value> {
    let mut extra_fields = BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (key, observed) in object {
            if !matches!(
                key.as_str(),
                "id" | "createdAt"
                    | "updatedAt"
                    | "title"
                    | "handle"
                    | "status"
                    | "descriptionHtml"
                    | "vendor"
                    | "productType"
                    | "tags"
                    | "totalInventory"
                    | "tracksInventory"
                    | "variants"
                    | "media"
                    | "collections"
            ) {
                extra_fields.insert(key.clone(), observed.clone());
            }
        }
    }
    extra_fields
}

pub(in crate::proxy) fn product_state_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id,
        "createdAt": product.created_at,
        "updatedAt": product.updated_at,
        "title": product.title,
        "handle": product.handle,
        "status": product.status,
        "descriptionHtml": product.description_html,
        "vendor": product.vendor,
        "productType": product.product_type,
        "tags": product.tags,
        "templateSuffix": product.template_suffix,
        "seo": {
            "title": product.seo_title,
            "description": product.seo_description
        },
        "totalInventory": product.total_inventory,
        "tracksInventory": product.tracks_inventory,
        "media": connection_json(product.media.clone()),
        "variants": connection_json(product.variants.clone()),
        "collections": connection_json(product.collections.clone()),
        "extraFields": product.extra_fields
    })
}

pub(in crate::proxy) fn product_variant_state_map_json(
    variants: &BTreeMap<String, ProductVariantRecord>,
) -> Value {
    Value::Object(
        variants
            .iter()
            .map(|(id, variant)| (id.clone(), product_variant_state_json(variant)))
            .collect(),
    )
}

pub(in crate::proxy) fn product_variant_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, ProductVariantRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            product_variant_state_from_json(value).map(|variant| (id.clone(), variant))
        })
        .collect()
}

pub(in crate::proxy) fn product_variant_state_from_json(
    value: &Value,
) -> Option<ProductVariantRecord> {
    let inventory_item = value.get("inventoryItem")?;
    Some(ProductVariantRecord {
        id: value.get("id")?.as_str()?.to_string(),
        product_id: value.get("productId")?.as_str()?.to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sku: value
            .get("sku")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        barcode: value
            .get("barcode")
            .and_then(Value::as_str)
            .map(str::to_string),
        price: value
            .get("price")
            .and_then(Value::as_str)
            .unwrap_or("0.00")
            .to_string(),
        compare_at_price: value
            .get("compareAtPrice")
            .and_then(Value::as_str)
            .map(str::to_string),
        taxable: value
            .get("taxable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        inventory_policy: value
            .get("inventoryPolicy")
            .and_then(Value::as_str)
            .unwrap_or("DENY")
            .to_string(),
        inventory_quantity: value
            .get("inventoryQuantity")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        selected_options: value
            .get("selectedOptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(ProductVariantSelectedOption {
                    name: option.get("name")?.as_str()?.to_string(),
                    value: option.get("value")?.as_str()?.to_string(),
                })
            })
            .collect(),
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item.get("id")?.as_str()?.to_string(),
            tracked: inventory_item
                .get("tracked")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            requires_shipping: inventory_item
                .get("requiresShipping")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: product_variant_state_extra_fields(
                inventory_item,
                &["id", "tracked", "requiresShipping"],
            ),
        },
        extra_fields: product_variant_state_extra_fields(
            value,
            &[
                "id",
                "productId",
                "title",
                "sku",
                "barcode",
                "price",
                "compareAtPrice",
                "taxable",
                "inventoryPolicy",
                "inventoryQuantity",
                "selectedOptions",
                "inventoryItem",
                "mediaIds",
                "media",
            ],
        ),
        media_ids: variant_media_ids_from_json(value),
    })
}

pub(in crate::proxy) fn product_variant_state_json(variant: &ProductVariantRecord) -> Value {
    // Shopify returns `null` (not an empty string) for a variant with no SKU. The state
    // parser reads a null SKU back as an empty string, so this round-trips cleanly.
    let sku = if variant.sku.is_empty() {
        Value::Null
    } else {
        json!(variant.sku)
    };
    let mut value = json!({
        "id": variant.id,
        "productId": variant.product_id,
        "title": variant.title,
        "sku": sku,
        "barcode": variant.barcode,
        "price": variant.price,
        "compareAtPrice": variant.compare_at_price,
        "taxable": variant.taxable,
        "inventoryPolicy": variant.inventory_policy,
        "inventoryQuantity": variant.inventory_quantity,
        "selectedOptions": variant.selected_options.iter().map(|option| {
            json!({ "name": option.name, "value": option.value })
        }).collect::<Vec<_>>(),
        "inventoryItem": {
            "id": variant.inventory_item.id,
            "tracked": variant.inventory_item.tracked,
            "requiresShipping": variant.inventory_item.requires_shipping
        }
    });
    if let Some(map) = value.as_object_mut() {
        for (key, field_value) in &variant.extra_fields {
            map.insert(key.clone(), field_value.clone());
        }
        if let Some(inventory_item) = map.get_mut("inventoryItem").and_then(Value::as_object_mut) {
            for (key, field_value) in &variant.inventory_item.extra_fields {
                inventory_item.insert(key.clone(), field_value.clone());
            }
        }
        // Round-trip the variant→media attachment so chained mutation targets
        // (append-media → detach-media → downstream-read share an evolving
        // dump/restore state) preserve which library media a variant carries.
        if !variant.media_ids.is_empty() {
            map.insert("mediaIds".to_string(), json!(variant.media_ids));
        }
    }
    value
}

pub(in crate::proxy) fn product_variant_state_extra_fields(
    value: &Value,
    known_fields: &[&str],
) -> BTreeMap<String, Value> {
    value
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.iter())
        .filter(|(key, _)| !known_fields.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(in crate::proxy) fn product_cursor(product: &ProductRecord) -> &str {
    &product.id
}

pub(in crate::proxy) fn product_count_json(count: usize, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "count" => Some(json!(count)),
        "precision" => Some(json!("EXACT")),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_connection_json(
    records: &[SavedSearchRecord],
    root_selection: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    selected_typed_connection(
        records,
        root_selection,
        saved_search_read_json,
        saved_search_cursor,
        |page_info_selection| {
            saved_search_page_info_json(
                records,
                page_info_selection,
                has_next_page,
                has_previous_page,
            )
        },
    )
}

pub(in crate::proxy) fn saved_search_read_json(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
) -> Value {
    saved_search_json_with_query(record, selections, &saved_search_read_query(&record.query))
}

pub(in crate::proxy) fn saved_search_json(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
) -> Value {
    saved_search_json_with_query(record, selections, &record.query)
}

pub(in crate::proxy) fn saved_search_json_with_query(
    record: &SavedSearchRecord,
    selections: &[SelectedField],
    query_display: &str,
) -> Value {
    let filters = saved_search_filters(query_display);
    let legacy_id = saved_search_legacy_resource_id(&record.id);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SavedSearch")),
        "id" => Some(json!(record.id)),
        "legacyResourceId" => Some(json!(legacy_id)),
        "name" => Some(json!(record.name)),
        "query" => Some(json!(query_display)),
        "resourceType" => Some(json!(record.resource_type)),
        "searchTerms" => Some(json!(saved_search_search_terms(query_display))),
        "filters" => Some(Value::Array(
            filters
                .iter()
                .map(|(key, value)| saved_search_filter_json(key, value, &selection.selection))
                .collect(),
        )),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_state_map_json(
    saved_searches: &BTreeMap<String, SavedSearchRecord>,
) -> Value {
    Value::Object(
        saved_searches
            .iter()
            .map(|(id, record)| (id.clone(), saved_search_state_json(record)))
            .collect(),
    )
}

pub(in crate::proxy) fn saved_search_state_map_from_json(
    value: &Value,
) -> BTreeMap<String, SavedSearchRecord> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| {
            saved_search_state_from_json(value).map(|record| (id.clone(), record))
        })
        .collect()
}

pub(in crate::proxy) fn saved_search_state_from_json(value: &Value) -> Option<SavedSearchRecord> {
    Some(SavedSearchRecord {
        id: value.get("id")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        query: value.get("query")?.as_str()?.to_string(),
        resource_type: value.get("resourceType")?.as_str()?.to_string(),
    })
}

pub(in crate::proxy) fn rust_state_dump_path_exists(dump: &Value, path: &str) -> bool {
    path.split('.')
        .try_fold(dump, |current, segment| current.get(segment))
        .is_some()
}

pub(in crate::proxy) fn saved_search_state_json(record: &SavedSearchRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "query": record.query,
        "resourceType": record.resource_type
    })
}

pub(in crate::proxy) fn saved_search_filter_json(
    key: &str,
    value: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SearchFilter")),
        "key" => Some(json!(key)),
        "value" => Some(json!(value)),
        _ => None,
    })
}

pub(in crate::proxy) fn saved_search_page_info_json(
    records: &[SavedSearchRecord],
    selections: &[SelectedField],
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    selected_json(
        &connection_page_info(
            has_next_page,
            has_previous_page,
            records.first().map(saved_search_cursor),
            records.last().map(saved_search_cursor),
        ),
        selections,
    )
}

pub(in crate::proxy) fn saved_search_mutation_payload_json(
    record: Option<&SavedSearchRecord>,
    payload_selections: &[SelectedField],
    saved_search_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "savedSearch" => Some(match record {
                Some(record) => saved_search_json(record, saved_search_selections),
                None => Value::Null,
            }),
            "userErrors" => Some(Value::Array(
                user_errors
                    .iter()
                    .map(|error| selected_json(error, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn saved_search_required_input_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let document = parsed_document(query, variables)?;
    let operation_name = document
        .operation_name
        .as_deref()
        .unwrap_or("AnonymousOperation");
    let field = document.root_fields.iter().find(|field| {
        matches!(
            field.name.as_str(),
            "savedSearchCreate" | "savedSearchUpdate"
        )
    })?;
    let input_type = match field.name.as_str() {
        "savedSearchCreate" => "SavedSearchCreateInput",
        "savedSearchUpdate" => "SavedSearchUpdateInput",
        _ => return None,
    };
    let variable_input = variables.get("input");
    let input = match field.arguments.get("input") {
        Some(ResolvedValue::Object(input)) => input,
        _ => return None,
    };

    if variable_input.is_some() {
        let value = variable_input
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        let mut errors = Vec::new();
        if field.name == "savedSearchCreate" && !input.contains_key("resourceType") {
            errors.push(invalid_variable_required_field_error(
                "resourceType",
                input_type,
                value.clone(),
                55,
            ));
        }
        if field.name == "savedSearchCreate" && !input.contains_key("name") {
            errors.push(invalid_variable_required_field_error(
                "name",
                input_type,
                value.clone(),
                47,
            ));
        }
        if field.name == "savedSearchUpdate" && !input.contains_key("id") {
            errors.push(invalid_variable_required_field_error(
                "id", input_type, value, 47,
            ));
        }
        return (!errors.is_empty()).then(|| ok_json(json!({ "errors": errors })));
    }

    let required_fields: &[(&str, &str)] = match field.name.as_str() {
        "savedSearchCreate" => &[
            ("name", "String!"),
            ("query", "String!"),
            ("resourceType", "SearchResultType!"),
        ],
        "savedSearchUpdate" => &[("id", "ID!")],
        _ => &[],
    };
    let errors = required_fields
        .iter()
        .filter(|(name, _)| !input.contains_key(*name))
        .map(|(name, ty)| {
            missing_required_input_attribute_error(
                operation_name,
                &field.name,
                input_type,
                name,
                ty,
            )
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Some(ok_json(json!({ "errors": errors })));
    }
    None
}

pub(in crate::proxy) fn missing_required_input_attribute_error(
    operation_name: &str,
    root_field: &str,
    input_object_type: &str,
    argument_name: &str,
    argument_type: &str,
) -> Value {
    json!({
        "message": format!("Argument '{}' on InputObject '{}' is required. Expected type {}", argument_name, input_object_type, argument_type),
        "locations": [{ "line": 2, "column": 28 }],
        "path": [format!("mutation {}", operation_name), root_field, "input", argument_name],
        "extensions": {
            "code": "missingRequiredInputObjectAttribute",
            "argumentName": argument_name,
            "argumentType": argument_type,
            "inputObjectType": input_object_type
        }
    })
}

pub(in crate::proxy) fn invalid_variable_required_field_error(
    field: &str,
    input_object_type: &str,
    value: Value,
    column: u64,
) -> Value {
    json!({
        "message": format!("Variable $input of type {}! was provided invalid value for {} (Expected value to not be null)", input_object_type, field),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "value": value,
            "problems": [{ "path": [field], "explanation": "Expected value to not be null" }]
        }
    })
}

pub(in crate::proxy) fn saved_search_name_taken_user_error() -> Value {
    json!({
        "field": ["input", "name"],
        "message": "Name has already been taken"
    })
}

pub(in crate::proxy) fn saved_search_delete_payload_json(
    deleted_id: Option<&str>,
    payload_selections: &[SelectedField],
    user_errors: Vec<Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedSavedSearchId" => Some(match deleted_id {
                Some(id) => json!(id),
                None => Value::Null,
            }),
            "shop" => Some(selected_json(&synthetic_shop_json(), &selection.selection)),
            "userErrors" => Some(Value::Array(user_errors.clone())),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn saved_search_input_from_field(
    field: &RootFieldSelection,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match field.arguments.get("input") {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(in crate::proxy) enum SavedSearchQueryValidationOperation {
    Create,
    Update,
}

pub(in crate::proxy) fn saved_search_query_user_errors(
    operation: SavedSearchQueryValidationOperation,
    resource_type: &str,
    query: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resource_type == "ORDER" && query.contains("reference_location_id:") {
        let field = match operation {
            SavedSearchQueryValidationOperation::Create => json!(["input", "query"]),
            SavedSearchQueryValidationOperation::Update => json!(["input", "searchTerms"]),
        };
        errors.push(json!({
            "field": field,
            "message": "Search terms is invalid, 'reference_location_id' is a reserved filter name"
        }));
    }
    let filters = saved_search_filters(query);
    let mut invalid_filters: Vec<String> = filters
        .iter()
        .filter_map(|(key, _)| {
            if saved_search_known_filter(resource_type, key)
                || saved_search_reserved_filter(resource_type, key)
            {
                None
            } else {
                Some(saved_search_base_filter_key(key).to_string())
            }
        })
        .collect();
    invalid_filters.sort();
    invalid_filters.dedup();
    for key in invalid_filters {
        errors.push(json!({
            "field": ["input", "query"],
            "message": format!("Query is invalid, '{}' is not a valid filter", key)
        }));
    }
    if resource_type == "PRODUCT" {
        let has_collection = filters.iter().any(|(key, _)| key == "collection_id");
        let incompatible: Vec<&str> = ["tag", "published_status", "error_feedback"]
            .iter()
            .copied()
            .filter(|needle| filters.iter().any(|(key, _)| key == *needle))
            .collect();
        if has_collection && !incompatible.is_empty() {
            let mut keys = vec!["collection_id"];
            keys.extend(incompatible);
            errors.push(json!({
                "field": ["input", "query"],
                "message": format!("Query has incompatible filters: {}", keys.join(", "))
            }));
        }
    }
    errors
}

fn saved_search_reserved_filter(resource_type: &str, key: &str) -> bool {
    resource_type == "ORDER" && saved_search_base_filter_key(key) == "reference_location_id"
}

pub(in crate::proxy) fn saved_search_known_filter(resource_type: &str, key: &str) -> bool {
    let base_key = saved_search_base_filter_key(key);
    match resource_type {
        "PRODUCT" => {
            matches!(
                base_key,
                "collection_id"
                    | "created_at"
                    | "error_feedback"
                    | "handle"
                    | "id"
                    | "inventory_total"
                    | "product_type"
                    | "published_at"
                    | "published_status"
                    | "sku"
                    | "status"
                    | "tag"
                    | "title"
                    | "updated_at"
                    | "vendor"
            ) || base_key.starts_with("metafields.")
        }
        "COLLECTION" => matches!(
            base_key,
            "collection_type"
                | "handle"
                | "id"
                | "product_id"
                | "product_publication_status"
                | "publishable_status"
                | "published_at"
                | "published_status"
                | "title"
                | "updated_at"
        ),
        "ORDER" => matches!(
            base_key,
            "channel_id"
                | "created_at"
                | "customer_id"
                | "email"
                | "financial_status"
                | "fulfillment_status"
                | "id"
                | "location_id"
                | "name"
                | "processed_at"
                | "sales_channel"
                | "status"
                | "tag"
                | "test"
                | "updated_at"
        ),
        "DRAFT_ORDER" => matches!(
            base_key,
            "created_at"
                | "customer_id"
                | "email"
                | "id"
                | "name"
                | "status"
                | "tag"
                | "updated_at"
        ),
        "FILE" => matches!(
            base_key,
            "created_at"
                | "filename"
                | "id"
                | "media_type"
                | "original_source"
                | "status"
                | "updated_at"
        ),
        "DISCOUNT_REDEEM_CODE" => matches!(
            base_key,
            "code" | "created_at" | "discount_id" | "id" | "status" | "updated_at"
        ),
        _ => true,
    }
}

fn saved_search_base_filter_key(key: &str) -> &str {
    key.trim_end_matches("_not")
        .trim_end_matches("_min")
        .trim_end_matches("_max")
}

pub(in crate::proxy) fn normalize_saved_search_query(query: &str) -> String {
    query.replace("metafields.$app.", "metafields.app--347082227713.")
}

pub(in crate::proxy) fn saved_search_read_query(query: &str) -> String {
    let namespace_normalized = normalize_saved_search_query(query);
    let quote_normalized = namespace_normalized.replace('\'', "\"");
    let canonical = canonical_saved_search_query(&quote_normalized);
    if saved_search_filters(&canonical).is_empty() && canonical.contains('-') {
        canonical.replace('-', "\\-")
    } else {
        canonical
    }
}

pub(in crate::proxy) fn canonical_saved_search_query(query: &str) -> String {
    let tokens = saved_search_query_tokens(query);
    if tokens.len() == 2 {
        let first_is_filter = saved_search_filter_from_token(tokens[0].as_str()).is_some();
        let second_is_filter = saved_search_filter_from_token(tokens[1].as_str()).is_some();
        if first_is_filter && !second_is_filter {
            return format!("{} {}", tokens[1], tokens[0]);
        }
    }
    if let Some((key, value)) = saved_search_filter_from_token(query) {
        if key == "inventory_total_min" && query.starts_with("-inventory_total:<") {
            return format!("inventory_total:>={}", value);
        }
    }
    query.to_string()
}

pub(in crate::proxy) fn saved_search_search_terms(query: &str) -> String {
    let display_query = query.replace('\'', "\"");
    let tokens = saved_search_query_tokens(&display_query);
    let has_grouping = display_query.contains(" OR ")
        || display_query.contains('(')
        || display_query.contains(')');
    let mut terms = Vec::new();
    for token in tokens {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')');
        if has_grouping && token.starts_with('-') {
            continue;
        }
        if !has_grouping && saved_search_filter_from_token(trimmed).is_some() {
            continue;
        }
        terms.push(token);
    }
    terms.join(" ").replace("\\-", "-")
}

pub(in crate::proxy) fn is_reserved_saved_search_name(resource_type: &str, name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    let reserved = match resource_type {
        "PRODUCT" => &["all products"][..],
        "ORDER" => &["all"][..],
        "DRAFT_ORDER" => &["all drafts"][..],
        "FILE" => &["all files"][..],
        "COLLECTION" => &["all collections"][..],
        "PRICE_RULE" => &["all price rules"][..],
        "DISCOUNT_REDEEM_CODE" => &["all codes"][..],
        _ => &[],
    };
    reserved
        .iter()
        .any(|reserved_name| normalized == *reserved_name)
}

pub(in crate::proxy) fn product_mutation_payload_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    payload_selections: &[SelectedField],
    product_selections: &[SelectedField],
    currency_code: &str,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "product" => Some(product_json_with_variants_and_currency(
                product,
                variants,
                product_selections,
                currency_code,
            )),
            "userErrors" => Some(json!([])),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn product_variant_record_from_create_input(
    input: &BTreeMap<String, ResolvedValue>,
    id: String,
    product_id: String,
    inventory_item_id: String,
) -> ProductVariantRecord {
    let mut variant = ProductVariantRecord {
        id,
        product_id,
        title: "Default Title".to_string(),
        sku: String::new(),
        barcode: None,
        price: "0.00".to_string(),
        compare_at_price: None,
        taxable: true,
        inventory_policy: "DENY".to_string(),
        inventory_quantity: 0,
        selected_options: Vec::new(),
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item_id,
            tracked: true,
            requires_shipping: true,
            extra_fields: BTreeMap::new(),
        },
        media_ids: Vec::new(),
        extra_fields: BTreeMap::new(),
    };
    apply_product_variant_input(&mut variant, input);
    variant
}

pub(in crate::proxy) fn apply_product_variant_input(
    variant: &mut ProductVariantRecord,
    input: &BTreeMap<String, ResolvedValue>,
) {
    if let Some(title) = resolved_string_field(input, "title") {
        variant.title = title;
    }
    if let Some(sku) = resolved_string_field(input, "sku") {
        variant.sku = sku;
    }
    if input.contains_key("barcode") {
        variant.barcode = resolved_string_field(input, "barcode");
    }
    if let Some(price) = resolved_string_field(input, "price") {
        variant.price = price;
    }
    if input.contains_key("compareAtPrice") {
        variant.compare_at_price = resolved_string_field(input, "compareAtPrice");
    }
    if let Some(taxable) = resolved_bool_field(input, "taxable") {
        variant.taxable = taxable;
    }
    if let Some(inventory_policy) = resolved_string_field(input, "inventoryPolicy") {
        variant.inventory_policy = inventory_policy;
    }
    if let Some(inventory_quantity) = resolved_int_field(input, "inventoryQuantity") {
        variant.inventory_quantity = inventory_quantity;
    }
    if let Some(inventory_quantity) = resolved_object_list_field(input, "inventoryQuantities")
        .into_iter()
        .filter_map(|quantity| resolved_int_field(&quantity, "availableQuantity"))
        .next()
    {
        variant.inventory_quantity = inventory_quantity;
    }
    for field in [
        "taxCode",
        "position",
        "requiresComponents",
        "showUnitPrice",
        "unitPriceMeasurement",
    ] {
        if let Some(value) = input.get(field) {
            variant
                .extra_fields
                .insert(field.to_string(), resolved_value_json(value));
        }
    }
    let selected_options = resolved_product_variant_selected_options(input);
    if input.contains_key("selectedOptions")
        || input.contains_key("options")
        || input.contains_key("optionValues")
    {
        variant.selected_options = selected_options;
    }
    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if let Some(sku) = resolved_string_field(&inventory_item, "sku") {
            variant.sku = sku;
        }
        if let Some(tracked) = resolved_bool_field(&inventory_item, "tracked") {
            variant.inventory_item.tracked = tracked;
        }
        if let Some(requires_shipping) = resolved_bool_field(&inventory_item, "requiresShipping") {
            variant.inventory_item.requires_shipping = requires_shipping;
        }
        if let Some(id) = resolved_string_field(&inventory_item, "id") {
            variant.inventory_item.id = id;
        }
        for field in [
            "sku",
            "countryCodeOfOrigin",
            "provinceCodeOfOrigin",
            "measurement",
        ] {
            if let Some(value) = inventory_item.get(field) {
                variant
                    .inventory_item
                    .extra_fields
                    .insert(field.to_string(), resolved_value_json(value));
            }
        }
        if let Some(value) = inventory_item.get("harmonizedSystemCode") {
            let value = match value {
                ResolvedValue::String(value) => {
                    Value::String(product_variant_normalized_harmonized_system_code(value))
                }
                _ => resolved_value_json(value),
            };
            variant
                .inventory_item
                .extra_fields
                .insert("harmonizedSystemCode".to_string(), value);
        }
    }
}

fn product_variant_normalized_harmonized_system_code(value: &str) -> String {
    value.chars().filter(char::is_ascii_alphanumeric).collect()
}

pub(in crate::proxy) fn resolved_product_variant_selected_options(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantSelectedOption> {
    let selected_options = resolved_object_list_field(input, "selectedOptions")
        .into_iter()
        .filter_map(|option| {
            Some(ProductVariantSelectedOption {
                name: resolved_string_field(&option, "name")?,
                value: resolved_string_field(&option, "value")?,
            })
        })
        .collect::<Vec<_>>();
    if !selected_options.is_empty() || input.contains_key("selectedOptions") {
        return selected_options;
    }
    let option_values = resolved_object_list_field(input, "optionValues")
        .into_iter()
        .filter_map(|option| {
            Some(ProductVariantSelectedOption {
                name: resolved_string_field(&option, "optionName")
                    .or_else(|| resolved_string_field(&option, "name"))
                    .unwrap_or_else(|| "Title".to_string()),
                value: resolved_string_field(&option, "name")
                    .or_else(|| resolved_string_field(&option, "linkedMetafieldValue"))?,
            })
        })
        .collect::<Vec<_>>();
    if !option_values.is_empty() || input.contains_key("optionValues") {
        return option_values;
    }
    match input.get("options") {
        Some(ResolvedValue::List(options)) => options
            .iter()
            .enumerate()
            .filter_map(|(index, option)| match option {
                ResolvedValue::String(value) => Some(ProductVariantSelectedOption {
                    name: format!("Option{}", index + 1),
                    value: value.clone(),
                }),
                ResolvedValue::Object(object) => Some(ProductVariantSelectedOption {
                    name: resolved_string_field(object, "name")
                        .unwrap_or_else(|| format!("Option{}", index + 1)),
                    value: resolved_string_field(object, "value")?,
                }),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn product_variant_input_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    product_variant_input_user_errors_with_prefix(input, &[])
}

pub(in crate::proxy) fn product_variant_input_user_errors_with_prefix(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &[String],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.get("price") == Some(&ResolvedValue::Null) {
        errors.push(json!({
            "field": prefixed_error_field(field_prefix, &["price"]),
            "message": "Price can't be blank",
            "code": "INVALID"
        }));
    } else if let Some(price) = resolved_variant_decimal(input, "price") {
        if price < 0.0 {
            errors.push(json!({
                "field": prefixed_error_field(field_prefix, &["price"]),
                "message": "Price must be greater than or equal to 0",
                "code": "GREATER_THAN_OR_EQUAL_TO"
            }));
        } else if price >= 1_000_000_000_000_000_000.0 {
            errors.push(json!({
                "field": prefixed_error_field(field_prefix, &["price"]),
                "message": "Price must be less than 1000000000000000000",
                "code": "INVALID_INPUT"
            }));
        }
    }

    if let Some(compare_at_price) = resolved_variant_decimal(input, "compareAtPrice") {
        if compare_at_price >= 1_000_000_000_000_000_000.0 {
            errors.push(json!({
                "field": prefixed_error_field(field_prefix, &["compareAtPrice"]),
                "message": "must be less than 1000000000000000000",
                "code": "INVALID_INPUT"
            }));
        }
    }

    if let Some(quantity) = resolved_int_field(input, "inventoryQuantity") {
        if quantity > 1_000_000_000 {
            errors.push(json!({
                "field": prefixed_error_field(field_prefix, &["inventoryQuantity"]),
                "message": "Inventory quantity must be less than or equal to 1000000000",
                "code": "INVALID_INPUT"
            }));
        }
    }
    for quantity in resolved_object_list_field(input, "inventoryQuantities") {
        if let Some(available_quantity) = resolved_int_field(&quantity, "availableQuantity") {
            if available_quantity > 1_000_000_000 {
                errors.push(json!({
                    "field": prefixed_error_field(field_prefix, &["inventoryQuantities"]),
                    "message": "Inventory quantity must be less than or equal to 1000000000",
                    "code": "INVALID_INPUT"
                }));
                break;
            }
        }
    }

    if resolved_string_field(input, "sku").is_some_and(|sku| sku.chars().count() > 255) {
        errors.push(json!({
            "field": prefixed_error_field(field_prefix, &["sku"]),
            "message": "SKU is too long (maximum is 255 characters)",
            "code": "INVALID_INPUT"
        }));
    }
    if resolved_string_field(input, "barcode").is_some_and(|barcode| barcode.chars().count() > 255)
    {
        errors.push(json!({
            "field": prefixed_error_field(field_prefix, &["barcode"]),
            "message": "Barcode is too long (maximum is 255 characters)",
            "code": "INVALID_INPUT"
        }));
    }

    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if resolved_string_field(&inventory_item, "sku")
            .is_some_and(|sku| sku.chars().count() > 255)
        {
            let bulk_field = !field_prefix.is_empty();
            errors.push(json!({
                "field": if bulk_field {
                    prefixed_error_field(field_prefix, &[])
                } else {
                    prefixed_error_field(field_prefix, &["inventoryItem", "sku"])
                },
                "message": "SKU is too long (maximum is 255 characters)",
                "code": "INVALID_INPUT"
            }));
            if bulk_field {
                errors.push(json!({
                    "field": prefixed_error_field(field_prefix, &[]),
                    "message": "is too long (maximum is 255 characters)",
                    "code": Value::Null
                }));
            }
        }
    }

    for (option_index, option) in resolved_product_variant_selected_options(input)
        .into_iter()
        .enumerate()
    {
        if option.value.chars().count() > 255 {
            errors.push(json!({
                "field": if input.contains_key("optionValues") {
                    prefixed_error_field(
                        field_prefix,
                        &["optionValues", &option_index.to_string(), "name"],
                    )
                } else {
                    prefixed_error_field(field_prefix, &["options"])
                },
                "message": "Option value name is too long",
                "code": "INVALID_INPUT"
            }));
            break;
        }
    }

    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if let Some(measurement) = resolved_object_field(&inventory_item, "measurement") {
            if let Some(weight) = resolved_object_field(&measurement, "weight") {
                if let Some(value) = resolved_variant_decimal(&weight, "value") {
                    if value < 0.0 {
                        errors.push(json!({
                            "field": variant_weight_error_field(field_prefix),
                            "message": "Weight must be greater than or equal to 0",
                            "code": "GREATER_THAN_OR_EQUAL_TO"
                        }));
                    } else if value >= 2_000_000_000.0 {
                        errors.push(json!({
                            "field": variant_weight_error_field(field_prefix),
                            "message": "Weight must be less than 2000000000",
                            "code": "INVALID_INPUT"
                        }));
                    }
                }
                if let Some(unit) = resolved_string_field(&weight, "unit") {
                    if !matches!(unit.as_str(), "KILOGRAMS" | "GRAMS" | "POUNDS" | "OUNCES") {
                        errors.push(json!({
                            "field": variant_weight_error_field(field_prefix),
                            "message": format!("Weight unit must be one of KILOGRAMS, GRAMS, POUNDS, OUNCES"),
                            "code": "INVALID_INPUT"
                        }));
                    }
                }
            }
        }
    }

    errors
}

fn prefixed_error_field(prefix: &[String], suffix: &[&str]) -> Value {
    Value::Array(
        prefix
            .iter()
            .cloned()
            .chain(suffix.iter().map(|field| (*field).to_string()))
            .map(Value::String)
            .collect(),
    )
}

fn variant_weight_error_field(prefix: &[String]) -> Value {
    if prefix.is_empty() {
        prefixed_error_field(prefix, &["inventoryItem", "measurement", "weight"])
    } else {
        prefixed_error_field(prefix, &[])
    }
}

fn resolved_variant_decimal(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<f64> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => value.parse::<f64>().ok(),
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::Float(value)) => Some(*value),
        _ => None,
    }
}

pub(in crate::proxy) fn no_key_on_variant_create_response(field: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("Field '{}' is not allowed on create", field),
            "extensions": {
                "code": "no_key_on_create",
                "key": field
            }
        }]
    }))
}
pub(in crate::proxy) fn product_create_user_errors_response(
    query: &str,
    errors: Vec<Value>,
) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| ("productCreate".to_string(), Vec::new()));
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let errors = errors
        .into_iter()
        .map(|error| selected_json(&error, &error_selection))
        .collect::<Vec<_>>();
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"product": null, "userErrors": errors}), &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn product_delete_payload_json(
    deleted_product_id: &str,
    payload_selections: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedProductId" => Some(json!(deleted_product_id)),
            "userErrors" => Some(json!([])),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn product_delete_async_operation_payload(operation_id: &str) -> Value {
    json!({
        "deletedProductId": null,
        "productDeleteOperation": {
            "id": operation_id,
            "status": "CREATED",
            "deletedProductId": null,
            "userErrors": []
        },
        "userErrors": []
    })
}

pub(in crate::proxy) fn product_delete_async_duplicate_payload() -> Value {
    json!({
        "deletedProductId": null,
        "productDeleteOperation": null,
        "userErrors": [{
            "field": null,
            "message": "Another operation already in progress. Please wait until current one is finished."
        }]
    })
}

pub(in crate::proxy) fn product_create_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    product_input(query, variables)
}

/// Extract the taxonomy category GID from a product mutation input. Shopify accepts
/// the category as a scalar `category` GID, or nested under the legacy
/// `productCategory`/`standardProductType`/`standardizedProductType` objects keyed by
/// `productTaxonomyNodeId`.
pub(in crate::proxy) fn product_category_input_id(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    resolved_string_field(input, "category")
        .or_else(|| resolved_object_string_field(input, "productCategory", "productTaxonomyNodeId"))
        .or_else(|| {
            resolved_object_string_field(input, "standardProductType", "productTaxonomyNodeId")
        })
        .or_else(|| {
            resolved_object_string_field(input, "standardizedProductType", "productTaxonomyNodeId")
        })
}

/// Resolve a taxonomy category GID to its `{id, fullName}` shape. Shopify materializes
/// `category.fullName` from its global product taxonomy; we mirror the well-known nodes
/// the taxonomy exposes (falling back to the bare id for nodes we don't model).
pub(in crate::proxy) fn product_category_value(id: &str) -> Value {
    let full_name = match id {
        "gid://shopify/TaxonomyCategory/aa-1-1" => "Apparel & Accessories > Clothing > Activewear",
        "gid://shopify/TaxonomyCategory/na" => "Uncategorized",
        other => other,
    };
    json!({ "id": id, "fullName": full_name })
}

pub(in crate::proxy) fn is_saved_search_root(root: &str) -> bool {
    matches!(
        root,
        "automaticDiscountSavedSearches"
            | "codeDiscountSavedSearches"
            | "collectionSavedSearches"
            | "customerSavedSearches"
            | "discountRedeemCodeSavedSearches"
            | "draftOrderSavedSearches"
            | "fileSavedSearches"
            | "orderSavedSearches"
            | "productSavedSearches"
    )
}

pub(in crate::proxy) fn saved_search_resource_type(root: &str) -> &'static str {
    match root {
        "automaticDiscountSavedSearches" => "DISCOUNT",
        "codeDiscountSavedSearches" => "DISCOUNT",
        "collectionSavedSearches" => "COLLECTION",
        "customerSavedSearches" => "CUSTOMER",
        "discountRedeemCodeSavedSearches" => "DISCOUNT_REDEEM_CODE",
        "draftOrderSavedSearches" => "DRAFT_ORDER",
        "fileSavedSearches" => "FILE",
        "orderSavedSearches" => "ORDER",
        "productSavedSearches" => "PRODUCT",
        _ => "UNKNOWN",
    }
}

pub(in crate::proxy) fn default_saved_searches(resource_type: &str) -> Vec<SavedSearchRecord> {
    match resource_type {
        "ORDER" => vec![
            saved_search_record(
                "gid://shopify/SavedSearch/3634391515442",
                "Unfulfilled",
                "status:open fulfillment_status:unshipped,partial",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391548210",
                "Unpaid",
                "status:open financial_status:unpaid",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391580978",
                "Open",
                "status:open",
                "ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634391613746",
                "Archived",
                "status:closed",
                "ORDER",
            ),
        ],
        "DRAFT_ORDER" => vec![
            saved_search_record(
                "gid://shopify/SavedSearch/3634390597938",
                "Open and invoice sent",
                "status:open_and_invoice_sent",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390630706",
                "Open",
                "status:open",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390663474",
                "Invoice sent",
                "status:invoice_sent",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390696242",
                "Completed",
                "status:completed",
                "DRAFT_ORDER",
            ),
            saved_search_record(
                "gid://shopify/SavedSearch/3634390729010",
                "Submitted for review",
                "status:open source:online_store",
                "DRAFT_ORDER",
            ),
        ],
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn default_saved_search_by_id(id: &str) -> Option<SavedSearchRecord> {
    [
        "ORDER",
        "DRAFT_ORDER",
        "PRODUCT",
        "COLLECTION",
        "CUSTOMER",
        "FILE",
        "DISCOUNT_REDEEM_CODE",
        "DISCOUNT",
    ]
    .iter()
    .flat_map(|resource_type| default_saved_searches(resource_type))
    .find(|record| record.id == id)
}

pub(in crate::proxy) fn saved_search_record(
    id: &str,
    name: &str,
    query: &str,
    resource_type: &str,
) -> SavedSearchRecord {
    SavedSearchRecord {
        id: id.to_string(),
        name: name.to_string(),
        query: query.to_string(),
        resource_type: resource_type.to_string(),
    }
}

pub(in crate::proxy) fn saved_search_cursor(record: &SavedSearchRecord) -> String {
    format!("cursor:{}", record.id)
}

pub(in crate::proxy) fn saved_search_legacy_resource_id(id: &str) -> String {
    resource_id_tail(id).to_string()
}

pub(in crate::proxy) fn saved_search_filters(query: &str) -> Vec<(String, String)> {
    let query = normalize_saved_search_query(query);
    let tokens = saved_search_query_tokens(&query);
    let grouped = query.contains(" OR ") || query.contains('(') || query.contains(')');
    tokens
        .iter()
        .filter_map(|term| {
            let trimmed = term.trim_matches(|ch| ch == '(' || ch == ')');
            if grouped && !trimmed.starts_with('-') {
                return None;
            }
            saved_search_filter_from_token(trimmed)
        })
        .collect()
}

pub(in crate::proxy) fn saved_search_filter_from_token(term: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = term.split_once(':')?;
    if raw_key.is_empty() || raw_value.is_empty() {
        return None;
    }
    let mut key = raw_key.to_string();
    let mut value = raw_value.trim_matches('"').to_string();
    let negated = key.starts_with('-');
    if negated {
        key = key.trim_start_matches('-').to_string();
    }
    if value == "*" {
        value = "true".to_string();
    }
    if let Some(stripped) = value.strip_prefix(">=").or_else(|| value.strip_prefix('>')) {
        key = if negated {
            format!("{}_max", key)
        } else {
            format!("{}_min", key)
        };
        value = stripped.to_string();
    } else if let Some(stripped) = value.strip_prefix("<=").or_else(|| value.strip_prefix('<')) {
        key = if negated {
            format!("{}_min", key)
        } else {
            format!("{}_max", key)
        };
        value = stripped.to_string();
    } else if negated {
        key = format!("{}_not", key);
    }
    Some((key, value))
}

pub(in crate::proxy) fn saved_search_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in query.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(in crate::proxy) fn product_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let mut arguments = root_field_arguments(query, variables)?;
    match arguments
        .remove("product")
        .or_else(|| arguments.remove("input"))
    {
        Some(ResolvedValue::Object(input)) => Some(input),
        _ => None,
    }
}

pub(in crate::proxy) fn product_variant_input(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let mut arguments = root_field_arguments(query, variables)?;
    match arguments.remove("input") {
        Some(ResolvedValue::Object(input)) => Some(input),
        _ => None,
    }
}

pub(in crate::proxy) fn product_create_status_validation_error(
    request: &Request,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == "productCreate")?;
    let (argument_name, input_object_type) = if field.raw_arguments.contains_key("product") {
        ("product", "ProductCreateInput")
    } else {
        ("input", "ProductInput")
    };
    let input = field.raw_arguments.get(argument_name)?;
    product_status_input_field_validation_error(
        request,
        query,
        &field,
        input,
        ProductStatusInputContext {
            argument_name,
            input_object_type,
            field_name: "status",
            expected_type: "ProductStatus",
        },
    )
}

pub(in crate::proxy) fn product_status_argument_validation_error(
    request: &Request,
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
    container_type_name: &str,
    container_name: &str,
    expected_type: &str,
) -> Option<Response> {
    let raw = field.raw_arguments.get(argument_name)?;
    match raw {
        RawArgumentValue::Variable { name, value } => {
            let status = resolved_status_value(value.as_ref()?)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            let definition = variable_definition_info(query, name);
            let variable_type = definition
                .as_ref()
                .map(|definition| definition.type_display.clone())
                .unwrap_or_else(|| expected_type.to_string());
            let location = definition.map(|definition| definition.location);
            Some(invalid_product_status_variable_error(
                request,
                name,
                &variable_type,
                value.as_ref()?,
                None,
                &status,
                location,
            ))
        }
        raw => {
            let status = raw_product_status_value(raw)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            Some(invalid_product_status_literal_error(
                query,
                field,
                ProductStatusLiteralError {
                    value: &status,
                    argument_name,
                    type_name: container_type_name,
                    container_name,
                    expected_type,
                    location: None,
                },
            ))
        }
    }
}

fn product_status_input_field_validation_error(
    request: &Request,
    query: &str,
    field: &RootFieldSelection,
    input: &RawArgumentValue,
    context: ProductStatusInputContext<'_>,
) -> Option<Response> {
    match input {
        RawArgumentValue::Object(input) => {
            let status = raw_product_status_value(input.get(context.field_name)?)?;
            if product_status_allowed(&status, request) {
                return None;
            }
            let location = root_argument_value_location(query, field, context.argument_name);
            Some(invalid_product_status_literal_error(
                query,
                field,
                ProductStatusLiteralError {
                    value: &status,
                    argument_name: context.field_name,
                    type_name: "InputObject",
                    container_name: context.input_object_type,
                    expected_type: context.expected_type,
                    location,
                },
            ))
        }
        RawArgumentValue::Variable { name, value } => {
            let value = value.as_ref()?;
            let status = match value {
                ResolvedValue::Object(input) => resolved_string_field(input, context.field_name)?,
                _ => return None,
            };
            if product_status_allowed(&status, request) {
                return None;
            }
            let definition = variable_definition_info(query, name);
            let variable_type = definition
                .as_ref()
                .map(|definition| definition.type_display.clone())
                .unwrap_or_else(|| context.input_object_type.to_string());
            let location = definition.map(|definition| definition.location);
            Some(invalid_product_status_variable_error(
                request,
                name,
                &variable_type,
                value,
                Some(context.field_name),
                &status,
                location,
            ))
        }
        _ => None,
    }
}

fn invalid_product_status_literal_error(
    query: &str,
    field: &RootFieldSelection,
    error: ProductStatusLiteralError<'_>,
) -> Response {
    let operation_path = parsed_document(query, &BTreeMap::new())
        .map(|document| document.operation_path)
        .unwrap_or_else(|| "mutation".to_string());
    let path = if error.type_name == "InputObject" {
        let input_argument_name = field
            .raw_arguments
            .contains_key("product")
            .then_some("product")
            .or_else(|| field.raw_arguments.contains_key("input").then_some("input"))
            .unwrap_or("input");
        json!([
            operation_path,
            field.name.clone(),
            input_argument_name,
            error.argument_name
        ])
    } else {
        json!([operation_path, field.name.clone(), error.argument_name])
    };
    let location = error.location.unwrap_or(field.location);
    ok_json(json!({
        "errors": [{
            "message": format!(
                "Argument '{}' on {} '{}' has an invalid value ({}). Expected type '{}'.",
                error.argument_name, error.type_name, error.container_name, error.value, error.expected_type
            ),
            "locations": [{"line": location.line, "column": location.column}],
            "path": path,
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": error.type_name,
                "argumentName": error.argument_name
            }
        }]
    }))
}

fn root_argument_value_location(
    query: &str,
    field: &RootFieldSelection,
    argument_name: &str,
) -> Option<SourceLocation> {
    let mut line = field.location.line;
    let mut column = field.location.column;
    let start = byte_offset_for_location(query, field.location)?;
    let haystack = &query[start..];
    let argument_start = haystack.find(argument_name)?;
    let after_name = start + argument_start + argument_name.len();
    let after_colon = query[after_name..].find(':')? + after_name + 1;
    let value_offset = query[after_colon..]
        .char_indices()
        .find_map(|(offset, ch)| (!ch.is_whitespace()).then_some(after_colon + offset))?;

    for ch in query[start..value_offset].chars() {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Some(SourceLocation { line, column })
}

fn byte_offset_for_location(query: &str, location: SourceLocation) -> Option<usize> {
    let mut line = 1;
    let mut column = 1;
    for (offset, ch) in query.char_indices() {
        if line == location.line && column == location.column {
            return Some(offset);
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line == location.line && column == location.column).then_some(query.len())
}

fn invalid_product_status_variable_error(
    request: &Request,
    variable_name: &str,
    variable_type: &str,
    value: &ResolvedValue,
    field_name: Option<&str>,
    invalid_status: &str,
    location: Option<SourceLocation>,
) -> Response {
    let explanation = format!(
        "Expected \"{}\" to be one of: {}",
        invalid_status,
        product_status_allowed_values_label(request)
    );
    let message = field_name.map_or_else(
        || format!("Variable ${variable_name} of type {variable_type} was provided invalid value"),
        |field_name| {
            format!(
                "Variable ${variable_name} of type {variable_type} was provided invalid value for {field_name} ({explanation})"
            )
        },
    );
    let path = field_name
        .map(|field_name| json!([field_name]))
        .unwrap_or_else(|| json!([]));
    ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{
                "line": location.map(|location| location.line).unwrap_or(1),
                "column": location.map(|location| location.column).unwrap_or(1)
            }],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": resolved_value_json(value),
                "problems": [{
                    "path": path,
                    "explanation": explanation
                }]
            }
        }]
    }))
}

fn raw_product_status_value(value: &RawArgumentValue) -> Option<String> {
    match value {
        RawArgumentValue::Enum(value) | RawArgumentValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_status_value(value: &ResolvedValue) -> Option<String> {
    match value {
        ResolvedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn product_status_allowed(status: &str, request: &Request) -> bool {
    PRODUCT_STATUS_BASE_VALUES.contains(&status)
        || (status == "UNLISTED" && product_status_allows_unlisted(request))
}

fn product_status_allowed_values_label(request: &Request) -> String {
    let mut values = PRODUCT_STATUS_BASE_VALUES.to_vec();
    if product_status_allows_unlisted(request) {
        values.push("UNLISTED");
    }
    values.join(", ")
}

fn product_status_allows_unlisted(request: &Request) -> bool {
    admin_graphql_version(&request.path).is_some_and(|version| version_at_least(version, 2025, 10))
}

pub(in crate::proxy) fn version_at_least(
    version: &str,
    minimum_year: u16,
    minimum_month: u8,
) -> bool {
    let Some((year, month)) = parse_year_month_version(version) else {
        return false;
    };
    (year, month) >= (minimum_year, minimum_month)
}

fn parse_year_month_version(version: &str) -> Option<(u16, u8)> {
    let (year, month) = version.split_once('-')?;
    Some((year.parse().ok()?, month.parse().ok()?))
}

pub(in crate::proxy) fn product_delete_required_id_error(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let field = root_fields(query, variables)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.name == "productDelete")?;
    let input = field
        .raw_arguments
        .get("input")
        .or_else(|| field.raw_arguments.get("product"))?;

    match input {
        RawArgumentValue::Object(input) => match input.get("id") {
            None => Some(product_delete_inline_missing_id_error()),
            Some(value) if value.is_literal_null() => Some(product_delete_inline_null_id_error()),
            _ => None,
        },
        RawArgumentValue::Variable { name, value: None } => {
            Some(product_delete_variable_required_id_error(Value::Null, name))
        }
        RawArgumentValue::Variable {
            name,
            value: Some(ResolvedValue::Object(input)),
        } => match input.get("id") {
            None => Some(product_delete_variable_required_id_error(
                resolved_value_json(&ResolvedValue::Object(input.clone())),
                name,
            )),
            Some(ResolvedValue::Null) => Some(product_delete_variable_required_id_error(
                resolved_value_json(&ResolvedValue::Object(input.clone())),
                name,
            )),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::proxy) fn product_update_missing_product(query: &str) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| ("productUpdate".to_string(), Vec::new()));
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let error = selected_json(
        &json!({
            "field": ["id"],
            "message": "Product does not exist",
            "code": "NOT_FOUND"
        }),
        &error_selection,
    );
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"product": null, "userErrors": [error]}), &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn product_delete_missing_product(query: &str) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| ("productDelete".to_string(), Vec::new()));
    let error_selection =
        selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
    let error = selected_json(
        &json!({
            "field": ["id"],
            "message": "Product does not exist",
            "code": "NOT_FOUND"
        }),
        &error_selection,
    );
    ok_json(json!({
        "data": {
            response_key: selected_json(&json!({"deletedProductId": null, "userErrors": [error]}), &payload_selection)
        }
    }))
}

pub(in crate::proxy) fn product_delete_inline_missing_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
            "locations": [{"line": 3, "column": 26}],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "missingRequiredInputObjectAttribute",
                "argumentName": "id",
                "argumentType": "ID!",
                "inputObjectType": "ProductDeleteInput"
            }
        }]
    }))
}

pub(in crate::proxy) fn product_delete_inline_null_id_error() -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
            "locations": [{"line": 3, "column": 26}],
            "path": ["mutation", "productDelete", "input", "id"],
            "extensions": {
                "code": "argumentLiteralsIncompatible",
                "typeName": "InputObject",
                "argumentName": "id"
            }
        }]
    }))
}

pub(in crate::proxy) fn product_delete_variable_required_id_error(
    value: Value,
    variable_name: &str,
) -> Response {
    ok_json(json!({
        "errors": [{
            "message": format!("Variable ${} of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)", variable_name),
            "locations": [{"line": 2, "column": 37}],
            "extensions": {
                "code": "INVALID_VARIABLE",
                "value": value,
                "problems": [{
                    "path": ["id"],
                    "explanation": "Expected value to not be null"
                }]
            }
        }]
    }))
}

pub(in crate::proxy) fn product_variant_media_user_error(
    field: &[&str],
    message: &str,
    code: &str,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

pub(in crate::proxy) fn variant_media_ids_from_json(value: &Value) -> Vec<String> {
    value
        .get("mediaIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|media_id| media_id.as_str().map(str::to_string))
        .chain(
            value
                .get("media")
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|media| media.get("id").and_then(Value::as_str).map(str::to_string)),
        )
        .fold(Vec::new(), |mut ids, id| {
            if !ids.iter().any(|existing| existing == &id) {
                ids.push(id);
            }
            ids
        })
}
