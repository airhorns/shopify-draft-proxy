use super::media::media_file_record_from_node;
use super::*;
use base64::Engine as _;

const OWNER_METAFIELD_HYDRATE_QUERY: &str = "query OwnerMetafieldsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Collection { id title handle metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Customer { id displayName email metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Order { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Company { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }";

impl DraftProxy {
    // metafieldsSet/metafieldsDelete read their `metafields` list from the
    // resolved root-field arguments so inline-document forms work, not only the
    // `$metafields` variable form (matches the Gleam reference, which reads from
    // the field arguments). Falls back to top-level variables for safety.
    pub(in crate::proxy) fn owner_metafields_set(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "metafieldsSet".to_string());
        let inputs = metafields_mutation_inputs(query, variables, "metafieldsSet");
        let fallback_reference_ids = if inputs.len() <= 25 {
            self.hydrate_metafield_reference_ids(
                request,
                metafields_set_reference_values(&inputs),
                metafields_set_product_owner_ids(&inputs),
            )
        } else {
            BTreeSet::new()
        };
        let mut user_errors = metafields_set_input_errors(&inputs, |id| {
            self.metafield_reference_exists(id) || fallback_reference_ids.contains(id)
        });
        user_errors.extend(metafields_set_definition_user_errors(
            &inputs,
            &self.store.staged.metafield_definitions,
        ));
        if !user_errors.is_empty() {
            let metafields = if inputs.len() > 25 {
                Value::Null
            } else {
                json!([])
            };
            let payload = json!({"metafields": metafields, "userErrors": user_errors});
            return MutationOutcome::response(ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            ));
        }
        self.hydrate_owner_metafield_ids(
            request,
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "ownerId"))
                .collect(),
        );
        let mut metafields = Vec::new();
        let mut staged_owner_ids = Vec::new();
        for input in inputs {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let metafield_type = resolved_string_field(&input, "type")
                .or_else(|| {
                    self.store
                        .staged
                        .metafield_definitions
                        .get(&(namespace.clone(), key.clone()))
                        .filter(|definition| {
                            definition["ownerType"].as_str() == Some(owner_type_from_gid(&owner_id))
                        })
                        .and_then(|definition| definition["type"]["name"].as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&input, "value").unwrap_or_default();
            let index = self
                .store
                .staged
                .owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + metafields.len()
                + 1;
            let existing = self.owner_metafield(&owner_id, &namespace, &key);
            let id = existing
                .as_ref()
                .and_then(|metafield| metafield.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index));
            let metafield = if let Some(mut record) =
                custom_data_metafield_type_matrix_record(&namespace, &key)
            {
                record["owner"] = owner_reference_from_gid(&owner_id);
                record
            } else {
                let compare_digest = existing
                    .as_ref()
                    .filter(|metafield| {
                        metafield.get("value").and_then(Value::as_str) == Some(value.as_str())
                    })
                    .and_then(|metafield| metafield.get("compareDigest"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("local-metafield-digest-{index}"));
                let timestamp = owner_metafield_timestamp(index as u64);
                let created_at = existing
                    .as_ref()
                    .and_then(|metafield| metafield.get("createdAt"))
                    .and_then(Value::as_str)
                    .unwrap_or(&timestamp);
                let updated_at = existing
                    .as_ref()
                    .filter(|metafield| {
                        metafield.get("value").and_then(Value::as_str) == Some(value.as_str())
                    })
                    .and_then(|metafield| metafield.get("updatedAt"))
                    .and_then(Value::as_str)
                    .unwrap_or(&timestamp);
                json!({
                    "id": id,
                    "namespace": namespace,
                    "key": key,
                    "type": metafield_type,
                    "value": normalize_metafield_value_string(&metafield_type, &value),
                    "jsonValue": metafield_json_value(&metafield_type, &value),
                    "compareDigest": compare_digest,
                    "createdAt": created_at,
                    "updatedAt": updated_at,
                    "ownerType": owner_type_from_gid(&owner_id),
                    "owner": owner_reference_from_gid(&owner_id),
                })
            };
            self.store.staged.deleted_owner_metafields.remove(&(
                owner_id.clone(),
                namespace.clone(),
                key.clone(),
            ));
            let owner_metafields = self
                .store
                .staged
                .owner_metafields
                .entry(owner_id.clone())
                .or_default();
            if let Some(existing) = owner_metafields.iter_mut().find(|existing| {
                existing.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                    && existing.get("key").and_then(Value::as_str) == Some(key.as_str())
            }) {
                *existing = metafield.clone();
            } else {
                owner_metafields.push(metafield.clone());
            }
            if !staged_owner_ids.iter().any(|id| id == &owner_id) {
                staged_owner_ids.push(owner_id);
            }
            metafields.push(metafield);
        }
        let payload = json!({"metafields": metafields, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("metafieldsSet", "products", staged_owner_ids),
        )
    }

    pub(in crate::proxy) fn owner_metafields_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "metafieldsDelete".to_string());
        let inputs = metafields_mutation_inputs(query, variables, "metafieldsDelete");
        // A delete targeting another app's reserved namespace is not permitted;
        // Shopify rejects the whole batch before deleting anything.
        if inputs.iter().any(|input| {
            app_namespace_belongs_to_other_app(&canonical_app_metafield_namespace(
                resolved_string_field(input, "namespace").as_deref(),
            ))
        }) {
            let payload = json!({
                "deletedMetafields": [],
                "userErrors": [{
                    "field": ["metafields"],
                    "message": "Access to this namespace and key on Metafields for this resource type is not allowed."
                }]
            });
            return MutationOutcome::response(ok_json(
                json!({"data": {response_key: selected_json(&payload, &payload_selection)}}),
            ));
        }
        self.hydrate_owner_metafield_ids(
            request,
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "ownerId"))
                .collect(),
        );
        let mut deleted = Vec::new();
        let mut staged_owner_ids = Vec::new();
        for input in inputs {
            let owner_id = resolved_string_field(&input, "ownerId").unwrap_or_default();
            let namespace = canonical_app_metafield_namespace(
                resolved_string_field(&input, "namespace").as_deref(),
            );
            let key = resolved_string_field(&input, "key").unwrap_or_default();
            let owner_metafields = self
                .store
                .staged
                .owner_metafields
                .entry(owner_id.clone())
                .or_default();
            let before_len = owner_metafields.len();
            owner_metafields.retain(|existing| {
                existing.get("namespace").and_then(Value::as_str) != Some(namespace.as_str())
                    || existing.get("key").and_then(Value::as_str) != Some(key.as_str())
            });
            if before_len == owner_metafields.len() {
                deleted.push(Value::Null);
            } else {
                self.store.staged.deleted_owner_metafields.insert((
                    owner_id.clone(),
                    namespace.clone(),
                    key.clone(),
                ));
                deleted
                    .push(json!({"ownerId": owner_id.clone(), "namespace": namespace, "key": key}));
            }
            if !staged_owner_ids.iter().any(|id| id == &owner_id) {
                staged_owner_ids.push(owner_id);
            }
        }
        let payload = json!({"deletedMetafields": deleted, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("metafieldsDelete", "products", staged_owner_ids),
        )
    }

    pub(in crate::proxy) fn should_handle_owner_metafields_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> bool {
        let fields = root_fields(query, variables).unwrap_or_default();
        let mut has_non_product_owner_read = false;
        let mut needs_live_product_hydration = false;
        for field in fields {
            if !Self::owner_field_selects_metafields_at_root(&field.name, &field.selection) {
                continue;
            }
            if self.config.read_mode == ReadMode::LiveHybrid {
                let owner_id = self.owner_field_id(&field, variables);
                let cold = self.owner_needs_metafield_hydration(&field.name, &owner_id);
                // A cold (unstaged) owner that also selects sub-resources the
                // metafields overlay cannot synthesize (addresses, orders, events, ...)
                // must forward the whole read upstream as a passthrough rather than be
                // answered with a metafields-only projection that silently drops them.
                if cold
                    && !Self::owner_metafields_read_selection_is_metafields_only(&field.selection)
                {
                    continue;
                }
            }
            match field.name.as_str() {
                "collection" | "customer" | "order" | "company" => {
                    has_non_product_owner_read = true;
                }
                "product" | "productVariant" if self.config.read_mode == ReadMode::LiveHybrid => {
                    let owner_id = self.owner_field_id(&field, variables);
                    if self.owner_needs_metafield_hydration(&field.name, &owner_id) {
                        needs_live_product_hydration = true;
                    }
                }
                _ => {}
            }
        }
        has_non_product_owner_read || needs_live_product_hydration
    }

    /// True when an owner read selects only fields the metafields overlay can synthesize
    /// for a cold (unstaged) owner: `id`, `__typename`, `metafield`, `metafields`. Any other
    /// field (addresses, orders, events, ...) cannot be projected from an empty base, so the
    /// read must instead forward upstream as a full passthrough.
    fn owner_metafields_read_selection_is_metafields_only(selections: &[SelectedField]) -> bool {
        selections.iter().all(|selection| {
            matches!(
                selection.name.as_str(),
                "id" | "__typename" | "metafield" | "metafields"
            )
        })
    }

    pub(in crate::proxy) fn owner_metafields_read(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        let fields = root_fields(query, variables).unwrap_or_default();
        self.hydrate_owner_metafield_read_fields(request, &fields, variables);
        for field in fields {
            if !matches!(
                field.name.as_str(),
                "product" | "productVariant" | "collection" | "customer" | "order" | "company"
            ) {
                continue;
            }
            let owner = self.owner_metafield_owner_json(&field, variables);
            data.insert(field.response_key, owner);
        }
        ok_json(json!({"data": Value::Object(data)}))
    }

    fn hydrate_owner_metafield_read_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
        variables: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let ids = fields
            .iter()
            .filter(|field| {
                Self::owner_field_selects_metafields_at_root(&field.name, &field.selection)
            })
            .flat_map(|field| {
                let owner_id = self.owner_field_id(field, variables);
                let mut ids = Vec::new();
                if self.owner_needs_metafield_hydration(&field.name, &owner_id) {
                    ids.push(owner_id.clone());
                }
                if field.name == "product" {
                    ids.extend(self.owner_variant_ids_for_hydration(&field.selection, &owner_id));
                }
                ids
            })
            .collect::<Vec<_>>();
        self.hydrate_owner_metafield_ids(request, ids);
    }

    fn hydrate_owner_metafield_ids(&mut self, request: &Request, ids: Vec<String>) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut ids = ids
            .into_iter()
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": OWNER_METAFIELD_HYDRATE_QUERY,
                "operationName": "OwnerMetafieldsHydrateNodes",
                "variables": { "ids": ids },
            }),
        );
        if response.status >= 400 {
            return;
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            for node in nodes {
                self.stage_observed_owner_metafield_node(node);
            }
        }
    }

    fn hydrate_metafield_reference_ids(
        &mut self,
        request: &Request,
        ids: Vec<String>,
        product_owner_ids: BTreeSet<String>,
    ) -> BTreeSet<String> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return BTreeSet::new();
        }
        let mut ids = ids
            .into_iter()
            .filter(|id| !id.is_empty() && !self.metafield_reference_exists(id))
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return BTreeSet::new();
        }

        let mut product_domain_ids = Vec::new();
        let mut generic_ids = Vec::new();
        for id in ids {
            match shopify_gid_resource_type(&id) {
                Some("Product" | "ProductVariant" | "Collection") => product_domain_ids.push(id),
                _ => generic_ids.push(id),
            }
        }
        let mut fallback_reference_ids = BTreeSet::new();
        if !product_domain_ids.is_empty() {
            let response = self.upstream_post(
                request,
                json!({
                    "query": PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY,
                    "operationName": "ProductsHydrateNodes",
                    "variables": { "ids": product_domain_ids.clone() }
                }),
            );
            if response.status >= 400 {
                fallback_reference_ids.extend(product_domain_ids.iter().filter_map(|id| {
                    metafield_product_domain_reference_fallback(id, &product_owner_ids)
                }));
            } else {
                self.observe_nodes_response(&response);
            }
        }
        if generic_ids.is_empty() {
            return fallback_reference_ids;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": "query MetafieldReferenceHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { id __typename } }",
                "operationName": "MetafieldReferenceHydrateNodes",
                "variables": { "ids": generic_ids },
            }),
        );
        if response.status >= 400 {
            return fallback_reference_ids;
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            for node in nodes {
                self.stage_metafield_reference_node(node);
            }
        }
        fallback_reference_ids
    }

    fn stage_metafield_reference_node(&mut self, node: &Value) {
        let Some(id) = node
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        self.store.staged.metafield_reference_ids.insert(id.clone());
        match shopify_gid_resource_type(&id) {
            Some("Product") => self.store.stage_observed_product_json(node),
            Some("ProductVariant") => {
                if let Some(variant) = product_variant_state_from_observed_json(node) {
                    self.store.stage_product_variant(variant);
                }
            }
            Some("Collection") => {
                self.store
                    .staged
                    .collections
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Customer") => {
                self.store
                    .staged
                    .customers
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Order") => {
                self.store
                    .staged
                    .orders
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Company") => {
                self.store
                    .staged
                    .b2b_companies
                    .entry(id)
                    .or_insert_with(|| node.clone());
            }
            Some("Metaobject") => {
                if !self.store.staged.metaobjects.is_tombstoned(&id) {
                    self.store
                        .staged
                        .metaobjects
                        .entry(id)
                        .or_insert_with(|| node.clone());
                }
            }
            Some("MediaImage" | "Video" | "ExternalVideo" | "Model3d" | "GenericFile") => {
                if let Some(record) = media_file_record_from_node(node) {
                    self.store.staged.media_files.entry(id).or_insert(record);
                }
            }
            _ => {}
        }
    }

    fn metafield_reference_exists(&self, id: &str) -> bool {
        if self.store.staged.metafield_reference_ids.contains(id) {
            return true;
        }
        match shopify_gid_resource_type(id) {
            Some("Product") => self.store.product_by_id(id).is_some(),
            Some("ProductVariant") => self.store.product_variant_by_id(id).is_some(),
            Some("Collection") => self.store.collection_by_id(id).is_some(),
            Some("Customer") => {
                self.store.staged.customers.contains_key(id)
                    && !self.store.staged.customers.is_tombstoned(id)
            }
            Some("Order") => {
                self.store.staged.orders.contains_key(id)
                    && !self.store.staged.orders.is_tombstoned(id)
            }
            Some("Company") => self.store.staged.b2b_companies.contains_key(id),
            Some("Metaobject") => {
                self.store.staged.metaobjects.contains_key(id)
                    && !self.store.staged.metaobjects.is_tombstoned(id)
            }
            Some("MediaImage" | "Video" | "ExternalVideo" | "Model3d" | "GenericFile") => {
                self.store.staged.media_files.contains_key(id)
                    && !self.store.staged.media_files.is_tombstoned(id)
            }
            _ => false,
        }
    }

    fn owner_needs_metafield_hydration(&self, root_field: &str, owner_id: &str) -> bool {
        match root_field {
            "product" => self.store.product_by_id(owner_id).is_none(),
            "productVariant" => self.store.product_variant_by_id(owner_id).is_none(),
            "collection" => !self.store.staged.collections.contains_key(owner_id),
            "customer" => !self.store.staged.customers.contains_key(owner_id),
            "order" => !self.store.staged.orders.contains_key(owner_id),
            "company" => !self.store.staged.b2b_companies.contains_key(owner_id),
            _ => false,
        }
    }

    fn stage_observed_owner_metafield_node(&mut self, node: &Value) {
        let Some(owner_id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        match shopify_gid_resource_type(&owner_id) {
            Some("Product") => self.store.stage_observed_product_json(node),
            Some("ProductVariant") => {
                if let Some(variant) = product_variant_state_from_observed_json(node)
                    .or_else(|| owner_product_variant_state_from_observed_json(node))
                {
                    self.store.stage_product_variant(variant);
                }
                if let Some(product) = node.get("product") {
                    self.store.stage_observed_product_json(product);
                }
            }
            Some("Collection") => {
                self.store
                    .staged
                    .collections
                    .insert(owner_id.clone(), node.clone());
            }
            Some("Customer") => {
                self.store
                    .staged
                    .customers
                    .insert(owner_id.clone(), node.clone());
            }
            Some("Order") => {
                self.store
                    .staged
                    .orders
                    .insert(owner_id.clone(), node.clone());
            }
            Some("Company") => {
                self.store
                    .staged
                    .b2b_companies
                    .insert(owner_id.clone(), node.clone());
            }
            _ => {}
        }
        self.stage_observed_owner_metafields(&owner_id, node);
    }

    fn owner_variant_ids_for_hydration(
        &self,
        selections: &[SelectedField],
        product_id: &str,
    ) -> Vec<String> {
        if !selections.iter().any(|selection| {
            selection.name == "variants"
                && Self::owner_field_selects_metafields(&selection.selection)
        }) {
            return Vec::new();
        }
        self.store
            .product_variants_for_product(product_id)
            .into_iter()
            .map(|variant| variant.id)
            .filter(|variant_id| self.owner_needs_metafield_hydration("productVariant", variant_id))
            .collect()
    }

    fn stage_observed_owner_metafields(&mut self, owner_id: &str, node: &Value) {
        let mut records = node
            .get("metafields")
            .and_then(|connection| connection.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(page_info) = node
            .get("metafields")
            .and_then(|connection| connection.get("pageInfo"))
        {
            apply_metafield_connection_cursors(&mut records, page_info);
        }
        for value in node
            .as_object()
            .into_iter()
            .flat_map(|object| object.values())
        {
            if value.get("namespace").and_then(Value::as_str).is_some()
                && value.get("key").and_then(Value::as_str).is_some()
                && value.get("id").and_then(Value::as_str).is_some()
            {
                records.push(value.clone());
            }
        }
        for record in records {
            self.upsert_owner_metafield_record(owner_id, record);
        }
    }

    fn upsert_owner_metafield_record(&mut self, owner_id: &str, mut record: Value) {
        let Some(namespace) = record
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some(key) = record
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if self.store.staged.deleted_owner_metafields.contains(&(
            owner_id.to_string(),
            namespace.clone(),
            key.clone(),
        )) {
            return;
        }
        record["owner"] = owner_reference_from_gid(owner_id);
        // Metafields not backed by a definition return `definition: null`; hydration
        // and metafieldsSet inputs never carry one, so default it so singular
        // `metafield(namespace:, key:) { definition }` reads emit null, not undefined.
        if record.get("definition").is_none() {
            record["definition"] = Value::Null;
        }
        let owner_metafields = self
            .store
            .staged
            .owner_metafields
            .entry(owner_id.to_string())
            .or_default();
        if let Some(existing) = owner_metafields.iter_mut().find(|existing| {
            existing.get("namespace").and_then(Value::as_str) == Some(namespace.as_str())
                && existing.get("key").and_then(Value::as_str) == Some(key.as_str())
        }) {
            if record.get("__cursor").is_none() {
                if let Some(cursor) = existing.get("__cursor").cloned() {
                    record["__cursor"] = cursor;
                }
            }
            *existing = record;
        } else {
            owner_metafields.push(record);
        }
    }

    /// Stage the `metafields` array on a product-variant create/update input into
    /// the owner-metafield overlay keyed by the variant GID, mirroring how
    /// `metafieldsSet` records owner metafields. This lets a follow-up
    /// `variants { nodes { metafield(namespace:, key:) } }` read resolve the
    /// metafield through the same overlay path used for products.
    pub(super) fn stage_input_variant_metafields(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        for metafield in resolved_object_list_field(input, "metafields") {
            let Some(namespace) = resolved_string_field(&metafield, "namespace") else {
                continue;
            };
            let Some(key) = resolved_string_field(&metafield, "key") else {
                continue;
            };
            let value = resolved_string_field(&metafield, "value").unwrap_or_default();
            let metafield_type = resolved_string_field(&metafield, "type")
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let index = self
                .store
                .staged
                .owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + 1;
            let timestamp = owner_metafield_timestamp(index as u64);
            let record = json!({
                "id": format!("gid://shopify/Metafield/{index}"),
                "namespace": namespace,
                "key": key,
                "type": metafield_type,
                "value": normalize_metafield_value_string(&metafield_type, &value),
                "jsonValue": metafield_json_value(&metafield_type, &value),
                "compareDigest": format!("local-metafield-digest-{index}"),
                "createdAt": timestamp,
                "updatedAt": timestamp,
                "ownerType": owner_type_from_gid(owner_id),
            });
            self.upsert_owner_metafield_record(owner_id, record);
        }
    }

    fn owner_record_json_for_read(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        match root_field {
            "product" => {
                let product = self.store.product_by_id(owner_id)?;
                let variants = self.store.product_variants_for_product(owner_id);
                let base = product_json_with_variants_and_currency(
                    product,
                    &variants,
                    selections,
                    &self.store.shop_currency_code(),
                );
                Some(
                    self.owner_metafield_overlay_owner_json_with_product_variants(
                        root_field,
                        owner_id,
                        selections,
                        &product.variants,
                        base,
                    ),
                )
            }
            "productVariant" => {
                let variant = self.store.product_variant_by_id(owner_id)?;
                let base = product_variant_json(
                    variant,
                    self.store.product_by_id(&variant.product_id),
                    selections,
                );
                Some(
                    self.owner_metafield_overlay_owner_json(root_field, owner_id, selections, base),
                )
            }
            "collection" => self.store.staged.collections.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            "customer" => self.store.staged.customers.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            "order" => self.store.staged.orders.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            "company" => self.store.staged.b2b_companies.get(owner_id).map(|record| {
                self.owner_metafield_overlay_owner_json(
                    root_field,
                    owner_id,
                    selections,
                    selected_json(record, selections),
                )
            }),
            _ => None,
        }
    }

    fn owner_metafield_owner_json(
        &self,
        field: &RootFieldSelection,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let owner_id = self.owner_field_id(field, variables);
        self.owner_record_json_for_read(&field.name, &owner_id, &field.selection)
            .unwrap_or_else(|| {
                self.minimal_owner_json_for_read(&field.name, &owner_id, &field.selection)
            })
    }

    pub(super) fn minimal_owner_json_for_read(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
    ) -> Value {
        self.owner_metafield_overlay_owner_json(root_field, owner_id, selections, json!({}))
    }

    pub(super) fn owner_metafield_overlay_owner_json(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        base: Value,
    ) -> Value {
        self.owner_metafield_overlay_owner_json_with_product_variants(
            root_field,
            owner_id,
            selections,
            &[],
            base,
        )
    }

    pub(super) fn owner_metafield_overlay_owner_json_with_product_variants(
        &self,
        root_field: &str,
        owner_id: &str,
        selections: &[SelectedField],
        fallback_product_variants: &[Value],
        base: Value,
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "__typename" => Some(json!(owner_typename_from_root(root_field))),
            "id" => Some(json!(owner_id)),
            "metafield" => Some(self.selected_owner_metafield_overlay(owner_id, selection, &base)),
            "metafields" => {
                Some(self.selected_owner_metafields_connection_overlay(owner_id, selection, &base))
            }
            "variants"
                if root_field == "product"
                    && Self::owner_field_selects_metafields(&selection.selection) =>
            {
                Some(self.selected_product_variants_with_metafields(
                    owner_id,
                    fallback_product_variants,
                    selection,
                ))
            }
            _ => base
                .get(selection.response_key.as_str())
                .or_else(|| base.get(selection.name.as_str()))
                .cloned(),
        })
    }

    fn selected_product_variants_with_metafields(
        &self,
        product_id: &str,
        fallback_variants: &[Value],
        selection: &SelectedField,
    ) -> Value {
        #[derive(Clone)]
        enum VariantSource {
            Record(Box<ProductVariantRecord>),
            Fallback(Value),
        }
        #[derive(Clone)]
        struct VariantEntry {
            id: String,
            source: VariantSource,
        }

        let normalized_variants = self.store.product_variants_for_product(product_id);
        let normalized_ids = normalized_variants
            .iter()
            .map(|variant| variant.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut entries = fallback_variants
            .iter()
            .filter_map(|variant| {
                let id = variant.get("id").and_then(Value::as_str)?;
                (!normalized_ids.contains(id)).then(|| VariantEntry {
                    id: id.to_string(),
                    source: VariantSource::Fallback(variant.clone()),
                })
            })
            .collect::<Vec<_>>();
        entries.extend(normalized_variants.into_iter().map(|variant| VariantEntry {
            id: variant.id.clone(),
            source: VariantSource::Record(Box::new(variant)),
        }));

        let (entries, page_info) =
            connection_window(&entries, &selection.arguments, |entry| entry.id.clone());
        let node_selection = nested_selected_fields(&selection.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&selection.selection, &["edges", "node"]);
        let page_info_selection = nested_selected_fields(&selection.selection, &["pageInfo"]);
        let render_variant =
            |entry: &VariantEntry, selections: &[SelectedField]| match &entry.source {
                VariantSource::Record(variant) => {
                    let base = product_variant_json(
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        selections,
                    );
                    self.owner_metafield_overlay_owner_json(
                        "productVariant",
                        &variant.id,
                        selections,
                        base,
                    )
                }
                VariantSource::Fallback(variant) => {
                    let base = selected_json(variant, selections);
                    self.owner_metafield_overlay_owner_json(
                        "productVariant",
                        &entry.id,
                        selections,
                        base,
                    )
                }
            };
        let mut connection = serde_json::Map::new();
        for selected in &selection.selection {
            let value = match selected.name.as_str() {
                "nodes" => Some(Value::Array(
                    entries
                        .iter()
                        .map(|entry| render_variant(entry, &node_selection))
                        .collect(),
                )),
                "edges" => Some(Value::Array(
                    entries
                        .iter()
                        .map(|entry| {
                            json!({
                                "cursor": entry.id,
                                "node": render_variant(entry, &edge_node_selection)
                            })
                        })
                        .collect(),
                )),
                "pageInfo" => Some(selected_json(&page_info, &page_info_selection)),
                _ => None,
            };
            if let Some(value) = value {
                connection.insert(selected.response_key.clone(), value);
            }
        }
        Value::Object(connection)
    }

    fn owner_field_selects_metafields_at_root(
        root_field: &str,
        selections: &[SelectedField],
    ) -> bool {
        selections.iter().any(|selection| {
            matches!(selection.name.as_str(), "metafield" | "metafields")
                || (root_field == "product"
                    && selection.name == "variants"
                    && Self::owner_field_selects_metafields(&selection.selection))
        })
    }

    pub(super) fn owner_field_selects_direct_metafields(selections: &[SelectedField]) -> bool {
        selections
            .iter()
            .any(|selection| matches!(selection.name.as_str(), "metafield" | "metafields"))
    }

    fn owner_field_selects_metafields(selections: &[SelectedField]) -> bool {
        selections.iter().any(|selection| {
            matches!(selection.name.as_str(), "metafield" | "metafields")
                || Self::owner_field_selects_metafields(&selection.selection)
        })
    }

    fn owner_field_id(
        &self,
        field: &RootFieldSelection,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> String {
        field
            .arguments
            .get("id")
            .and_then(resolved_value_string)
            .or_else(|| resolved_string_arg(variables, "id"))
            .or_else(|| resolved_string_arg(variables, "productId"))
            .or_else(|| resolved_string_arg(variables, "variantId"))
            .or_else(|| resolved_string_arg(variables, "collectionId"))
            .or_else(|| resolved_string_arg(variables, "customerId"))
            .or_else(|| resolved_string_arg(variables, "orderId"))
            .or_else(|| resolved_string_arg(variables, "companyId"))
            .unwrap_or_default()
    }

    fn selected_owner_metafield(&self, owner_id: &str, selection: &SelectedField) -> Value {
        let namespace =
            resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        self.owner_metafield(owner_id, &namespace, &key)
            .map(|metafield| selected_json(&metafield, &selection.selection))
            .unwrap_or(Value::Null)
    }

    fn selected_owner_metafield_overlay(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        base: &Value,
    ) -> Value {
        let namespace =
            resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        if self.owner_metafield_has_local_effect(owner_id, &namespace, &key) {
            return self.selected_owner_metafield(owner_id, selection);
        }
        base.get(selection.response_key.as_str())
            .or_else(|| base.get(selection.name.as_str()))
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn selected_owner_metafields_connection(
        &self,
        owner_id: &str,
        selection: &SelectedField,
    ) -> Value {
        let namespace = resolved_string_field(&selection.arguments, "namespace");
        let mut records = self.owner_metafields(owner_id, namespace.as_deref());

        // Relay pagination over the owner's metafields (stored id-ascending, which
        // mirrors Shopify's default metafield ordering). `after` drops everything up
        // to and including the cursor record; `first` truncates and drives
        // hasNextPage so chained `metafields(first:n, after:)` reads page correctly.
        let mut has_previous_page = false;
        if let Some(after) = resolved_string_field(&selection.arguments, "after") {
            if let Some(index) = records
                .iter()
                .position(|record| metafield_cursor(record).as_deref() == Some(after.as_str()))
            {
                records = records.split_off(index + 1);
                has_previous_page = true;
            }
        }
        let total_after_cursor = records.len();
        let mut has_next_page = false;
        if let Some(first) = resolved_int_field(&selection.arguments, "first") {
            if first >= 0 {
                let limit = first as usize;
                has_next_page = total_after_cursor > limit;
                records.truncate(limit);
            }
        }

        let node_selection = nested_selected_fields(&selection.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&selection.selection, &["edges", "node"]);
        let nodes = records
            .iter()
            .map(|metafield| selected_json(metafield, &node_selection))
            .collect::<Vec<_>>();
        let edges = records
            .iter()
            .map(|metafield| {
                let cursor = metafield_cursor(metafield).unwrap_or_default();
                json!({
                    "cursor": cursor,
                    "node": selected_json(metafield, &edge_node_selection)
                })
            })
            .collect::<Vec<_>>();
        let start_cursor = records.first().and_then(metafield_cursor);
        let end_cursor = records.last().and_then(metafield_cursor);
        let connection = json!({
            "nodes": nodes,
            "edges": edges,
            "pageInfo": metafield_connection_page_info(
                start_cursor,
                end_cursor,
                has_next_page,
                has_previous_page
            )
        });
        selected_json(&connection, &selection.selection)
    }

    fn selected_owner_metafields_connection_overlay(
        &self,
        owner_id: &str,
        selection: &SelectedField,
        base: &Value,
    ) -> Value {
        if !self.owner_has_metafield_local_effects(owner_id) {
            if let Some(base_value) = base
                .get(selection.response_key.as_str())
                .or_else(|| base.get(selection.name.as_str()))
            {
                return base_value.clone();
            }
        }
        self.selected_owner_metafields_connection(owner_id, selection)
    }

    fn owner_metafield(&self, owner_id: &str, namespace: &str, key: &str) -> Option<Value> {
        if self.store.staged.deleted_owner_metafields.contains(&(
            owner_id.to_string(),
            namespace.to_string(),
            key.to_string(),
        )) {
            return None;
        }
        self.store
            .staged
            .owner_metafields
            .get(owner_id)?
            .iter()
            .find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    && metafield.get("key").and_then(Value::as_str) == Some(key)
            })
            .cloned()
    }

    fn owner_metafield_has_local_effect(&self, owner_id: &str, namespace: &str, key: &str) -> bool {
        self.store
            .staged
            .owner_metafields
            .get(owner_id)
            .is_some_and(|metafields| {
                metafields.iter().any(|metafield| {
                    metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                        && metafield.get("key").and_then(Value::as_str) == Some(key)
                })
            })
            || self.store.staged.deleted_owner_metafields.contains(&(
                owner_id.to_string(),
                namespace.to_string(),
                key.to_string(),
            ))
    }

    fn owner_has_metafield_local_effects(&self, owner_id: &str) -> bool {
        self.store
            .staged
            .owner_metafields
            .get(owner_id)
            .is_some_and(|metafields| !metafields.is_empty())
            || self
                .store
                .staged
                .deleted_owner_metafields
                .iter()
                .any(|(deleted_owner_id, _, _)| deleted_owner_id == owner_id)
    }

    fn owner_metafields(&self, owner_id: &str, namespace: Option<&str>) -> Vec<Value> {
        self.store
            .staged
            .owner_metafields
            .get(owner_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|metafield| {
                let metafield_namespace = metafield.get("namespace").and_then(Value::as_str);
                let metafield_key = metafield.get("key").and_then(Value::as_str);
                namespace.is_none_or(|namespace| metafield_namespace == Some(namespace))
                    && !matches!(
                        (metafield_namespace, metafield_key),
                        (Some(namespace), Some(key))
                            if self.store.staged.deleted_owner_metafields.contains(&(
                                owner_id.to_string(),
                                namespace.to_string(),
                                key.to_string()
                            ))
                    )
            })
            .collect()
    }

    /// Stage product metafields supplied through a `metafields` create/update input so that
    /// downstream `metafield`/`metafields` reads resolve them on the owning product.
    pub(super) fn stage_owner_metafields_from_input(
        &mut self,
        owner_id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) {
        for metafield_input in resolved_object_list_field(input, "metafields") {
            let namespace =
                resolved_string_field(&metafield_input, "namespace").unwrap_or_default();
            let key = resolved_string_field(&metafield_input, "key").unwrap_or_default();
            if namespace.is_empty() && key.is_empty() {
                continue;
            }
            let metafield_type = resolved_string_field(&metafield_input, "type")
                .unwrap_or_else(|| "single_line_text_field".to_string());
            let value = resolved_string_field(&metafield_input, "value").unwrap_or_default();
            let index = self
                .store
                .staged
                .owner_metafields
                .values()
                .map(Vec::len)
                .sum::<usize>()
                + 1;
            let timestamp = owner_metafield_timestamp(index as u64);
            let metafield = json!({
                "id": format!("gid://shopify/Metafield/{index}"),
                "namespace": namespace,
                "key": key,
                "type": metafield_type,
                "value": normalize_metafield_value_string(&metafield_type, &value),
                "jsonValue": metafield_json_value(&metafield_type, &value),
                "compareDigest": format!("local-metafield-digest-{index}"),
                "createdAt": timestamp,
                "updatedAt": timestamp,
                "ownerType": owner_type_from_gid(owner_id),
                "owner": owner_reference_from_gid(owner_id),
            });
            self.store.staged.deleted_owner_metafields.remove(&(
                owner_id.to_string(),
                namespace.clone(),
                key.clone(),
            ));
            self.store
                .staged
                .owner_metafields
                .entry(owner_id.to_string())
                .or_default()
                .push(metafield);
        }
    }
}

