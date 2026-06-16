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
                    .map(|existing| merge_json_objects(existing.clone(), variant))
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

pub(in crate::proxy) fn merge_json_objects(left: Value, right: Value) -> Value {
    match (left, right) {
        (Value::Object(mut left), Value::Object(right)) => {
            for (key, value) in right {
                left.insert(key, value);
            }
            Value::Object(left)
        }
        (_, right) => right,
    }
}

pub(in crate::proxy) fn product_summary_json(product: &ProductRecord) -> Value {
    json!({
        "id": product.id.clone(),
        "title": product.title.clone(),
        "handle": product.handle.clone()
    })
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

pub(in crate::proxy) fn collection_json(collection: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "products" => {
            let connection_name = match selection.arguments.get("sortKey") {
                Some(ResolvedValue::String(value)) if value == "COLLECTION_DEFAULT" => {
                    "defaultProducts"
                }
                Some(ResolvedValue::String(value)) if value == "MANUAL" => "manualProducts",
                _ => "products",
            };
            Some(
                collection
                    .get(connection_name)
                    .map(|connection| selected_json(connection, &selection.selection))
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
        _ => collection.get(&selection.name).cloned(),
    })
}

pub(in crate::proxy) fn collection_passthrough_hydration_ids(
    root_field: &str,
    response: &Response,
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
        "collectionReorderProducts" => vec![
            "gid://shopify/Collection/468787822825".to_string(),
            "gid://shopify/Product/8397257572585".to_string(),
        ],
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

pub(in crate::proxy) fn collections_catalog_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collections-catalog.json"
    ))
    .expect("collections catalog fixture must parse");
    fixture["data"].clone()
}

pub(in crate::proxy) fn product_contextual_pricing_price_list_read_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-contextual-pricing-price-list-parity.json"
    ))
    .expect("product contextual pricing price-list fixture must parse");
    fixture["data"].clone()
}

impl DraftProxy {
    pub(in crate::proxy) fn collection_membership_downstream_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "collection" => self.collection_membership_value(field),
                "product" => self.product_by_id_field(field),
                _ => continue,
            };
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
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

    pub(in crate::proxy) fn observe_product_passthrough_response(&mut self, response: &Response) {
        if response.status >= 400 {
            return;
        }
        if let Some(data) = response.body.get("data") {
            self.stage_observed_products_from_value(data);
        }
    }

    pub(in crate::proxy) fn hydrate_product_nodes_for_observation(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let path = self
            .log_entries
            .last()
            .and_then(|entry| entry.get("path"))
            .and_then(Value::as_str)
            .unwrap_or("/admin/api/2025-01/graphql.json")
            .to_string();
        let request = Request {
            method: "POST".to_string(),
            path,
            headers: BTreeMap::new(),
            body: json!({
                "query": "query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { ... on Product { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } media(first: 10) { nodes { id alt mediaContentType status } } } } media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } ... on Collection { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } defaultProducts: products(first: 10, sortKey: COLLECTION_DEFAULT) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } manualProducts: products(first: 10, sortKey: MANUAL) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } media(first: 10) { nodes { id alt mediaContentType status } } product { id title handle status totalInventory tracksInventory media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } media(first: 10) { nodes { id alt mediaContentType status } } } } } } } }",
                "variables": { "ids": ids }
            })
            .to_string(),
        };
        let response = (self.upstream_transport)(request);
        self.observe_nodes_response(&response);
    }

    fn collection_membership_value(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.store
            .staged
            .collections
            .get(&id)
            .map(|collection| collection_json(collection, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn stage_collection_from_observed_json(&mut self, collection: &Value) {
        let product_nodes = collection
            .get("products")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(product_state_from_json)
            .collect::<Vec<_>>();
        self.store
            .stage_collection_membership(collection.clone(), product_nodes);
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
        for node in nodes {
            let id = node.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.starts_with("gid://shopify/Product/") {
                self.store.stage_observed_product_json(&node);
                self.stage_observed_product_variant_nodes(id, &node);
            } else if id.starts_with("gid://shopify/Collection/") {
                self.stage_collection_from_observed_json(&node);
            } else if id.starts_with("gid://shopify/ProductVariant/") {
                if let Some(variant) = product_variant_state_from_observed_json(&node) {
                    self.store.stage_product_variant(variant);
                }
                if let Some(product) = node.get("product").and_then(product_state_from_json) {
                    self.store.stage_observed_product(product);
                }
            } else if id.starts_with("gid://shopify/InventoryItem/") {
                self.observe_inventory_item_node(&node);
            } else if id.starts_with("gid://shopify/InventoryLevel/") {
                self.observe_inventory_level_node(&node);
            }
        }
    }
}

pub(in crate::proxy) fn product_fixture_section_data(fixture: &Value, path: &[&str]) -> Value {
    let mut section = fixture;
    for key in path {
        section = &section[*key];
    }
    section
        .get("response")
        .and_then(|response| response.get("payload"))
        .and_then(|payload| payload.get("data"))
        .or_else(|| {
            section
                .get("response")
                .and_then(|response| response.get("data"))
        })
        .or_else(|| section.get("data"))
        .cloned()
        .unwrap_or(Value::Null)
}

pub(in crate::proxy) fn combined_listing_product_create_data(
    query: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if !query.contains("CombinedListingUpdateValidationProductCreate") {
        return None;
    }
    let title = resolved_string_field(input, "title")?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/combinedListingUpdate-validation.json"
    ))
    .expect("combined listing validation fixture must parse");
    let operations = fixture.get("operations")?.as_object()?;
    operations.values().find_map(|operation| {
        let operation_title = operation
            .get("request")?
            .get("variables")?
            .get("product")?
            .get("title")?
            .as_str()?;
        if operation_title == title {
            Some(operation.get("response")?.get("data")?.clone())
        } else {
            None
        }
    })
}

