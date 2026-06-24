use super::*;

const DISCOUNT_DEFAULT_TIMESTAMP: &str = "2026-04-27T19:32:14Z";
const DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE: &str =
    "Only one of context or customerSelection can be provided.";
const DISCOUNT_MINIMUM_QUANTITY_UPPER_BOUND: i64 = 2_147_483_647;
const DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND: i64 = 1_000_000_000_000_000_000;
const DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND_DECIMAL: &str = "1000000000000000000";
const SHOPIFY_FUNCTION_BY_ID_QUERY: &str = "query ShopifyFunctionById($id: String!) {\n  shopifyFunction(id: $id) {\n    id\n    title\n    handle\n    apiType\n    description\n    appKey\n    app {\n      id\n      title\n      handle\n      apiKey\n    }\n  }\n}\n";
const SHOPIFY_FUNCTION_BY_HANDLE_QUERY: &str = "query ShopifyFunctionByHandle($handle: String!) {\n  shopifyFunctions(first: 1, handle: $handle) {\n    nodes {\n      id\n      title\n      handle\n      apiType\n      description\n      appKey\n      app {\n        id\n        title\n        handle\n        apiKey\n      }\n    }\n  }\n}\n";
const SHOP_SUBSCRIPTION_CAPABILITY_QUERY: &str =
    "query DraftProxyShopSubscriptionCapability {\n  shop {\n    features {\n      sellsSubscriptions\n    }\n  }\n}\n";
/// Availability probe forwarded when activating an app discount. Shopify checks
/// that the backing Function is still installed/available before activating; a
/// revoked function returns an empty `nodes` list and activation fails with a
/// base-field INTERNAL_ERROR. Must match the recorded cassette call byte-for-byte.
const SHOPIFY_FUNCTION_AVAILABILITY_QUERY: &str = "query ShopifyFunctionAvailabilityForDiscountActivation($handle: String!) { shopifyFunctions(first: 1, handle: $handle) { nodes { id title handle apiType description appKey app { id title handle apiKey } } } }";
/// Shop-wide redeem-code uniqueness probe forwarded during code-discount create
/// validation. A code already assigned to any discount in the shop is rejected
/// with a `TAKEN` userError; rather than relying on locally injected state, the
/// proxy learns this the way Shopify's own admin does — by looking the code up —
/// and so resolves uniqueness against the real backend in `live-hybrid`. The
/// query text is shared verbatim with the conformance capture script
/// (`scripts/capture-discount-validation-conformance.ts`) so the request the
/// proxy forwards matches the recorded `DiscountUniquenessCheck` cassette call
/// byte-for-byte (the cassette matcher is strict on query text + variables).
const DISCOUNT_UNIQUENESS_QUERY: &str =
    include_str!("../../config/parity-requests/discounts/discount-uniqueness-check.graphql");
/// Read query used to hydrate a discount that is not staged locally so an
/// activate/deactivate transition can be applied against its real dates and
/// status. Must match the recorded cassette `DiscountHydrate` upstream call
/// byte-for-byte (the cassette matcher is strict on query text + variables).
const DISCOUNT_HYDRATE_QUERY: &str = "#graphql\n  query DiscountHydrate($id: ID!) {\n    codeNode: codeDiscountNode(id: $id) {\n      id\n      codeDiscount {\n        __typename\n        ... on DiscountCodeBasic {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n          codes(first: 250) {\n            nodes {\n              id\n              code\n            }\n          }\n        }\n        ... on DiscountCodeApp {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n        ... on DiscountCodeBxgy {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n        ... on DiscountCodeFreeShipping {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n      }\n    }\n    automaticNode: automaticDiscountNode(id: $id) {\n      id\n      automaticDiscount {\n        __typename\n        ... on DiscountAutomaticBasic {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n        ... on DiscountAutomaticApp {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n        ... on DiscountAutomaticBxgy {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n        ... on DiscountAutomaticFreeShipping {\n          title\n          status\n          startsAt\n          endsAt\n          updatedAt\n        }\n      }\n    }\n  }\n";
/// Item-entitlement existence probe forwarded before a discount create is
/// validated. Discounts that entitle products / variants / collections must
/// reject references to entities that do not exist in the shop; rather than
/// relying on locally injected state, the proxy resolves existence the way
/// Shopify's own admin does — by looking the referenced nodes up — and observes
/// the result so the per-field existence checks (see `discount_reference_*`)
/// decide against real store state. The query text is shared verbatim with the
/// conformance capture script so the request the proxy forwards matches the
/// recorded `ProductsHydrateNodes` cassette call byte-for-byte (the cassette
/// matcher is strict on query text + variables).
const DISCOUNT_ITEM_REFS_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/discounts/discount-item-refs-hydrate.graphql");
/// Buyer-context segment existence/name probe forwarded before a discount's
/// `context.customerSegments` selection is materialized. A discount scoped to a
/// customer segment echoes back the segment's display name; rather than relying
/// on locally injected segment state, the proxy resolves the name the way
/// Shopify's own admin does — by reading the referenced segment — and stages the
/// result so `resolve_discount_context_names` bakes the real name. The query text
/// is shared verbatim with the conformance capture script so the request the
/// proxy forwards matches the recorded `DiscountContextSegmentHydrate` cassette
/// call byte-for-byte (the cassette matcher is strict on query text + variables).
const DISCOUNT_CONTEXT_SEGMENT_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/discounts/discount-context-segment-hydrate.graphql");

impl DraftProxy {
    pub(in crate::proxy) fn discounts_query_response(
        &self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if self.should_forward_cold_discount_read(&fields) {
            return (self.upstream_transport)(request.clone());
        }
        ok_json(json!({ "data": self.discounts_query_data(&fields) }))
    }

