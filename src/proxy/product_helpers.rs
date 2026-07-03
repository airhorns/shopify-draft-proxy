use super::*;
use crate::graphql::RawArgumentValue;

mod collections;
mod product_tail;
mod saved_search;

pub(in crate::proxy) use self::collections::*;
pub(in crate::proxy) use self::saved_search::*;

const PRODUCT_STATUS_BASE_VALUES: &[&str] = &["ACTIVE", "ARCHIVED", "DRAFT"];

// The batched node-hydrate query the proxy forwards to observe pre-existing
// products / variants / collections in LiveHybrid. Shared verbatim with the
// conformance capture scripts so re-recorded cassettes match byte-for-byte.
pub(in crate::proxy) const PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY: &str = include_str!(
    "../../config/parity-requests/products/products-hydrate-nodes-observation.graphql"
);

pub(in crate::proxy) const COLLECTION_REORDER_PRODUCTS_COLLECTION_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/products/collectionReorderProducts-collection-hydrate.graphql"
);

// The generic observation query above does not select product `options`, which the
// productOptionsReorder graph needs. This options-aware node hydrate selects the
// option/optionValue graph (and variants) and is forwarded only by the reorder
// owner-hydrate path. Kept as a shared `.graphql` doc so re-recorded cassettes match
// the emitted forward byte-for-byte.
pub(in crate::proxy) const PRODUCT_OPTIONS_HYDRATE_NODES_QUERY: &str =
    include_str!("../../config/parity-requests/products/product-options-hydrate-nodes.graphql");

// Publication-membership hydrate forwarded the first time the local publication
// engine publishes a publishable resource (product / collection) it has never
// seen. It reads the resource's title/status and the set of publications it is
// already published on (e.g. the default Online Store), so a pre-existing
// resource's membership is discovered by reading upstream rather than injected
// via `/__meta/seed`. Shared verbatim with the cassette so the forward matches
// byte-for-byte.
pub(in crate::proxy) const PUBLICATION_RESOURCE_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/products/publication-resource-hydrate-nodes.graphql"
);

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
    if product.extra_fields.contains_key("productPublications") {
        return true;
    }
    let resource_nodes = product
        .extra_fields
        .get("resourcePublicationsV2")
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array);
    if resource_nodes.is_some_and(|nodes| !nodes.is_empty()) {
        return true;
    }
    product
        .extra_fields
        .get("resourcePublicationsCount")
        .and_then(|count| count.get("count"))
        .and_then(Value::as_u64)
        == Some(0)
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
    let entries = product_visible_publication_entries(product);
    selected_typed_connection(
        &entries,
        selections,
        |entry, selections| product_publication_connection_node_json(product, entry, selections),
        |entry| entry.publication_id.clone(),
        |selections| selected_json(&empty_page_info(), selections),
    )
}

fn resource_publication_connection_json(
    product: &ProductRecord,
    typename: &str,
    selections: &[SelectedField],
) -> Value {
    let entries = product_visible_publication_entries(product);
    selected_typed_connection(
        &entries,
        selections,
        |entry, selections| {
            resource_publication_connection_node_json(product, entry, typename, selections)
        },
        |entry| entry.publication_id.clone(),
        |selections| selected_json(&empty_page_info(), selections),
    )
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
                .and_then(resolved_value_string)
                .unwrap_or_default();
            Some(Value::Bool(product_is_published_on_publication(
                product,
                &publication_id,
            )))
        }
        "availablePublicationsCount" | "resourcePublicationsCount" => product
            .extra_fields
            .get(&selection.name)
            .cloned()
            .map(|value| selected_json(&value, &selection.selection))
            .or_else(|| {
                Some(selected_count_json(
                    product_visible_publication_entries(product).len(),
                    &selection.selection,
                ))
            }),
        "publications" => product
            .extra_fields
            .get("publications")
            .cloned()
            .map(|value| selected_json(&value, &selection.selection))
            .or_else(|| {
                Some(product_publication_connection_json(
                    product,
                    &selection.selection,
                ))
            }),
        "productPublications" => Some(product_publication_connection_json(
            product,
            &selection.selection,
        )),
        "resourcePublications" => Some(resource_publication_connection_json(
            product,
            "ResourcePublication",
            &selection.selection,
        )),
        "resourcePublicationsV2" => product
            .extra_fields
            .get("resourcePublicationsV2")
            .cloned()
            .map(|value| selected_json(&value, &selection.selection))
            .or_else(|| {
                Some(resource_publication_connection_json(
                    product,
                    "ResourcePublicationV2",
                    &selection.selection,
                ))
            }),
        "resourcePublicationOnCurrentPublication" => Some(Value::Null),
        _ => None,
    }
}