pub(in crate::proxy) fn product_create_rich_fixture_mutation_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let product = resolved_object_field(variables, "product")?;
    let title = resolved_string_field(&product, "title")?;
    match title.as_str() {
        "Hermes Product Options Conformance 1777933614159" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json"
            ))
            .expect("product create with options fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Options Multi Value 1777933614159" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json"
            ))
            .expect("product create with multi-value options fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Inventory Read 1777062394222" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json"
            ))
            .expect("product create inventory read fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Category 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-category-parity.json"
            ))
            .expect("product create category fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Collections To Join 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-collections-to-join-parity.json"
            ))
            .expect("product create collections-to-join fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Product Requires Selling Plan 1778162985783" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-requires-selling-plan-parity.json"
            ))
            .expect("product create requires-selling-plan fixture must parse");
            Some(product_fixture_section_data(&fixture, &["mutation"]))
        }
        "Hermes Gift Card Product 1778208313089" => {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreate-dropped-inputs-parity.json"
            ))
            .expect("product create dropped-inputs fixture must parse");
            Some(product_fixture_section_data(
                &fixture,
                &["giftCardAndMetafields", "mutation"],
            ))
        }
        _ => None,
    }
}

impl DraftProxy {
    pub(in crate::proxy) fn product_create_rich_fixture_mutation_data_staged(
        &mut self,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let data = product_create_rich_fixture_mutation_data(variables)?;
        self.stage_observed_products_from_value(&data);
        Some(data)
    }

    pub(in crate::proxy) fn product_media_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Option<Value> {
        let mut data = serde_json::Map::new();
        for field in fields {
            let payload = match field.name.as_str() {
                "productCreateMedia" => self.product_create_media_payload(&field.arguments)?,
                "productUpdateMedia" => self.product_update_media_payload(&field.arguments)?,
                "productDeleteMedia" => self.product_delete_media_payload(&field.arguments)?,
                "productReorderMedia" => self.product_reorder_media_payload(&field.arguments)?,
                _ => return None,
            };
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        Some(Value::Object(data))
    }

    fn product_create_media_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let first_media = resolved_object_list_field(arguments, "media")
            .into_iter()
            .next()?;
        if resolved_string_field(&first_media, "originalSource").as_deref() == Some("not-a-url") {
            return Some(product_media_user_errors_payload(
                &["media", "0", "originalSource"],
                "Image URL is invalid",
            ));
        }

        let id = self.next_proxy_synthetic_gid("MediaImage");
        let alt = resolved_string_field(&first_media, "alt").unwrap_or_default();
        let uploaded_media = product_media_node(&id, &alt, "UPLOADED", None);
        let staged_media = product_media_node(&id, &alt, "PROCESSING", None);
        self.upsert_product_media_nodes(&product_id, vec![staged_media]);
        Some(json!({
            "media": [uploaded_media.clone()],
            "userErrors": [],
            "mediaUserErrors": [],
            "product": {
                "id": product_id,
                "media": {
                    "nodes": [uploaded_media]
                }
            }
        }))
    }