    /// Decide whether a discount read carries no relevant local overlay state and
    /// should therefore be forwarded byte-for-byte to the upstream backend. This is
    /// the read side of the overlay: when the proxy has nothing staged that could
    /// answer the query, the only correct answer comes from the real store. In the
    /// parity harness this resolves through the cassette's fallback (the forwarded
    /// request matches the captured request exactly) or an explicit recorded call.
    fn should_forward_cold_discount_read(&self, fields: &[RootFieldSelection]) -> bool {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return false;
        }
        if fields.is_empty() {
            return false;
        }
        fields
            .iter()
            .all(|field| self.discount_read_field_is_cold(field))
    }

    /// A single discount read root field is "cold" when nothing in the local
    /// overlay can answer it: the requested id/code is neither staged nor locally
    /// deleted, or (for catalog connections) there is no staged discount state at
    /// all. Locally deleted ids are intentionally NOT cold — forwarding them would
    /// resurrect a discount the caller removed in this session.
    fn discount_read_field_is_cold(&self, field: &RootFieldSelection) -> bool {
        match field.name.as_str() {
            "discountNode" | "codeDiscountNode" | "automaticDiscountNode" => {
                let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                !self.store.staged.discounts.contains_key(&id)
                    && !self.store.staged.discounts.is_tombstoned(&id)
            }
            "codeDiscountNodeByCode" => {
                let code = resolved_field_string_arg(field, "code").unwrap_or_default();
                !self
                    .store
                    .staged
                    .discount_code_index
                    .contains_key(&code.to_ascii_uppercase())
            }
            "discountNodes"
            | "discountNodesCount"
            | "automaticDiscountNodes"
            | "codeDiscountNodes" => !self.has_staged_discounts(),
            "discountRedeemCodeBulkCreation" => {
                let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                !self
                    .store
                    .staged
                    .discount_redeem_code_bulk_creations
                    .contains_key(&id)
            }
            _ => false,
        }
    }

    pub(in crate::proxy) fn has_staged_discounts(&self) -> bool {
        !self.store.staged.discounts.is_empty()
            || !self.store.staged.discounts.tombstones.is_empty()
            || !self
                .store
                .staged
                .discount_redeem_code_bulk_creations
                .is_empty()
    }

    pub(in crate::proxy) fn discounts_mutation(
        &mut self,
        _request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(fields) = root_fields(query, variables) else {
            return MutationOutcome::response(json_error(400, "Could not parse GraphQL operation"));
        };
        if let Some(response) = discount_document_level_error_response(&fields) {
            return MutationOutcome::response(response);
        }
        // Resolve the existence of any product / variant / collection entitlement
        // references up front by forwarding a single batched node lookup and
        // observing the result, so the per-field create validation below decides
        // INVALID references against real store state instead of seeded state.
        self.hydrate_discount_item_refs(_request, &fields);
        // Resolve any buyer-context customer / segment members the same way: forward
        // a read for each referenced customer / segment that is not already staged and
        // observe the result, so `resolve_discount_context_names` bakes the real
        // display name / segment name from store state instead of a seeded precondition.
        self.hydrate_discount_context_refs(_request, &fields);
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        let mut top_level_errors = Vec::new();
        for field in fields {
            if let Some(error) = discount_field_top_level_error(&field) {
                top_level_errors.push(error);
                data.insert(field.response_key.clone(), Value::Null);
                continue;
            }
            let outcome = self.discount_mutation_field(_request, &field);
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&outcome.value, &field.selection),
            );
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !top_level_errors.is_empty() {
            body["errors"] = Value::Array(top_level_errors);
        }
        let response = ok_json(body);
        for draft in &mut log_drafts {
            if draft.staged_resource_ids.is_empty() {
                draft.status = "failed".to_string();
                draft.notes =
                    "Discount mutation handled locally and returned userErrors; no resource staged."
                        .to_string();
            }
        }
        MutationOutcome::with_log_drafts(response, log_drafts)
    }

    fn discount_mutation_field(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        match field.name.as_str() {
            "discountCodeBasicCreate" => self.discount_create(
                request,
                field,
                "basicCodeDiscount",
                "code",
                "DiscountCodeBasic",
            ),
            "discountCodeBasicUpdate" => self.discount_update(
                request,
                field,
                "basicCodeDiscount",
                "code",
                "DiscountCodeBasic",
            ),
            "discountCodeBxgyCreate" => self.discount_create(
                request,
                field,
                "bxgyCodeDiscount",
                "code",
                "DiscountCodeBxgy",
            ),
            "discountCodeBxgyUpdate" => self.discount_update(
                request,
                field,
                "bxgyCodeDiscount",
                "code",
                "DiscountCodeBxgy",
            ),
            "discountCodeFreeShippingCreate" => self.discount_create(
                request,
                field,
                "freeShippingCodeDiscount",
                "code",
                "DiscountCodeFreeShipping",
            ),
            "discountCodeFreeShippingUpdate" => self.discount_update(
                request,
                field,
                "freeShippingCodeDiscount",
                "code",
                "DiscountCodeFreeShipping",
            ),
            "discountCodeAppCreate" => self.app_discount_create(
                request,
                field,
                "codeAppDiscount",
                "code",
                "DiscountCodeApp",
            ),
            "discountCodeAppUpdate" => self.app_discount_update(
                request,
                field,
                "codeAppDiscount",
                "code",
                "DiscountCodeApp",
            ),
            "discountAutomaticBasicCreate" => self.discount_create(
                request,
                field,
                "automaticBasicDiscount",
                "automatic",
                "DiscountAutomaticBasic",
            ),
            "discountAutomaticBasicUpdate" => self.discount_update(
                request,
                field,
                "automaticBasicDiscount",
                "automatic",
                "DiscountAutomaticBasic",
            ),
            "discountAutomaticBxgyCreate" => self.discount_create(
                request,
                field,
                "automaticBxgyDiscount",
                "automatic",
                "DiscountAutomaticBxgy",
            ),
            "discountAutomaticBxgyUpdate" => self.discount_update(
                request,
                field,
                "automaticBxgyDiscount",
                "automatic",
                "DiscountAutomaticBxgy",
            ),
            "discountAutomaticFreeShippingCreate" => self.discount_create(
                request,
                field,
                "freeShippingAutomaticDiscount",
                "automatic",
                "DiscountAutomaticFreeShipping",
            ),
            "discountAutomaticFreeShippingUpdate" => self.discount_update(
                request,
                field,
                "freeShippingAutomaticDiscount",
                "automatic",
                "DiscountAutomaticFreeShipping",
            ),
            "discountAutomaticAppCreate" => self.app_discount_create(
                request,
                field,
                "automaticAppDiscount",
                "automatic",
                "DiscountAutomaticApp",
            ),
            "discountAutomaticAppUpdate" => self.app_discount_update(
                request,
                field,
                "automaticAppDiscount",
                "automatic",
                "DiscountAutomaticApp",
            ),
            "discountCodeActivate"
            | "discountCodeDeactivate"
            | "discountAutomaticActivate"
            | "discountAutomaticDeactivate" => self.discount_status_transition(request, field),
            "discountCodeDelete" | "discountAutomaticDelete" => self.discount_delete(field),
            "discountCodeBulkActivate"
            | "discountCodeBulkDeactivate"
            | "discountCodeBulkDelete"
            | "discountAutomaticBulkActivate"
            | "discountAutomaticBulkDeactivate"
            | "discountAutomaticBulkDelete" => self.discount_bulk_action(field),
            "discountRedeemCodeBulkAdd" => self.discount_redeem_code_bulk_add(request, field),
            "discountCodeRedeemCodeBulkDelete" => self.discount_redeem_code_bulk_delete(field),
            _ => MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                vec![discount_null_field_user_error(
                    "Local staging for this discount mutation is not implemented.",
                    Some("NOT_IMPLEMENTED"),
                )],
            )),
        }
    }

    fn discount_create(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
        input_arg: &str,
        discount_kind: &str,
        typename: &str,
    ) -> MutationFieldOutcome {
        let input = discount_input(field, input_arg);
        let mut user_errors = discount_input_user_errors(input.as_ref(), input_arg, typename, true);
        if let Some(error) =
            self.discount_subscription_gate_error(request, input.as_ref(), input_arg)
        {
            user_errors.push(error);
        }
        if let Some(input_map) = input.as_ref() {
            user_errors.extend(self.discount_reference_user_errors(request, input_map, input_arg));
        }
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                user_errors,
            ));
        }
        let input = input.unwrap_or_default();
        let id_type = if discount_kind == "automatic" {
            "DiscountAutomaticNode"
        } else {
            "DiscountCodeNode"
        };
        let id = self.next_proxy_synthetic_gid(id_type);
        // A code discount auto-creates a DiscountRedeemCode, which Shopify allocates
        // the next sequential id to. Reserve that id so the global synthetic counter
        // stays in lockstep with captured local-runtime id sequences.
        if discount_kind != "automatic"
            && resolved_string_path(&input, &["code"])
                .map(|code| !code.trim().is_empty())
                .unwrap_or(false)
        {
            let _ = self.next_proxy_synthetic_gid("DiscountRedeemCode");
        }
        let mut record = discount_record_from_input(&id, discount_kind, typename, &input, None);
        self.resolve_discount_context_names(&mut record);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(&field.name, discount_node_for_record(&record), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    /// Fill in buyer-context member display names / segment names from records the
    /// store already holds (seeded preconditions or entities staged earlier in the
    /// scenario). The discount record only carries member ids until this runs, so
    /// baking the names here means every later read of `record["context"]` resolves
    /// them without re-querying — mirroring how live Shopify materializes the
    /// selection from the referenced customer/segment records.
    fn resolve_discount_context_names(&self, record: &mut Value) {
        let Some(context) = record.get_mut("context") else {
            return;
        };
        if let Some(customers) = context.get_mut("customers").and_then(Value::as_array_mut) {
            for customer in customers {
                let Some(id) = customer
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                else {
                    continue;
                };
                if let Some(display_name) = self
                    .store
                    .staged
                    .customers
                    .get(&id)
                    .and_then(|record| record.get("displayName"))
                    .filter(|value| !value.is_null())
                    .cloned()
                {
                    customer["displayName"] = display_name;
                }
            }
        }
        if let Some(segments) = context.get_mut("segments").and_then(Value::as_array_mut) {
            for segment in segments {
                let Some(id) = segment
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                else {
                    continue;
                };
                if let Some(name) = self
                    .store
                    .staged
                    .segments
                    .get(&id)
                    .and_then(|record| record.get("name"))
                    .filter(|value| !value.is_null())
                    .cloned()
                {
                    segment["name"] = name;
                }
            }
        }
    }

    /// Forward a single batched `nodes(ids:)` lookup for every product / variant /
    /// collection entitlement reference across all create fields in the mutation,
    /// then observe the response into local product/collection/variant state. This
    /// runs before any field is validated so the existence checks in
    /// `discount_items_reference_errors` resolve against the references Shopify
    /// actually recognizes, exactly mirroring how the live admin learns which ids
    /// are valid. The ids are deduplicated and sorted (numeric tail ascending, ties
    /// broken by gid string) so the forwarded request matches the recorded
    /// `ProductsHydrateNodes` cassette byte-for-byte. Only forwarded in
    /// `live-hybrid`; in `snapshot` the existence checks keep their permissive
    /// default (no upstream to consult).
    fn hydrate_discount_item_refs(&mut self, request: &Request, fields: &[RootFieldSelection]) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut ids: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for field in fields {
            let Some(input_arg) = discount_create_input_arg(&field.name) else {
                continue;
            };
            let Some(input) = discount_input(field, input_arg) else {
                continue;
            };
            for selection in ["customerBuys", "customerGets"] {
                for path in [
                    [selection, "items", "products", "productsToAdd"],
                    [selection, "items", "products", "productVariantsToAdd"],
                    [selection, "items", "collections", "add"],
                ] {
                    for id in resolved_string_list_path(&input, &path) {
                        // The reference is already authoritative if it was staged
                        // earlier in the scenario; only unknown ids need a lookup.
                        if self.discount_reference_already_staged(&id) {
                            continue;
                        }
                        if seen.insert(id.clone()) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
        if ids.is_empty() {
            return;
        }
        ids.sort_by(|left, right| compare_resource_ids(left, right));
        let response = self.upstream_post(
            request,
            json!({
                "query": DISCOUNT_ITEM_REFS_HYDRATE_QUERY,
                "operationName": "ProductsHydrateNodes",
                "variables": { "ids": ids }
            }),
        );
        self.observe_nodes_response(&response);
    }

    /// Forward a read for every buyer-context customer / segment member referenced
    /// by a create / update field that is not already staged, and observe the result
    /// into local customer / segment state. This runs before any field materializes
    /// its `context` so `resolve_discount_context_names` resolves member display
    /// names / segment names against the records Shopify actually recognizes — the
    /// same way the live admin reads them — instead of a seeded precondition. Each
    /// referenced id is looked up at most once per mutation; ids already staged
    /// earlier in the scenario are skipped. Only forwarded in `live-hybrid`; in
    /// `snapshot` mode there is no upstream to consult, so the names degrade to the
    /// permissive "id only" default (mirroring `hydrate_discount_item_refs`).
    fn hydrate_discount_context_refs(&mut self, request: &Request, fields: &[RootFieldSelection]) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut customer_ids: Vec<String> = Vec::new();
        let mut segment_ids: Vec<String> = Vec::new();
        let mut seen_customers: BTreeSet<String> = BTreeSet::new();
        let mut seen_segments: BTreeSet<String> = BTreeSet::new();
        for field in fields {
            let Some(input_arg) = discount_mutation_input_arg(&field.name) else {
                continue;
            };
            let Some(input) = discount_input(field, input_arg) else {
                continue;
            };
            for id in resolved_string_list_path(&input, &["context", "customers", "add"]) {
                if !self.store.staged.customers.contains_key(&id)
                    && seen_customers.insert(id.clone())
                {
                    customer_ids.push(id);
                }
            }
            for id in resolved_string_list_path(&input, &["context", "customerSegments", "add"]) {
                if !self.store.staged.segments.contains_key(&id) && seen_segments.insert(id.clone())
                {
                    segment_ids.push(id);
                }
            }
        }
        for id in customer_ids {
            if let Some(customer) = self.hydrate_customer_for_mutation(request, &id) {
                self.store.staged.customers.insert(id, customer);
            }
        }
        for id in segment_ids {
            self.hydrate_discount_context_segment(request, &id);
        }
    }

    /// Forward a `segment(id:)` read for a single buyer-context segment and stage the
    /// normalized record so `resolve_discount_context_names` resolves its name from
    /// real store state. No-op when the lookup misses (non-200 or a null segment) —
    /// the permissive default that never fabricates a name — so a scenario that does
    /// not record the read simply leaves the member id un-named, exactly as before.
    fn hydrate_discount_context_segment(&mut self, request: &Request, id: &str) {
        let response = self.upstream_post(
            request,
            json!({
                "query": DISCOUNT_CONTEXT_SEGMENT_HYDRATE_QUERY,
                "operationName": "DiscountContextSegmentHydrate",
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let segment = response.body["data"]["segment"].clone();
        if segment.is_null() {
            return;
        }
        let field = |key: &str| segment.get(key).cloned().unwrap_or(Value::Null);
        let record = json!({
            "__typename": "Segment",
            "id": id,
            "name": field("name"),
            "query": field("query"),
            "creationDate": field("creationDate"),
            "lastEditDate": field("lastEditDate"),
            "tagMigrated": false,
            "valid": true,
            "percentageSnapshot": null,
            "percentageSnapshotUpdatedAt": null,
            "translation": null,
            "author": null
        });
        self.store.staged.segments.insert(id.to_string(), record);
    }

    /// Whether a product / variant / collection gid is already present in staged
    /// state (and so does not need an upstream existence lookup).
    fn discount_reference_already_staged(&self, gid: &str) -> bool {
        if gid.starts_with("gid://shopify/Product/") {
            self.store.has_product(gid)
        } else if gid.starts_with("gid://shopify/ProductVariant/") {
            self.store.product_variant_by_id(gid).is_some()
        } else if gid.starts_with("gid://shopify/Collection/") {
            self.store.collection_by_id(gid).is_some()
        } else {
            false
        }
    }

    /// Referential-integrity validation that depends on the real backend's
    /// contents: a duplicate redeem code (TAKEN) and item-entitlement references to
    /// products / variants / collections that do not exist. Shopify resolves these
    /// against its catalog; the proxy mirrors that by forwarding a uniqueness lookup
    /// upstream (see `discount_code_is_taken`) and a batched node lookup
    /// (`hydrate_discount_item_refs`) before validation. Item-entitlement existence
    /// is enforced once the referenced nodes have been observed — except the
    /// universally-invalid `/0` sentinel id, which never resolves on any Shopify
    /// store.
    fn discount_reference_user_errors(
        &self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        input_arg: &str,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if let Some(code) = resolved_string_path(input, &["code"]) {
            if !code.trim().is_empty() && self.discount_code_is_taken(request, &code) {
                errors.push(discount_user_error(
                    vec![input_arg, "code"],
                    "Code must be unique. Please try a different code.",
                    "TAKEN",
                ));
            }
        }
        for selection in ["customerBuys", "customerGets"] {
            errors.extend(self.discount_items_reference_errors(input, input_arg, selection));
        }
        errors
    }

    /// Whether `code` is already assigned to a discount in the shop and so cannot be
    /// reused. A code staged earlier in the same scenario is taken without consulting
    /// upstream. Otherwise the proxy resolves uniqueness the real way: it forwards a
    /// `codeDiscountNodeByCode` lookup and treats a non-null node as taken. In
    /// `snapshot` mode (no upstream) and when the upstream lookup cannot be resolved
    /// (non-200 — e.g. a parity scenario that does not record a uniqueness call), it
    /// degrades to "not taken", the permissive default that never fabricates a
    /// rejection. This mirrors `fetch_shop_sells_subscriptions`.
    fn discount_code_is_taken(&self, request: &Request, code: &str) -> bool {
        if self
            .store
            .staged
            .discount_code_index
            .contains_key(&code.to_ascii_uppercase())
        {
            return true;
        }
        if self.config.read_mode != ReadMode::LiveHybrid {
            return false;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DISCOUNT_UNIQUENESS_QUERY,
                "variables": { "code": code }
            }),
        );
        if response.status != 200 {
            return false;
        }
        !response.body["data"]["codeDiscountNodeByCode"].is_null()
    }

    /// Existence / conflict validation for one entitlement selection (`customerBuys`
    /// or `customerGets`). Order within a block mirrors Shopify: collections (a
    /// conflict when products are also present, otherwise an existence check), then
    /// products, then variants.
    fn discount_items_reference_errors(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        input_arg: &str,
        selection: &str,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        let products =
            resolved_string_list_path(input, &[selection, "items", "products", "productsToAdd"]);
        let variants = resolved_string_list_path(
            input,
            &[selection, "items", "products", "productVariantsToAdd"],
        );
        let collections =
            resolved_string_list_path(input, &[selection, "items", "collections", "add"]);
        let has_product_refs = !products.is_empty() || !variants.is_empty();
        if !collections.is_empty() {
            if has_product_refs {
                errors.push(discount_user_error(
                    vec![input_arg, selection, "items", "collections", "add"],
                    "Cannot entitle collections in combination with product variants or products",
                    "CONFLICT",
                ));
            } else {
                for collection_id in &collections {
                    if !self.discount_reference_collection_exists(collection_id) {
                        errors.push(discount_user_error(
                            vec![input_arg, selection, "items", "collections", "add"],
                            &format!(
                                "Collection with id: {} is invalid",
                                resource_id_tail(collection_id)
                            ),
                            "INVALID",
                        ));
                    }
                }
            }
        }
        for product_id in &products {
            if !self.discount_reference_product_exists(product_id) {
                errors.push(discount_user_error(
                    vec![input_arg, selection, "items", "products", "productsToAdd"],
                    &format!(
                        "Product with id: {} is invalid",
                        resource_id_tail(product_id)
                    ),
                    "INVALID",
                ));
            }
        }
        for variant_id in &variants {
            if !self.discount_reference_product_variant_exists(variant_id) {
                errors.push(discount_user_error(
                    vec![
                        input_arg,
                        selection,
                        "items",
                        "products",
                        "productVariantsToAdd",
                    ],
                    &format!(
                        "Product variant with id: {} is invalid",
                        resource_id_tail(variant_id)
                    ),
                    "INVALID",
                ));
            }
        }
        errors
    }

    fn discount_reference_product_exists(&self, gid: &str) -> bool {
        if resource_id_tail(gid) == "0" {
            return false;
        }
        if self.store.has_product_state() {
            self.store.has_product(gid)
        } else {
            true
        }
    }

    fn discount_reference_product_variant_exists(&self, gid: &str) -> bool {
        if resource_id_tail(gid) == "0" {
            return false;
        }
        let authoritative = !self.store.staged.product_variants.records.is_empty()
            || !self.store.base.product_variants.records.is_empty();
        if authoritative {
            self.store.product_variant_by_id(gid).is_some()
        } else {
            true
        }
    }

    fn discount_reference_collection_exists(&self, gid: &str) -> bool {
        if resource_id_tail(gid) == "0" {
            return false;
        }
        if self.store.has_collection_state() {
            self.store.collection_by_id(gid).is_some()
        } else {
            true
        }
    }

    fn discount_update(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
        input_arg: &str,
        discount_kind: &str,
        typename: &str,
    ) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let input = discount_input(field, input_arg);
        let existing_record = self.discount_record(&id).cloned();
        let user_errors = match existing_record.as_ref() {
            None => vec![json!({
                "field": ["id"],
                "message": "Discount does not exist",
                "code": Value::Null,
                "extraInfo": Value::Null
            })],
            Some(existing) => {
                // A "bulk" code discount (one carrying more than one redeem code,
                // typically populated via discountRedeemCodeBulkAdd) cannot have its
                // single `code` rewritten through a plain update — Shopify rejects the
                // attempt with a null-coded base error rather than mutating the record.
                let is_bulk = existing
                    .get("codes")
                    .and_then(Value::as_array)
                    .map(|codes| codes.len() > 1)
                    .unwrap_or(false);
                let changes_code = input
                    .as_ref()
                    .map(|input| resolved_string_path(input, &["code"]).is_some())
                    .unwrap_or(false);
                if is_bulk && changes_code {
                    vec![json!({
                        "field": ["id"],
                        "message": "Cannot update the code of a bulk discount.",
                        "code": Value::Null,
                        "extraInfo": Value::Null
                    })]
                } else {
                    let mut errors =
                        discount_input_user_errors(input.as_ref(), input_arg, typename, false);
                    if let Some(error) =
                        self.discount_subscription_gate_error(request, input.as_ref(), input_arg)
                    {
                        errors.push(error);
                    }
                    errors
                }
            }
        };
        if !user_errors.is_empty() {
            return MutationFieldOutcome::unlogged(discount_payload_for_root(
                &field.name,
                Value::Null,
                user_errors,
            ));
        }
        let existing = self.discount_record(&id).cloned();
        let mut record = discount_record_from_input(
            &id,
            discount_kind,
            typename,
            &input.unwrap_or_default(),
            existing.as_ref(),
        );
        self.resolve_discount_context_names(&mut record);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(&field.name, discount_node_for_record(&record), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn app_discount_create(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
        input_arg: &str,
        discount_kind: &str,
        typename: &str,
    ) -> MutationFieldOutcome {
        let input = discount_input(field, input_arg);
        let errors = app_discount_input_user_errors(input.as_ref(), input_arg, typename, true);
        if !errors.is_empty() {
            return MutationFieldOutcome::unlogged(app_discount_payload_for_root(
                &field.name,
                Value::Null,
                errors,
            ));
        }
        let input = input.unwrap_or_default();
        let function = match self.app_discount_function_for_input(request, &input, input_arg) {
            Ok(function) => function,
            Err(error) => {
                return MutationFieldOutcome::unlogged(app_discount_payload_for_root(
                    &field.name,
                    Value::Null,
                    vec![error],
                ))
            }
        };
        let id_type = if discount_kind == "automatic" {
            "DiscountAutomaticNode"
        } else {
            "DiscountCodeNode"
        };
        let id = self.next_proxy_synthetic_gid(id_type);
        // A code app discount auto-creates a DiscountRedeemCode for its `code`, which
        // Shopify allocates the next sequential id to. Reserve that id so the global
        // synthetic counter stays in lockstep with captured id sequences (mirrors the
        // basic `discount_create` reservation).
        if discount_kind != "automatic"
            && resolved_string_path(&input, &["code"])
                .map(|code| !code.trim().is_empty())
                .unwrap_or(false)
        {
            let _ = self.next_proxy_synthetic_gid("DiscountRedeemCode");
        }
        let mut record = discount_record_from_input(&id, discount_kind, typename, &input, None);
        attach_app_discount_function(&mut record, &function);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            app_discount_payload_for_root(
                &field.name,
                discount_body_for_record(&record),
                Vec::new(),
            ),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn app_discount_update(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
        input_arg: &str,
        discount_kind: &str,
        typename: &str,
    ) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let input = discount_input(field, input_arg);
        let mut errors = if self.discount_record(&id).is_none() {
            vec![discount_user_error(
                vec!["id"],
                "Discount does not exist.",
                "INVALID",
            )]
        } else {
            app_discount_input_user_errors(input.as_ref(), input_arg, typename, false)
        };
        if !errors.is_empty() {
            return MutationFieldOutcome::unlogged(app_discount_payload_for_root(
                &field.name,
                Value::Null,
                errors,
            ));
        }
        let input = input.unwrap_or_default();
        let function = match self.app_discount_function_for_input(request, &input, input_arg) {
            Ok(function) => function,
            Err(error) => {
                errors.push(error);
                return MutationFieldOutcome::unlogged(app_discount_payload_for_root(
                    &field.name,
                    Value::Null,
                    errors,
                ));
            }
        };
        let existing = self.discount_record(&id).cloned();
        let mut record =
            discount_record_from_input(&id, discount_kind, typename, &input, existing.as_ref());
        attach_app_discount_function(&mut record, &function);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            app_discount_payload_for_root(
                &field.name,
                discount_body_for_record(&record),
                Vec::new(),
            ),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    fn discount_status_transition(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let activating = field.name.ends_with("Activate");
        let mut record = match self.discount_record(&id).cloned() {
            Some(record) => record,
            None => match self.hydrate_discount_record(request, &id) {
                // Not staged locally: hydrate the discount from upstream so the
                // transition applies against its real dates/status.
                Some(record) => record,
                // A truly-unknown id hydrates to nothing. Activate/deactivate of an
                // unknown id reports the type-specific "Code/Automatic discount does
                // not exist." message, the same phrasing delete uses.
                None => {
                    return MutationFieldOutcome::unlogged(discount_payload_for_root(
                        &field.name,
                        Value::Null,
                        vec![discount_unknown_id_user_error(&field.name)],
                    ))
                }
            },
        };
        // Activating an app discount re-checks that its backing Function is still
        // available; a revoked function fails activation with a base-field
        // INTERNAL_ERROR rather than transitioning the discount.
        if activating {
            if let Some(handle) = record
                .get("shopifyFunction")
                .and_then(|function| function.get("handle"))
                .and_then(Value::as_str)
            {
                if !self.app_discount_function_available(request, handle) {
                    return MutationFieldOutcome::unlogged(discount_payload_for_root(
                        &field.name,
                        Value::Null,
                        vec![json!({
                            "field": ["base"],
                            "message": "Discount could not be activated.",
                            "code": "INTERNAL_ERROR"
                        })],
                    ));
                }
            }
        }
        let current_status = record["status"].as_str().unwrap_or_default();
        // An idempotent transition — activating an already-active discount, or
        // deactivating an already-expired one — is a no-op: Shopify leaves
        // startsAt/endsAt/updatedAt exactly as they were. A SCHEDULED discount being
        // deactivated is a real transition (it gets an endsAt and becomes EXPIRED).
        let is_noop = if activating {
            current_status == "ACTIVE"
        } else {
            current_status == "EXPIRED"
        };
        if !is_noop {
            let new_status = if activating { "ACTIVE" } else { "EXPIRED" };
            record["status"] = json!(new_status);
            record["updatedAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
            if activating {
                record["endsAt"] = Value::Null;
            } else if record.get("endsAt").and_then(Value::as_str).is_none() {
                record["endsAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
            }
        }
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(&field.name, discount_node_for_record(&record), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    /// Hydrate a discount that is not present in the local overlay by reading it
    /// from upstream (the live store, or the cassette's recorded `DiscountHydrate`
    /// call). Returns a discount record built from the upstream node, or `None`
    /// when the id resolves to neither a code nor an automatic discount (or no
    /// upstream is available, e.g. snapshot mode).
    fn hydrate_discount_record(&self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": DISCOUNT_HYDRATE_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        let data = response.body.get("data")?;
        let (node, kind, disc_key) = if data
            .get("codeNode")
            .map(|node| !node.is_null())
            .unwrap_or(false)
        {
            (&data["codeNode"], "code", "codeDiscount")
        } else if data
            .get("automaticNode")
            .map(|node| !node.is_null())
            .unwrap_or(false)
        {
            (&data["automaticNode"], "automatic", "automaticDiscount")
        } else {
            return None;
        };
        let node_id = node.get("id").and_then(Value::as_str)?.to_string();
        let disc = node.get(disc_key)?;
        let typename = disc
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let codes = disc
            .get("codes")
            .and_then(|codes| codes.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|code_node| {
                        json!({
                            "id": code_node.get("id").cloned().unwrap_or(Value::Null),
                            "code": code_node.get("code").cloned().unwrap_or(Value::Null),
                            "asyncUsageCount": 0
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let codes_count = codes.len();
        Some(json!({
            "id": node_id,
            "kind": kind,
            "typename": typename,
            "title": disc.get("title").cloned().unwrap_or(Value::Null),
            "status": disc.get("status").cloned().unwrap_or(Value::Null),
            "startsAt": disc.get("startsAt").cloned().unwrap_or(Value::Null),
            "endsAt": disc.get("endsAt").cloned().unwrap_or(Value::Null),
            "createdAt": disc.get("createdAt").cloned().unwrap_or(Value::Null),
            "updatedAt": disc.get("updatedAt").cloned().unwrap_or(Value::Null),
            "asyncUsageCount": 0,
            "codes": codes,
            "codesCount": {
                "count": codes_count,
                "precision": "EXACT"
            }
        }))
    }

    /// Whether the Function backing an app discount is still available for
    /// activation. Forwards a `ShopifyFunctionAvailabilityForDiscountActivation`
    /// read; an empty `nodes` list means the function was revoked. When the probe
    /// cannot be resolved (no upstream / no recorded call) we assume the function
    /// is available so non-revocation scenarios activate normally.
    fn app_discount_function_available(&self, request: &Request, handle: &str) -> bool {
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOPIFY_FUNCTION_AVAILABILITY_QUERY,
                "variables": { "handle": handle }
            }),
        );
        if response.status != 200 {
            return true;
        }
        response.body["data"]["shopifyFunctions"]["nodes"]
            .as_array()
            .map(|nodes| !nodes.is_empty())
            .unwrap_or(true)
    }

    fn discount_delete(&mut self, field: &RootFieldSelection) -> MutationFieldOutcome {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let exists = self.discount_record(&id).is_some();
        if !exists {
            return MutationFieldOutcome::unlogged(discount_delete_payload(
                &field.name,
                Value::Null,
                vec![discount_unknown_id_user_error(&field.name)],
            ));
        }
        self.store.staged.discounts.tombstone_staged(&id);
        self.store
            .staged
            .discount_code_index
            .retain(|_, discount_id| discount_id != &id);
        MutationFieldOutcome::staged(
            discount_delete_payload(&field.name, json!(id.clone()), Vec::new()),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    /// Resolver-level selector validation shared by the discount bulk activate /
    /// deactivate / delete mutations (`discount{Code,Automatic}Bulk*`). Shopify
    /// requires exactly one of `ids`, `search`, or `savedSearchId`; supplying more
    /// than one is rejected up front with a `job: null` payload and a
    /// `TOO_MANY_ARGUMENTS` base error. The code and automatic families phrase the
    /// message differently. Single/zero-selector jobs are not staged locally, so
    /// those paths keep the not-implemented marker (they only reach this handler as
    /// a sibling of a locally-dispatched mutation; standalone bulk requests are
    /// forwarded upstream instead).
    fn discount_bulk_action(&self, field: &RootFieldSelection) -> MutationFieldOutcome {
        if redeem_code_bulk_delete_selector_count(field) > 1 {
            let message = if field.name.starts_with("discountAutomatic") {
                "Only one of IDs, search argument or saved search ID is allowed."
            } else {
                "Only one of 'ids', 'search' or 'saved_search_id' is allowed."
            };
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [discount_null_field_user_error(message, Some("TOO_MANY_ARGUMENTS"))],
            }));
        }
        MutationFieldOutcome::unlogged(json!({
            "job": Value::Null,
            "userErrors": [discount_null_field_user_error(
                "Local staging for this discount mutation is not implemented.",
                Some("NOT_IMPLEMENTED"),
            )],
        }))
    }

    /// Apply the local-overlay consequences of a discount bulk activate /
    /// deactivate / delete mutation that was forwarded upstream. The async job
    /// itself runs server-side (the forwarded response carries the real `job`),
    /// but the proxy's overlay must reflect the resulting state so later reads in
    /// the same scenario observe the transition. We only act on staged discounts
    /// matching the mutation's selector (`ids` or `search`) and its
    /// code/automatic kind, and only when the upstream accepted the job (a
    /// non-null `job` with no userErrors) — rejected validation cases leave the
    /// overlay untouched. `savedSearchId` selectors are not resolved locally; the
    /// forwarded response still stands for those.
    pub(in crate::proxy) fn apply_discount_bulk_overlay_effects(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        response_body: &Value,
    ) {
        let Some(fields) = root_fields(query, variables) else {
            return;
        };
        for field in &fields {
            let Some((kind, action)) = discount_bulk_root_action(&field.name) else {
                continue;
            };
            let payload = &response_body["data"][&field.response_key];
            if payload.is_null() {
                continue;
            }
            let job_accepted = payload
                .get("job")
                .map(|job| !job.is_null())
                .unwrap_or(false);
            let no_user_errors = payload
                .get("userErrors")
                .and_then(Value::as_array)
                .map(|errors| errors.is_empty())
                .unwrap_or(true);
            if !job_accepted || !no_user_errors {
                continue;
            }
            for id in self.discount_bulk_selector_ids(field, kind) {
                self.apply_discount_bulk_transition(&id, action);
            }
        }
    }

    /// Resolve the staged discount ids a bulk mutation's selector targets,
    /// restricted to the mutation's discount kind (`code` / `automatic`). An
    /// `ids` selector keeps only the supplied ids that resolve to a staged
    /// discount of the right kind; a `search` selector matches the staged
    /// overlay with the same query semantics reads use.
    fn discount_bulk_selector_ids(&self, field: &RootFieldSelection, kind: &str) -> Vec<String> {
        if let Some(ResolvedValue::List(values)) = field.arguments.get("ids") {
            return values
                .iter()
                .filter_map(|value| match value {
                    ResolvedValue::String(id) => Some(id.clone()),
                    _ => None,
                })
                .filter(|id| {
                    self.discount_record(id)
                        .map(|record| discount_kind(record) == kind)
                        .unwrap_or(false)
                })
                .collect();
        }
        if let Some(ResolvedValue::String(search)) = field.arguments.get("search") {
            return self
                .store
                .staged
                .discounts
                .values()
                .filter(|record| {
                    !self
                        .store
                        .staged
                        .discounts
                        .is_tombstoned(discount_id(record))
                })
                .filter(|record| discount_kind(record) == kind)
                .filter(|record| discount_matches_query(record, search))
                .map(|record| discount_id(record).to_string())
                .collect();
        }
        Vec::new()
    }

    /// Apply a single bulk transition to one staged discount. Activate/deactivate
    /// mirror the single `discount{Code,Automatic}{Activate,Deactivate}` mutation
    /// (idempotent no-op when already in the target status; deactivate stamps an
    /// `endsAt`); delete tombstones the discount and drops its codes from the
    /// code index.
    fn apply_discount_bulk_transition(&mut self, id: &str, action: DiscountBulkAction) {
        match action {
            DiscountBulkAction::Delete => {
                self.store.staged.discounts.tombstone(id.to_string());
                self.store.staged.discounts.remove(id);
                self.store
                    .staged
                    .discount_code_index
                    .retain(|_, discount_id| discount_id != id);
            }
            DiscountBulkAction::Activate | DiscountBulkAction::Deactivate => {
                let activating = matches!(action, DiscountBulkAction::Activate);
                if let Some(record) = self.store.staged.discounts.get_mut(id) {
                    let current_status = record["status"].as_str().unwrap_or_default();
                    let is_noop = if activating {
                        current_status == "ACTIVE"
                    } else {
                        current_status == "EXPIRED"
                    };
                    if !is_noop {
                        record["status"] = json!(if activating { "ACTIVE" } else { "EXPIRED" });
                        record["updatedAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
                        if activating {
                            record["endsAt"] = Value::Null;
                        } else if record.get("endsAt").and_then(Value::as_str).is_none() {
                            record["endsAt"] = json!(DISCOUNT_DEFAULT_TIMESTAMP);
                        }
                    }
                }
            }
        }
    }

    fn discount_redeem_code_bulk_add(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let discount_id = resolved_field_string_arg(field, "discountId").unwrap_or_default();
        if self.discount_record(&discount_id).is_none() {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [{
                    "field": ["discountId"],
                    "message": "Code discount does not exist.",
                    "code": "INVALID",
                    "extraInfo": Value::Null
                }]
            }));
        }
        let codes = resolved_redeem_codes(field);
        if codes.len() > 250 {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [{
                    "field": ["codes"],
                    "message": format!("The input array size of {} is greater than the maximum allowed of 250.", codes.len()),
                    "code": "MAX_INPUT_SIZE_EXCEEDED",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if codes.is_empty() {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [{
                    "field": ["codes"],
                    "message": "Codes can't be blank",
                    "code": "BLANK",
                    "extraInfo": Value::Null
                }]
            }));
        }
        // Codes already assigned to any discount in the shop (uppercased). Code
        // uniqueness is shop-wide, so a code that exists on another discount is
        // rejected here. Captured before this batch mutates the index.
        let mut existing_codes: BTreeSet<String> = self
            .store
            .staged
            .discount_code_index
            .keys()
            .cloned()
            .collect();
        // Codes not already known from local staged state have their shop-wide
        // uniqueness resolved the real way: forward a `codeDiscountNodeByCode`
        // lookup per candidate and treat a non-null node as taken (see
        // `discount_code_is_taken`). Only codes that would otherwise be accepted
        // are probed — format failures and in-batch duplicates are rejected
        // locally without consulting upstream, and a code already in the local
        // index is taken without a redundant forward. In `snapshot` mode (no
        // upstream) and when the lookup can't be resolved, the probe degrades to
        // "not taken", so scenarios that record no uniqueness call behave exactly
        // as a fresh batch would.
        for (index, code) in codes.iter().enumerate() {
            let normalized = code.to_ascii_uppercase();
            if existing_codes.contains(&normalized) {
                continue;
            }
            if !redeem_code_errors(code, &codes, index, &existing_codes).is_empty() {
                continue;
            }
            if self.discount_code_is_taken(request, code) {
                existing_codes.insert(normalized);
            }
        }
        let creation_id = self.next_proxy_synthetic_gid("DiscountRedeemCodeBulkCreation");
        // A later `discountRedeemCodeBulkCreation(id:)` read always observes the
        // completed job, so we store the validated result (per-code errors + final
        // counts) keyed by the creation id.
        let mut completed = discount_redeem_code_bulk_creation(&codes, &existing_codes, false);
        completed["id"] = json!(creation_id.clone());
        self.store
            .staged
            .discount_redeem_code_bulk_creations
            .insert(creation_id.clone(), completed.clone());
        // Schema-shaped `[DiscountRedeemCodeInput!]` submissions mirror Shopify's
        // async API: the mutation returns the still-running snapshot (done=false,
        // zeroed counts, no per-code results) and the completed creation is only
        // observed on a later read. Legacy `[String!]` (local-runtime) submissions
        // complete synchronously, so the mutation returns the finished creation.
        let response_creation = if redeem_codes_are_string_inputs(field) {
            completed
        } else {
            let mut pending = discount_redeem_code_bulk_creation(&codes, &existing_codes, true);
            pending["id"] = json!(creation_id.clone());
            pending
        };
        if let Some(record) = self.store.staged.discounts.get_mut(&discount_id) {
            let mut next = record["codes"].as_array().cloned().unwrap_or_else(Vec::new);
            for (index, code) in codes.iter().enumerate() {
                // Only codes that pass validation are actually assigned.
                if redeem_code_accepted(code, &codes, index, &existing_codes) {
                    next.push(json!({
                        "id": synthetic_shopify_gid("DiscountRedeemCode", stable_redeem_code_suffix(code)),
                        "code": code,
                        "asyncUsageCount": 0
                    }));
                }
            }
            record["codesCount"] = json!({ "count": next.len(), "precision": "EXACT" });
            record["codes"] = Value::Array(next);
        }
        self.rebuild_discount_code_index();
        MutationFieldOutcome::staged(
            json!({ "bulkCreation": response_creation, "userErrors": [] }),
            LogDraft::staged(&field.name, "discounts", vec![discount_id, creation_id]),
        )
    }

    fn discount_redeem_code_bulk_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> MutationFieldOutcome {
        let selector_count = redeem_code_bulk_delete_selector_count(field);
        if selector_count == 0 {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [discount_null_field_user_error(
                    "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
                    Some("MISSING_ARGUMENT")
                )]
            }));
        }
        if selector_count > 1 {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [discount_null_field_user_error(
                    "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
                    Some("TOO_MANY_ARGUMENTS")
                )]
            }));
        }
        let discount_id = resolved_field_string_arg(field, "discountId").unwrap_or_default();
        if self.discount_record(&discount_id).is_none() {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [{
                    "field": ["discountId"],
                    "message": "Code discount does not exist.",
                    "code": "INVALID",
                    "extraInfo": Value::Null
                }]
            }));
        }
        let ids_to_delete: BTreeSet<String> = match field.arguments.get("ids") {
            Some(ResolvedValue::List(ids)) if ids.is_empty() => {
                return MutationFieldOutcome::unlogged(json!({
                    "job": Value::Null,
                    "userErrors": [discount_null_field_user_error(
                        "Something went wrong, please try again.",
                        None
                    )]
                }));
            }
            Some(ResolvedValue::List(ids)) => ids
                .iter()
                .filter_map(|id| match id {
                    ResolvedValue::String(id) => Some(id.clone()),
                    _ => None,
                })
                .collect(),
            _ => BTreeSet::new(),
        };
        if matches!(field.arguments.get("search"), Some(ResolvedValue::String(search)) if search.trim().is_empty())
        {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [{
                    "field": ["search"],
                    "message": "'Search' can't be blank.",
                    "code": "BLANK",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if field.arguments.contains_key("savedSearchId")
            || field.arguments.contains_key("saved_search_id")
        {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [{
                    "field": ["savedSearchId"],
                    "message": "Invalid 'saved_search_id'.",
                    "code": "INVALID",
                    "extraInfo": Value::Null
                }]
            }));
        }
        if let Some(record) = self.store.staged.discounts.get_mut(&discount_id) {
            if let Some(codes) = record["codes"].as_array() {
                record["codes"] = Value::Array(
                    codes
                        .iter()
                        .filter(|code| {
                            code.get("id")
                                .and_then(Value::as_str)
                                .map(|id| !ids_to_delete.contains(id))
                                .unwrap_or(true)
                        })
                        .cloned()
                        .collect(),
                );
            }
            let count = record["codes"].as_array().map(Vec::len).unwrap_or(0);
            record["codesCount"] = json!({ "count": count, "precision": "EXACT" });
        }
        self.rebuild_discount_code_index();
        MutationFieldOutcome::staged(
            json!({
                "job": { "id": self.next_proxy_synthetic_gid("Job"), "done": true, "query": Value::Null },
                "userErrors": []
            }),
            LogDraft::staged(&field.name, "discounts", vec![discount_id]),
        )
    }

    fn discounts_query_data(&self, fields: &[RootFieldSelection]) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.discount_record(&id).map(discount_admin_node_for_record)
                }
                "codeDiscountNode" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.discount_record(&id).map(discount_node_for_record)
                }
                "automaticDiscountNode" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.discount_record(&id)
                        .filter(|record| discount_kind(record) == "automatic")
                        .map(discount_node_for_record)
                }
                "codeDiscountNodeByCode" => {
                    let code = resolved_field_string_arg(field, "code").unwrap_or_default();
                    self.store
                        .staged
                        .discount_code_index
                        .get(&code.to_ascii_uppercase())
                        .and_then(|id| self.discount_record(id))
                        .map(discount_node_for_record)
                }
                "discountNodes" => Some(json!({
                    "nodes": self.filtered_discount_records(field).into_iter().map(discount_admin_node_for_record).collect::<Vec<_>>()
                })),
                "automaticDiscountNodes" | "codeDiscountNodes" => {
                    let want_kind = if field.name == "automaticDiscountNodes" {
                        "automatic"
                    } else {
                        "code"
                    };
                    let nodes = self
                        .filtered_discount_records(field)
                        .into_iter()
                        .filter(|record| discount_kind(record) == want_kind)
                        .map(discount_node_for_record)
                        .collect::<Vec<_>>();
                    let (windowed, page_info) =
                        connection_window(&nodes, &field.arguments, value_id_cursor);
                    Some(connection_json_with_cursor(
                        windowed,
                        |_, node| value_id_cursor(node),
                        page_info,
                    ))
                }
                "discountNodesCount" => Some(json!({
                    "count": self.filtered_discount_records(field).len(),
                    "precision": "EXACT"
                })),
                "discountRedeemCodeBulkCreation" => {
                    let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                    self.store
                        .staged
                        .discount_redeem_code_bulk_creations
                        .get(&id)
                        .cloned()
                }
                _ => None,
            }
            .unwrap_or(Value::Null);
            let selected = if value.is_null() {
                Value::Null
            } else {
                selected_json(&value, &field.selection)
            };
            data.insert(field.response_key.clone(), selected);
        }
        Value::Object(data)
    }

    fn filtered_discount_records(&self, field: &RootFieldSelection) -> Vec<&Value> {
        let query = resolved_field_string_arg(field, "query").unwrap_or_default();
        self.store
            .staged
            .discounts
            .values()
            .filter(|record| {
                !self
                    .store
                    .staged
                    .discounts
                    .is_tombstoned(discount_id(record))
            })
            .filter(|record| discount_matches_query(record, &query))
            .collect()
    }

    pub(in crate::proxy) fn discount_node_value_by_id(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        self.discount_record(id).map(|record| {
            // A `node(id:)` read resolves to the concrete DiscountCodeNode /
            // DiscountAutomaticNode type, which expose `codeDiscount` /
            // `automaticDiscount` respectively (not the DiscountNode interface's
            // `discount`). `discount_node_for_record` emits the right accessor
            // for both kinds; the `discount`-keyed admin node shape is only for
            // the `discountNode(id:)` root field.
            let value = discount_node_for_record(record);
            selected_json(&value, selection)
        })
    }

    fn discount_record(&self, id: &str) -> Option<&Value> {
        self.store.staged.discounts.get(id)
    }

    fn stage_discount_record(&mut self, record: Value) {
        let id = discount_id(&record).to_string();
        self.store.staged.discounts.insert(id, record);
        self.rebuild_discount_code_index();
    }

    fn rebuild_discount_code_index(&mut self) {
        self.store.staged.discount_code_index.clear();
        for (id, record) in &self.store.staged.discounts {
            if self.store.staged.discounts.is_tombstoned(id) {
                continue;
            }
            for code in discount_record_codes(record) {
                self.store
                    .staged
                    .discount_code_index
                    .insert(code.to_ascii_uppercase(), id.clone());
            }
        }
    }

    fn app_discount_function_for_input(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        input_arg: &str,
    ) -> Result<Value, Value> {
        let function_id = resolved_non_blank_string_field(input, "functionId");
        let function_handle = resolved_non_blank_string_field(input, "functionHandle");
        let identifier = function_id.as_deref().or(function_handle.as_deref());
        let Some(identifier) = identifier else {
            return Err(app_discount_user_error(
                vec![json!(input_arg), json!("functionHandle")],
                "Function id can't be blank.",
                Some("MISSING_FUNCTION_IDENTIFIER"),
            ));
        };
        let field_name = if function_id.is_some() {
            "functionId"
        } else {
            "functionHandle"
        };
        let function = self
            .app_discount_function_from_staged_discounts(
                function_id.as_deref(),
                function_handle.as_deref(),
            )
            .or_else(|| {
                function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
            })
            .or_else(|| {
                self.fetch_shopify_function(
                    request,
                    function_id.as_deref(),
                    function_handle.as_deref(),
                )
            });
        let Some(function) = function else {
            return Err(app_discount_user_error(
                vec![json!(input_arg), json!(field_name)],
                &format!(
                    "Function {identifier} not found. Ensure that it is released in the current app (347082227713), and that the app is installed."
                ),
                Some("INVALID"),
            ));
        };
        if !app_discount_function_api_type_is_supported(&function) {
            return Err(app_discount_user_error(
                vec![json!(input_arg), json!(field_name)],
                "Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].",
                None,
            ));
        }
        Ok(function)
    }

    fn app_discount_function_from_staged_discounts(
        &self,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        self.store
            .staged
            .discounts
            .values()
            .filter_map(|record| record.get("shopifyFunction"))
            .find(|function| {
                id.is_some_and(|id| function["id"].as_str() == Some(id))
                    || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
            })
            .cloned()
    }

    fn fetch_shopify_function(
        &self,
        request: &Request,
        id: Option<&str>,
        handle: Option<&str>,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        if let Some(id) = id {
            return self.fetch_shopify_function_by_id(request, id);
        }
        handle.and_then(|handle| self.fetch_shopify_function_by_handle(request, handle))
    }

    fn fetch_shopify_function_by_id(&self, request: &Request, id: &str) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOPIFY_FUNCTION_BY_ID_QUERY,
                "variables": { "id": id }
            }),
        );
        if response.status != 200 {
            return None;
        }
        response.body["data"]["shopifyFunction"].as_object()?;
        Some(response.body["data"]["shopifyFunction"].clone())
    }

    fn fetch_shopify_function_by_handle(&self, request: &Request, handle: &str) -> Option<Value> {
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOPIFY_FUNCTION_BY_HANDLE_QUERY,
                "variables": { "handle": handle }
            }),
        );
        if response.status != 200 {
            return None;
        }
        response.body["data"]["shopifyFunctions"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .cloned()
    }

    /// Whether the upstream shop sells subscriptions. Subscription/recurring
    /// discount fields (`appliesOnSubscription`, `appliesOnOneTimePurchase`,
    /// `recurringCycleLimit`) are gated by the shop's selling-plan plan: a shop
    /// that does not sell subscriptions rejects them with "... is not permitted
    /// for this shop." We learn the capability the same way Shopify's own admin
    /// does — by reading `shop.features.sellsSubscriptions` — and cache it for the
    /// remainder of the scenario. When the capability cannot be resolved (no
    /// upstream available, e.g. the default synthetic local-runtime shop) we
    /// default to `false`, which is the gated, non-subscription behaviour.
    fn ensure_shop_sells_subscriptions(&mut self, request: &Request) -> bool {
        if let Some(cached) = self.shop_sells_subscriptions {
            return cached;
        }
        let resolved = self.fetch_shop_sells_subscriptions(request);
        self.shop_sells_subscriptions = Some(resolved);
        resolved
    }

    fn fetch_shop_sells_subscriptions(&self, request: &Request) -> bool {
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": SHOP_SUBSCRIPTION_CAPABILITY_QUERY,
                "variables": {}
            }),
        );
        if response.status != 200 {
            return false;
        }
        response.body["data"]["shop"]["features"]["sellsSubscriptions"]
            .as_bool()
            .unwrap_or(false)
    }

    /// Gate subscription/recurring discount fields on the shop's capability. The
    /// candidate "... is not permitted for this shop." error is only surfaced when
    /// the input actually carries such a field (and is not the subscriptions-only
    /// carveout) AND the upstream shop does not sell subscriptions. The capability
    /// probe is only forwarded when there is a candidate error to gate, so plain
    /// discounts never incur an extra upstream round-trip.
    fn discount_subscription_gate_error(
        &mut self,
        request: &Request,
        input: Option<&BTreeMap<String, ResolvedValue>>,
        input_arg: &str,
    ) -> Option<Value> {
        let input = input?;
        // bxgy discounts reject subscription/one-time fields outright with a
        // bxgy-specific message (emitted by discount_bxgy_customer_gets_user_errors),
        // so the shop-capability gate must not also fire its generic message.
        if input_arg.to_lowercase().contains("bxgy") {
            return None;
        }
        let candidate = discount_subscription_field_user_error(input, input_arg)?;
        if self.ensure_shop_sells_subscriptions(request) {
            None
        } else {
            Some(candidate)
        }
    }
}