/// The canonical `Publication` record the local publication engine stages and
/// serves. A publication's backing `Channel` shares the publication's numeric
/// id suffix and name, so both are derived rather than recorded per scenario.
pub(in crate::proxy) fn publication_record_json(id: &str, name: &str, auto_publish: bool) -> Value {
    let suffix = resource_id_path_tail(id);
    let channel_id = shopify_gid("Channel", suffix);
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
                source_errors.push(user_error_omit_code(
                    vec![
                        "media".to_string(),
                        index.to_string(),
                        "originalSource".to_string(),
                    ],
                    "Image URL is invalid",
                    Some("INVALID"),
                ));
                continue;
            }
            let media_content_type = resolved_string_field(item, "mediaContentType")
                .unwrap_or_else(|| infer_product_media_content_type(&original_source).to_string());
            let id = self.next_proxy_synthetic_gid(product_media_gid_type(&media_content_type));
            let alt = resolved_string_field(item, "alt").unwrap_or_default();
            created.push(product_media_node_with_type(
                &id,
                &alt,
                &media_content_type,
                "UPLOADED",
                None,
                Some(&original_source),
            ));
            staged.push(product_media_node_with_type(
                &id,
                &alt,
                &media_content_type,
                if media_content_type == "IMAGE" {
                    "PROCESSING"
                } else {
                    "UPLOADED"
                },
                None,
                Some(&original_source),
            ));
        }

        if source_errors.is_empty() && !self.ensure_product_for_media(request, &product_id) {
            return Some(json!({
                "media": Value::Null,
                "userErrors": [product_does_not_exist_error("productId")],
                "mediaUserErrors": [product_does_not_exist_error("productId")],
                "product": Value::Null,
            }));
        }

        let mut product_media_nodes = self.product_known_media(&product_id);
        product_media_nodes.extend(created.clone());
        if !staged.is_empty() {
            self.append_product_media_nodes(&product_id, staged);
        }

        Some(json!({
            "media": created.clone(),
            "userErrors": source_errors.clone(),
            "mediaUserErrors": source_errors,
            "product": {
                "id": product_id,
                "media": { "nodes": product_media_nodes }
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
        let missing_media_ids: Vec<String> = media_inputs
            .iter()
            .filter_map(|item| resolved_string_field(item, "id"))
            .filter(|id| !media_nodes_contain(&overlay, id))
            .collect();
        if !missing_media_ids.is_empty() {
            let error = media_missing_ids_error("media", &missing_media_ids);
            return Some(json!({
                "media": Value::Null,
                "userErrors": [error.clone()],
                "mediaUserErrors": [error],
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
                if node.get("mediaContentType").and_then(Value::as_str) == Some("IMAGE") {
                    // Preserve an observed ProductImage id so downstream deletes can
                    // still derive `deletedProductImageIds` from the asset.
                    match node.get("image").and_then(|image| image.get("id")) {
                        Some(image_id) => {
                            node["image"] = json!({ "id": image_id, "url": ready_url })
                        }
                        None => node["image"] = json!({ "url": ready_url }),
                    }
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
        let media_ids = list_string_field(arguments, "mediaIds");

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
        let missing_media_ids: Vec<String> = media_ids
            .iter()
            .filter(|id| !media_nodes_contain(&known, id))
            .cloned()
            .collect();
        if !missing_media_ids.is_empty() {
            let error = media_missing_ids_error("mediaIds", &missing_media_ids);
            return Some(json!({
                "deletedMediaIds": Value::Null,
                "deletedProductImageIds": Value::Null,
                "userErrors": [error.clone()],
                "mediaUserErrors": [error],
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

        // Reorder operates on media that already exists on the product. If the
        // product has not been staged locally yet, hydrate it from upstream so
        // existing media (and their alt text) are observed rather than guessed.
        if !self.ensure_product_for_media(request, &product_id) {
            return Some(product_media_user_errors_payload(
                ["id"],
                "Product does not exist",
                "PRODUCT_DOES_NOT_EXIST",
            ));
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
        let timestamp = default_product_timestamp();
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
        let known = self
            .store
            .product_staged_or_base(product_id)
            .and_then(|product| {
                product
                    .media
                    .into_iter()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(id))
            });
        let mut node = known.unwrap_or_else(|| {
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
            product_media_node_with_type(id, &alt, "IMAGE", "PROCESSING", None, None)
        });
        node["status"] = json!("PROCESSING");
        node
    }
}

fn product_media_node_with_type(
    id: &str,
    alt: &str,
    media_content_type: &str,
    status: &str,
    image_url: Option<&str>,
    original_source: Option<&str>,
) -> Value {
    let image = image_url
        .map(|url| json!({ "url": url }))
        .unwrap_or(Value::Null);
    let typename = product_media_typename(media_content_type);
    let mut node = json!({
        "__typename": typename,
        "id": id,
        "alt": alt,
        "mediaContentType": media_content_type,
        "status": status,
        "preview": {
            "image": image.clone()
        }
    });
    if media_content_type == "IMAGE" {
        node["image"] = image;
    } else if matches!(media_content_type, "VIDEO" | "MODEL_3D") {
        if let Some(source) = original_source {
            node["originalSource"] = json!({ "url": source });
            node["sources"] = json!([{ "url": source }]);
        }
    }
    node
}

fn product_media_typename(media_content_type: &str) -> &'static str {
    match media_content_type {
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "VIDEO" => "Video",
        _ => "MediaImage",
    }
}

fn product_media_gid_type(media_content_type: &str) -> &'static str {
    match media_content_type {
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "VIDEO" => "Video",
        _ => "MediaImage",
    }
}

fn product_media_ready_url() -> &'static str {
    "https://cdn.shopify.com/s/files/1/0637/5541/9881/files/png.png?v=1776550664"
}

fn infer_product_media_content_type(original_source: &str) -> &'static str {
    if product_media_source_is_external_video(original_source) {
        return "EXTERNAL_VIDEO";
    }
    match file_extension(original_source).as_str() {
        "mp4" | "mov" | "m4v" | "webm" => "VIDEO",
        "glb" | "gltf" | "usdz" => "MODEL_3D",
        _ => "IMAGE",
    }
}

fn product_media_source_is_external_video(original_source: &str) -> bool {
    let source = original_source.to_ascii_lowercase();
    source.contains("youtube.com/") || source.contains("youtu.be/") || source.contains("vimeo.com/")
}

fn product_media_user_errors_payload(
    field: impl Into<crate::proxy::schema_validation::UserErrorField>,
    message: &str,
    code: &str,
) -> Value {
    let errors = json!([user_error_omit_code(field, message, Some(code))]);
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
    user_error_omit_code(
        [field],
        "Product does not exist",
        Some("PRODUCT_DOES_NOT_EXIST"),
    )
}

fn media_missing_ids_error(field: &str, ids: &[String]) -> Value {
    let joined_ids = ids.join(",");
    let message = if ids.len() == 1 {
        format!("Media id {joined_ids} does not exist")
    } else {
        format!("Media ids {joined_ids} do not exist")
    };
    user_error_omit_code([field], &message, Some("MEDIA_DOES_NOT_EXIST"))
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
        "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
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
        "userErrors" => selected_user_errors_field(user_errors.as_slice(), selection),
        _ => None,
    })
}

pub(in crate::proxy) fn default_product_timestamp() -> String {
    "2024-01-01T00:00:00.000Z".to_string()
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

pub(in crate::proxy) fn product_json_with_currency(
    product: &ProductRecord,
    selections: &[SelectedField],
    currency_code: &str,
) -> Value {
    product_json_with_variants_and_currency(product, &[], selections, currency_code)
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

pub(in crate::proxy) fn product_matches_search_query(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    query: &str,
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let tokens = product_search_tokens(query);
    if tokens.is_empty() {
        return true;
    }
    let mut parser = ProductSearchParser::new(tokens);
    parser
        .parse()
        .map(|expression| expression.matches(product, variants))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProductSearchToken {
    Term { value: String, quoted: bool },
    LParen,
    RParen,
    Minus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProductSearchExpression {
    Term(ProductSearchTerm),
    Not(Box<ProductSearchExpression>),
    And(Vec<ProductSearchExpression>),
    Or(Vec<ProductSearchExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductSearchTerm {
    field: Option<String>,
    value: String,
}

struct ProductSearchParser {
    tokens: Vec<ProductSearchToken>,
    index: usize,
}

impl ProductSearchParser {
    fn new(tokens: Vec<ProductSearchToken>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse(&mut self) -> Option<ProductSearchExpression> {
        let expression = self.parse_or()?;
        Some(expression)
    }

    fn parse_or(&mut self) -> Option<ProductSearchExpression> {
        let mut expressions = vec![self.parse_and()?];
        while self.consume_operator("OR") {
            let Some(right) = self.parse_and() else {
                break;
            };
            expressions.push(right);
        }
        Some(if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            ProductSearchExpression::Or(expressions)
        })
    }

    fn parse_and(&mut self) -> Option<ProductSearchExpression> {
        let mut expressions = Vec::new();
        while self.index < self.tokens.len() {
            if self.peek_rparen() || self.peek_operator("OR") {
                break;
            }
            self.consume_operator("AND");
            if self.peek_rparen() || self.peek_operator("OR") {
                break;
            }
            if let Some(expression) = self.parse_unary() {
                expressions.push(expression);
            } else {
                break;
            }
        }
        Some(if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            ProductSearchExpression::And(expressions)
        })
    }

    fn parse_unary(&mut self) -> Option<ProductSearchExpression> {
        if matches!(self.tokens.get(self.index), Some(ProductSearchToken::Minus)) {
            self.index += 1;
            return self
                .parse_unary()
                .map(|expression| ProductSearchExpression::Not(Box::new(expression)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<ProductSearchExpression> {
        match self.tokens.get(self.index).cloned()? {
            ProductSearchToken::Term { value, quoted } => {
                self.index += 1;
                Some(ProductSearchExpression::Term(ProductSearchTerm::new(
                    value, quoted,
                )))
            }
            ProductSearchToken::LParen => {
                self.index += 1;
                let expression = self.parse_or()?;
                if self.peek_rparen() {
                    self.index += 1;
                }
                Some(expression)
            }
            ProductSearchToken::RParen | ProductSearchToken::Minus => None,
        }
    }

    fn peek_rparen(&self) -> bool {
        matches!(
            self.tokens.get(self.index),
            Some(ProductSearchToken::RParen)
        )
    }

    fn peek_operator(&self, operator: &str) -> bool {
        matches!(
            self.tokens.get(self.index),
            Some(ProductSearchToken::Term { value, quoted: false })
                if value.eq_ignore_ascii_case(operator)
        )
    }

    fn consume_operator(&mut self, operator: &str) -> bool {
        if self.peek_operator(operator) {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

impl ProductSearchExpression {
    fn matches(&self, product: &ProductRecord, variants: &[ProductVariantRecord]) -> bool {
        match self {
            ProductSearchExpression::Term(term) => term.matches(product, variants),
            ProductSearchExpression::Not(expression) => !expression.matches(product, variants),
            ProductSearchExpression::And(expressions) => expressions
                .iter()
                .all(|expression| expression.matches(product, variants)),
            ProductSearchExpression::Or(expressions) => expressions
                .iter()
                .any(|expression| expression.matches(product, variants)),
        }
    }
}

impl ProductSearchTerm {
    fn new(value: String, quoted: bool) -> Self {
        if !quoted {
            if let Some((field, value)) = value.split_once(':') {
                if !field.is_empty() && !value.is_empty() {
                    return Self {
                        field: Some(field.to_ascii_lowercase()),
                        value: value.trim_matches('"').trim_matches('\'').to_string(),
                    };
                }
            }
        }
        Self { field: None, value }
    }

    fn matches(&self, product: &ProductRecord, variants: &[ProductVariantRecord]) -> bool {
        let value = self.value.trim();
        if value.is_empty() {
            return true;
        }
        match self.field.as_deref() {
            Some("status") => product.status.eq_ignore_ascii_case(value),
            Some("vendor") => product_search_string_matches(&product.vendor, value),
            Some("product_type") => product_search_string_matches(&product.product_type, value),
            Some("title") => product_search_string_matches(&product.title, value),
            Some("tag") => product_matches_search_tag(product, value),
            Some("tag_not") => !product_matches_search_tag(product, value),
            Some("sku") => product_matches_search_sku(product, variants, value),
            Some("published_status") => product_matches_published_status(product, value),
            Some("created_at") => product_matches_date_query(&product.created_at, value),
            Some("updated_at") => product_matches_date_query(&product.updated_at, value),
            Some(_) => false,
            None => product_matches_free_text(product, variants, value),
        }
    }
}

fn product_search_tokens(query: &str) -> Vec<ProductSearchToken> {
    let mut tokens = Vec::new();
    let chars = query.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ch if ch.is_whitespace() => {
                index += 1;
            }
            '(' => {
                tokens.push(ProductSearchToken::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(ProductSearchToken::RParen);
                index += 1;
            }
            '-' => {
                tokens.push(ProductSearchToken::Minus);
                index += 1;
            }
            '"' | '\'' => {
                let quote = chars[index];
                index += 1;
                let mut value = String::new();
                while index < chars.len() && chars[index] != quote {
                    value.push(chars[index]);
                    index += 1;
                }
                if index < chars.len() {
                    index += 1;
                }
                tokens.push(ProductSearchToken::Term {
                    value,
                    quoted: true,
                });
            }
            _ => {
                let mut value = String::new();
                while index < chars.len()
                    && !chars[index].is_whitespace()
                    && chars[index] != '('
                    && chars[index] != ')'
                {
                    if chars[index] == '"' || chars[index] == '\'' {
                        let quote = chars[index];
                        index += 1;
                        while index < chars.len() && chars[index] != quote {
                            value.push(chars[index]);
                            index += 1;
                        }
                        if index < chars.len() {
                            index += 1;
                        }
                    } else {
                        value.push(chars[index]);
                        index += 1;
                    }
                }
                if !value.is_empty() {
                    tokens.push(ProductSearchToken::Term {
                        value,
                        quoted: false,
                    });
                }
            }
        }
    }
    tokens
}

fn product_matches_free_text(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    product_search_string_matches(&product.title, value)
        || product_search_string_matches(&product.handle, value)
        || product_search_string_matches(&product.vendor, value)
        || product_search_string_matches(&product.product_type, value)
        || product_matches_search_tag(product, value)
        || product_matches_search_sku(product, variants, value)
}

fn product_matches_search_tag(product: &ProductRecord, value: &str) -> bool {
    product
        .tags
        .iter()
        .any(|tag| product_search_string_matches(tag, value))
}

fn product_matches_search_sku(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    variants
        .iter()
        .any(|variant| product_search_string_matches(&variant.sku, value))
        || product.variants.iter().any(|variant| {
            variant
                .get("sku")
                .and_then(Value::as_str)
                .is_some_and(|sku| product_search_string_matches(sku, value))
        })
}

fn product_search_string_matches(actual: &str, query_value: &str) -> bool {
    let actual = actual.to_ascii_lowercase();
    let query_value = query_value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if query_value.is_empty() {
        return true;
    }
    if let Some(prefix) = query_value.strip_suffix('*') {
        return actual
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&query_value)
}

fn product_matches_published_status(product: &ProductRecord, value: &str) -> bool {
    let published = product_is_published(product);
    match value.to_ascii_lowercase().as_str() {
        "published" => published,
        "unpublished" => !published,
        "any" => true,
        _ => false,
    }
}

fn product_is_published(product: &ProductRecord) -> bool {
    product
        .extra_fields
        .get("publishedAt")
        .is_some_and(|published_at| !published_at.is_null())
        || !product_visible_publication_entries(product).is_empty()
}

fn product_matches_date_query(actual: &str, query_value: &str) -> bool {
    let (operator, expected) = product_search_comparator(query_value);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}

fn product_search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
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
    product_variant_state_from_json_parts(
        value,
        product_id,
        ProductVariantInventoryItemMode::Optional,
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
    )
}

#[derive(Clone, Copy)]
enum ProductVariantInventoryItemMode {
    Optional,
    Required,
}

fn product_variant_state_from_json_parts(
    value: &Value,
    product_id: String,
    inventory_item_mode: ProductVariantInventoryItemMode,
    extra_field_exclusions: &[&str],
) -> Option<ProductVariantRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let inventory_item = value.get("inventoryItem");
    let inventory_item_id = match inventory_item_mode {
        ProductVariantInventoryItemMode::Optional => inventory_item
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| shopify_gid("InventoryItem", resource_id_tail(&id))),
        ProductVariantInventoryItemMode::Required => {
            inventory_item?.get("id")?.as_str()?.to_string()
        }
    };
    Some(ProductVariantRecord {
        id,
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
            id: inventory_item_id,
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
        extra_fields: product_variant_state_extra_fields(value, extra_field_exclusions),
    })
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
        .unwrap_or_else(default_product_timestamp);
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
    let product_id = value.get("productId")?.as_str()?.to_string();
    product_variant_state_from_json_parts(
        value,
        product_id,
        ProductVariantInventoryItemMode::Required,
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
    )
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

pub(in crate::proxy) fn rust_state_dump_path_exists(dump: &Value, path: &str) -> bool {
    path.split('.')
        .try_fold(dump, |current, segment| current.get(segment))
        .is_some()
}

pub(in crate::proxy) fn product_mutation_payload_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    payload_selections: &[SelectedField],
    product_selections: &[SelectedField],
    currency_code: &str,
    shop: Option<&Value>,
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "product" => Some(product_json_with_variants_and_currency(
                product,
                variants,
                product_selections,
                currency_code,
            )),
            "shop" => shop.map(|shop| selected_json(shop, &selection.selection)),
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
    let mut variant = empty_product_variant_record(product_id, id, inventory_item_id);
    variant.inventory_item.tracked = true;
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

pub(in crate::proxy) fn product_variant_input_user_errors_with_prefix(
    input: &BTreeMap<String, ResolvedValue>,
    field_prefix: &[String],
) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.get("price") == Some(&ResolvedValue::Null) {
        errors.push(user_error(
            prefixed_error_field(field_prefix, &["price"]),
            "Price can't be blank",
            Some("INVALID"),
        ));
    } else if let Some(price) = resolved_f64_path(input, &["price"]) {
        if price < 0.0 {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["price"]),
                "Price must be greater than or equal to 0",
                Some("GREATER_THAN_OR_EQUAL_TO"),
            ));
        } else if price >= 1_000_000_000_000_000_000.0 {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["price"]),
                "Price must be less than 1000000000000000000",
                Some("INVALID_INPUT"),
            ));
        }
    }

    if let Some(compare_at_price) = resolved_f64_path(input, &["compareAtPrice"]) {
        if compare_at_price >= 1_000_000_000_000_000_000.0 {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["compareAtPrice"]),
                "must be less than 1000000000000000000",
                Some("INVALID_INPUT"),
            ));
        }
    }

    if let Some(quantity) = resolved_int_field(input, "inventoryQuantity") {
        if quantity > 1_000_000_000 {
            errors.push(user_error(
                prefixed_error_field(field_prefix, &["inventoryQuantity"]),
                "Inventory quantity must be less than or equal to 1000000000",
                Some("INVALID_INPUT"),
            ));
        }
    }
    for quantity in resolved_object_list_field(input, "inventoryQuantities") {
        if let Some(available_quantity) = resolved_int_field(&quantity, "availableQuantity") {
            if available_quantity > 1_000_000_000 {
                errors.push(user_error(
                    prefixed_error_field(field_prefix, &["inventoryQuantities"]),
                    "Inventory quantity must be less than or equal to 1000000000",
                    Some("INVALID_INPUT"),
                ));
                break;
            }
        }
    }

    if resolved_string_field(input, "sku").is_some_and(|sku| sku.chars().count() > 255) {
        errors.push(user_error(
            prefixed_error_field(field_prefix, &["sku"]),
            "SKU is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
        ));
    }
    if resolved_string_field(input, "barcode").is_some_and(|barcode| barcode.chars().count() > 255)
    {
        errors.push(user_error(
            prefixed_error_field(field_prefix, &["barcode"]),
            "Barcode is too long (maximum is 255 characters)",
            Some("INVALID_INPUT"),
        ));
    }

    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if resolved_string_field(&inventory_item, "sku")
            .is_some_and(|sku| sku.chars().count() > 255)
        {
            let bulk_field = !field_prefix.is_empty();
            errors.push(user_error(
                if bulk_field {
                    prefixed_error_field(field_prefix, &[])
                } else {
                    prefixed_error_field(field_prefix, &["inventoryItem", "sku"])
                },
                "SKU is too long (maximum is 255 characters)",
                Some("INVALID_INPUT"),
            ));
            if bulk_field {
                errors.push(user_error(
                    prefixed_error_field(field_prefix, &[]),
                    "is too long (maximum is 255 characters)",
                    None,
                ));
            }
        }
    }

    for (option_index, option) in resolved_product_variant_selected_options(input)
        .into_iter()
        .enumerate()
    {
        if option.value.chars().count() > 255 {
            errors.push(user_error(
                if input.contains_key("optionValues") {
                    prefixed_error_field(
                        field_prefix,
                        &["optionValues", &option_index.to_string(), "name"],
                    )
                } else {
                    prefixed_error_field(field_prefix, &["options"])
                },
                "Option value name is too long",
                Some("INVALID_INPUT"),
            ));
            break;
        }
    }

    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
        if let Some(measurement) = resolved_object_field(&inventory_item, "measurement") {
            if let Some(weight) = resolved_object_field(&measurement, "weight") {
                if let Some(value) = resolved_f64_path(&weight, &["value"]) {
                    if value < 0.0 {
                        errors.push(user_error(
                            variant_weight_error_field(field_prefix),
                            "Weight must be greater than or equal to 0",
                            Some("GREATER_THAN_OR_EQUAL_TO"),
                        ));
                    } else if value >= 2_000_000_000.0 {
                        errors.push(user_error(
                            variant_weight_error_field(field_prefix),
                            "Weight must be less than 2000000000",
                            Some("INVALID_INPUT"),
                        ));
                    }
                }
                if let Some(unit) = resolved_string_field(&weight, "unit") {
                    if !matches!(unit.as_str(), "KILOGRAMS" | "GRAMS" | "POUNDS" | "OUNCES") {
                        errors.push(user_error(
                            variant_weight_error_field(field_prefix),
                            "Weight unit must be one of KILOGRAMS, GRAMS, POUNDS, OUNCES",
                            Some("INVALID_INPUT"),
                        ));
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
    shop: &Value,
    errors: Vec<Value>,
) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| ("productCreate".to_string(), Vec::new()));
    ok_json(json!({
        "data": {
            response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                "product" => Some(Value::Null),
                "shop" => Some(selected_json(shop, &selection.selection)),
                "userErrors" => selected_user_errors_field(errors.as_slice(), selection),
                _ => None,
            })
        }
    }))
}

pub(in crate::proxy) fn product_delete_payload_json(
    deleted_product_id: &str,
    shop: &Value,
    payload_selections: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedProductId" => Some(json!(deleted_product_id)),
            "shop" => Some(selected_json(shop, &selection.selection)),
            "userErrors" => Some(json!([])),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn product_delete_async_operation_payload(
    operation_id: &str,
    shop: &Value,
    payload_selections: &[SelectedField],
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedProductId" => Some(Value::Null),
            "productDeleteOperation" => Some(selected_payload_json(
                &selection.selection,
                |operation_selection| match operation_selection.name.as_str() {
                    "id" => Some(json!(operation_id)),
                    "status" => Some(json!("CREATED")),
                    "deletedProductId" => Some(Value::Null),
                    "userErrors" => Some(json!([])),
                    _ => None,
                },
            )),
            "shop" => Some(selected_json(shop, &selection.selection)),
            "userErrors" => Some(json!([])),
            _ => None,
        }
    })
}

pub(in crate::proxy) fn product_delete_async_duplicate_payload(
    shop: &Value,
    payload_selections: &[SelectedField],
) -> Value {
    let user_errors = [json!({
        "field": null,
        "message": "Another operation already in progress. Please wait until current one is finished."
    })];
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "deletedProductId" => Some(Value::Null),
            "productDeleteOperation" => Some(Value::Null),
            "shop" => Some(selected_json(shop, &selection.selection)),
            "userErrors" => selected_user_errors_field(&user_errors, selection),
            _ => None,
        }
    })
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
            let location = crate::proxy::schema_validation::inline_argument_value_location(
                query,
                field,
                context.argument_name,
            );
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
        "errors": [invalid_variable_error_envelope(
            message,
            location.unwrap_or(SourceLocation { line: 1, column: 1 }),
            resolved_value_json(value),
            json!([{ "path": path, "explanation": explanation }]),
        )]
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
    product_missing_product_response(query, "productUpdate", "product", None)
}