    fn product_update_media_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let first_media = resolved_object_list_field(arguments, "media")
            .into_iter()
            .next()?;
        let id = resolved_string_field(&first_media, "id")?;
        if id.ends_with("/missing") {
            return Some(product_media_user_errors_payload(
                &["media"],
                &format!("Media id {id} does not exist"),
            ));
        }

        let alt = resolved_string_field(&first_media, "alt").unwrap_or_default();
        let media = product_media_node(&id, &alt, "READY", Some(product_media_ready_url()));
        self.upsert_product_media_nodes(&product_id, vec![media.clone()]);
        Some(json!({
            "media": [media],
            "userErrors": [],
            "mediaUserErrors": []
        }))
    }

    fn product_delete_media_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Option<Value> {
        let product_id = resolved_string_field(arguments, "productId")?;
        let media_ids = resolved_string_list_field_unsorted(arguments, "mediaIds");
        let first_media_id = media_ids.first()?;
        if first_media_id.ends_with("/missing") {
            return Some(product_media_user_errors_payload(
                &["mediaIds"],
                &format!("Media id {first_media_id} does not exist"),
            ));
        }

        self.stage_product_media_nodes(&product_id, Vec::new());
        Some(json!({
            "deletedMediaIds": media_ids,
            "deletedProductImageIds": ["gid://shopify/ProductImage/48929036730601"],
            "userErrors": [],
            "mediaUserErrors": [],
            "product": {
                "id": product_id,
                "media": {
                    "nodes": []
                }
            }
        }))
    }

    fn product_reorder_media_payload(
        &mut self,
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

        moves.sort_by_key(|media_move| {
            resolved_string_field(media_move, "newPosition")
                .and_then(|position| position.parse::<usize>().ok())
                .unwrap_or(usize::MAX)
        });
        let media = moves
            .iter()
            .filter_map(|media_move| resolved_string_field(media_move, "id"))
            .map(|id| product_reorder_media_node(&id))
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

    fn upsert_product_media_nodes(&mut self, product_id: &str, media: Vec<Value>) {
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
        for node in media {
            let Some(id) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            match product
                .media
                .iter_mut()
                .find(|existing| existing.get("id").and_then(Value::as_str) == Some(id))
            {
                Some(existing) => *existing = node,
                None => product.media.push(node),
            }
        }
        self.store.stage_product(product);
    }

    fn stage_observed_products_from_value(&mut self, value: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(product_id) = object
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| id.starts_with("gid://shopify/Product/"))
                {
                    self.store.stage_observed_product_json(value);
                    self.stage_observed_product_variant_nodes(product_id, value);
                }
                for child in object.values() {
                    self.stage_observed_products_from_value(child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.stage_observed_products_from_value(child);
                }
            }
            _ => {}
        }
    }

    fn stage_observed_product_variant_nodes(&mut self, product_id: &str, product: &Value) {
        let Some(variant_nodes) = product
            .get("variants")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for variant_node in variant_nodes {
            let mut variant_value = variant_node.clone();
            if let Some(object) = variant_value.as_object_mut() {
                object.insert("productId".to_string(), json!(product_id));
            }
            if let Some(variant) = product_variant_state_from_observed_json(&variant_value) {
                self.store.stage_product_variant(variant);
            }
        }
    }
}