fn discount_input(
    field: &RootFieldSelection,
    input_arg: &str,
) -> Option<BTreeMap<String, ResolvedValue>> {
    match field.arguments.get(input_arg) {
        Some(ResolvedValue::Object(input)) => Some(input.clone()),
        _ => None,
    }
}

fn discount_input_user_errors(
    input: Option<&BTreeMap<String, ResolvedValue>>,
    input_arg: &str,
    typename: &str,
    create: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let Some(input) = input else {
        errors.push(discount_user_error(
            vec![input_arg],
            "Input is required.",
            "REQUIRED",
        ));
        return errors;
    };
    // Free-shipping (SHIPPING-class) discounts validate the combinesWith/discount-class
    // constraint ahead of the title, and an automatic free-shipping discount does not
    // require a title at all (Shopify derives one). Surface that ordering up front; the
    // generic combinesWith resolver below intentionally no longer re-emits this error.
    let is_free_shipping = typename.contains("FreeShipping");
    let is_automatic = !typename.starts_with("DiscountCode");
    let combines_invalid = is_free_shipping
        && resolved_bool_path(input, &["combinesWith", "shippingDiscounts"]) == Some(true);
    if combines_invalid {
        errors.push(discount_user_error(
            vec![input_arg, "combinesWith"],
            "The combinesWith settings are not valid for the discount class.",
            "INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS",
        ));
    }
    // (bxgy) `customerGets` cannot entitle "all" items; Shopify reports this ahead of
    // the title-blank check.
    if typename.contains("Bxgy")
        && resolved_bool_path(input, &["customerGets", "items", "all"]) == Some(true)
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets"],
            "Items in 'customer get' cannot be set to all",
            "INVALID",
        ));
    }
    // When an automatic free-shipping discount also has an invalid combinesWith
    // (shippingDiscounts=true), Shopify reports only the combinesWith error and
    // suppresses the title-blank error. A code free-shipping discount reports both,
    // and an automatic free-shipping discount with a *valid* combinesWith still
    // rejects a blank title — so the suppression is gated on combines_invalid.
    let skip_title_blank = is_automatic && combines_invalid;
    if let Some(title) = resolved_string_path(input, &["title"]) {
        if title.trim().is_empty() {
            if !skip_title_blank {
                errors.push(discount_user_error(
                    vec![input_arg, "title"],
                    "Title can't be blank",
                    "BLANK",
                ));
            }
        } else if title.chars().count() > 255 {
            errors.push(discount_user_error(
                vec![input_arg, "title"],
                "Title is too long (maximum is 255 characters)",
                "TOO_LONG",
            ));
        }
    } else if !skip_title_blank {
        errors.push(discount_user_error(
            vec![input_arg, "title"],
            "Title can't be blank",
            "BLANK",
        ));
    }
    if typename.starts_with("DiscountCode") && create {
        match resolved_string_path(input, &["code"]) {
            Some(code) if code.is_empty() => errors.push(discount_user_error(
                vec![input_arg, "code"],
                "Code is too short (minimum is 1 character)",
                "TOO_SHORT",
            )),
            Some(code) if code.contains('\n') || code.contains('\r') => {
                errors.push(discount_user_error(
                    vec![input_arg, "code"],
                    "Code cannot contain newline characters.",
                    "INVALID",
                ))
            }
            Some(code) if code.chars().count() > 255 => errors.push(discount_user_error(
                vec![input_arg, "code"],
                "Code is too long (maximum is 255 characters)",
                "TOO_LONG",
            )),
            Some(_) => {}
            None => errors.push(discount_user_error(
                vec![input_arg, "code"],
                "Code can't be blank",
                "BLANK",
            )),
        }
    }
    if let Some(error) = discount_context_customer_selection_user_error(input, input_arg) {
        errors.push(error);
    }
    if resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["minimumRequirement", "quantity"],
    )
    .is_some()
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["minimumRequirement", "subtotal"],
        )
        .is_some()
    {
        errors.push(discount_user_error(
            vec![
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal cannot be defined when minimum quantity is.",
            "CONFLICT",
        ));
        errors.push(discount_user_error(
            vec![
                input_arg,
                "minimumRequirement",
                "quantity",
                "greaterThanOrEqualToQuantity",
            ],
            "Minimum quantity cannot be defined when minimum subtotal is.",
            "CONFLICT",
        ));
    }
    if !typename.contains("Bxgy")
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerGets", "value", "discountOnQuantity"],
        )
        .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets", "value", "discountOnQuantity"],
            "discountOnQuantity field is only permitted with bxgy discounts.",
            "INVALID",
        ));
    }
    if typename.contains("Bxgy") {
        errors.extend(discount_bxgy_customer_gets_user_errors(
            input, input_arg, typename, create,
        ));
    }
    // NOTE: subscription/recurring field gating is applied by the caller
    // (discount_create / discount_update) because it depends on the upstream
    // shop's `sellsSubscriptions` capability, which requires `&mut self`.
    if let Some(error) = discount_numeric_user_error(input, input_arg, typename) {
        errors.push(error);
    }
    errors.extend(discount_usage_recurring_bounds_user_errors(
        input, input_arg,
    ));
    errors.extend(discount_combines_with_user_errors(
        input, input_arg, typename,
    ));
    if let (Some(starts_at), Some(ends_at)) = (
        resolved_string_path(input, &["startsAt"]),
        resolved_string_path(input, &["endsAt"]),
    ) {
        if !ends_at.trim().is_empty() && !starts_at.trim().is_empty() && ends_at < starts_at {
            errors.push(discount_user_error(
                vec![input_arg, "endsAt"],
                "Ends at needs to be after starts_at",
                "INVALID",
            ));
        }
    }
    errors
}