pub(in crate::proxy) fn product_delete_missing_product(query: &str, shop: &Value) -> Response {
    product_missing_product_response(query, "productDelete", "deletedProductId", Some(shop))
}

fn product_missing_product_response(
    query: &str,
    default_response_key: &str,
    null_payload_field: &str,
    shop: Option<&Value>,
) -> Response {
    let (response_key, payload_selection) = primary_root_field(query, &BTreeMap::new())
        .map(|field| (field.response_key, field.selection))
        .unwrap_or_else(|| (default_response_key.to_string(), Vec::new()));
    let user_errors = [user_error(
        ["id"],
        "Product does not exist",
        Some("NOT_FOUND"),
    )];
    ok_json(json!({
        "data": {
            response_key: selected_payload_json(&payload_selection, |selection| match selection.name.as_str() {
                field if field == null_payload_field => Some(Value::Null),
                "shop" => shop.map(|shop| selected_json(shop, &selection.selection)),
                "userErrors" => selected_user_errors_field(&user_errors, selection),
                _ => None,
            })
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
    let message = format!("Variable ${variable_name} of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)");
    ok_json(json!({
        "errors": [invalid_variable_error_envelope(
            message,
            SourceLocation { line: 2, column: 37 },
            value,
            json!([{ "path": ["id"], "explanation": "Expected value to not be null" }]),
        )]
    }))
}

pub(in crate::proxy) fn product_variant_media_user_error(
    field: &[&str],
    message: &str,
    code: &str,
) -> Value {
    user_error(field, message, Some(code))
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