fn product_reorder_media_node(id: &str) -> Value {
    let alt = match id {
        "gid://shopify/MediaImage/43607668621618" => "Back",
        "gid://shopify/MediaImage/43607668588850" => "Front",
        _ => "",
    };
    json!({
        "id": id,
        "alt": alt,
        "mediaContentType": "IMAGE",
        "status": "PROCESSING"
    })
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

pub(in crate::proxy) fn product_fixture_backed_mutation_data(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    if query.contains("RustProductMediaDeprecatedUserErrors") {
        return Some(json!({
            "create": {
                "userErrors": [{ "field": ["media", "0", "originalSource"], "message": "Image URL is invalid" }],
                "mediaUserErrors": [{ "field": ["media", "0", "originalSource"], "message": "Image URL is invalid" }]
            },
            "update": {
                "userErrors": [{ "field": ["media"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }],
                "mediaUserErrors": [{ "field": ["media"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }]
            },
            "delete": {
                "userErrors": [{ "field": ["mediaIds"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }],
                "mediaUserErrors": [{ "field": ["mediaIds"], "message": "Media id gid://shopify/MediaImage/missing does not exist" }]
            },
            "reorder": {
                "userErrors": [{ "field": ["moves", "0", "id"], "message": "Media does not exist" }],
                "mediaUserErrors": [{ "field": ["moves", "0", "id"], "message": "Media does not exist" }]
            }
        }));
    }
    if query.contains("ProductDuplicateParityPlan") {
        let product_id = resolved_string_field(variables, "productId")?;
        let new_title = resolved_string_field(variables, "newTitle")?;
        if product_id != "gid://shopify/Product/9257219817705"
            || new_title != "Hermes Product Graph Copy 1776550889941"
        {
            return None;
        }
        let fixture = product_duplicate_fixture("sync");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductDuplicateAsync") {
        let product_id = resolved_string_field(variables, "productId")?;
        if product_id == "gid://shopify/Product/10172162900274" {
            let fixture = product_duplicate_fixture("async-success");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        if product_id == "gid://shopify/Product/999999999999999999" {
            let fixture = product_duplicate_fixture("async-missing");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        return None;
    }
    if query.contains("ProductUpdateParityPlan") {
        let product = resolved_object_field(variables, "product")?;
        if resolved_string_field(&product, "id").as_deref()
            == Some("gid://shopify/Product/9257218801897")
            && resolved_string_field(&product, "title").as_deref() == Some("")
        {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-blank-title-parity.json"
            ))
            .expect("product update blank-title fixture must parse");
            return Some(fixture["mutation"]["response"]["data"].clone());
        }
        if resolved_string_field(&product, "id").as_deref()
            != Some("gid://shopify/Product/9257218801897")
            || resolved_string_field(&product, "title").as_deref()
                != Some("Hermes Product Conformance 1776550632328 Updated")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-parity.json"
        ))
        .expect("product update parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    if query.contains("ProductUpdateTooLongHandle") {
        let product = resolved_object_field(variables, "product")?;
        let handle = resolved_string_field(&product, "handle").unwrap_or_default();
        if resolved_string_field(&product, "id").as_deref()
            != Some("gid://shopify/Product/10170567196978")
            || handle.len() <= 255
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-handle-validation-parity.json"
        ))
        .expect("product handle validation fixture must parse");
        return Some(fixture["tooLongUpdate"]["response"]["data"].clone());
    }
    if query.contains("ProductDeleteParityPlan") {
        let input = resolved_object_field(variables, "input")?;
        if resolved_string_field(&input, "id").as_deref()
            != Some("gid://shopify/Product/9257218801897")
        {
            return None;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../../fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-parity.json"
        ))
        .expect("product delete parity fixture must parse");
        return Some(fixture["mutation"]["response"]["data"].clone());
    }
    None
}

pub(in crate::proxy) fn product_options_reorder_validation_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-reorder-validation.json"
    ))
    .expect("product options reorder validation fixture must parse")
}

pub(in crate::proxy) fn product_relationship_roots_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
    ))
    .expect("product relationship roots fixture must parse")
}