/// Validate the `customerGets` block for buy-X-get-Y (bxgy) discounts.
///
/// Shopify constrains bxgy `customerGets` far more tightly than ordinary
/// discounts:
///
/// * the reward `value` may only be `discountOnQuantity` — a `percentage` or
///   `discountAmount` reward is rejected with INVALID;
/// * a code bxgy create must specify `discountOnQuantity.quantity`; an
///   automatic bxgy create omits the quantity-blank check (a Shopify quirk
///   where only code bxgy validates the quantity at create time);
/// * `appliesOnSubscription` / `appliesOnOneTimePurchase` are not supported on
///   bxgy discounts at all (distinct message for code vs automatic).
fn discount_bxgy_customer_gets_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
    typename: &str,
    create: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let input_value = ResolvedValue::Object(input.clone());
    let is_code = typename.starts_with("DiscountCode");
    let unsupported_message = if is_code {
        "This field is not supported by bxgy discounts."
    } else {
        "This field is not supported by automatic bxgy discounts."
    };

    if resolved_object_path(Some(&input_value), &["customerGets", "value", "percentage"]).is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets", "value", "percentage"],
            "Only discountOnQuantity permitted with bxgy discounts.",
            "INVALID",
        ));
    }
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "value", "discountAmount"],
    )
    .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets", "value", "discountAmount"],
            "Only discountOnQuantity permitted with bxgy discounts.",
            "INVALID",
        ));
    }
    if is_code && create {
        let quantity_blank = match resolved_object_path(
            Some(&input_value),
            &["customerGets", "value", "discountOnQuantity", "quantity"],
        ) {
            Some(ResolvedValue::String(q)) => q.trim().is_empty(),
            None => true,
            Some(_) => false,
        };
        if quantity_blank {
            errors.push(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Quantity cannot be blank.",
                "BLANK",
            ));
        }
    }
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "appliesOnSubscription"],
    )
    .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets", "appliesOnSubscription"],
            unsupported_message,
            "INVALID",
        ));
    }
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "appliesOnOneTimePurchase"],
    )
    .is_some()
    {
        errors.push(discount_user_error(
            vec![input_arg, "customerGets", "appliesOnOneTimePurchase"],
            unsupported_message,
            "INVALID",
        ));
    }
    // A bxgy create must entitle concrete `customerBuys` items; an "all" items block
    // (or an omitted one) is rejected as undefined. Validated on create only — an
    // update that leaves `customerBuys` untouched must not be forced to redefine it.
    if create {
        let buys_items_present =
            resolved_object_path(Some(&input_value), &["customerBuys", "items"]).is_some();
        let buys_all = resolved_bool_path(input, &["customerBuys", "items", "all"]) == Some(true);
        if !buys_items_present || buys_all {
            errors.push(discount_user_error(
                vec![input_arg, "customerBuys", "items"],
                "Items in 'customer buys' must be defined",
                "BLANK",
            ));
        }
    }
    errors
}

/// Validate `combinesWith` against the discount class. Two business rules apply:
///
/// * `productDiscountsWithTagsOnSameCartLine` is a plan-gated, PRODUCT-class-only
///   setting. This store's plan is not entitled to it, and a basic (non-product)
///   discount can never set it, so both errors are surfaced together.
/// * A discount may not combine with its own class. A free-shipping (SHIPPING
///   class) discount that sets `combinesWith.shippingDiscounts` is self-combining,
///   which Shopify rejects with `INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS`.
///
/// Tag add/remove overlaps are handled earlier as a top-level BAD_REQUEST error, so
/// they never reach this resolver-level validation.
fn discount_combines_with_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
    _typename: &str,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_object_path(
        Some(&input_value),
        &["combinesWith", "productDiscountsWithTagsOnSameCartLine"],
    )
    .is_some()
    {
        errors.push(discount_user_error(
            vec![
                input_arg,
                "combinesWith",
                "productDiscountsWithTagsOnSameCartLine",
            ],
            "The shop's plan does not allow setting `productDiscountsWithTagsOnSameCartLine`.",
            "PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_NOT_ENTITLED",
        ));
        errors.push(discount_user_error(
            vec![input_arg, "combinesWith", "productDiscountsWithTagsOnSameCartLine"],
            "Combines with product discounts with tags on same cart line is only valid for discounts with the PRODUCT discount class",
            "INVALID_PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_FOR_DISCOUNT_CLASS",
        ));
    }
    // NOTE: the free-shipping self-combine (INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS)
    // error is emitted ahead of the title check in `discount_input_user_errors` to
    // match Shopify's validation order, so it is intentionally not re-emitted here.
    errors
}

fn discount_context_customer_selection_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
) -> Option<Value> {
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_object_path(Some(&input_value), &["context"]).is_some()
        && resolved_object_path(Some(&input_value), &["customerSelection"]).is_some()
    {
        return Some(discount_user_error(
            vec![input_arg, "context"],
            DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE,
            "INVALID",
        ));
    }
    None
}

/// Map a discount mutation root field to its typed input argument name, then
/// return the resolved input object. The public Admin API names the create/update
/// input argument after the discount kind (e.g. `basicCodeDiscount`), not `input`.
fn discount_field_input(field: &RootFieldSelection) -> Option<BTreeMap<String, ResolvedValue>> {
    let input_arg = match field.name.as_str() {
        "discountCodeBasicCreate" | "discountCodeBasicUpdate" => "basicCodeDiscount",
        "discountCodeBxgyCreate" | "discountCodeBxgyUpdate" => "bxgyCodeDiscount",
        "discountCodeFreeShippingCreate" | "discountCodeFreeShippingUpdate" => {
            "freeShippingCodeDiscount"
        }
        "discountAutomaticBasicCreate" | "discountAutomaticBasicUpdate" => "automaticBasicDiscount",
        "discountAutomaticBxgyCreate" | "discountAutomaticBxgyUpdate" => "automaticBxgyDiscount",
        "discountAutomaticFreeShippingCreate" | "discountAutomaticFreeShippingUpdate" => {
            "freeShippingAutomaticDiscount"
        }
        _ => return None,
    };
    discount_input(field, input_arg)
}

/// Variable-coercion failures abort the whole GraphQL document before any resolver
/// runs, so Shopify returns only an `errors` array with no `data`. Detect bxgy
/// numeric coercion failures here and short-circuit the entire mutation.
fn discount_document_level_error_response(fields: &[RootFieldSelection]) -> Option<Response> {
    for field in fields {
        if !field.name.contains("Bxgy") {
            continue;
        }
        let Some(input) = discount_field_input(field) else {
            continue;
        };
        let is_code = field.name.starts_with("discountCode");
        let is_create = field.name.ends_with("Create");
        let graphql_type = if is_code {
            "DiscountCodeBxgyInput"
        } else {
            "DiscountAutomaticBxgyInput"
        };
        if let Some(error) = discount_bxgy_variable_error(&input, is_code, is_create, graphql_type)
        {
            return Some(ok_json(json!({ "errors": [error] })));
        }
    }
    None
}

/// Detect a single discount field's resolver-level rejection that Shopify surfaces
/// as a top-level `BAD_REQUEST` error keyed by the field alias: add/remove id
/// overlaps, customerSelection-all conflicts, multiple customerGets value types, and
/// combinesWith tag overlaps. Sibling fields in the same document still resolve, so
/// this returns just the one error (the caller nulls the field's data slot).
fn discount_field_top_level_error(field: &RootFieldSelection) -> Option<Value> {
    if field.name == "discountRedeemCodeBulkAdd" {
        let codes = resolved_redeem_codes(field);
        if codes.len() > 250 {
            // Shopify enforces the 250-entry list ceiling at the GraphQL layer
            // before the resolver runs, so it surfaces as a top-level error
            // (not a userError).
            return Some(json!({
                "message": format!(
                    "The input array size of {} is greater than the maximum allowed of 250.",
                    codes.len()
                ),
                "locations": [{ "line": field.location.line, "column": field.location.column }],
                "path": [field.response_key.clone(), "codes".to_string()],
                "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" },
            }));
        }
    }
    let input = discount_field_input(field)?;
    let message = discount_bad_request_conflict_message(&input)?;
    Some(json!({
        "message": message,
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "extensions": { "code": "BAD_REQUEST" },
        "path": [field.response_key.clone()],
    }))
}

fn discount_bad_request_conflict_message(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_bool_path(input, &["customerSelection", "all"]) == Some(true) {
        if resolved_object_path(Some(&input_value), &["customerSelection", "customers"]).is_some()
            || resolved_object_path(
                Some(&input_value),
                &["customerSelection", "customerSavedSearches"],
            )
            .is_some()
        {
            return Some(
                "A discount cannot have customerSelection set to all, when customers or customerSavedSearches is specified."
                    .to_string(),
            );
        }
        if resolved_object_path(
            Some(&input_value),
            &["customerSelection", "customerSegments"],
        )
        .is_some()
        {
            return Some(
                "A discount cannot have customerSelection set to all, when customerSegments is specified."
                    .to_string(),
            );
        }
    }
    if let Some(ResolvedValue::Object(value)) =
        resolved_object_path(Some(&input_value), &["customerGets", "value"])
    {
        let present = ["percentage", "discountOnQuantity", "discountAmount"]
            .iter()
            .filter(|key| value.contains_key(**key))
            .count();
        if present > 1 {
            return Some(
                "A discount can only have one of percentage, discountOnQuantity or discountAmount."
                    .to_string(),
            );
        }
    }
    if discount_add_remove_overlap(
        input,
        &["customerSelection", "customers", "add"],
        &["customerSelection", "customers", "remove"],
    ) {
        return Some("A customer id is present in `add` and `remove` fields".to_string());
    }
    for base in [["customerGets", "items"], ["customerBuys", "items"]] {
        if discount_add_remove_overlap(
            input,
            &[base[0], base[1], "products", "productVariantsToAdd"],
            &[base[0], base[1], "products", "productVariantsToRemove"],
        ) {
            return Some(
                "The same ProductVariant id is present in both 'add' and 'remove' fields"
                    .to_string(),
            );
        }
        if discount_add_remove_overlap(
            input,
            &[base[0], base[1], "collections", "add"],
            &[base[0], base[1], "collections", "remove"],
        ) {
            return Some(
                "The same Collection id is present in both 'add' and 'remove' fields".to_string(),
            );
        }
    }
    if discount_add_remove_overlap(
        input,
        &["destination", "countries", "add"],
        &["destination", "countries", "remove"],
    ) {
        return Some("A country code is present in `add` and `remove` field".to_string());
    }
    for tag_field in [
        "productDiscountsWithTagsOnSameCartLine",
        "orderDiscountsWithTagsOnSameCartLine",
        "shippingDiscountsWithTagsOnSameCartLine",
    ] {
        if discount_add_remove_overlap(
            input,
            &["combinesWith", tag_field, "add"],
            &["combinesWith", tag_field, "remove"],
        ) {
            return Some(format!(
                "The same tag is present in both `add` and `remove` fields of `{tag_field}`."
            ));
        }
    }
    None
}

fn discount_add_remove_overlap(
    input: &BTreeMap<String, ResolvedValue>,
    add_path: &[&str],
    remove_path: &[&str],
) -> bool {
    let add = resolved_string_list_path(input, add_path);
    if add.is_empty() {
        return false;
    }
    let remove: std::collections::BTreeSet<String> = resolved_string_list_path(input, remove_path)
        .into_iter()
        .collect();
    add.iter().any(|id| remove.contains(id))
}

fn app_discount_input_user_errors(
    input: Option<&BTreeMap<String, ResolvedValue>>,
    input_arg: &str,
    typename: &str,
    create: bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let Some(input) = input else {
        errors.push(app_discount_user_error(
            vec![json!(input_arg)],
            "Input is required.",
            Some("REQUIRED"),
        ));
        return errors;
    };
    let code_app = typename == "DiscountCodeApp";
    let validate_title = !code_app || create || resolved_string_path(input, &["title"]).is_some();
    if validate_title {
        match resolved_string_path(input, &["title"]) {
            Some(title) if title.trim().is_empty() => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("title")],
                if code_app {
                    "can't be blank"
                } else {
                    "Title can't be blank."
                },
                Some("INVALID"),
            )),
            Some(title) if title.chars().count() > 255 => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("title")],
                "is too long (maximum is 255 characters)",
                Some("INVALID"),
            )),
            Some(_) => {}
            None => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("title")],
                if code_app {
                    "Required argument not found."
                } else {
                    "Title can't be blank."
                },
                Some("INVALID"),
            )),
        }
    }
    if code_app {
        match resolved_string_path(input, &["code"]) {
            Some(code) if code.trim().is_empty() => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("code")],
                "Discount code can't be blank.",
                Some("INVALID"),
            )),
            Some(code) if code.contains('\n') || code.contains('\r') => {
                errors.push(app_discount_user_error(
                    vec![json!(input_arg), json!("code")],
                    "Code cannot contain newline characters.",
                    Some("INVALID"),
                ))
            }
            Some(code) if code.chars().count() > 255 => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("code")],
                "Code is too long (maximum is 255 characters)",
                Some("INVALID"),
            )),
            Some(_) => {}
            None if create => errors.push(app_discount_user_error(
                vec![json!(input_arg), json!("code")],
                "Discount code can't be blank.",
                Some("INVALID"),
            )),
            None => {}
        }
    }
    if create && resolved_non_blank_string_field(input, "startsAt").is_none() {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("startsAt")],
            "Starts at can't be blank.",
            Some("INVALID"),
        ));
    }
    if matches!(
        resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &["discountClasses"]),
        Some(ResolvedValue::List(values)) if values.is_empty()
    ) {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("discountClasses")],
            "Discount classes can't be empty.",
            Some("INVALID"),
        ));
    }
    if discount_context_customer_selection_user_error(input, input_arg).is_some() {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("context")],
            DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE,
            Some("INVALID"),
        ));
    }
    if app_discount_empty_customer_selection(input) {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("customerSelection")],
            "a minimum of one prerequisite segment or prerequisite customer must be provided",
            Some("INVALID"),
        ));
    }
    if typename == "DiscountAutomaticApp" && input.contains_key("channelIds") {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("channelIds")],
            "Channel IDs are not supported for automatic app discounts.",
            Some("INVALID"),
        ));
    }
    if resolved_bool_path(input, &["markets", "removeAllMarkets"]).unwrap_or(false)
        && !resolved_string_list_path(input, &["markets", "add"]).is_empty()
    {
        errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("markets")],
            "Cannot add markets while removeAllMarkets is true.",
            Some("INVALID"),
        ));
    }
    let function_id = resolved_non_blank_string_field(input, "functionId");
    let function_handle = resolved_non_blank_string_field(input, "functionHandle");
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => errors.push(app_discount_user_error(
            vec![json!(input_arg), json!("functionHandle")],
            "Function id can't be blank.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        )),
        (true, true) => errors.push(app_discount_user_error(
            vec![json!(input_arg)],
            "Only one of functionId or functionHandle is allowed.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        )),
        _ => {}
    }
    errors
}

fn app_discount_empty_customer_selection(input: &BTreeMap<String, ResolvedValue>) -> bool {
    matches!(
        resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerSelection", "customerSegments", "add"],
        ),
        Some(ResolvedValue::List(values)) if values.is_empty()
    ) || matches!(
        resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerSelection", "customers", "add"],
        ),
        Some(ResolvedValue::List(values)) if values.is_empty()
    )
}

fn app_discount_user_error(field: Vec<Value>, message: &str, code: Option<&str>) -> Value {
    user_error_with_extra_info(field, message, code, Value::Null)
}