fn owner_reference_from_gid(owner_id: &str) -> Value {
    json!({
        "__typename": owner_typename_from_gid(owner_id),
        "id": owner_id
    })
}

fn metafields_set_product_owner_ids(
    inputs: &[BTreeMap<String, ResolvedValue>],
) -> BTreeSet<String> {
    inputs
        .iter()
        .filter_map(|input| resolved_string_field(input, "ownerId"))
        .filter(|id| shopify_gid_resource_type(id) == Some("Product"))
        .collect()
}

fn metafield_product_domain_reference_fallback(
    id: &str,
    product_owner_ids: &BTreeSet<String>,
) -> Option<String> {
    if resource_id_tail(id).parse::<u64>().is_err() {
        return None;
    }
    match shopify_gid_resource_type(id) {
        Some("Product") if product_owner_ids.contains(id) => Some(id.to_string()),
        Some("ProductVariant" | "Collection") if !product_owner_ids.is_empty() => {
            Some(id.to_string())
        }
        _ => None,
    }
}

fn owner_product_variant_state_from_observed_json(value: &Value) -> Option<ProductVariantRecord> {
    let id = value.get("id")?.as_str()?.to_string();
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
            extra_fields: BTreeMap::new(),
        },
        media_ids: variant_media_ids_from_json(value),
        extra_fields: BTreeMap::new(),
    })
}