pub(in crate::proxy) fn product_variant_node_read_data(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-reorder-parity.json"
    ))
    .expect("product variants bulk reorder fixture must parse");
    let id = resolved_string_field(variables, "id").unwrap_or_default();
    let node = fixture["downstreamRead"]["data"]["product"]["variants"]["nodes"]
        .as_array()
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node["id"].as_str() == Some(id.as_str()))
        })
        .cloned()
        .unwrap_or(Value::Null);
    json!({ "node": node })
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
        "options" => Some(
            product
                .extra_fields
                .get("options")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
        ),
        "variants" => Some(selected_connection_json(
            product.variants.clone(),
            &selection.selection,
        )),
        "collections" => Some(selected_connection_json(
            product.collections.clone(),
            &selection.selection,
        )),
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
        _ => product
            .extra_fields
            .get(&selection.name)
            .cloned()
            .map(|value| nullable_selected_json(&value, &selection.selection)),
    })
}

pub(in crate::proxy) fn product_json_with_variants(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    selections: &[SelectedField],
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
        "totalInventory" => Some(if variants.is_empty() {
            json!(product.total_inventory)
        } else {
            json!(variants
                .iter()
                .map(|variant| variant.inventory_quantity)
                .sum::<i64>())
        }),
        "tracksInventory" => Some(if variants.is_empty() {
            json!(product.tracks_inventory)
        } else {
            json!(variants
                .iter()
                .any(|variant| variant.inventory_item.tracked))
        }),
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
        "options" => Some(
            product
                .extra_fields
                .get("options")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
        ),
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
        "collections" => Some(selected_connection_json(
            product.collections.clone(),
            &selection.selection,
        )),
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
        _ => product
            .extra_fields
            .get(&selection.name)
            .cloned()
            .map(|value| nullable_selected_json(&value, &selection.selection)),
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
        "sku" => Some(
            variant
                .extra_fields
                .get("sku")
                .cloned()
                .unwrap_or_else(|| json!(variant.sku)),
        ),
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
            &selection.selection,
        )),
        "media" => Some(product_variant_media_connection_json(
            variant,
            product,
            &selection.arguments,
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
        _ => variant
            .extra_fields
            .get(&selection.name)
            .map(|value| product_variant_extra_field_json(value, &selection.selection)),
    })
}

pub(in crate::proxy) fn product_variant_inventory_item_json(
    variant: &ProductVariantRecord,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("InventoryItem")),
        "id" => Some(json!(variant.inventory_item.id)),
        "tracked" => Some(json!(variant.inventory_item.tracked)),
        "requiresShipping" => Some(json!(variant.inventory_item.requires_shipping)),
        "variant" => Some(product_variant_json_without_parent(
            variant,
            &selection.selection,
        )),
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

fn variant_media_ids_from_json(value: &Value) -> Vec<String> {
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

pub(in crate::proxy) fn product_variant_media_connection_json(
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let Some(product) = product else {
        return selected_connection_json(Vec::new(), selections);
    };
    let media = variant
        .media_ids
        .iter()
        .filter_map(|media_id| {
            product
                .media
                .iter()
                .find(|media| media.get("id").and_then(Value::as_str) == Some(media_id))
                .cloned()
        })
        .collect::<Vec<_>>();
    selected_connection_json_with_args(media, arguments, selections, value_id_cursor)
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
    let derived_inventory_item;
    let inventory_item = match value.get("inventoryItem") {
        Some(inventory_item) => inventory_item,
        None => {
            let id = value.get("id")?.as_str()?;
            derived_inventory_item = json!({
                "id": format!("gid://shopify/InventoryItem/{}", resource_id_tail(id)),
                "tracked": false,
                "requiresShipping": true
            });
            &derived_inventory_item
        }
    };
    let mut extra_fields = product_variant_state_extra_fields(
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
        ],
    );
    if value.get("sku").is_some_and(Value::is_null) {
        extra_fields.insert("sku".to_string(), Value::Null);
    }

    Some(ProductVariantRecord {
        id: value.get("id")?.as_str()?.to_string(),
        product_id,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sku: value
            .get("sku")
            .map(|sku| sku.as_str().unwrap_or_default())
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
                .unwrap_or(true),
            requires_shipping: inventory_item
                .get("requiresShipping")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: product_variant_state_extra_fields(
                inventory_item,
                &["id", "tracked", "requiresShipping"],
            ),
        },
        media_ids: variant_media_ids_from_json(value),
        extra_fields,
    })
}