/// Enforce the signed 32-bit integer bounds Shopify applies to `usageLimit` and
/// `recurringCycleLimit`. These accumulate (a value below the minimum trips both the
/// "must be greater than 0" and the "must be >= -2147483648" guards).
fn discount_usage_recurring_bounds_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
) -> Vec<Value> {
    const I32_MAX: i64 = 2147483647;
    const I32_MIN: i64 = -2147483648;
    let mut errors = Vec::new();
    if let Some(usage_limit) = resolved_i64_path(input, &["usageLimit"]) {
        if usage_limit > I32_MAX {
            errors.push(discount_user_error(
                vec![input_arg, "usageLimit"],
                "Usage limit must be less than or equal to 2147483647",
                "LESS_THAN_OR_EQUAL_TO",
            ));
        }
        if usage_limit <= 0 {
            errors.push(discount_user_error(
                vec![input_arg, "usageLimit"],
                "Usage limit must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if usage_limit < I32_MIN {
            errors.push(discount_user_error(
                vec![input_arg, "usageLimit"],
                "Usage limit must be greater than or equal to -2147483648",
                "GREATER_THAN_OR_EQUAL_TO",
            ));
        }
    }
    if let Some(recurring_cycle_limit) = resolved_i64_path(input, &["recurringCycleLimit"]) {
        if recurring_cycle_limit > I32_MAX {
            errors.push(discount_user_error(
                vec![input_arg, "recurringCycleLimit"],
                "Recurring cycle limit must be less than or equal to 2147483647",
                "LESS_THAN_OR_EQUAL_TO",
            ));
        }
    }
    errors
}

fn discount_subscription_field_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
) -> Option<Value> {
    let input_value = ResolvedValue::Object(input.clone());
    // The "subscriptions-only" carveout: a discount scoped to subscriptions only
    // (appliesOnSubscription: true AND appliesOnOneTimePurchase: false) is the one
    // selling-plan path the shop IS entitled to, so its subscription/recurring
    // fields are permitted. Any other use of these fields is gated off.
    let subscription_only = |scope: &[&str]| -> bool {
        let on_sub: Vec<&str> = scope
            .iter()
            .copied()
            .chain(["appliesOnSubscription"])
            .collect();
        let on_one: Vec<&str> = scope
            .iter()
            .copied()
            .chain(["appliesOnOneTimePurchase"])
            .collect();
        resolved_bool_path(input, &on_sub) == Some(true)
            && resolved_bool_path(input, &on_one) == Some(false)
    };
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "appliesOnSubscription"],
    )
    .is_some()
        && !subscription_only(&["customerGets"])
    {
        return Some(discount_user_error(
            vec![input_arg, "customerGets", "appliesOnSubscription"],
            "Customer gets applies on subscription is not permitted for this shop.",
            "INVALID",
        ));
    }
    if resolved_object_path(Some(&input_value), &["appliesOnSubscription"]).is_some()
        && !subscription_only(&[])
    {
        return Some(discount_user_error(
            vec![input_arg, "appliesOnSubscription"],
            "Applies on subscription is not permitted for this shop.",
            "INVALID",
        ));
    }
    if resolved_object_path(Some(&input_value), &["appliesOnOneTimePurchase"]).is_some()
        && !subscription_only(&[])
    {
        return Some(discount_user_error(
            vec![input_arg, "appliesOnOneTimePurchase"],
            "Applies on one time purchase is not permitted for this shop.",
            "INVALID",
        ));
    }
    if resolved_object_path(Some(&input_value), &["recurringCycleLimit"]).is_some()
        && !subscription_only(&[])
        && !subscription_only(&["customerGets"])
    {
        return Some(discount_user_error(
            vec![input_arg, "recurringCycleLimit"],
            "Recurring cycle limit is not permitted for this shop.",
            "INVALID",
        ));
    }
    None
}

fn discount_numeric_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    input_arg: &str,
    typename: &str,
) -> Option<Value> {
    let is_automatic_basic = typename == "DiscountAutomaticBasic";
    if let Some(minimum_quantity) = resolved_i64_path(
        input,
        &[
            "minimumRequirement",
            "quantity",
            "greaterThanOrEqualToQuantity",
        ],
    ) {
        if minimum_quantity >= DISCOUNT_MINIMUM_QUANTITY_UPPER_BOUND {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "minimumRequirement",
                    "quantity",
                    "greaterThanOrEqualToQuantity",
                ],
                "Minimum quantity must be less than 2147483647",
                "LESS_THAN",
            ));
        }
    }
    if resolved_decimal_path_at_or_above(
        input,
        &[
            "minimumRequirement",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
        ],
        DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND,
        DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND_DECIMAL,
    ) {
        return Some(discount_user_error(
            vec![
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal must be less than 1000000000000000000",
            "LESS_THAN",
        ));
    }
    if let Some(percentage) = resolved_f64_path(input, &["customerGets", "value", "percentage"]) {
        let outside_range = if is_automatic_basic {
            !(percentage > 0.0 && percentage <= 1.0)
        } else {
            !(0.0..=1.0).contains(&percentage)
        };
        if outside_range {
            return Some(discount_user_error(
                vec![input_arg, "customerGets", "value", "percentage"],
                "Value must be between 0.0 and 1.0",
                "VALUE_OUTSIDE_RANGE",
            ));
        }
    }
    if let Some(amount) = resolved_f64_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        if is_automatic_basic && amount <= 0.0 {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be less than 0",
                "GREATER_THAN",
            ));
        }
        if !is_automatic_basic && amount < 0.0 {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be less than or equal to 0",
                "LESS_THAN_OR_EQUAL_TO",
            ));
        }
        if amount >= 1_000_000_000_000_000_000.0 {
            return Some(discount_user_error(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be greater than -1000000000000000000",
                "LESS_THAN",
            ));
        }
    }
    if typename.contains("Bxgy") {
        if let Some(error) = discount_bxgy_user_error(input, input_arg) {
            return Some(error);
        }
    }
    None
}

fn discount_record_from_input(
    id: &str,
    kind: &str,
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
) -> Value {
    let title = resolved_string_path(input, &["title"])
        .or_else(|| existing.and_then(|record| record["title"].as_str().map(str::to_string)))
        .unwrap_or_else(|| "Untitled discount".to_string());
    let code = resolved_string_path(input, &["code"])
        .or_else(|| existing.and_then(|record| record["code"].as_str().map(str::to_string)));
    let starts_at = resolved_string_path(input, &["startsAt"])
        .or_else(|| existing.and_then(|record| record["startsAt"].as_str().map(str::to_string)))
        .unwrap_or_else(|| DISCOUNT_DEFAULT_TIMESTAMP.to_string());
    let ends_at = resolved_string_path(input, &["endsAt"])
        .map(Value::String)
        .or_else(|| existing.map(|record| record["endsAt"].clone()))
        .unwrap_or(Value::Null);
    let created_at = existing
        .and_then(|record| record["createdAt"].as_str().map(str::to_string))
        .unwrap_or_else(|| DISCOUNT_DEFAULT_TIMESTAMP.to_string());
    let status = discount_status_from_dates(&starts_at, &ends_at);
    let combines_with = resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["combinesWith"],
    )
    .map(resolved_value_json)
    .or_else(|| existing.map(|record| record["combinesWith"].clone()))
    .unwrap_or_else(|| {
        json!({
            "productDiscounts": false,
            "orderDiscounts": false,
            "shippingDiscounts": false
        })
    });
    let codes = code
        .as_ref()
        .map(|code| {
            json!([{
                "id": synthetic_shopify_gid("DiscountRedeemCode", stable_redeem_code_suffix(code)),
                "code": code,
                "asyncUsageCount": 0
            }])
        })
        .or_else(|| existing.map(|record| record["codes"].clone()))
        .unwrap_or_else(|| json!([]));
    json!({
        "id": id,
        "kind": kind,
        "typename": typename,
        "title": title,
        "code": code,
        "status": status,
        "startsAt": starts_at,
        "endsAt": ends_at,
        "createdAt": created_at,
        "updatedAt": DISCOUNT_DEFAULT_TIMESTAMP,
        "asyncUsageCount": 0,
        "usageLimit": resolved_i64_path(input, &["usageLimit"]).map(Value::from).unwrap_or(Value::Null),
        "usesPerOrderLimit": resolved_i64_path(input, &["usesPerOrderLimit"]).map(Value::from).unwrap_or(Value::Null),
        "recurringCycleLimit": resolved_i64_path(input, &["recurringCycleLimit"])
            .map(Value::from)
            .or_else(|| existing.map(|record| record["recurringCycleLimit"].clone()))
            .unwrap_or(Value::Null),
        "discountClasses": discount_classes_for_input(typename, input),
        "combinesWith": combines_with,
        "context": discount_context_from_input(input),
        "customerBuys": discount_customer_buys_from_input(typename, input),
        "customerGets": discount_customer_gets_from_input(typename, input),
        "minimumRequirement": discount_minimum_requirement_from_input(input),
        "destinationSelection": discount_destination_selection_from_input(input),
        "maximumShippingPrice": discount_maximum_shipping_price_from_input(input),
        "appliesOncePerCustomer": resolved_bool_path(input, &["appliesOncePerCustomer"]).unwrap_or(false),
        "appliesOnOneTimePurchase": resolved_bool_path(input, &["appliesOnOneTimePurchase"]).unwrap_or(true),
        "appliesOnSubscription": resolved_bool_path(input, &["appliesOnSubscription"]).unwrap_or(false),
        "codes": codes,
        "codesCount": {
            "count": codes.as_array().map(Vec::len).unwrap_or(0),
            "precision": "EXACT"
        },
        "metafields": discount_metafields_from_input(input)
            .or_else(|| existing.map(|record| record["metafields"].clone()))
            .unwrap_or_else(|| json!([])),
        "summary": discount_summary_for_input(typename, input)
    })
}

fn attach_app_discount_function(record: &mut Value, function: &Value) {
    record["shopifyFunction"] = function.clone();
    record["appDiscountType"] = app_discount_type_for_function(function);
}

fn app_discount_type_for_function(function: &Value) -> Value {
    let function_id = function["handle"]
        .as_str()
        .or_else(|| function["id"].as_str())
        .unwrap_or_default();
    json!({
        "appKey": function.get("appKey").cloned().unwrap_or(Value::Null),
        "functionId": function_id,
        "title": function.get("title").cloned().unwrap_or(Value::Null),
        "description": function.get("description").cloned().unwrap_or(Value::Null)
    })
}

fn app_discount_function_api_type_is_supported(function: &Value) -> bool {
    let api_type = function["apiType"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        api_type.as_str(),
        "discount" | "product_discounts" | "order_discounts" | "shipping_discounts"
    )
}

fn discount_node_for_record(record: &Value) -> Value {
    let discount = discount_body_for_record(record);
    if discount_kind(record) == "automatic" {
        json!({
            "id": discount_id(record),
            "automaticDiscount": discount,
            "__typename": "DiscountAutomaticNode"
        })
    } else {
        json!({
            "id": discount_id(record),
            "codeDiscount": discount,
            "__typename": "DiscountCodeNode"
        })
    }
}

fn discount_admin_node_for_record(record: &Value) -> Value {
    json!({
        "id": discount_id(record),
        "discount": discount_body_for_record(record),
        "__typename": if discount_kind(record) == "automatic" {
            "DiscountAutomaticNode"
        } else {
            "DiscountCodeNode"
        }
    })
}

fn discount_body_for_record(record: &Value) -> Value {
    let metafields = record
        .get("metafields")
        .cloned()
        .unwrap_or_else(|| json!([]));
    json!({
        "__typename": record["typename"],
        "discountId": record["id"],
        "title": record["title"],
        "status": record["status"],
        "summary": record["summary"],
        "startsAt": record["startsAt"],
        "endsAt": record["endsAt"],
        "createdAt": record["createdAt"],
        "updatedAt": record["updatedAt"],
        "asyncUsageCount": record["asyncUsageCount"],
        "usageLimit": record["usageLimit"],
        "usesPerOrderLimit": record["usesPerOrderLimit"],
        "discountClasses": record["discountClasses"],
        "combinesWith": record["combinesWith"],
        "context": record["context"],
        "customerBuys": record["customerBuys"],
        "customerGets": record["customerGets"],
        "minimumRequirement": record["minimumRequirement"],
        "codes": {
            "nodes": record["codes"],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        },
        "codesCount": record["codesCount"],
        "destinationSelection": record["destinationSelection"],
        "maximumShippingPrice": record["maximumShippingPrice"],
        "appliesOncePerCustomer": record["appliesOncePerCustomer"],
        "appliesOnOneTimePurchase": record["appliesOnOneTimePurchase"],
        "appliesOnSubscription": record["appliesOnSubscription"],
        "recurringCycleLimit": record.get("recurringCycleLimit").cloned().unwrap_or(Value::Null),
        "appDiscountType": record.get("appDiscountType").cloned().unwrap_or(Value::Null),
        "metafields": {
            "nodes": metafields,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        }
    })
}

fn app_discount_payload_for_root(root: &str, node: Value, user_errors: Vec<Value>) -> Value {
    let node_key = if root.starts_with("discountAutomatic") {
        "automaticAppDiscount"
    } else {
        "codeAppDiscount"
    };
    json!({
        node_key: if node.is_null() { Value::Null } else { node },
        "userErrors": user_errors
    })
}

fn discount_payload_for_root(root: &str, node: Value, user_errors: Vec<Value>) -> Value {
    let node_key = if root.starts_with("discountAutomatic") {
        "automaticDiscountNode"
    } else {
        "codeDiscountNode"
    };
    json!({
        node_key: if node.is_null() { Value::Null } else { node },
        "userErrors": user_errors
    })
}

fn discount_delete_payload(root: &str, deleted_id: Value, user_errors: Vec<Value>) -> Value {
    let key = if root == "discountAutomaticDelete" {
        "deletedAutomaticDiscountId"
    } else {
        "deletedCodeDiscountId"
    };
    json!({ key: deleted_id, "userErrors": user_errors })
}

fn discount_unknown_id_user_error(root: &str) -> Value {
    let message = if root.starts_with("discountAutomatic") {
        "Automatic discount does not exist."
    } else {
        "Code discount does not exist."
    };
    discount_user_error(vec!["id"], message, "INVALID")
}

fn discount_id(record: &Value) -> &str {
    record["id"].as_str().unwrap_or_default()
}

fn discount_kind(record: &Value) -> &str {
    record["kind"].as_str().unwrap_or_default()
}

fn discount_record_codes(record: &Value) -> Vec<String> {
    let mut codes = Vec::new();
    if let Some(redeem_codes) = record.get("codes").and_then(Value::as_array) {
        for redeem_code in redeem_codes {
            if let Some(code) = redeem_code.get("code").and_then(Value::as_str) {
                codes.push(code.to_string());
            }
        }
    }
    codes
}

/// The three discount bulk transitions. Code and automatic families share the
/// same effects; the family only narrows which staged discounts are eligible.
#[derive(Clone, Copy)]
enum DiscountBulkAction {
    Activate,
    Deactivate,
    Delete,
}

/// Classify a bulk mutation root field into its (discount kind, action). Returns
/// `None` for anything that is not one of the six
/// `discount{Code,Automatic}Bulk{Activate,Deactivate,Delete}` mutations (notably
/// the redeem-code bulk add/delete mutations, which are handled separately).
fn discount_bulk_root_action(name: &str) -> Option<(&'static str, DiscountBulkAction)> {
    match name {
        "discountCodeBulkActivate" => Some(("code", DiscountBulkAction::Activate)),
        "discountCodeBulkDeactivate" => Some(("code", DiscountBulkAction::Deactivate)),
        "discountCodeBulkDelete" => Some(("code", DiscountBulkAction::Delete)),
        "discountAutomaticBulkActivate" => Some(("automatic", DiscountBulkAction::Activate)),
        "discountAutomaticBulkDeactivate" => Some(("automatic", DiscountBulkAction::Deactivate)),
        "discountAutomaticBulkDelete" => Some(("automatic", DiscountBulkAction::Delete)),
        _ => None,
    }
}

/// Whether a mutation root field is a discount bulk activate / deactivate /
/// delete. These forward upstream for the async `job`, then apply their effect
/// to the local overlay so later reads stay consistent.
pub(in crate::proxy) fn is_discount_bulk_action_root(name: &str) -> bool {
    discount_bulk_root_action(name).is_some()
}

fn discount_matches_query(record: &Value, query: &str) -> bool {
    let normalized = query.to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }
    if normalized.contains("status:active") && record["status"].as_str() != Some("ACTIVE") {
        return false;
    }
    if normalized.contains("status:expired") && record["status"].as_str() != Some("EXPIRED") {
        return false;
    }
    if normalized.contains("status:scheduled") && record["status"].as_str() != Some("SCHEDULED") {
        return false;
    }
    if normalized.contains("type:free_shipping") {
        return record["typename"]
            .as_str()
            .map(|typename| typename.contains("FreeShipping"))
            .unwrap_or(false);
    }
    if normalized.contains("type:automatic") {
        return discount_kind(record) == "automatic";
    }
    // `type:app` narrows to app-managed (Function-backed) discounts, whose
    // concrete type is DiscountCodeApp / DiscountAutomaticApp.
    if normalized.contains("type:app") {
        return record["typename"]
            .as_str()
            .map(|typename| typename.contains("App"))
            .unwrap_or(false);
    }
    // `discount_class:<class>` narrows by the discount's discountClasses set
    // (PRODUCT / ORDER / SHIPPING). Multiple class tokens AND together.
    for token in normalized.split_whitespace() {
        if let Some(class) = token.strip_prefix("discount_class:") {
            let matches_class = record["discountClasses"]
                .as_array()
                .map(|classes| {
                    classes
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|existing| existing.eq_ignore_ascii_case(class))
                })
                .unwrap_or(false);
            if !matches_class {
                return false;
            }
        }
    }
    true
}

fn resolved_string_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn resolved_non_blank_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    resolved_string_field(input, field).filter(|value| !value.trim().is_empty())
}

fn resolved_f64_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<f64> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Float(value)) => Some(*value),
        Some(ResolvedValue::Int(value)) => Some(*value as f64),
        Some(ResolvedValue::String(value)) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn resolved_decimal_path_at_or_above(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
    integer_limit: i64,
    decimal_integer_limit: &str,
) -> bool {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Int(value)) => *value >= integer_limit,
        Some(ResolvedValue::Float(value)) => *value >= integer_limit as f64,
        Some(ResolvedValue::String(value)) => {
            decimal_string_at_or_above(value, decimal_integer_limit)
        }
        _ => false,
    }
}

fn decimal_string_at_or_above(raw: &str, integer_limit: &str) -> bool {
    let trimmed = raw.trim();
    let unsigned = trimmed.strip_prefix('+').unwrap_or(trimmed);
    if unsigned.starts_with('-') {
        return false;
    }
    if unsigned.contains('e') || unsigned.contains('E') {
        return unsigned
            .parse::<f64>()
            .map(|value| {
                integer_limit
                    .parse::<f64>()
                    .map(|limit| value >= limit)
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    }
    let integer = unsigned.split('.').next().unwrap_or("");
    if !integer.chars().all(|character| character.is_ascii_digit()) {
        return false;
    }
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    integer.len() > integer_limit.len()
        || (integer.len() == integer_limit.len() && integer >= integer_limit)
}

fn discount_status_from_dates(starts_at: &str, ends_at: &Value) -> &'static str {
    if starts_at > DISCOUNT_DEFAULT_TIMESTAMP {
        return "SCHEDULED";
    }
    if ends_at
        .as_str()
        .map(|ends_at| ends_at <= DISCOUNT_DEFAULT_TIMESTAMP)
        .unwrap_or(false)
    {
        return "EXPIRED";
    }
    "ACTIVE"
}

fn discount_classes_for_input(typename: &str, input: &BTreeMap<String, ResolvedValue>) -> Value {
    let explicit_classes = resolved_string_list_path(input, &["discountClasses"]);
    if !explicit_classes.is_empty() {
        return json!(explicit_classes);
    }
    if typename.contains("FreeShipping") {
        return json!(["SHIPPING"]);
    }
    let input_value = ResolvedValue::Object(input.clone());
    let items = resolved_object_path(Some(&input_value), &["customerGets", "items"]);
    if let Some(ResolvedValue::Object(items)) = items {
        if items.contains_key("products") || items.contains_key("collections") {
            return json!(["PRODUCT"]);
        }
    }
    json!(["ORDER"])
}

fn discount_context_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    // The buyer-context selection echoes back the customer/segment members it was
    // pointed at. We record the referenced ids here; display names / segment names
    // are filled in by `resolve_discount_context_names` from entities the store
    // already knows about (live Shopify resolves these from existing records too).
    if resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["context", "customers"],
    )
    .is_some()
    {
        let customers = resolved_string_list_path(input, &["context", "customers", "add"])
            .into_iter()
            .map(|id| json!({ "__typename": "Customer", "id": id }))
            .collect::<Vec<_>>();
        return json!({ "__typename": "DiscountCustomers", "customers": customers });
    }
    if resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["context", "customerSegments"],
    )
    .is_some()
    {
        let segments = resolved_string_list_path(input, &["context", "customerSegments", "add"])
            .into_iter()
            .map(|id| json!({ "__typename": "Segment", "id": id }))
            .collect::<Vec<_>>();
        return json!({ "__typename": "DiscountCustomerSegments", "segments": segments });
    }
    json!({ "__typename": "DiscountBuyerSelectionAll", "all": "ALL" })
}