fn owner_typename_from_gid(owner_id: &str) -> &'static str {
    metafield_owner_gid_resource_type(owner_id)
}

fn owner_metafield_timestamp(ordinal: u64) -> String {
    product_mutation_timestamp(ordinal)
}

fn apply_metafield_connection_cursors(records: &mut [Value], page_info: &Value) {
    if let Some((record, cursor)) = page_info
        .get("startCursor")
        .and_then(Value::as_str)
        .and_then(|cursor| {
            shopify_cursor_resource_tail(cursor)
                .and_then(|tail| metafield_record_by_tail_mut(records, &tail))
                .map(|record| (record, cursor.to_string()))
        })
    {
        record["__cursor"] = json!(cursor);
    }
    if let Some((record, cursor)) =
        page_info
            .get("endCursor")
            .and_then(Value::as_str)
            .and_then(|cursor| {
                shopify_cursor_resource_tail(cursor)
                    .and_then(|tail| metafield_record_by_tail_mut(records, &tail))
                    .map(|record| (record, cursor.to_string()))
            })
    {
        record["__cursor"] = json!(cursor);
    }
    if records.len() == 1 {
        if let Some(cursor) = page_info
            .get("startCursor")
            .and_then(Value::as_str)
            .or_else(|| page_info.get("endCursor").and_then(Value::as_str))
        {
            records[0]["__cursor"] = json!(cursor);
        }
    }
}