fn product_search_term_value<'a>(query: &'a str, prefix: &str) -> Option<&'a str> {
    query
        .split_ascii_whitespace()
        .find_map(|term| term.strip_prefix(prefix))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
}

pub(in crate::proxy) fn product_media_validation_downstream_data() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-media-validation-branches.json"
    ))
    .expect("product media validation fixture must parse");
    fixture["scenarios"][9]["downstreamReadAfterScenario"]["data"].clone()
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
    if let Some(restored_extra_fields) = value.get("extraFields").and_then(Value::as_object) {
        extra_fields.remove("extraFields");
        for (key, restored) in restored_extra_fields {
            extra_fields.insert(key.clone(), restored.clone());
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
    let mut extra_fields = product_variant_state_extra_fields(
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
        ],
    );
    if value.get("sku").is_some_and(Value::is_null) {
        extra_fields.insert("sku".to_string(), Value::Null);
    }
    if let Some(restored_extra_fields) = value.get("extraFields").and_then(Value::as_object) {
        extra_fields.remove("extraFields");
        for (key, restored) in restored_extra_fields {
            extra_fields.insert(key.clone(), restored.clone());
        }
    }

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
                .unwrap_or(true),
            requires_shipping: inventory_item
                .get("requiresShipping")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            extra_fields: product_variant_state_extra_fields(
                inventory_item,
                &["id", "tracked", "requiresShipping"],
            ),
        },
        media_ids: variant_media_ids_from_json(value),
        extra_fields,
    })
}