fn discount_customer_buys_from_input(
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    if !typename.contains("Bxgy") {
        return Value::Null;
    }
    let quantity = resolved_scalar_text_path(input, &["customerBuys", "value", "quantity"])
        .unwrap_or_else(|| "1".to_string());
    json!({
        "value": { "__typename": "DiscountQuantity", "quantity": quantity },
        "items": discount_items_from_input(input, &["customerBuys", "items"])
    })
}

fn discount_customer_gets_from_input(
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let value = if typename.contains("Bxgy") {
        discount_on_quantity_value_from_input(input)
    } else if let Some(percentage) =
        resolved_f64_path(input, &["customerGets", "value", "percentage"])
    {
        json!({ "__typename": "DiscountPercentage", "percentage": percentage })
    } else if let Some(amount) = resolved_decimal_text_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": amount, "currencyCode": "CAD" },
            "appliesOnEachItem": false
        })
    } else {
        json!({ "__typename": "DiscountPercentage", "percentage": 0.1 })
    };
    json!({
        "value": value,
        "items": if typename.contains("Bxgy") {
            discount_items_from_input(input, &["customerGets", "items"])
        } else {
            json!({ "__typename": "AllDiscountItems", "allItems": true })
        },
        "appliesOnOneTimePurchase": true,
        "appliesOnSubscription": false
    })
}

fn discount_on_quantity_value_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let quantity = resolved_scalar_text_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    )
    .unwrap_or_else(|| "1".to_string());
    let effect = if let Some(percentage) = resolved_f64_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "percentage",
        ],
    ) {
        json!({ "__typename": "DiscountPercentage", "percentage": percentage })
    } else if let Some(amount) = resolved_decimal_text_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "discountAmount",
            "amount",
        ],
    ) {
        json!({
            "__typename": "DiscountAmount",
            "amount": { "amount": amount, "currencyCode": "CAD" },
            "appliesOnEachItem": false
        })
    } else {
        json!({ "__typename": "DiscountPercentage", "percentage": 1.0 })
    };
    json!({
        "__typename": "DiscountOnQuantity",
        "quantity": { "quantity": quantity },
        "effect": effect
    })
}

fn discount_items_from_input(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Value {
    let input_value = ResolvedValue::Object(input.clone());
    let mut products_path = path.to_vec();
    products_path.push("products");
    if resolved_object_path(Some(&input_value), &products_path).is_some() {
        let mut product_ids_path = products_path.clone();
        product_ids_path.push("productsToAdd");
        let mut variant_ids_path = products_path;
        variant_ids_path.push("productVariantsToAdd");
        return json!({
            "__typename": "DiscountProducts",
            "products": {
                "nodes": resolved_string_list_path(input, &product_ids_path)
                    .into_iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>()
            },
            "productVariants": {
                "nodes": resolved_string_list_path(input, &variant_ids_path)
                    .into_iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>()
            }
        });
    }
    let mut collections_path = path.to_vec();
    collections_path.push("collections");
    if resolved_object_path(Some(&input_value), &collections_path).is_some() {
        let mut add_path = collections_path.clone();
        add_path.push("add");
        let mut collections_to_add_path = collections_path;
        collections_to_add_path.push("collectionsToAdd");
        let ids = resolved_string_list_path(input, &add_path)
            .into_iter()
            .chain(resolved_string_list_path(input, &collections_to_add_path))
            .collect::<Vec<_>>();
        return json!({
            "__typename": "DiscountCollections",
            "collections": {
                "nodes": ids.into_iter().map(|id| json!({ "id": id })).collect::<Vec<_>>()
            }
        });
    }
    json!({ "__typename": "AllDiscountItems", "allItems": true })
}

fn discount_minimum_requirement_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    if let Some(amount) = resolved_decimal_text_path(
        input,
        &[
            "minimumRequirement",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
        ],
    ) {
        return json!({
            "__typename": "DiscountMinimumSubtotal",
            "greaterThanOrEqualToSubtotal": {
                "amount": amount,
                "currencyCode": "CAD"
            }
        });
    }
    if let Some(quantity) = resolved_i64_path(
        input,
        &[
            "minimumRequirement",
            "quantity",
            "greaterThanOrEqualToQuantity",
        ],
    ) {
        return json!({
            "__typename": "DiscountMinimumQuantity",
            "greaterThanOrEqualToQuantity": quantity
        });
    }
    Value::Null
}

fn discount_destination_selection_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_object_path(Some(&input_value), &["destination", "countries"]).is_some() {
        let countries = resolved_string_list_path(input, &["destination", "countries", "add"]);
        return json!({
            "__typename": "DiscountCountries",
            "countries": countries,
            "includeRestOfWorld": resolved_bool_path(input, &["destination", "countries", "includeRestOfWorld"]).unwrap_or(false)
        });
    }
    json!({ "__typename": "DiscountCountryAll", "allCountries": true })
}

fn discount_maximum_shipping_price_from_input(input: &BTreeMap<String, ResolvedValue>) -> Value {
    resolved_decimal_text_path(input, &["maximumShippingPrice"])
        .map(|amount| json!({ "amount": amount, "currencyCode": "CAD" }))
        .unwrap_or(Value::Null)
}

fn discount_metafields_from_input(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => Some(Value::Array(
            metafields
                .iter()
                .enumerate()
                .filter_map(|(index, value)| match value {
                    ResolvedValue::Object(metafield) => Some(json!({
                        "id": synthetic_shopify_gid("Metafield", format!("discount-app-{index}")),
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "createdAt": DISCOUNT_DEFAULT_TIMESTAMP,
                        "updatedAt": DISCOUNT_DEFAULT_TIMESTAMP
                    })),
                    _ => None,
                })
                .collect(),
        )),
        _ => None,
    }
}

fn resolved_decimal_text_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(shopify_decimal_text(value)),
        Some(ResolvedValue::Float(value)) => Some(shopify_decimal_text(&value.to_string())),
        Some(ResolvedValue::Int(value)) => Some(shopify_decimal_text(&value.to_string())),
        _ => None,
    }
}

fn shopify_decimal_text(value: &str) -> String {
    let Ok(parsed) = value.parse::<f64>() else {
        return value.to_string();
    };
    let mut formatted = parsed.to_string();
    if !formatted.contains('.') {
        formatted.push_str(".0");
    }
    formatted
}

fn resolved_scalar_text_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        Some(ResolvedValue::Int(value)) => Some(value.to_string()),
        Some(ResolvedValue::Float(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn resolved_string_list_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Vec<String> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn resolved_bool_path(input: &BTreeMap<String, ResolvedValue>, path: &[&str]) -> Option<bool> {
    match resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path) {
        Some(ResolvedValue::Bool(value)) => Some(*value),
        _ => None,
    }
}

/// The input argument name carrying the discount payload for each create field,
/// or `None` for non-create fields. Used to walk a create's entitlement
/// references before validation.
fn discount_create_input_arg(field_name: &str) -> Option<&'static str> {
    match field_name {
        "discountCodeBasicCreate" => Some("basicCodeDiscount"),
        "discountCodeBxgyCreate" => Some("bxgyCodeDiscount"),
        "discountCodeFreeShippingCreate" => Some("freeShippingCodeDiscount"),
        "discountCodeAppCreate" => Some("codeAppDiscount"),
        "discountAutomaticBasicCreate" => Some("automaticBasicDiscount"),
        "discountAutomaticBxgyCreate" => Some("automaticBxgyDiscount"),
        "discountAutomaticFreeShippingCreate" => Some("freeShippingAutomaticDiscount"),
        "discountAutomaticAppCreate" => Some("automaticAppDiscount"),
        _ => None,
    }
}

/// The input argument name carrying the discount payload for each create *or
/// update* field. Buyer-context member resolution walks both, because the segment
/// / customer selection a discount echoes back can be established at create time or
/// switched at update time and must resolve against real store state in either case.
fn discount_mutation_input_arg(field_name: &str) -> Option<&'static str> {
    match field_name {
        "discountCodeBasicCreate" | "discountCodeBasicUpdate" => Some("basicCodeDiscount"),
        "discountCodeBxgyCreate" | "discountCodeBxgyUpdate" => Some("bxgyCodeDiscount"),
        "discountCodeFreeShippingCreate" | "discountCodeFreeShippingUpdate" => {
            Some("freeShippingCodeDiscount")
        }
        "discountCodeAppCreate" | "discountCodeAppUpdate" => Some("codeAppDiscount"),
        "discountAutomaticBasicCreate" | "discountAutomaticBasicUpdate" => {
            Some("automaticBasicDiscount")
        }
        "discountAutomaticBxgyCreate" | "discountAutomaticBxgyUpdate" => {
            Some("automaticBxgyDiscount")
        }
        "discountAutomaticFreeShippingCreate" | "discountAutomaticFreeShippingUpdate" => {
            Some("freeShippingAutomaticDiscount")
        }
        "discountAutomaticAppCreate" | "discountAutomaticAppUpdate" => Some("automaticAppDiscount"),
        _ => None,
    }
}

/// Order two Shopify resource gids the way the conformance capture scripts do:
/// by numeric id tail ascending, with ties (and any non-numeric tail) broken by
/// the full gid string. Keeps a forwarded `nodes(ids:)` batch in the same order
/// as the recorded cassette so the request matches byte-for-byte.
fn compare_resource_ids(left: &str, right: &str) -> std::cmp::Ordering {
    match (
        resource_id_tail(left).parse::<u128>(),
        resource_id_tail(right).parse::<u128>(),
    ) {
        (Ok(left_tail), Ok(right_tail)) if left_tail != right_tail => left_tail.cmp(&right_tail),
        _ => left.cmp(right),
    }
}

fn discount_summary_for_input(typename: &str, input: &BTreeMap<String, ResolvedValue>) -> String {
    if typename.contains("FreeShipping") {
        return "Free shipping".to_string();
    } else if typename.contains("Bxgy") {
        return discount_bxgy_summary(input);
    }
    "Discount".to_string()
}

fn discount_bxgy_summary(input: &BTreeMap<String, ResolvedValue>) -> String {
    let buy_quantity =
        resolved_i64_path(input, &["customerBuys", "value", "quantity"]).unwrap_or(1);
    let get_quantity = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    )
    .unwrap_or(1);
    let effect_percentage = resolved_f64_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "percentage",
        ],
    )
    .unwrap_or(1.0);
    let buy_item = if buy_quantity == 1 { "item" } else { "items" };
    let get_item = if get_quantity == 1 { "item" } else { "items" };
    if (effect_percentage - 1.0).abs() < f64::EPSILON {
        format!("Buy {buy_quantity} {buy_item}, get {get_quantity} {get_item} free")
    } else {
        let percent = (effect_percentage * 100.0).round() as i64;
        format!("Buy {buy_quantity} {buy_item}, get {get_quantity} {get_item} at {percent}% off")
    }
}

pub(in crate::proxy) fn gift_card_lifecycle_base_card(id: &str) -> Value {
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": resource_id_path_tail(id),
        "lastCharacters": "2053",
        "maskedCode": "•••• •••• •••• 2053",
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": "2027-04-26",
        "note": "HAR-310 conformance gift card",
        "templateSuffix": null,
        "createdAt": "2026-04-29T09:31:02Z",
        "updatedAt": "2026-04-29T09:31:02Z",
        "initialValue": { "amount": "5.0", "currencyCode": "CAD" },
        "balance": { "amount": "5.0", "currencyCode": "CAD" },
        "customer": { "id": "gid://shopify/Customer/10552623464754" },
        "recipientAttributes": {
            "message": "HAR-464 recipient message",
            "preferredName": "HAR-464 recipient",
            "sendNotificationAt": null,
            "recipient": { "id": "gid://shopify/Customer/10552623464754" }
        },
        "transactions": {
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }
    })
}

pub(in crate::proxy) fn gift_card_configuration_record() -> Value {
    json!({
        "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
        "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
    })
}

pub(in crate::proxy) fn push_gift_card_transaction(card: &mut Value, transaction: Value) {
    if !card.get("transactions").is_some_and(Value::is_object) {
        card["transactions"] = json!({
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        });
    }
    if let Some(nodes) = card["transactions"]["nodes"].as_array_mut() {
        nodes.push(transaction);
    }
}

pub(in crate::proxy) fn gift_card_connection_json(
    cards: &[Value],
    selections: &[SelectedField],
) -> Value {
    let full = connection_json_with_empty_edges(cards.to_vec());
    selected_json(&full, selections)
}

pub(in crate::proxy) fn gift_card_count_json(count: usize, selections: &[SelectedField]) -> Value {
    let full = json!({ "count": count, "precision": "EXACT" });
    selected_json(&full, selections)
}

pub(in crate::proxy) fn backup_region_country(country_code: &str) -> Option<Value> {
    match country_code {
        "AE" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110482738",
            "name": "United Arab Emirates",
            "code": "AE"
        })),
        "AT" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110515506",
            "name": "Austria",
            "code": "AT"
        })),
        "AU" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110548274",
            "name": "Australia",
            "code": "AU"
        })),
        "BE" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110581042",
            "name": "Belgium",
            "code": "BE"
        })),
        "CA" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110417202",
            "name": "Canada",
            "code": "CA"
        })),
        "CH" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110613810",
            "name": "Switzerland",
            "code": "CH"
        })),
        "CZ" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110646578",
            "name": "Czechia",
            "code": "CZ"
        })),
        "DE" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110679346",
            "name": "Germany",
            "code": "DE"
        })),
        "DK" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110712114",
            "name": "Denmark",
            "code": "DK"
        })),
        "ES" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110744882",
            "name": "Spain",
            "code": "ES"
        })),
        "FI" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110777650",
            "name": "Finland",
            "code": "FI"
        })),
        "MX" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062111334706",
            "name": "Mexico",
            "code": "MX"
        })),
        "US" => Some(json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110449970",
            "name": "United States",
            "code": "US"
        })),
        _ => None,
    }
}

pub(in crate::proxy) fn backup_region_country_code_coercion_error(
    message: &str,
    operation_path: &str,
    code: &str,
) -> Value {
    let mut extensions = serde_json::Map::from_iter([("code".to_string(), json!(code))]);
    if code == "missingRequiredInputObjectAttribute" {
        extensions.insert("argumentName".to_string(), json!("countryCode"));
        extensions.insert("argumentType".to_string(), json!("CountryCode!"));
        extensions.insert(
            "inputObjectType".to_string(),
            json!("BackupRegionUpdateInput"),
        );
    } else {
        extensions.insert("typeName".to_string(), json!("InputObject"));
        extensions.insert("argumentName".to_string(), json!("countryCode"));
    }

    json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": 2, "column": 30 }],
            "path": [operation_path, "backupRegionUpdate", "region", "countryCode"],
            "extensions": extensions
        }]
    })
}

pub(in crate::proxy) fn is_known_shipping_package_id(id: &str) -> bool {
    matches!(
        id,
        "gid://shopify/ShippingPackage/1"
            | "gid://shopify/ShippingPackage/2"
            | "gid://shopify/ShippingPackage/10"
    )
}

pub(in crate::proxy) fn seed_shipping_package(id: &str) -> Value {
    match id {
        "gid://shopify/ShippingPackage/10" => json!({
            "id": "gid://shopify/ShippingPackage/10",
            "name": "Carrier flat-rate box",
            "type": "BOX",
            "boxType": "FLAT_RATE",
            "default": false,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-05-05T00:00:00.000Z",
            "updatedAt": "2026-05-05T00:00:00.000Z"
        }),
        "gid://shopify/ShippingPackage/2" => json!({
            "id": "gid://shopify/ShippingPackage/2",
            "name": "Backup mailer",
            "type": "ENVELOPE",
            "default": false,
            "weight": { "value": 0.5, "unit": "KILOGRAMS" },
            "dimensions": { "length": 8, "width": 6, "height": 1, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
        _ => json!({
            "id": id,
            "name": "Starter box",
            "type": "BOX",
            "default": true,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
    }
}

pub(in crate::proxy) fn merge_shipping_package_input(
    package: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    for (key, value) in input {
        package[key] = resolved_value_json(value);
    }
}

pub(in crate::proxy) fn local_node_value(
    id: &str,
    selection: &[SelectedField],
    backup_region: Option<&Value>,
) -> Option<Value> {
    if is_safe_no_data_node_gid(id) {
        return Some(Value::Null);
    }
    if let Some(region) = backup_region {
        if region.get("id").and_then(Value::as_str) == Some(id) {
            return Some(selected_json(region, selection));
        }
    }
    let full = match id {
        "gid://shopify/ShopAddress/63755419881" => json!({
            "id": "gid://shopify/ShopAddress/63755419881",
            "address1": "103 ossington",
            "address2": null,
            "city": "Ottawa",
            "company": null,
            "coordinatesValidated": false,
            "country": "Canada",
            "countryCodeV2": "CA",
            "formatted": ["103 ossington", "Ottawa ON k1s3b7", "Canada"],
            "formattedArea": "Ottawa ON, Canada",
            "latitude": 45.389817,
            "longitude": -75.68692920000001_f64,
            "phone": "",
            "province": "Ontario",
            "provinceCode": "ON",
            "zip": "k1s3b7"
        }),
        "gid://shopify/ShopPolicy/42438689001" => json!({
            "id": "gid://shopify/ShopPolicy/42438689001",
            "title": "Contact",
            "body": "<p></p>",
            "type": "CONTACT_INFORMATION",
            "url": "https://checkout.shopify.com/63755419881/policies/42438689001.html?locale=en",
            "createdAt": "2026-04-25T11:52:28Z",
            "updatedAt": "2026-04-25T11:52:29Z",
            "translations": []
        }),
        _ => return None,
    };
    Some(selected_json(&full, selection))
}

pub(in crate::proxy) fn is_safe_no_data_node_gid(id: &str) -> bool {
    [
        "gid://shopify/CashTrackingSession/",
        "gid://shopify/PointOfSaleDevice/",
        "gid://shopify/ShopifyPaymentsDispute/",
    ]
    .iter()
    .any(|prefix| id.starts_with(prefix))
}

pub(in crate::proxy) fn finance_risk_no_data_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "cashTrackingSession"
            | "pointOfSaleDevice"
            | "dispute"
            | "disputeEvidence"
            | "shopPayPaymentRequestReceipt" => Value::Null,
            "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" => {
                selected_json(&empty_nodes_edges_connection(), &field.selection)
            }
            _ => Value::Null,
        };
        data.insert(field.response_key.clone(), value);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn empty_nodes_edges_connection() -> Value {
    connection_json_with_empty_edges(Vec::new())
}

pub(in crate::proxy) fn discount_bxgy_variable_error(
    input: &BTreeMap<String, ResolvedValue>,
    is_code: bool,
    is_create: bool,
    graphql_type: &str,
) -> Option<Value> {
    let column = match (is_code, is_create) {
        (true, true) => 50,
        (true, false) => 60,
        (false, true) => 55,
        (false, false) => 65,
    };

    if let Some(value) = input.get("usesPerOrderLimit") {
        match (is_code, value) {
            (true, ResolvedValue::String(raw)) => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("Could not coerce value \"{raw}\" to Int"),
                    false,
                    column,
                ));
            }
            (false, ResolvedValue::String(raw)) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => {}
                Ok(n) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 '{n}' is out of range"),
                        true,
                        column,
                    ));
                }
                Err(_) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
            },
            (false, ResolvedValue::Int(n)) if *n < 0 => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("UnsignedInt64 '{n}' is out of range"),
                    true,
                    column,
                ));
            }
            _ => {}
        }
    }

    for (path, label) in [
        (
            vec!["customerBuys", "value", "quantity"],
            "customerBuys.value.quantity",
        ),
        (
            vec!["customerGets", "value", "discountOnQuantity", "quantity"],
            "customerGets.value.discountOnQuantity.quantity",
        ),
    ] {
        if let Some(value) =
            resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &path)
        {
            match value {
                ResolvedValue::String(raw) if raw.contains('.') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
                ResolvedValue::String(raw) if raw.starts_with('-') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 '{raw}' is out of range"),
                        true,
                        column,
                    ));
                }
                _ => {}
            }
        }
    }
    None
}