fn metafield_record_by_tail_mut<'a>(records: &'a mut [Value], tail: &str) -> Option<&'a mut Value> {
    records.iter_mut().find(|record| {
        record
            .get("id")
            .and_then(Value::as_str)
            .map(resource_id_tail)
            .is_some_and(|record_tail| record_tail == tail)
    })
}

fn shopify_cursor_resource_tail(cursor: &str) -> Option<String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(cursor)
        .ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("last_id")
        .and_then(|last_id| {
            last_id
                .as_u64()
                .map(|id| id.to_string())
                .or_else(|| last_id.as_str().map(str::to_string))
        })
        .filter(|tail| !tail.is_empty())
}

fn metafield_cursor(metafield: &Value) -> Option<String> {
    // Prefer a cursor captured from an upstream connection's pageInfo; otherwise
    // synthesize Shopify's id-ordered metafield cursor — base64 of
    // `{"last_id":<numeric>,"last_value":"<numeric>"}` — from the record id so
    // relay pagination works for any backend, not just recorded fixtures.
    if let Some(cursor) = metafield.get("__cursor").and_then(Value::as_str) {
        return Some(cursor.to_string());
    }
    let id = metafield.get("id").and_then(Value::as_str)?;
    let tail = resource_id_tail(id);
    if let Ok(last_id) = tail.parse::<u64>() {
        if let Ok(bytes) = serde_json::to_vec(&json!({ "last_id": last_id, "last_value": tail })) {
            return Some(base64::engine::general_purpose::STANDARD.encode(bytes));
        }
    }
    if id.starts_with("gid://") {
        Some(format!("cursor:{id}"))
    } else {
        Some(id.to_string())
    }
}

fn metafield_connection_page_info(
    start_cursor: Option<String>,
    end_cursor: Option<String>,
    has_next_page: bool,
    has_previous_page: bool,
) -> Value {
    json!({
        "hasNextPage": has_next_page,
        "hasPreviousPage": has_previous_page,
        "startCursor": start_cursor,
        "endCursor": end_cursor
    })
}

fn owner_typename_from_root(root_field: &str) -> &'static str {
    match root_field {
        "product" => "Product",
        "productVariant" => "ProductVariant",
        "collection" => "Collection",
        "customer" => "Customer",
        "order" => "Order",
        "company" => "Company",
        _ => "Node",
    }
}