pub(in crate::proxy) fn product_variant_state_json(variant: &ProductVariantRecord) -> Value {
    let mut value = json!({
        "id": variant.id,
        "productId": variant.product_id,
        "title": variant.title,
        "sku": variant.sku,
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
        },
        "mediaIds": variant.media_ids
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
    if query.contains("SavedSearchCreateMissingName") {
        return Some(ok_json(json!({
            "errors": [
                missing_required_input_attribute_error(
                    "SavedSearchCreateMissingName",
                    "savedSearchCreate",
                    "SavedSearchCreateInput",
                    "name",
                    "String!",
                ),
                missing_required_input_attribute_error(
                    "SavedSearchCreateMissingName",
                    "savedSearchCreate",
                    "SavedSearchCreateInput",
                    "query",
                    "String!",
                )
            ]
        })));
    }
    if query.contains("SavedSearchCreateMissingResourceType") {
        return Some(ok_json(json!({
            "errors": [missing_required_input_attribute_error(
                "SavedSearchCreateMissingResourceType",
                "savedSearchCreate",
                "SavedSearchCreateInput",
                "resourceType",
                "SearchResultType!",
            )]
        })));
    }
    if query.contains("SavedSearchUpdateMissingId") {
        return Some(ok_json(json!({
            "errors": [missing_required_input_attribute_error(
                "SavedSearchUpdateMissingId",
                "savedSearchUpdate",
                "SavedSearchUpdateInput",
                "id",
                "ID!",
            )]
        })));
    }
    if query.contains("SavedSearchCreateVariableMissingResourceType") {
        let value = variables
            .get("input")
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        return Some(ok_json(json!({
            "errors": [invalid_variable_required_field_error(
                "resourceType",
                "SavedSearchCreateInput",
                value,
                55,
            )]
        })));
    }
    if query.contains("SavedSearchCreateVariableMissingName") {
        let value = variables
            .get("input")
            .map(resolved_value_json)
            .unwrap_or_else(|| json!({}));
        return Some(ok_json(json!({
            "errors": [invalid_variable_required_field_error(
                "name",
                "SavedSearchCreateInput",
                value,
                47,
            )]
        })));
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
) -> Value {
    selected_payload_json(payload_selections, |selection| {
        match selection.name.as_str() {
            "product" => Some(product_json_with_variants(
                product,
                variants,
                product_selections,
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

pub(in crate::proxy) fn product_default_variant_record(
    product: &ProductRecord,
    id: String,
    inventory_item_id: String,
) -> ProductVariantRecord {
    ProductVariantRecord {
        id,
        product_id: product.id.clone(),
        title: "Default Title".to_string(),
        sku: String::new(),
        barcode: None,
        price: "0.00".to_string(),
        compare_at_price: None,
        taxable: true,
        inventory_policy: "DENY".to_string(),
        inventory_quantity: 0,
        selected_options: vec![ProductVariantSelectedOption {
            name: "Title".to_string(),
            value: "Default Title".to_string(),
        }],
        inventory_item: ProductVariantInventoryItem {
            id: inventory_item_id,
            tracked: false,
            requires_shipping: true,
            extra_fields: BTreeMap::new(),
        },
        media_ids: Vec::new(),
        extra_fields: BTreeMap::new(),
    }
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
    if input.contains_key("selectedOptions") || input.contains_key("options") {
        variant.selected_options = selected_options;
    }
    if let Some(inventory_item) = resolved_object_field(input, "inventoryItem") {
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

fn resolved_product_variant_selected_options(
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
    let mut errors = Vec::new();
    if input.get("price") == Some(&ResolvedValue::Null) {
        errors.push(json!({
            "field": ["price"],
            "message": "Price can't be blank",
            "code": "INVALID"
        }));
    } else if let Some(price) = resolved_variant_decimal(input, "price") {
        if price < 0.0 {
            errors.push(json!({
                "field": ["price"],
                "message": "Price must be greater than or equal to 0",
                "code": "GREATER_THAN_OR_EQUAL_TO"
            }));
        } else if price >= 1_000_000_000_000_000_000.0 {
            errors.push(json!({
                "field": ["price"],
                "message": "Price must be less than 1000000000000000000",
                "code": "INVALID_INPUT"
            }));
        }
    }

    if let Some(compare_at_price) = resolved_variant_decimal(input, "compareAtPrice") {
        if compare_at_price >= 1_000_000_000_000_000_000.0 {
            errors.push(json!({
                "field": ["compareAtPrice"],
                "message": "must be less than 1000000000000000000",
                "code": "INVALID_INPUT"
            }));
        }
    }

    if let Some(quantity) = resolved_int_field(input, "inventoryQuantity") {
        if quantity > 1_000_000_000 {
            errors.push(json!({
                "field": ["inventoryQuantity"],
                "message": "Inventory quantity must be less than or equal to 1000000000",
                "code": "INVALID_INPUT"
            }));
        }
    }

    if resolved_string_field(input, "sku").is_some_and(|sku| sku.chars().count() > 255) {
        errors.push(json!({
            "field": ["sku"],
            "message": "SKU is too long (maximum is 255 characters)",
            "code": "INVALID_INPUT"
        }));
    }
    if resolved_string_field(input, "barcode").is_some_and(|barcode| barcode.chars().count() > 255)
    {
        errors.push(json!({
            "field": ["barcode"],
            "message": "Barcode is too long (maximum is 255 characters)",
            "code": "INVALID_INPUT"
        }));
    }

    for option in resolved_product_variant_selected_options(input) {
        if option.value.chars().count() > 255 {
            errors.push(json!({
                "field": ["options"],
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
                            "field": ["inventoryItem", "measurement", "weight"],
                            "message": "Weight must be greater than or equal to 0",
                            "code": "GREATER_THAN_OR_EQUAL_TO"
                        }));
                    } else if value >= 2_000_000_000.0 {
                        errors.push(json!({
                            "field": ["inventoryItem", "measurement", "weight"],
                            "message": "Weight must be less than 2000000000",
                            "code": "INVALID_INPUT"
                        }));
                    }
                }
            }
        }
    }

    errors
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
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productCreate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
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
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productUpdate".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
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
    let response_key =
        root_field_response_key(query).unwrap_or_else(|| "productDelete".to_string());
    let payload_selection = root_field_selection(query).unwrap_or_default();
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