pub(in crate::proxy) fn discount_bxgy_invalid_variable(
    graphql_type: &str,
    label: &str,
    path: Vec<&str>,
    explanation: String,
    include_problem_message: bool,
    column: i64,
) -> Value {
    let mut problem = serde_json::Map::new();
    problem.insert("path".to_string(), json!(path));
    problem.insert("explanation".to_string(), json!(explanation));
    if include_problem_message {
        problem.insert("message".to_string(), problem["explanation"].clone());
    }
    json!({
        "message": format!("Variable $input of type {graphql_type}! was provided invalid value for {label} ({})", problem["explanation"].as_str().unwrap_or_default()),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "problems": [Value::Object(problem)]
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &str,
) -> Option<Value> {
    if let Some(value) = input.get("usesPerOrderLimit") {
        if let Some(n) = resolved_i64(value) {
            if n == 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit cannot be zero",
                    "VALUE_OUTSIDE_RANGE",
                ));
            }
            if n < 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be greater than 0",
                    "GREATER_THAN",
                ));
            }
            if n > 2_147_483_647 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be less than or equal to 2147483647",
                    "LESS_THAN_OR_EQUAL_TO",
                ));
            }
        }
    }

    if let Some(n) = resolved_i64_path(input, &["customerBuys", "value", "quantity"]) {
        if n == 0 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }

    if let Some(n) = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    ) {
        if n == 0 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }
    None
}

pub(in crate::proxy) fn resolved_i64_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<i64> {
    resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path).and_then(resolved_i64)
}

pub(in crate::proxy) fn resolved_i64(value: &ResolvedValue) -> Option<i64> {
    match value {
        ResolvedValue::Int(n) => Some(*n),
        ResolvedValue::String(raw) => raw.parse::<i64>().ok(),
        _ => None,
    }
}

pub(in crate::proxy) fn discount_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    user_error_with_extra_info(field, message, Some(code), Value::Null)
}

pub(in crate::proxy) fn resolved_object_path<'a>(
    value: Option<&'a ResolvedValue>,
    path: &[&str],
) -> Option<&'a ResolvedValue> {
    let mut current = value?;
    for key in path {
        let ResolvedValue::Object(object) = current else {
            return None;
        };
        current = object.get(*key)?;
    }
    Some(current)
}

pub(in crate::proxy) fn function_by_id_or_handle(
    id: Option<&str>,
    handle: Option<&str>,
) -> Option<Value> {
    function_catalog().into_iter().find(|function| {
        id.is_some_and(|id| function["id"].as_str() == Some(id))
            || handle.is_some_and(|handle| function["handle"].as_str() == Some(handle))
    })
}

pub(in crate::proxy) fn function_catalog_by_api_type(api_type: &str) -> Vec<Value> {
    function_catalog()
        .into_iter()
        .filter(|function| function["apiType"].as_str() == Some(api_type))
        .collect()
}

fn function_catalog() -> Vec<Value> {
    vec![
        local_validation_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-alpha",
            "title": "Validation Alpha",
            "handle": "validation-alpha",
            "apiType": "VALIDATION"
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/validation-beta",
            "title": "Validation Beta",
            "handle": "validation-beta",
            "apiType": "VALIDATION"
        }),
        json!({
            "id": "019dd44b-127f-7061-a930-422cbd4a751f",
            "title": "t:name",
            "handle": "conformance-validation",
            "apiType": "VALIDATION"
        }),
        functions_owner_validation_function(),
        local_cart_transform_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/cart-beta",
            "title": "Cart Beta",
            "handle": "cart-beta",
            "apiType": "CART_TRANSFORM"
        }),
        json!({
            "id": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "title": "Conformance Cart Transform",
            "handle": "conformance-cart-transform",
            "apiType": "CART_TRANSFORM"
        }),
        json!({
            "id": "019dd44b-127f-724b-a49c-70fc98ff4d72",
            "title": "Conformance Cart Transform",
            "handle": "cart-transform-delete-shape",
            "apiType": "CART_TRANSFORM"
        }),
        functions_owner_cart_function(),
        local_fulfillment_constraint_rule_function(),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-validation-plan",
            "title": "Guardrail validation plan",
            "handle": "guardrail-validation-plan",
            "apiType": "VALIDATION",
            "createGuardrailCode": "CUSTOM_APP_FUNCTION_NOT_ELIGIBLE",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate functions from a custom app."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-validation-required-input",
            "title": "Guardrail validation required input",
            "handle": "guardrail-validation-required-input",
            "apiType": "VALIDATION",
            "createGuardrailCode": "REQUIRED_INPUT_FIELD",
            "createGuardrailMessage": "Required input field must be present."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-plan",
            "title": "Guardrail cart transform plan",
            "handle": "guardrail-cart-transform-plan",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "CUSTOM_APP_FUNCTION_NOT_ELIGIBLE",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate functions from a custom app."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-pending-deletion",
            "title": "Guardrail cart transform pending deletion",
            "handle": "guardrail-cart-transform-pending-deletion",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "FUNCTION_PENDING_DELETION",
            "createGuardrailMessage": "Function is pending deletion."
        }),
        json!({
            "id": "gid://shopify/ShopifyFunction/guardrail-cart-transform-plus-only",
            "title": "Guardrail cart transform Plus only",
            "handle": "guardrail-cart-transform-plus-only",
            "apiType": "CART_TRANSFORM",
            "createGuardrailCode": "FUNCTION_IS_PLUS_ONLY",
            "createGuardrailMessage": "Shop must be on a Shopify Plus plan to activate this function."
        }),
    ]
}

fn function_identifier_input(
    input: &BTreeMap<String, ResolvedValue>,
) -> (Option<String>, Option<String>) {
    (
        resolved_string_field(input, "functionId"),
        resolved_string_field(input, "functionHandle"),
    )
}

fn function_payload_identifier_field(function_id: &Option<String>) -> &'static str {
    if function_id.is_some() {
        "functionId"
    } else {
        "functionHandle"
    }
}

fn function_user_error(field: Vec<Value>, message: &str, code: Option<&str>) -> Value {
    user_error(field, message, code)
}

fn validation_payload_error(error: Value) -> Value {
    json!({ "validation": Value::Null, "userErrors": [error] })
}

fn cart_transform_payload_error(error: Value) -> Value {
    json!({ "cartTransform": Value::Null, "userErrors": [error] })
}

fn validation_identifier_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let (function_id, function_handle) = function_identifier_input(input);
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(validation_payload_error(function_user_error(
            vec![json!("validation"), json!("functionHandle")],
            "Either function_id or function_handle must be provided.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        ))),
        (true, true) => Some(validation_payload_error(function_user_error(
            vec![json!("validation")],
            "Only one of function_id or function_handle can be provided, not both.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        ))),
        _ => None,
    }
}

fn cart_transform_identifier_error(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Option<Value> {
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(cart_transform_payload_error(function_user_error(
            vec![json!("functionHandle")],
            "Either function_id or function_handle must be provided.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
        ))),
        (true, true) => Some(cart_transform_payload_error(function_user_error(
            vec![json!("functionHandle")],
            "Only one of function_id or function_handle can be provided, not both.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
        ))),
        _ => None,
    }
}

fn validation_function_resolution_payload(
    input: &BTreeMap<String, ResolvedValue>,
) -> Result<Value, Value> {
    if let Some(payload) = validation_identifier_error(input) {
        return Err(payload);
    }
    let (function_id, function_handle) = function_identifier_input(input);
    let field_name = function_payload_identifier_field(&function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            validation_payload_error(function_user_error(
                vec![json!("validation"), json!(field_name)],
                "Extension not found.",
                Some("NOT_FOUND"),
            ))
        })?;
    if function["apiType"].as_str() != Some("VALIDATION") {
        return Err(validation_payload_error(function_user_error(
            vec![json!("validation"), json!(field_name)],
            "Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].",
            Some("FUNCTION_DOES_NOT_IMPLEMENT"),
        )));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(validation_payload_error(function_user_error(
            vec![json!("validation"), json!(field_name)],
            function["createGuardrailMessage"]
                .as_str()
                .unwrap_or_default(),
            Some(code),
        )));
    }
    Ok(function)
}

fn cart_transform_function_resolution_payload(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Result<Value, Value> {
    if let Some(payload) = cart_transform_identifier_error(function_id, function_handle) {
        return Err(payload);
    }
    let field_name = function_payload_identifier_field(function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            cart_transform_payload_error(function_user_error(
                vec![json!(field_name)],
                "Extension not found.",
                Some("FUNCTION_NOT_FOUND"),
            ))
        })?;
    if function["apiType"].as_str() != Some("CART_TRANSFORM") {
        let code = if function_id.is_some() {
            "FUNCTION_NOT_FOUND"
        } else {
            "FUNCTION_DOES_NOT_IMPLEMENT"
        };
        return Err(cart_transform_payload_error(function_user_error(
            vec![json!(field_name)],
            "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].",
            Some(code),
        )));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(cart_transform_payload_error(function_user_error(
            vec![json!(field_name)],
            function["createGuardrailMessage"]
                .as_str()
                .unwrap_or_default(),
            Some(code),
        )));
    }
    Ok(function)
}

fn metafield_input_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let field = vec![
        json!("validation"),
        json!("metafields"),
        json!(index.to_string()),
    ];
    let namespace = resolved_string_field(metafield, "namespace").unwrap_or_default();
    let key = resolved_string_field(metafield, "key");
    let type_name = resolved_string_field(metafield, "type");
    let value = resolved_string_field(metafield, "value");

    if key.is_none() {
        return Some(function_user_error(field, "presence", None));
    }
    if type_name.as_deref().unwrap_or_default().is_empty() {
        return Some(function_user_error(
            field,
            "One or more required inputs are blank.",
            Some("BLANK"),
        ));
    }
    if value.is_none() {
        return Some(function_user_error(field, "presence", None));
    }
    if namespace == "shopify" {
        return Some(function_user_error(
            field,
            "ApiPermission metafields can only be created or updated by the app owner.",
            Some("APP_NOT_AUTHORIZED"),
        ));
    }
    match type_name.as_deref() {
        Some("single_line_text_field") => {
            if value.as_deref() == Some("") {
                Some(function_user_error(
                    field,
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                ))
            } else {
                None
            }
        }
        Some("number_integer") => {
            if value
                .as_deref()
                .is_some_and(|value| value.parse::<i64>().is_ok())
            {
                None
            } else {
                Some(function_user_error(
                    field,
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                ))
            }
        }
        Some("json") => None,
        _ => Some(function_user_error(
            field,
            "The type is invalid.",
            Some("INVALID_TYPE"),
        )),
    }
}

fn validation_metafield_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => metafield_input_error(metafield, index),
                _ => Some(function_user_error(
                    vec![
                        json!("validation"),
                        json!("metafields"),
                        json!(index.to_string()),
                    ],
                    "The value is invalid.",
                    Some("INVALID_VALUE"),
                )),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafields_from_input(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    match input.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::Object(metafield) => Some(json!({
                    "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                    "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                    "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                    "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                    "updatedAt": "2026-05-07T08:02:25Z"
                })),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn validation_metafield_connection(metafields: Vec<Value>) -> Value {
    json!({ "nodes": metafields })
}

fn upsert_validation_metafields(record: &mut Value, metafields: Vec<Value>) {
    let existing = record["metafields"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut merged = existing;
    for metafield in metafields {
        let namespace = metafield["namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let key = metafield["key"].as_str().unwrap_or_default().to_string();
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing["namespace"].as_str() == Some(namespace.as_str())
                && existing["key"].as_str() == Some(key.as_str())
        }) {
            *existing = metafield;
        } else {
            merged.push(metafield);
        }
    }
    record["metafields"] = validation_metafield_connection(merged);
}

fn selected_title(input: &BTreeMap<String, ResolvedValue>, function: &Value) -> String {
    match input.get("title") {
        Some(ResolvedValue::String(title)) => title.clone(),
        Some(ResolvedValue::Null) | None => {
            function["title"].as_str().unwrap_or_default().to_string()
        }
        _ => String::new(),
    }
}

fn active_validation_count(records: &BTreeMap<String, Value>, exclude_id: Option<&str>) -> usize {
    records
        .iter()
        .filter(|(id, record)| {
            Some(id.as_str()) != exclude_id && record["enable"].as_bool() == Some(true)
        })
        .count()
}

pub(in crate::proxy) fn local_function_connection_from_nodes(nodes: Vec<Value>) -> Value {
    let start_cursor = nodes
        .first()
        .and_then(|node| node["id"].as_str())
        .map(|id| format!("cursor:{id}"));
    let end_cursor = nodes
        .last()
        .and_then(|node| node["id"].as_str())
        .map(|id| format!("cursor:{id}"));
    json!({
        "nodes": nodes,
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": start_cursor.map(Value::from).unwrap_or(Value::Null),
            "endCursor": end_cursor.map(Value::from).unwrap_or(Value::Null)
        }
    })
}

fn cart_transform_metafield_error(
    metafield: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let value = resolved_string_field(metafield, "value").unwrap_or_default();
    if value.is_empty() {
        return Some(function_user_error(
            vec![
                json!("metafields"),
                json!(index.to_string()),
                json!("value"),
            ],
            "may not be empty",
            Some("INVALID_METAFIELDS"),
        ));
    }
    if resolved_string_field(metafield, "type").as_deref() == Some("json")
        && serde_json::from_str::<Value>(&value).is_err()
    {
        return Some(function_user_error(
            vec![
                json!("metafields"),
                json!(index.to_string()),
                json!("value"),
            ],
            &format!(
                "is invalid JSON: unexpected token '{}' at line 1 column 1.",
                value
            ),
            Some("INVALID_METAFIELDS"),
        ));
    }
    None
}

fn cart_transform_metafield_errors(field: &RootFieldSelection) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    cart_transform_metafield_error(metafield, index)
                }
                _ => Some(function_user_error(
                    vec![
                        json!("metafields"),
                        json!(index.to_string()),
                        json!("value"),
                    ],
                    "may not be empty",
                    Some("INVALID_METAFIELDS"),
                )),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn staged_function_id_in_use(records: &BTreeMap<String, Value>, function_id: &str) -> bool {
    records
        .values()
        .any(|record| record["functionId"].as_str() == Some(function_id))
}

pub(in crate::proxy) fn cart_transform_metafields_from_field(
    field: &RootFieldSelection,
    ids: Vec<String>,
) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    let now = "2026-05-07T17:20:12Z";
                    Some(json!({
                        "id": match index {
                            0 => "gid://shopify/Metafield/43125986558258".to_string(),
                            1 => "gid://shopify/Metafield/43125986591026".to_string(),
                            _ => ids.get(index).cloned().unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index + 1)),
                        },
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "compareDigest": match index {
                            0 => "58440d4e2b7e81e7a5318441381af282c0a2ec83cf926af55397244ff23e1181".to_string(),
                            1 => "c30b019a8fd5bb26e69d73f4a11d3c12ac733b6063d8be2562d08dd2ce61344b".to_string(),
                            _ => format!("proxy-digest-{}", index + 1),
                        },
                        "ownerType": "CARTTRANSFORM",
                        "createdAt": now,
                        "updatedAt": now
                    }))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn cart_transform_record_for_selection(
    record: &Value,
    connection_selection: &[SelectedField],
) -> Value {
    let mut record = record.clone();
    let Some(node_selection) = selected_child_selection(connection_selection, "nodes") else {
        return record;
    };
    let Some(metafield_selection) = node_selection
        .iter()
        .find(|field| field.name == "metafield")
    else {
        return record;
    };
    let namespace = metafield_selection
        .arguments
        .get("namespace")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    let key = metafield_selection
        .arguments
        .get("key")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if let (Some(namespace), Some(key)) = (namespace, key) {
        let metafield = record["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node["namespace"].as_str() == Some(namespace)
                        && node["key"].as_str() == Some(key)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        record["metafield"] = metafield;
    }
    record
}

fn fulfillment_constraint_rule_payload_error(error: Value) -> Value {
    json!({ "fulfillmentConstraintRule": Value::Null, "userErrors": [error] })
}

fn fulfillment_constraint_rule_identifier_error(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Option<Value> {
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => Some(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!("functionHandle")],
                "Either function_id or function_handle must be provided.",
                Some("MISSING_FUNCTION_IDENTIFIER"),
            ),
        )),
        (true, true) => Some(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!("functionHandle")],
                "Only one of function_id or function_handle can be provided, not both.",
                Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
            ),
        )),
        _ => None,
    }
}

fn fulfillment_constraint_rule_delivery_method_types(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("deliveryMethodTypes") {
        Some(ResolvedValue::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                ResolvedValue::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn fulfillment_constraint_rule_delivery_method_error(
    delivery_method_types: &[String],
) -> Option<Value> {
    if delivery_method_types.is_empty() {
        Some(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!("deliveryMethodTypes")],
                "Delivery method types cannot be empty.",
                Some("INPUT_INVALID"),
            ),
        ))
    } else {
        None
    }
}

fn fulfillment_constraint_rule_function_not_found_error(
    field_name: &str,
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Value {
    let message = if let Some(handle) = function_handle {
        format!("Could not find function with handle: {handle}.")
    } else if let Some(id) = function_id {
        format!("Could not find function with id: {id}.")
    } else {
        "Could not find function.".to_string()
    };
    fulfillment_constraint_rule_payload_error(function_user_error(
        vec![json!(field_name)],
        &message,
        Some("FUNCTION_NOT_FOUND"),
    ))
}

fn fulfillment_constraint_rule_function_resolution_payload(
    function_id: &Option<String>,
    function_handle: &Option<String>,
) -> Result<Value, Value> {
    if let Some(payload) =
        fulfillment_constraint_rule_identifier_error(function_id, function_handle)
    {
        return Err(payload);
    }
    let field_name = function_payload_identifier_field(function_id);
    let function = function_by_id_or_handle(function_id.as_deref(), function_handle.as_deref())
        .ok_or_else(|| {
            fulfillment_constraint_rule_function_not_found_error(
                field_name,
                function_id,
                function_handle,
            )
        })?;
    if function["apiType"].as_str() != Some("FULFILLMENT_CONSTRAINT_RULE") {
        return Err(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!(field_name)],
                "Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.fulfillment-constraint-rule.run, cart.fulfillment-constraints.generate.run].",
                Some("FUNCTION_DOES_NOT_IMPLEMENT"),
            ),
        ));
    }
    if let Some(code) = function["createGuardrailCode"].as_str() {
        return Err(fulfillment_constraint_rule_payload_error(
            function_user_error(
                vec![json!(field_name)],
                function["createGuardrailMessage"]
                    .as_str()
                    .unwrap_or_default(),
                Some(code),
            ),
        ));
    }
    Ok(function)
}

fn fulfillment_constraint_rule_metafields_from_field(
    field: &RootFieldSelection,
    ids: Vec<String>,
) -> Vec<Value> {
    match field.arguments.get("metafields") {
        Some(ResolvedValue::List(metafields)) => metafields
            .iter()
            .enumerate()
            .filter_map(|(index, value)| match value {
                ResolvedValue::Object(metafield) => {
                    let now = "2026-05-07T17:20:12Z";
                    Some(json!({
                        "id": ids.get(index).cloned().unwrap_or_else(|| format!("gid://shopify/Metafield/{}", index + 1)),
                        "namespace": resolved_string_field(metafield, "namespace").unwrap_or_default(),
                        "key": resolved_string_field(metafield, "key").unwrap_or_default(),
                        "type": resolved_string_field(metafield, "type").unwrap_or_default(),
                        "value": resolved_string_field(metafield, "value").unwrap_or_default(),
                        "compareDigest": format!("proxy-fulfillment-constraint-digest-{}", index + 1),
                        "ownerType": "FULFILLMENTCONSTRAINTRULE",
                        "createdAt": now,
                        "updatedAt": now
                    }))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn fulfillment_constraint_rule_record_for_selection(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let mut record = record.clone();
    let Some(metafield_selection) = selection.iter().find(|field| field.name == "metafield") else {
        return record;
    };
    let namespace = metafield_selection
        .arguments
        .get("namespace")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    let key = metafield_selection
        .arguments
        .get("key")
        .and_then(|value| match value {
            ResolvedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if let (Some(namespace), Some(key)) = (namespace, key) {
        let metafield = record["metafields"]["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes.iter().find(|node| {
                    node["namespace"].as_str() == Some(namespace)
                        && node["key"].as_str() == Some(key)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        record["metafield"] = metafield;
    }
    record
}

impl DraftProxy {
    pub(in crate::proxy) fn function_validation_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return validation_payload_error(function_user_error(
                    vec![json!("validation")],
                    "Required input field must be present.",
                    Some("REQUIRED_INPUT_FIELD"),
                ));
            }
        };
        let function = match validation_function_resolution_payload(input) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return json!({ "validation": Value::Null, "userErrors": errors });
        }
        let enable = resolved_bool_field(input, "enable").unwrap_or(false);
        if enable && active_validation_count(&self.store.staged.function_validations, None) >= 25 {
            return validation_payload_error(function_user_error(
                Vec::new(),
                "Cannot have more than 25 active validation functions.",
                Some("MAX_VALIDATIONS_ACTIVATED"),
            ));
        }
        let id = if self.store.staged.function_validation_order.is_empty() {
            "gid://shopify/Validation/2".to_string()
        } else {
            format!(
                "gid://shopify/Validation/{}",
                self.store.staged.function_validation_order.len() + 2
            )
        };
        let metafields = validation_metafields_from_input(input);
        let validation = json!({
            "id": id,
            "title": selected_title(input, &function),
            "enable": enable,
            "enabled": enable,
            "blockOnFailure": resolved_bool_field(input, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "createdAt": "2024-01-01T00:00:01.000Z",
            "updatedAt": "2024-01-01T00:00:01.000Z",
            "shopifyFunction": function,
            "metafields": validation_metafield_connection(metafields)
        });
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let input = match field.arguments.get("validation") {
            Some(ResolvedValue::Object(input)) => input,
            _ => {
                return validation_payload_error(function_user_error(
                    vec![json!("validation")],
                    "Required input field must be present.",
                    Some("REQUIRED_INPUT_FIELD"),
                ));
            }
        };
        let Some(mut validation) = self.store.staged.function_validations.get(&id).cloned() else {
            return validation_payload_error(function_user_error(
                vec![json!("id")],
                "Extension not found.",
                Some("NOT_FOUND"),
            ));
        };
        let errors = validation_metafield_errors(input);
        if !errors.is_empty() {
            return json!({ "validation": Value::Null, "userErrors": errors });
        }
        let next_enable = resolved_bool_field(input, "enable")
            .or_else(|| resolved_bool_field(input, "enabled"))
            .unwrap_or(false);
        if next_enable
            && active_validation_count(&self.store.staged.function_validations, Some(&id)) >= 25
        {
            return validation_payload_error(function_user_error(
                Vec::new(),
                "Cannot have more than 25 active validation functions.",
                Some("MAX_VALIDATIONS_ACTIVATED"),
            ));
        }
        if let Some(title) = resolved_string_field(input, "title") {
            validation["title"] = json!(title);
        }
        validation["enable"] = json!(next_enable);
        validation["enabled"] = json!(next_enable);
        validation["blockOnFailure"] =
            json!(resolved_bool_field(input, "blockOnFailure").unwrap_or(false));
        validation["updatedAt"] = json!("2024-01-01T00:00:05.000Z");
        upsert_validation_metafields(&mut validation, validation_metafields_from_input(input));
        self.stage_function_validation(validation.clone());
        json!({ "validation": validation, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_validation_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self.store.staged.function_validations.remove(&id).is_some() {
            self.store
                .staged
                .function_validation_order
                .retain(|ordered_id| ordered_id != &id);
            if self
                .store
                .staged
                .function_validation
                .as_ref()
                .and_then(|record| record["id"].as_str())
                == Some(id.as_str())
            {
                self.store.staged.function_validation = self
                    .store
                    .staged
                    .function_validation_order
                    .last()
                    .and_then(|id| self.store.staged.function_validations.get(id).cloned());
            }
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            json!({
                "deletedId": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Extension not found.",
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_cart_transform_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let function_id = resolved_field_string_arg(field, "functionId");
        let function_handle = resolved_field_string_arg(field, "functionHandle");
        if let Some(payload) = cart_transform_identifier_error(&function_id, &function_handle) {
            return payload;
        }
        if let Some(function_id) = function_id.as_deref() {
            if staged_function_id_in_use(&self.store.staged.function_validations, function_id)
                || staged_function_id_in_use(
                    &self.store.staged.function_cart_transforms,
                    function_id,
                )
            {
                return cart_transform_payload_error(function_user_error(
                    vec![json!("functionId")],
                    "Could not enable cart transform because it is already registered",
                    Some("FUNCTION_ALREADY_REGISTERED"),
                ));
            }
        }
        let function =
            match cart_transform_function_resolution_payload(&function_id, &function_handle) {
                Ok(function) => function,
                Err(payload) => return payload,
            };
        let errors = cart_transform_metafield_errors(field);
        if !errors.is_empty() {
            return json!({ "cartTransform": Value::Null, "userErrors": errors });
        }
        let id = if self.store.staged.function_cart_transform_order.is_empty() {
            "gid://shopify/CartTransform/3".to_string()
        } else {
            format!(
                "gid://shopify/CartTransform/{}",
                self.store.staged.function_cart_transform_order.len() + 3
            )
        };
        let metafield_ids = match field.arguments.get("metafields") {
            Some(ResolvedValue::List(metafields)) => metafields
                .iter()
                .map(|_| self.next_proxy_synthetic_gid("Metafield"))
                .collect(),
            _ => Vec::new(),
        };
        let metafields = cart_transform_metafields_from_field(field, metafield_ids);
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut cart_transform = json!({
            "id": id,
            "blockOnFailure": resolved_bool_field(&field.arguments, "blockOnFailure").unwrap_or(false),
            "functionId": function["id"].clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if cart_transform["metafield"].is_null() {
            cart_transform.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_cart_transform(cart_transform.clone());
        json!({ "cartTransform": cart_transform, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_cart_transform_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self
            .store
            .staged
            .function_cart_transforms
            .remove(&id)
            .is_some()
        {
            self.store
                .staged
                .function_cart_transform_order
                .retain(|ordered_id| ordered_id != &id);
            if self
                .store
                .staged
                .function_cart_transform
                .as_ref()
                .and_then(|record| record["id"].as_str())
                == Some(id.as_str())
            {
                self.store.staged.function_cart_transform = self
                    .store
                    .staged
                    .function_cart_transform_order
                    .last()
                    .and_then(|id| self.store.staged.function_cart_transforms.get(id).cloned());
            }
            json!({ "deletedId": id, "userErrors": [] })
        } else {
            json!({
                "deletedId": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": format!("Could not find cart transform with id: {id}"),
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_create_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let function_id = resolved_field_string_arg(field, "functionId");
        let function_handle = resolved_field_string_arg(field, "functionHandle");
        if let Some(payload) =
            fulfillment_constraint_rule_identifier_error(&function_id, &function_handle)
        {
            return payload;
        }
        let delivery_method_types = fulfillment_constraint_rule_delivery_method_types(field);
        if let Some(payload) =
            fulfillment_constraint_rule_delivery_method_error(&delivery_method_types)
        {
            return payload;
        }
        let function = match fulfillment_constraint_rule_function_resolution_payload(
            &function_id,
            &function_handle,
        ) {
            Ok(function) => function,
            Err(payload) => return payload,
        };
        let id = format!(
            "gid://shopify/FulfillmentConstraintRule/{}",
            self.store
                .staged
                .function_fulfillment_constraint_rule_order
                .len()
                + 1
        );
        let metafield_ids = match field.arguments.get("metafields") {
            Some(ResolvedValue::List(metafields)) => metafields
                .iter()
                .map(|_| self.next_proxy_synthetic_gid("Metafield"))
                .collect(),
            _ => Vec::new(),
        };
        let metafields = fulfillment_constraint_rule_metafields_from_field(field, metafield_ids);
        let first_metafield = metafields.first().cloned().unwrap_or(Value::Null);
        let mut rule = json!({
            "id": id,
            "deliveryMethodTypes": delivery_method_types,
            "functionId": function["id"].clone(),
            "functionHandle": function["handle"].clone(),
            "function": function.clone(),
            "shopifyFunction": function,
            "metafield": first_metafield,
            "metafields": { "nodes": metafields }
        });
        if rule["metafield"].is_null() {
            rule.as_object_mut().unwrap().remove("metafield");
        }
        self.stage_function_fulfillment_constraint_rule(rule.clone());
        json!({ "fulfillmentConstraintRule": rule, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_update_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        let delivery_method_types = fulfillment_constraint_rule_delivery_method_types(field);
        if let Some(payload) =
            fulfillment_constraint_rule_delivery_method_error(&delivery_method_types)
        {
            return payload;
        }
        let Some(mut rule) = self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .get(&id)
            .cloned()
        else {
            return fulfillment_constraint_rule_payload_error(function_user_error(
                vec![json!("id")],
                &format!("Could not find FulfillmentConstraintRule with id: {id}"),
                Some("NOT_FOUND"),
            ));
        };
        rule["deliveryMethodTypes"] = json!(delivery_method_types);
        self.stage_function_fulfillment_constraint_rule(rule.clone());
        json!({ "fulfillmentConstraintRule": rule, "userErrors": [] })
    }

    pub(in crate::proxy) fn function_fulfillment_constraint_rule_delete_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let id = resolved_field_string_arg(field, "id").unwrap_or_default();
        if self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .remove(&id)
            .is_some()
        {
            self.store
                .staged
                .function_fulfillment_constraint_rule_order
                .retain(|ordered_id| ordered_id != &id);
            json!({ "success": true, "userErrors": [] })
        } else {
            json!({
                "success": false,
                "userErrors": [{
                    "field": ["id"],
                    "message": format!("Could not find FulfillmentConstraintRule with id: {id}"),
                    "code": "NOT_FOUND"
                }]
            })
        }
    }

    pub(in crate::proxy) fn function_tax_app_configure_payload(
        &mut self,
        field: &RootFieldSelection,
    ) -> Value {
        let ready = resolved_bool_field(&field.arguments, "ready").unwrap_or(true);
        json!({
            "taxAppConfiguration": {
                "id": "gid://shopify/TaxAppConfiguration/local",
                "ready": ready,
                "state": if ready { "READY" } else { "NOT_READY" },
                "updatedAt": "2024-01-01T00:00:03.000Z"
            },
            "userErrors": []
        })
    }

    fn stage_function_validation(&mut self, validation: Value) {
        let Some(id) = validation["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.staged.function_validations.contains_key(&id) {
            self.store.staged.function_validation_order.push(id.clone());
        }
        self.store
            .staged
            .function_validations
            .insert(id, validation.clone());
        self.store.staged.function_validation = Some(validation);
    }

    fn stage_function_cart_transform(&mut self, cart_transform: Value) {
        let Some(id) = cart_transform["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self.store.staged.function_cart_transforms.contains_key(&id) {
            self.store
                .staged
                .function_cart_transform_order
                .push(id.clone());
        }
        self.store
            .staged
            .function_cart_transforms
            .insert(id, cart_transform.clone());
        self.store.staged.function_cart_transform = Some(cart_transform);
    }

    fn stage_function_fulfillment_constraint_rule(&mut self, rule: Value) {
        let Some(id) = rule["id"].as_str().map(str::to_string) else {
            return;
        };
        if !self
            .store
            .staged
            .function_fulfillment_constraint_rules
            .contains_key(&id)
        {
            self.store
                .staged
                .function_fulfillment_constraint_rule_order
                .push(id.clone());
        }
        self.store
            .staged
            .function_fulfillment_constraint_rules
            .insert(id, rule);
    }
}

pub(in crate::proxy) fn local_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-local",
        "title": "Validation Local",
        "handle": "validation-local",
        "apiType": "VALIDATION"
    })
}

pub(in crate::proxy) fn local_cart_transform_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-transform-local",
        "title": "Cart Transform Local",
        "handle": "cart-transform-local",
        "apiType": "CART_TRANSFORM"
    })
}

pub(in crate::proxy) fn local_fulfillment_constraint_rule_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/fulfillment-constraint-local",
        "title": "Fulfillment Constraint Local",
        "handle": "fulfillment-constraint-local",
        "apiType": "FULFILLMENT_CONSTRAINT_RULE"
    })
}

/// Output fields defined on the `CartTransform` type (2026-04). A selection of
/// anything else is a query-validation error (`undefinedField`) Shopify rejects
/// before execution — so the read returns errors with no data.
const CART_TRANSFORM_OUTPUT_FIELDS: &[&str] = &[
    "id",
    "functionId",
    "blockOnFailure",
    "metafield",
    "metafields",
    "__typename",
];

pub(in crate::proxy) fn cart_transform_selection_errors(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    fields: &[RootFieldSelection],
) -> Vec<Value> {
    let operation_path = parsed_document(query, variables)
        .map(|document| document.operation_path)
        .unwrap_or_default();
    let mut errors = Vec::new();
    for field in fields {
        if field.name != "cartTransforms" {
            continue;
        }
        for child in &field.selection {
            // cartTransforms(first: N) { nodes { <CartTransform> } }
            //                          { edges { node { <CartTransform> } } }
            let (container_path, selections): (Vec<&str>, Vec<&SelectedField>) =
                match child.name.as_str() {
                    "nodes" => (vec!["nodes"], child.selection.iter().collect()),
                    "edges" => (
                        vec!["edges", "node"],
                        child
                            .selection
                            .iter()
                            .filter(|edge_child| edge_child.name == "node")
                            .flat_map(|node| node.selection.iter())
                            .collect(),
                    ),
                    _ => continue,
                };
            for selection in selections {
                if !CART_TRANSFORM_OUTPUT_FIELDS.contains(&selection.name.as_str()) {
                    errors.push(cart_transform_undefined_field_error(
                        query,
                        &operation_path,
                        &field.response_key,
                        &container_path,
                        &selection.name,
                    ));
                }
            }
        }
    }
    errors
}

fn cart_transform_undefined_field_error(
    query: &str,
    operation_path: &str,
    response_key: &str,
    container_path: &[&str],
    field_name: &str,
) -> Value {
    let location = cart_transform_field_token_location(query, field_name)
        .unwrap_or(SourceLocation { line: 1, column: 1 });
    let mut path = vec![Value::from(operation_path), Value::from(response_key)];
    path.extend(container_path.iter().map(|segment| Value::from(*segment)));
    path.push(Value::from(field_name));
    json!({
        "message": format!("Field '{field_name}' doesn't exist on type 'CartTransform'"),
        "locations": [{ "line": location.line, "column": location.column }],
        "path": path,
        "extensions": {
            "code": "undefinedField",
            "typeName": "CartTransform",
            "fieldName": field_name
        }
    })
}

fn cart_transform_field_token_location(query: &str, field_name: &str) -> Option<SourceLocation> {
    let bytes = query.as_bytes();
    let mut from = 0;
    while let Some(relative) = query[from..].find(field_name) {
        let index = from + relative;
        let after = index + field_name.len();
        let before_ok = index == 0 || !is_cart_transform_name_byte(bytes[index - 1]);
        let after_ok = after >= bytes.len() || !is_cart_transform_name_byte(bytes[after]);
        if before_ok && after_ok {
            let line = query[..index].bytes().filter(|byte| *byte == b'\n').count() + 1;
            let line_start = query[..index].rfind('\n').map_or(0, |newline| newline + 1);
            return Some(SourceLocation {
                line,
                column: index - line_start + 1,
            });
        }
        from = after;
    }
    None
}

fn is_cart_transform_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(in crate::proxy) fn resolved_enum_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn functions_owner_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-owned",
        "title": "Owned validation function",
        "handle": "validation-owned",
        "apiType": "VALIDATION",
        "description": "Function metadata captured from the installed app",
        "appKey": "validation-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/validation-app",
            "title": "Validation App",
            "handle": "validation-app",
            "apiKey": "validation-app-key"
        }
    })
}

pub(in crate::proxy) fn functions_owner_cart_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-owned",
        "title": "Owned cart function",
        "handle": "cart-owned",
        "apiType": "CART_TRANSFORM",
        "description": "Cart transform Function metadata captured from the installed app",
        "appKey": "cart-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/cart-app",
            "title": "Cart App",
            "handle": "cart-app",
            "apiKey": "cart-app-key"
        }
    })
}

pub(in crate::proxy) fn redeem_code_bulk_delete_selector_count(
    field: &RootFieldSelection,
) -> usize {
    let ids_present = field.arguments.contains_key("ids");
    let search_present = field.arguments.contains_key("search");
    let saved_search_present = field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id");
    ids_present as usize + search_present as usize + saved_search_present as usize
}

pub(in crate::proxy) fn discount_null_field_user_error(message: &str, code: Option<&str>) -> Value {
    user_error_with_extra_info(Value::Null, message, code, Value::Null)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation(
    codes: &[String],
    existing: &BTreeSet<String>,
    pending: bool,
) -> Value {
    let failed_count = if pending {
        0
    } else {
        codes
            .iter()
            .enumerate()
            .filter(|(index, code)| !redeem_code_accepted(code, codes, *index, existing))
            .count()
    };
    let imported_count = if pending {
        0
    } else {
        codes.len() - failed_count
    };
    // The caller assigns the synthetic creation id; this id is always overwritten.
    json!({
        "id": Value::Null,
        "done": !pending,
        "codesCount": codes.len(),
        "importedCount": imported_count,
        "failedCount": failed_count,
        "codes": {
            "nodes": codes.iter().enumerate().map(|(index, code)| discount_redeem_code_bulk_creation_node(code, codes, index, existing, pending)).collect::<Vec<_>>(),
            "edges": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": Value::Null, "endCursor": Value::Null }
        }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation_node(
    code: &str,
    codes: &[String],
    index: usize,
    existing: &BTreeSet<String>,
    pending: bool,
) -> Value {
    let errors = if pending {
        Vec::new()
    } else {
        redeem_code_errors(code, codes, index, existing)
    };
    let accepted = errors.is_empty();
    json!({
        "code": code,
        "errors": errors,
        "discountRedeemCode": if pending || !accepted { Value::Null } else { json!({
            "id": synthetic_shopify_gid("DiscountRedeemCode", stable_redeem_code_suffix(code)),
            "code": code
        }) }
    })
}

/// Whether a `discountRedeemCodeBulkAdd` `codes` argument was supplied as a bare
/// `[String!]` list (the legacy local-runtime shape) rather than the schema
/// `[DiscountRedeemCodeInput!]` object list. String submissions complete
/// synchronously; object submissions follow Shopify's async creation shape.
pub(in crate::proxy) fn redeem_codes_are_string_inputs(field: &RootFieldSelection) -> bool {
    match field.arguments.get("codes") {
        Some(ResolvedValue::List(items)) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| matches!(item, ResolvedValue::String(_)))
        }
        _ => false,
    }
}

pub(in crate::proxy) fn resolved_redeem_codes(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("codes") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => match object.get("code") {
                    Some(ResolvedValue::String(code)) => Some(code.clone()),
                    _ => None,
                },
                ResolvedValue::String(code) => Some(code.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_field_string_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn redeem_code_accepted(
    code: &str,
    codes: &[String],
    index: usize,
    existing: &BTreeSet<String>,
) -> bool {
    redeem_code_errors(code, codes, index, existing).is_empty()
}

/// Per-code validation for a `discountRedeemCodeBulkAdd` submission. `existing`
/// is the set of codes (uppercased) already assigned to any discount in the
/// shop before this batch; `codes`/`index` locate the code within the batch so
/// duplicates within the same submission can be detected.
pub(in crate::proxy) fn redeem_code_errors(
    code: &str,
    codes: &[String],
    index: usize,
    existing: &BTreeSet<String>,
) -> Vec<Value> {
    if code.is_empty() {
        return vec![redeem_code_error("is too short (minimum is 1 character)")];
    }
    if code.contains('\n') || code.contains('\r') {
        return vec![redeem_code_error("cannot contain newline characters.")];
    }
    if code.chars().count() > 255 {
        return vec![redeem_code_error("is too long (maximum is 255 characters)")];
    }
    let normalized = code.to_ascii_uppercase();
    // A second (or later) occurrence of the same code within this submission.
    if codes
        .iter()
        .take(index)
        .any(|candidate| candidate.to_ascii_uppercase() == normalized)
    {
        return vec![redeem_code_error(
            "Codes must be unique within BulkDiscountCodeCreation",
        )];
    }
    // The code is already assigned to some discount in the shop.
    if existing.contains(&normalized) {
        return vec![redeem_code_error(
            "must be unique. Please try a different code.",
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn redeem_code_error(message: &str) -> Value {
    user_error_with_extra_info(["code"], message, None, Value::Null)
}

pub(in crate::proxy) fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}
