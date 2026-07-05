use super::*;

const DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE: &str =
    "Only one of context or customerSelection can be provided.";
const DISCOUNT_MINIMUM_QUANTITY_UPPER_BOUND: i64 = 2_147_483_647;
const DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND: i64 = 1_000_000_000_000_000_000;
const DISCOUNT_MINIMUM_SUBTOTAL_UPPER_BOUND_DECIMAL: &str = "1000000000000000000";
const SHOPIFY_FUNCTION_BY_ID_QUERY: &str = "query ShopifyFunctionById($id: String!) {\n  shopifyFunction(id: $id) {\n    id\n    title\n    apiType\n    description\n    appKey\n    app {\n      id\n      title\n      handle\n      apiKey\n    }\n  }\n}\n";
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
const DISCOUNT_HYDRATE_QUERY: &str = r#"#graphql
  query DiscountHydrate($id: ID!) {
    codeNode: codeDiscountNode(id: $id) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
        }
        ... on DiscountCodeApp {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountCodeBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountCodeFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
      }
    }
    automaticNode: automaticDiscountNode(id: $id) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticApp {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
      }
    }
  }
"#;
const DISCOUNT_UPDATE_HYDRATE_QUERY: &str = r#"#graphql
  query DiscountHydrate($id: ID!) {
    codeNode: codeDiscountNode(id: $id) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
        }
        ... on DiscountCodeApp {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountCodeBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountCodeFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
          codes(first: 250) {
            nodes {
              id
              code
              asyncUsageCount
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
      }
    }
    automaticNode: automaticDiscountNode(id: $id) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticApp {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
        }
        ... on DiscountAutomaticFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
          asyncUsageCount
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
      }
    }
  }
"#;
/// Item-entitlement existence probe forwarded before a native discount create /
/// update is
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
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                !self.store.staged.discounts.contains_key(&id)
                    && !self.store.staged.discounts.is_tombstoned(&id)
            }
            "codeDiscountNodeByCode" => {
                let code = resolved_string_field(&field.arguments, "code").unwrap_or_default();
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
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
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
        // observing the result, so the per-field create/update validation below decides
        // INVALID references against real store state instead of seeded state.
        self.hydrate_discount_item_refs(_request, &fields);
        // Resolve any buyer-context customer / segment members the same way: forward
        // a read for each referenced customer / segment that is not already staged and
        // observe the result, so `resolve_discount_context_names` bakes the real
        // display name / segment name from store state instead of a seeded precondition.
        self.hydrate_discount_context_refs(_request, &fields);
        let mut log_drafts = Vec::new();
        let mut top_level_errors = Vec::new();
        let data = root_payload_json(&fields, |field| {
            if let Some(error) = discount_field_top_level_error(field) {
                top_level_errors.push(error);
                return Some(Value::Null);
            }
            let outcome = self.discount_mutation_field(_request, field);
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            Some(selected_json(&outcome.value, &field.selection))
        });
        let mut body = json!({ "data": data });
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
                vec![user_error_with_extra_info(
                    Value::Null,
                    "Local staging for this discount mutation is not implemented.",
                    Some("NOT_IMPLEMENTED"),
                    Value::Null,
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
            user_errors
                .extend(self.discount_reference_user_errors(request, input_map, input_arg, None));
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
        let shop_currency_code = self.store.shop_currency_code();
        let summary = self.discount_summary_for_input(typename, &input);
        let now_epoch = self.current_epoch_seconds();
        let timestamp = self.next_mutation_timestamp();
        let mut record = discount_record_from_input(
            &id,
            discount_kind,
            typename,
            &input,
            None,
            DiscountRecordBuildContext {
                shop_currency_code: &shop_currency_code,
                summary,
                timestamp: &timestamp,
                now_epoch,
            },
        );
        self.resolve_discount_context_names(&mut record);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(
                &field.name,
                self.discount_node_for_record(&record),
                Vec::new(),
            ),
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
        backfill_context_names(context, "customers", "id", "displayName", |id| {
            self.store
                .staged
                .customers
                .get(id)
                .and_then(|record| record.get("displayName"))
                .filter(|value| !value.is_null())
                .cloned()
        });
        backfill_context_names(context, "segments", "id", "name", |id| {
            self.store
                .staged
                .segments
                .get(id)
                .and_then(|record| record.get("name"))
                .filter(|value| !value.is_null())
                .cloned()
        });
    }

    /// Forward a single batched `nodes(ids:)` lookup for every product / variant /
    /// collection entitlement reference across all native create/update fields in the mutation,
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
            let Some(input_arg) = discount_mutation_input_arg(&field.name) else {
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
        if is_shopify_gid_of_type(gid, "Product") {
            self.store.has_product(gid)
        } else if is_shopify_gid_of_type(gid, "ProductVariant") {
            self.store.product_variant_by_id(gid).is_some()
        } else if is_shopify_gid_of_type(gid, "Collection") {
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
        exclude_discount_id: Option<&str>,
    ) -> Vec<Value> {
        let mut errors = Vec::new();
        if let Some(code) = resolved_string_path(input, &["code"]) {
            if !code.trim().is_empty()
                && self.discount_code_is_taken(request, &code, exclude_discount_id)
            {
                errors.push(user_error_with_extra_info(
                    vec![input_arg, "code"],
                    "Code must be unique. Please try a different code.",
                    Some("TAKEN"),
                    Value::Null,
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
    fn discount_code_is_taken(
        &self,
        request: &Request,
        code: &str,
        exclude_discount_id: Option<&str>,
    ) -> bool {
        if let Some(discount_id) = self
            .store
            .staged
            .discount_code_index
            .get(&code.to_ascii_uppercase())
        {
            return Some(discount_id.as_str()) != exclude_discount_id;
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
        let node = &response.body["data"]["codeDiscountNodeByCode"];
        if node.is_null() {
            return false;
        }
        node["id"]
            .as_str()
            .is_none_or(|discount_id| Some(discount_id) != exclude_discount_id)
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
                errors.push(user_error_with_extra_info(
                    vec![input_arg, selection, "items", "collections", "add"],
                    "Cannot entitle collections in combination with product variants or products",
                    Some("CONFLICT"),
                    Value::Null,
                ));
            } else {
                for collection_id in &collections {
                    if !self.discount_reference_collection_exists(collection_id) {
                        errors.push(user_error_with_extra_info(
                            vec![input_arg, selection, "items", "collections", "add"],
                            &format!(
                                "Collection with id: {} is invalid",
                                resource_id_tail(collection_id)
                            ),
                            Some("INVALID"),
                            Value::Null,
                        ));
                    }
                }
            }
        }
        for product_id in &products {
            if !self.discount_reference_product_exists(product_id) {
                errors.push(user_error_with_extra_info(
                    vec![input_arg, selection, "items", "products", "productsToAdd"],
                    &format!(
                        "Product with id: {} is invalid",
                        resource_id_tail(product_id)
                    ),
                    Some("INVALID"),
                    Value::Null,
                ));
            }
        }
        for variant_id in &variants {
            if !self.discount_reference_product_variant_exists(variant_id) {
                errors.push(user_error_with_extra_info(
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
                    Some("INVALID"),
                    Value::Null,
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = discount_input(field, input_arg);
        let existing_record = self
            .discount_record(&id)
            .cloned()
            .or_else(|| self.hydrate_discount_record_for_update(request, &id));
        let user_errors = match existing_record.as_ref() {
            None => vec![user_error_with_extra_info(
                ["id"],
                "Discount does not exist",
                None,
                Value::Null,
            )],
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
                    vec![user_error_with_extra_info(
                        ["id"],
                        "Cannot update the code of a bulk discount.",
                        None,
                        Value::Null,
                    )]
                } else {
                    let mut errors =
                        discount_input_user_errors(input.as_ref(), input_arg, typename, false);
                    if let Some(error) =
                        self.discount_subscription_gate_error(request, input.as_ref(), input_arg)
                    {
                        errors.push(error);
                    }
                    if let Some(input_map) = input.as_ref() {
                        errors.extend(self.discount_reference_user_errors(
                            request,
                            input_map,
                            input_arg,
                            Some(&id),
                        ));
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
        let input = input.unwrap_or_default();
        let summary = self.discount_summary_for_input(typename, &input);
        let shop_currency_code = self.store.shop_currency_code();
        let now_epoch = self.current_epoch_seconds();
        let timestamp = self.next_mutation_timestamp();
        let mut record = discount_record_from_input(
            &id,
            discount_kind,
            typename,
            &input,
            existing_record.as_ref(),
            DiscountRecordBuildContext {
                shop_currency_code: &shop_currency_code,
                summary,
                timestamp: &timestamp,
                now_epoch,
            },
        );
        self.resolve_discount_context_names(&mut record);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(
                &field.name,
                self.discount_node_for_record(&record),
                Vec::new(),
            ),
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
        let shop_currency_code = self.store.shop_currency_code();
        let summary = self.discount_summary_for_input(typename, &input);
        let now_epoch = self.current_epoch_seconds();
        let timestamp = self.next_mutation_timestamp();
        let mut record = discount_record_from_input(
            &id,
            discount_kind,
            typename,
            &input,
            None,
            DiscountRecordBuildContext {
                shop_currency_code: &shop_currency_code,
                summary,
                timestamp: &timestamp,
                now_epoch,
            },
        );
        attach_app_discount_function(&mut record, &function);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            app_discount_payload_for_root(
                &field.name,
                self.discount_body_for_record(&record),
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = discount_input(field, input_arg);
        let mut errors = if self.discount_record(&id).is_none() {
            vec![user_error_with_extra_info(
                vec!["id"],
                "Discount does not exist.",
                Some("INVALID"),
                Value::Null,
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
        let summary = self.discount_summary_for_input(typename, &input);
        let shop_currency_code = self.store.shop_currency_code();
        let now_epoch = self.current_epoch_seconds();
        let timestamp = self.next_mutation_timestamp();
        let mut record = discount_record_from_input(
            &id,
            discount_kind,
            typename,
            &input,
            existing.as_ref(),
            DiscountRecordBuildContext {
                shop_currency_code: &shop_currency_code,
                summary,
                timestamp: &timestamp,
                now_epoch,
            },
        );
        attach_app_discount_function(&mut record, &function);
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            app_discount_payload_for_root(
                &field.name,
                self.discount_body_for_record(&record),
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let expected_kind = discount_kind_for_lifecycle_root(&field.name);
        let activating = field.name.ends_with("Activate");
        let mut record = match self.discount_record(&id).cloned() {
            Some(record) if discount_kind(&record) == expected_kind => record,
            Some(_) => {
                return MutationFieldOutcome::unlogged(discount_payload_for_root(
                    &field.name,
                    Value::Null,
                    vec![discount_unknown_id_user_error(&field.name)],
                ))
            }
            None => match self.hydrate_discount_record(request, &id) {
                // Not staged locally: hydrate the discount from upstream so the
                // transition applies against its real dates/status.
                Some(record) if discount_kind(&record) == expected_kind => record,
                // A truly-unknown id hydrates to nothing. Activate/deactivate of an
                // unknown id reports the type-specific "Code/Automatic discount does
                // not exist." message, the same phrasing delete uses.
                _ => {
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
                        vec![user_error(
                            ["base"],
                            "Discount could not be activated.",
                            Some("INTERNAL_ERROR"),
                        )],
                    ));
                }
            }
        }
        let now_epoch = self.current_epoch_seconds();
        if !discount_transition_is_noop(&record, activating, now_epoch) {
            let timestamp = self.next_mutation_timestamp();
            apply_discount_activate_deactivate(&mut record, activating, &timestamp, now_epoch);
        }
        self.stage_discount_record(record.clone());
        MutationFieldOutcome::staged(
            discount_payload_for_root(
                &field.name,
                self.discount_node_for_record(&record),
                Vec::new(),
            ),
            LogDraft::staged(&field.name, "discounts", vec![id]),
        )
    }

    /// Hydrate a discount that is not present in the local overlay by reading it
    /// from upstream (the live store, or the cassette's recorded `DiscountHydrate`
    /// call). Returns a discount record built from the upstream node, or `None`
    /// when the id resolves to neither a code nor an automatic discount (or no
    /// upstream is available, e.g. snapshot mode).
    fn hydrate_discount_record(&self, request: &Request, id: &str) -> Option<Value> {
        self.hydrate_discount_record_with_query(request, id, DISCOUNT_HYDRATE_QUERY)
    }

    fn hydrate_discount_record_for_update(&self, request: &Request, id: &str) -> Option<Value> {
        self.hydrate_discount_record_with_query(request, id, DISCOUNT_UPDATE_HYDRATE_QUERY)
    }

    fn hydrate_discount_record_with_query(
        &self,
        request: &Request,
        id: &str,
        query: &str,
    ) -> Option<Value> {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
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
                            "asyncUsageCount": code_node.get("asyncUsageCount").cloned().unwrap_or_else(|| json!(0))
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
            "asyncUsageCount": disc.get("asyncUsageCount").cloned().unwrap_or_else(|| json!(0)),
            "appliesOnOneTimePurchase": disc.get("appliesOnOneTimePurchase").cloned().unwrap_or(Value::Null),
            "appliesOnSubscription": disc.get("appliesOnSubscription").cloned().unwrap_or(Value::Null),
            "codes": codes,
            "codesCount": count_object(codes_count)
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
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let expected_kind = discount_kind_for_lifecycle_root(&field.name);
        let exists = self
            .discount_record(&id)
            .map(|record| discount_kind(record) == expected_kind)
            .unwrap_or(false);
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
                "userErrors": [user_error_with_extra_info(Value::Null, message, Some("TOO_MANY_ARGUMENTS"), Value::Null)],
            }));
        }
        MutationFieldOutcome::unlogged(json!({
            "job": Value::Null,
            "userErrors": [user_error_with_extra_info(Value::Null, "Local staging for this discount mutation is not implemented.", Some("NOT_IMPLEMENTED"), Value::Null)],
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
                .filter(|record| self.discount_matches_query(record, search))
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
                let now_epoch = self.current_epoch_seconds();
                if let Some(mut record) = self.store.staged.discounts.get(id).cloned() {
                    if !discount_transition_is_noop(&record, activating, now_epoch) {
                        let timestamp = self.next_mutation_timestamp();
                        apply_discount_activate_deactivate(
                            &mut record,
                            activating,
                            &timestamp,
                            now_epoch,
                        );
                        self.stage_discount_record(record);
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
        let discount_id = resolved_string_field(&field.arguments, "discountId").unwrap_or_default();
        if !self
            .discount_record(&discount_id)
            .map(discount_record_accepts_redeem_code_bulk_add)
            .unwrap_or(false)
        {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [user_error_with_extra_info(vec!["discountId"], "Code discount does not exist.", Some("INVALID"), Value::Null)]
            }));
        }
        let codes = resolved_redeem_codes(field);
        if codes.len() > 250 {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [user_error_with_extra_info(vec!["codes"], &format!("The input array size of {} is greater than the maximum allowed of 250.", codes.len()), Some("MAX_INPUT_SIZE_EXCEEDED"), Value::Null)]
            }));
        }
        if codes.is_empty() {
            return MutationFieldOutcome::unlogged(json!({
                "bulkCreation": Value::Null,
                "userErrors": [user_error_with_extra_info(vec!["codes"], "Codes can't be blank", Some("BLANK"), Value::Null)]
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
            if self.discount_code_is_taken(request, code, None) {
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
            record["codesCount"] = count_object(next.len());
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
                "userErrors": [user_error_with_extra_info(Value::Null, "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.", Some("MISSING_ARGUMENT"), Value::Null)]
            }));
        }
        if selector_count > 1 {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [user_error_with_extra_info(Value::Null, "Only one of 'ids', 'search' or 'saved_search_id' is allowed.", Some("TOO_MANY_ARGUMENTS"), Value::Null)]
            }));
        }
        let discount_id = resolved_string_field(&field.arguments, "discountId").unwrap_or_default();
        if self.discount_record(&discount_id).is_none() {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [user_error_with_extra_info(vec!["discountId"], "Code discount does not exist.", Some("INVALID"), Value::Null)]
            }));
        }
        let ids_to_delete: BTreeSet<String> = match field.arguments.get("ids") {
            Some(ResolvedValue::List(ids)) if ids.is_empty() => {
                return MutationFieldOutcome::unlogged(json!({
                    "job": Value::Null,
                    "userErrors": [user_error_with_extra_info(Value::Null, "Something went wrong, please try again.", None, Value::Null)]
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
                "userErrors": [user_error_with_extra_info(vec!["search"], "'Search' can't be blank.", Some("BLANK"), Value::Null)]
            }));
        }
        if field.arguments.contains_key("savedSearchId")
            || field.arguments.contains_key("saved_search_id")
        {
            return MutationFieldOutcome::unlogged(json!({
                "job": Value::Null,
                "userErrors": [user_error_with_extra_info(vec!["savedSearchId"], "Invalid 'saved_search_id'.", Some("INVALID"), Value::Null)]
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
            record["codesCount"] = count_object(count);
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
        root_payload_json(fields, |field| {
            let value = match field.name.as_str() {
                "discountNode" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.discount_record(&id)
                        .map(|record| {
                            self.selected_discount_admin_node_for_record(record, &field.selection)
                        })
                        .unwrap_or(Value::Null)
                }
                "codeDiscountNode" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.discount_record(&id)
                        .map(|record| {
                            self.selected_discount_node_for_record(record, &field.selection)
                        })
                        .unwrap_or(Value::Null)
                }
                "automaticDiscountNode" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.discount_record(&id)
                        .filter(|record| discount_kind(record) == "automatic")
                        .map(|record| {
                            self.selected_discount_node_for_record(record, &field.selection)
                        })
                        .unwrap_or(Value::Null)
                }
                "codeDiscountNodeByCode" => {
                    let code = resolved_string_field(&field.arguments, "code").unwrap_or_default();
                    self.store
                        .staged
                        .discount_code_index
                        .get(&code.to_ascii_uppercase())
                        .and_then(|id| self.discount_record(id))
                        .map(|record| {
                            self.selected_discount_node_for_record(record, &field.selection)
                        })
                        .unwrap_or(Value::Null)
                }
                "discountNodes" => selected_staged_connection_with_args(
                    self.discount_connection_records(),
                    &field.arguments,
                    &field.selection,
                    discount_search_decision,
                    discount_staged_sort_key,
                    selected_discount_admin_node_for_record,
                    value_id_cursor,
                ),
                "automaticDiscountNodes" | "codeDiscountNodes" => {
                    let want_kind = if field.name == "automaticDiscountNodes" {
                        "automatic"
                    } else {
                        "code"
                    };
                    selected_staged_connection_with_args(
                        self.discount_connection_records(),
                        &field.arguments,
                        &field.selection,
                        |record, query| {
                            if discount_kind(record) == want_kind {
                                discount_search_decision(record, query)
                            } else {
                                StagedSearchDecision::NoMatch
                            }
                        },
                        discount_staged_sort_key,
                        selected_discount_node_for_record,
                        value_id_cursor,
                    )
                }
                "discountNodesCount" => selected_json(
                    &staged_count_with_limit_precision(
                        staged_connection_query(
                            self.discount_connection_records(),
                            &field.arguments,
                            discount_search_decision,
                            discount_staged_sort_key,
                            value_id_cursor,
                        )
                        .total_count,
                        &field.arguments,
                    ),
                    &field.selection,
                ),
                "discountRedeemCodeBulkCreation" => {
                    let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                    self.store
                        .staged
                        .discount_redeem_code_bulk_creations
                        .get(&id)
                        .cloned()
                        .map(|value| selected_json(&value, &field.selection))
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            Some(value)
        })
    }

    fn discount_connection_records(&self) -> Vec<Value> {
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
            .map(|record| self.discount_record_with_effective_status(record))
            .collect()
    }

    fn effective_discount_status(&self, record: &Value) -> &'static str {
        let starts_at = record
            .get("startsAt")
            .and_then(Value::as_str)
            .unwrap_or_default();
        discount_status_from_dates(starts_at, &record["endsAt"], self.current_epoch_seconds())
    }

    fn discount_record_with_effective_status(&self, record: &Value) -> Value {
        let mut record = record.clone();
        record["status"] = json!(self.effective_discount_status(&record));
        record
    }

    fn selected_discount_node_for_record(
        &self,
        record: &Value,
        selection: &[SelectedField],
    ) -> Value {
        let record = self.discount_record_with_effective_status(record);
        selected_discount_node_for_record(&record, selection)
    }

    fn selected_discount_admin_node_for_record(
        &self,
        record: &Value,
        selection: &[SelectedField],
    ) -> Value {
        let record = self.discount_record_with_effective_status(record);
        selected_discount_admin_node_for_record(&record, selection)
    }

    fn discount_body_for_record(&self, record: &Value) -> Value {
        discount_body_for_record(&self.discount_record_with_effective_status(record))
    }

    fn discount_node_for_record(&self, record: &Value) -> Value {
        discount_node_for_record(&self.discount_record_with_effective_status(record))
    }

    fn discount_matches_query(&self, record: &Value, query: &str) -> bool {
        discount_matches_query_with_status(record, query, self.effective_discount_status(record))
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
            let value = self.discount_node_for_record(record);
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
            return Err(user_error_with_extra_info(
                [input_arg, "functionHandle"],
                "Function id can't be blank.",
                Some("MISSING_FUNCTION_IDENTIFIER"),
                Value::Null,
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
                self.fetch_shopify_function(
                    request,
                    function_id.as_deref(),
                    function_handle.as_deref(),
                )
            });
        let Some(function) = function else {
            return Err(user_error_with_extra_info([input_arg, field_name], &format!(
                    "Function {identifier} not found. Ensure that it is released in the current app ({}), and that the app is installed.",
                    request_api_client_id(request)
                ), Some("INVALID"), Value::Null));
        };
        if !app_discount_function_api_type_is_supported(&function) {
            return Err(user_error_with_extra_info([input_arg, field_name], "Unexpected Function API. The provided function must implement one of the following extension targets: [product_discounts, order_discounts, shipping_discounts, discount].", None, Value::Null));
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

    fn discount_summary_for_input(
        &self,
        typename: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> String {
        if typename.contains("Bxgy") {
            return discount_bxgy_summary(input);
        }
        if typename.contains("FreeShipping") {
            return self.discount_free_shipping_summary_for_input(typename, input);
        }
        if typename.contains("Basic") {
            return self.discount_basic_summary_for_input(input);
        }
        "Discount".to_string()
    }

    fn discount_basic_summary_for_input(&self, input: &BTreeMap<String, ResolvedValue>) -> String {
        discount_summary_with_parts(
            format!(
                "{} {}",
                discount_amount_off_summary_value(input),
                self.discount_basic_scope_for_input(input)
            ),
            [discount_minimum_requirement_summary(input)],
        )
    }

    fn discount_free_shipping_summary_for_input(
        &self,
        typename: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> String {
        let scope = if typename.starts_with("DiscountAutomatic") {
            "all products".to_string()
        } else {
            discount_purchase_scope_summary(input, &[], "all products", false)
        };
        discount_summary_with_parts(
            format!("Free shipping on {scope}"),
            [
                discount_minimum_requirement_summary(input),
                Some(discount_destination_summary(input)),
                discount_maximum_shipping_price_summary(input),
                resolved_bool_path(input, &["appliesOncePerCustomer"])
                    .unwrap_or(false)
                    .then(|| "One use per customer".to_string()),
            ],
        )
    }

    fn discount_basic_scope_for_input(&self, input: &BTreeMap<String, ResolvedValue>) -> String {
        if let Some(title) = self.discount_product_title_scope(input, &["customerGets", "items"]) {
            return title;
        }
        discount_purchase_scope_summary(
            input,
            &["customerGets"],
            "entire order",
            self.shop_sells_subscriptions.unwrap_or(false),
        )
    }

    fn discount_product_title_scope(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        path: &[&str],
    ) -> Option<String> {
        let mut products_path = path.to_vec();
        products_path.extend(["products", "productsToAdd"]);
        let products = resolved_string_list_path(input, &products_path);
        if products.len() == 1 {
            if let Some(title) = self.discount_product_title_for_gid(&products[0]) {
                return Some(title);
            }
        }

        let mut variants_path = path.to_vec();
        variants_path.extend(["products", "productVariantsToAdd"]);
        let variants = resolved_string_list_path(input, &variants_path);
        if products.is_empty() && variants.len() == 1 {
            if let Some(title) = self.discount_product_title_for_gid(&variants[0]) {
                return Some(title);
            }
        }
        None
    }

    fn discount_product_title_for_gid(&self, gid: &str) -> Option<String> {
        if is_shopify_gid_of_type(gid, "Product") {
            return self
                .store
                .product_by_id(gid)
                .map(|product| product.title.clone())
                .filter(|title| !title.trim().is_empty());
        }
        if is_shopify_gid_of_type(gid, "ProductVariant") {
            let variant = self.store.product_variant_by_id(gid)?;
            return self
                .store
                .product_by_id(&variant.product_id)
                .map(|product| product.title.clone())
                .filter(|title| !title.trim().is_empty());
        }
        None
    }
}

fn backfill_context_names<F>(
    context: &mut Value,
    array_key: &str,
    source_field: &str,
    dest_field: &str,
    lookup: F,
) where
    F: Fn(&str) -> Option<Value>,
{
    let Some(items) = context.get_mut(array_key).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        let Some(id) = item
            .get(source_field)
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        if let Some(value) = lookup(&id) {
            item[dest_field] = value;
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
        errors.push(user_error_with_extra_info(
            vec![input_arg],
            "Input is required.",
            Some("REQUIRED"),
            Value::Null,
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
        errors.push(user_error_with_extra_info(
            vec![input_arg, "combinesWith"],
            "The combinesWith settings are not valid for the discount class.",
            Some("INVALID_COMBINES_WITH_FOR_DISCOUNT_CLASS"),
            Value::Null,
        ));
    }
    // (bxgy) `customerGets` cannot entitle "all" items; Shopify reports this ahead of
    // the title-blank check.
    if typename.contains("Bxgy")
        && resolved_bool_path(input, &["customerGets", "items", "all"]) == Some(true)
    {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "customerGets"],
            "Items in 'customer get' cannot be set to all",
            Some("INVALID"),
            Value::Null,
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
                errors.push(user_error_with_extra_info(
                    vec![input_arg, "title"],
                    "Title can't be blank",
                    Some("BLANK"),
                    Value::Null,
                ));
            }
        } else if title.chars().count() > 255 {
            errors.push(user_error_with_extra_info(
                vec![input_arg, "title"],
                "Title is too long (maximum is 255 characters)",
                Some("TOO_LONG"),
                Value::Null,
            ));
        }
    } else if !skip_title_blank {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "title"],
            "Title can't be blank",
            Some("BLANK"),
            Value::Null,
        ));
    }
    if typename.starts_with("DiscountCode") && create {
        match resolved_string_path(input, &["code"]) {
            Some(code) if code.is_empty() => errors.push(user_error_with_extra_info(
                vec![input_arg, "code"],
                "Code is too short (minimum is 1 character)",
                Some("TOO_SHORT"),
                Value::Null,
            )),
            Some(code) if code.contains('\n') || code.contains('\r') => {
                errors.push(user_error_with_extra_info(
                    vec![input_arg, "code"],
                    "Code cannot contain newline characters.",
                    Some("INVALID"),
                    Value::Null,
                ))
            }
            Some(code) if code.chars().count() > 255 => errors.push(user_error_with_extra_info(
                vec![input_arg, "code"],
                "Code is too long (maximum is 255 characters)",
                Some("TOO_LONG"),
                Value::Null,
            )),
            Some(_) => {}
            None => errors.push(user_error_with_extra_info(
                vec![input_arg, "code"],
                "Code can't be blank",
                Some("BLANK"),
                Value::Null,
            )),
        }
    }
    if create && resolved_non_blank_string_field(input, "startsAt").is_none() {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "startsAt"],
            "Starts at can't be blank",
            Some("BLANK"),
            Value::Null,
        ));
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
        errors.push(user_error_with_extra_info(
            vec![
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal cannot be defined when minimum quantity is.",
            Some("CONFLICT"),
            Value::Null,
        ));
        errors.push(user_error_with_extra_info(
            vec![
                input_arg,
                "minimumRequirement",
                "quantity",
                "greaterThanOrEqualToQuantity",
            ],
            "Minimum quantity cannot be defined when minimum subtotal is.",
            Some("CONFLICT"),
            Value::Null,
        ));
    }
    if !typename.contains("Bxgy")
        && resolved_object_path(
            Some(&ResolvedValue::Object(input.clone())),
            &["customerGets", "value", "discountOnQuantity"],
        )
        .is_some()
    {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "customerGets", "value", "discountOnQuantity"],
            "discountOnQuantity field is only permitted with bxgy discounts.",
            Some("INVALID"),
            Value::Null,
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
            errors.push(user_error_with_extra_info(
                vec![input_arg, "endsAt"],
                "Ends at needs to be after starts_at",
                Some("INVALID"),
                Value::Null,
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
        errors.push(user_error_with_extra_info(
            vec![input_arg, "customerGets", "value", "percentage"],
            "Only discountOnQuantity permitted with bxgy discounts.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "value", "discountAmount"],
    )
    .is_some()
    {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "customerGets", "value", "discountAmount"],
            "Only discountOnQuantity permitted with bxgy discounts.",
            Some("INVALID"),
            Value::Null,
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
            errors.push(user_error_with_extra_info(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Quantity cannot be blank.",
                Some("BLANK"),
                Value::Null,
            ));
        }
    }
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "appliesOnSubscription"],
    )
    .is_some()
    {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "customerGets", "appliesOnSubscription"],
            unsupported_message,
            Some("INVALID"),
            Value::Null,
        ));
    }
    if resolved_object_path(
        Some(&input_value),
        &["customerGets", "appliesOnOneTimePurchase"],
    )
    .is_some()
    {
        errors.push(user_error_with_extra_info(
            vec![input_arg, "customerGets", "appliesOnOneTimePurchase"],
            unsupported_message,
            Some("INVALID"),
            Value::Null,
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
            errors.push(user_error_with_extra_info(
                vec![input_arg, "customerBuys", "items"],
                "Items in 'customer buys' must be defined",
                Some("BLANK"),
                Value::Null,
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
        errors.push(user_error_with_extra_info(
            vec![
                input_arg,
                "combinesWith",
                "productDiscountsWithTagsOnSameCartLine",
            ],
            "The shop's plan does not allow setting `productDiscountsWithTagsOnSameCartLine`.",
            Some("PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_NOT_ENTITLED"),
            Value::Null,
        ));
        errors.push(user_error_with_extra_info(vec![input_arg, "combinesWith", "productDiscountsWithTagsOnSameCartLine"], "Combines with product discounts with tags on same cart line is only valid for discounts with the PRODUCT discount class", Some("INVALID_PRODUCT_DISCOUNTS_WITH_TAGS_ON_SAME_CART_LINE_FOR_DISCOUNT_CLASS"), Value::Null));
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
        return Some(user_error_with_extra_info(
            vec![input_arg, "context"],
            DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE,
            Some("INVALID"),
            Value::Null,
        ));
    }
    None
}

/// Map a discount mutation root field to its typed input argument name, then
/// return the resolved input object. The public Admin API names the create/update
/// input argument after the discount kind (e.g. `basicCodeDiscount`), not `input`.
fn discount_field_input(field: &RootFieldSelection) -> Option<BTreeMap<String, ResolvedValue>> {
    let input_arg = discount_mutation_input_arg(&field.name)?;
    if matches!(input_arg, "codeAppDiscount" | "automaticAppDiscount") {
        return None;
    }
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
        errors.push(user_error_with_extra_info(
            [input_arg],
            "Input is required.",
            Some("REQUIRED"),
            Value::Null,
        ));
        return errors;
    };
    let code_app = typename == "DiscountCodeApp";
    let validate_title = !code_app || create || resolved_string_path(input, &["title"]).is_some();
    if validate_title {
        match resolved_string_path(input, &["title"]) {
            Some(title) if title.trim().is_empty() => errors.push(user_error_with_extra_info(
                [input_arg, "title"],
                if code_app {
                    "can't be blank"
                } else {
                    "Title can't be blank."
                },
                Some("INVALID"),
                Value::Null,
            )),
            Some(title) if title.chars().count() > 255 => errors.push(user_error_with_extra_info(
                [input_arg, "title"],
                "is too long (maximum is 255 characters)",
                Some("INVALID"),
                Value::Null,
            )),
            Some(_) => {}
            None => errors.push(user_error_with_extra_info(
                [input_arg, "title"],
                if code_app {
                    "Required argument not found."
                } else {
                    "Title can't be blank."
                },
                Some("INVALID"),
                Value::Null,
            )),
        }
    }
    if code_app {
        match resolved_string_path(input, &["code"]) {
            Some(code) if code.trim().is_empty() => errors.push(user_error_with_extra_info(
                [input_arg, "code"],
                "Discount code can't be blank.",
                Some("INVALID"),
                Value::Null,
            )),
            Some(code) if code.contains('\n') || code.contains('\r') => {
                errors.push(user_error_with_extra_info(
                    [input_arg, "code"],
                    "Code cannot contain newline characters.",
                    Some("INVALID"),
                    Value::Null,
                ))
            }
            Some(code) if code.chars().count() > 255 => errors.push(user_error_with_extra_info(
                [input_arg, "code"],
                "Code is too long (maximum is 255 characters)",
                Some("INVALID"),
                Value::Null,
            )),
            Some(_) => {}
            None if create => errors.push(user_error_with_extra_info(
                [input_arg, "code"],
                "Discount code can't be blank.",
                Some("INVALID"),
                Value::Null,
            )),
            None => {}
        }
    }
    if create && resolved_non_blank_string_field(input, "startsAt").is_none() {
        errors.push(user_error_with_extra_info(
            [input_arg, "startsAt"],
            "Starts at can't be blank.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if matches!(
        resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &["discountClasses"]),
        Some(ResolvedValue::List(values)) if values.is_empty()
    ) {
        errors.push(user_error_with_extra_info(
            [input_arg, "discountClasses"],
            "Discount classes can't be empty.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if discount_context_customer_selection_user_error(input, input_arg).is_some() {
        errors.push(user_error_with_extra_info(
            [input_arg, "context"],
            DISCOUNT_CONTEXT_CUSTOMER_SELECTION_CONFLICT_MESSAGE,
            Some("INVALID"),
            Value::Null,
        ));
    }
    if app_discount_empty_customer_selection(input) {
        errors.push(user_error_with_extra_info(
            [input_arg, "customerSelection"],
            "a minimum of one prerequisite segment or prerequisite customer must be provided",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if typename == "DiscountAutomaticApp" && input.contains_key("channelIds") {
        errors.push(user_error_with_extra_info(
            [input_arg, "channelIds"],
            "Channel IDs are not supported for automatic app discounts.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if resolved_bool_path(input, &["markets", "removeAllMarkets"]).unwrap_or(false)
        && !resolved_string_list_path(input, &["markets", "add"]).is_empty()
    {
        errors.push(user_error_with_extra_info(
            [input_arg, "markets"],
            "Cannot add markets while removeAllMarkets is true.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    let function_id = resolved_non_blank_string_field(input, "functionId");
    let function_handle = resolved_non_blank_string_field(input, "functionHandle");
    match (function_id.is_some(), function_handle.is_some()) {
        (false, false) => errors.push(user_error_with_extra_info(
            [input_arg, "functionHandle"],
            "Function id can't be blank.",
            Some("MISSING_FUNCTION_IDENTIFIER"),
            Value::Null,
        )),
        (true, true) => errors.push(user_error_with_extra_info(
            [input_arg],
            "Only one of functionId or functionHandle is allowed.",
            Some("MULTIPLE_FUNCTION_IDENTIFIERS"),
            Value::Null,
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
            errors.push(user_error_with_extra_info(
                vec![input_arg, "usageLimit"],
                "Usage limit must be less than or equal to 2147483647",
                Some("LESS_THAN_OR_EQUAL_TO"),
                Value::Null,
            ));
        }
        if usage_limit <= 0 {
            errors.push(user_error_with_extra_info(
                vec![input_arg, "usageLimit"],
                "Usage limit must be greater than 0",
                Some("GREATER_THAN"),
                Value::Null,
            ));
        }
        if usage_limit < I32_MIN {
            errors.push(user_error_with_extra_info(
                vec![input_arg, "usageLimit"],
                "Usage limit must be greater than or equal to -2147483648",
                Some("GREATER_THAN_OR_EQUAL_TO"),
                Value::Null,
            ));
        }
    }
    if let Some(recurring_cycle_limit) = resolved_i64_path(input, &["recurringCycleLimit"]) {
        if recurring_cycle_limit > I32_MAX {
            errors.push(user_error_with_extra_info(
                vec![input_arg, "recurringCycleLimit"],
                "Recurring cycle limit must be less than or equal to 2147483647",
                Some("LESS_THAN_OR_EQUAL_TO"),
                Value::Null,
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
        return Some(user_error_with_extra_info(
            vec![input_arg, "customerGets", "appliesOnSubscription"],
            "Customer gets applies on subscription is not permitted for this shop.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if resolved_object_path(Some(&input_value), &["appliesOnSubscription"]).is_some()
        && !subscription_only(&[])
    {
        return Some(user_error_with_extra_info(
            vec![input_arg, "appliesOnSubscription"],
            "Applies on subscription is not permitted for this shop.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if resolved_object_path(Some(&input_value), &["appliesOnOneTimePurchase"]).is_some()
        && !subscription_only(&[])
    {
        return Some(user_error_with_extra_info(
            vec![input_arg, "appliesOnOneTimePurchase"],
            "Applies on one time purchase is not permitted for this shop.",
            Some("INVALID"),
            Value::Null,
        ));
    }
    if resolved_object_path(Some(&input_value), &["recurringCycleLimit"]).is_some()
        && !subscription_only(&[])
        && !subscription_only(&["customerGets"])
    {
        return Some(user_error_with_extra_info(
            vec![input_arg, "recurringCycleLimit"],
            "Recurring cycle limit is not permitted for this shop.",
            Some("INVALID"),
            Value::Null,
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
            return Some(user_error_with_extra_info(
                vec![
                    input_arg,
                    "minimumRequirement",
                    "quantity",
                    "greaterThanOrEqualToQuantity",
                ],
                "Minimum quantity must be less than 2147483647",
                Some("LESS_THAN"),
                Value::Null,
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
        return Some(user_error_with_extra_info(
            vec![
                input_arg,
                "minimumRequirement",
                "subtotal",
                "greaterThanOrEqualToSubtotal",
            ],
            "Minimum subtotal must be less than 1000000000000000000",
            Some("LESS_THAN"),
            Value::Null,
        ));
    }
    if let Some(percentage) = resolved_f64_path(input, &["customerGets", "value", "percentage"]) {
        let outside_range = if is_automatic_basic {
            !(percentage > 0.0 && percentage <= 1.0)
        } else {
            !(0.0..=1.0).contains(&percentage)
        };
        if outside_range {
            return Some(user_error_with_extra_info(
                vec![input_arg, "customerGets", "value", "percentage"],
                "Value must be between 0.0 and 1.0",
                Some("VALUE_OUTSIDE_RANGE"),
                Value::Null,
            ));
        }
    }
    if let Some(amount) = resolved_f64_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        if is_automatic_basic && amount <= 0.0 {
            return Some(user_error_with_extra_info(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be less than 0",
                Some("GREATER_THAN"),
                Value::Null,
            ));
        }
        if !is_automatic_basic && amount < 0.0 {
            return Some(user_error_with_extra_info(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be less than or equal to 0",
                Some("LESS_THAN_OR_EQUAL_TO"),
                Value::Null,
            ));
        }
        if amount >= 1_000_000_000_000_000_000.0 {
            return Some(user_error_with_extra_info(
                vec![
                    input_arg,
                    "customerGets",
                    "value",
                    "discountAmount",
                    "amount",
                ],
                "Value must be greater than -1000000000000000000",
                Some("LESS_THAN"),
                Value::Null,
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

struct DiscountRecordBuildContext<'a> {
    shop_currency_code: &'a str,
    summary: String,
    timestamp: &'a str,
    now_epoch: i64,
}

fn discount_record_from_input(
    id: &str,
    kind: &str,
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    context: DiscountRecordBuildContext<'_>,
) -> Value {
    let title = resolved_string_path(input, &["title"])
        .or_else(|| existing.and_then(|record| record["title"].as_str().map(str::to_string)))
        .unwrap_or_else(|| "Untitled discount".to_string());
    let code = resolved_string_path(input, &["code"])
        .or_else(|| existing.and_then(|record| record["code"].as_str().map(str::to_string)));
    let starts_at = resolved_string_path(input, &["startsAt"])
        .or_else(|| existing.and_then(|record| record["startsAt"].as_str().map(str::to_string)))
        .unwrap_or_else(|| context.timestamp.to_string());
    let ends_at = resolved_string_path(input, &["endsAt"])
        .map(Value::String)
        .or_else(|| existing.map(|record| record["endsAt"].clone()))
        .unwrap_or(Value::Null);
    let created_at = existing
        .and_then(|record| record["createdAt"].as_str().map(str::to_string))
        .unwrap_or_else(|| context.timestamp.to_string());
    let status = discount_status_from_dates(&starts_at, &ends_at, context.now_epoch);
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
    let existing_code_matches_input = code.as_ref().is_some_and(|code| {
        existing
            .and_then(|record| record["code"].as_str())
            .map(|existing_code| existing_code == code)
            .unwrap_or(false)
    });
    let codes = code
        .as_ref()
        .map(|code| {
            if existing_code_matches_input {
                existing
                    .map(|record| record["codes"].clone())
                    .unwrap_or_else(|| json!([]))
            } else {
                json!([{
                    "id": synthetic_shopify_gid("DiscountRedeemCode", stable_redeem_code_suffix(code)),
                    "code": code,
                    "asyncUsageCount": 0
                }])
            }
        })
        .or_else(|| existing.map(|record| record["codes"].clone()))
        .unwrap_or_else(|| json!([]));
    let async_usage_count = existing
        .and_then(|record| record.get("asyncUsageCount").cloned())
        .unwrap_or_else(|| json!(0));
    let customer_gets =
        discount_customer_gets_for_update(typename, input, existing, context.shop_currency_code);
    let applies_on_one_time_purchase = resolved_bool_path(input, &["appliesOnOneTimePurchase"])
        .map(Value::from)
        .or_else(|| {
            existing
                .and_then(|record| record["appliesOnOneTimePurchase"].as_bool())
                .map(Value::from)
        })
        .unwrap_or(Value::Bool(true));
    let applies_on_subscription = resolved_bool_path(input, &["appliesOnSubscription"])
        .map(Value::from)
        .or_else(|| {
            existing
                .and_then(|record| record["appliesOnSubscription"].as_bool())
                .map(Value::from)
        })
        .unwrap_or(Value::Bool(false));
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
        "updatedAt": context.timestamp,
        "asyncUsageCount": async_usage_count,
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
        "customerGets": customer_gets,
        "minimumRequirement": discount_minimum_requirement_from_input(input, context.shop_currency_code),
        "destinationSelection": discount_destination_selection_from_input(input),
        "maximumShippingPrice": discount_maximum_shipping_price_from_input(input, context.shop_currency_code),
        "appliesOncePerCustomer": resolved_bool_path(input, &["appliesOncePerCustomer"]).unwrap_or(false),
        "appliesOnOneTimePurchase": applies_on_one_time_purchase,
        "appliesOnSubscription": applies_on_subscription,
        "codes": codes,
        "codesCount": count_object(codes.as_array().map(Vec::len).unwrap_or(0)),
        "metafields": discount_metafields_from_input(input, context.timestamp)
            .or_else(|| existing.map(|record| record["metafields"].clone()))
            .unwrap_or_else(|| json!([])),
        "summary": context.summary
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

fn selected_discount_node_for_record(record: &Value, selection: &[SelectedField]) -> Value {
    selected_json(
        &discount_node_for_record_with_selection(record, selection),
        selection,
    )
}

fn selected_discount_admin_node_for_record(record: &Value, selection: &[SelectedField]) -> Value {
    selected_json(
        &discount_admin_node_for_record_with_selection(record, selection),
        selection,
    )
}

fn discount_node_for_record(record: &Value) -> Value {
    discount_node_for_record_with_selection(record, &[])
}

fn discount_node_for_record_with_selection(record: &Value, selection: &[SelectedField]) -> Value {
    if discount_kind(record) == "automatic" {
        let discount_selection =
            selected_child_selection(selection, "automaticDiscount").unwrap_or_default();
        json!({
            "id": discount_id(record),
            "automaticDiscount": discount_body_for_record_with_selection(record, &discount_selection),
            "__typename": "DiscountAutomaticNode"
        })
    } else {
        let discount_selection =
            selected_child_selection(selection, "codeDiscount").unwrap_or_default();
        json!({
            "id": discount_id(record),
            "codeDiscount": discount_body_for_record_with_selection(record, &discount_selection),
            "__typename": "DiscountCodeNode"
        })
    }
}

fn discount_admin_node_for_record_with_selection(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let discount_selection = selected_child_selection(selection, "discount").unwrap_or_default();
    json!({
        "id": discount_id(record),
        "discount": discount_body_for_record_with_selection(record, &discount_selection),
        "__typename": if discount_kind(record) == "automatic" {
            "DiscountAutomaticNode"
        } else {
            "DiscountCodeNode"
        }
    })
}

fn discount_body_for_record(record: &Value) -> Value {
    discount_body_for_record_with_selection(record, &[])
}

fn discount_body_for_record_with_selection(record: &Value, selection: &[SelectedField]) -> Value {
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
        "codes": discount_redeem_codes_connection_for_record(record, selection),
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
            "pageInfo": empty_page_info()
        }
    })
}

fn discount_redeem_codes_connection_for_record(
    record: &Value,
    selection: &[SelectedField],
) -> Value {
    let arguments = selected_discount_body_field(record, selection, "codes")
        .map(|field| field.arguments.clone())
        .unwrap_or_default();
    let codes = record["codes"].as_array().cloned().unwrap_or_default();
    let (windowed, page_info) = connection_window(&codes, &arguments, value_id_cursor);
    connection_json_with_cursor(windowed, |_, node| value_id_cursor(node), page_info)
}

fn selected_discount_body_field<'a>(
    record: &Value,
    selection: &'a [SelectedField],
    name: &str,
) -> Option<&'a SelectedField> {
    let typename = record.get("typename").and_then(Value::as_str);
    selection.iter().find(|field| {
        field.name == name
            && field
                .type_condition
                .as_deref()
                .is_none_or(|condition| Some(condition) == typename)
    })
}

fn discount_search_decision(record: &Value, query: Option<&str>) -> StagedSearchDecision {
    StagedSearchDecision::from_bool(discount_matches_query(record, query.unwrap_or_default()))
}

fn discount_matches_query(record: &Value, query: &str) -> bool {
    let status = record
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    discount_matches_query_with_status(record, query, status)
}

fn discount_staged_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let primary = match sort_key.unwrap_or("ID") {
        "CREATED_AT" => discount_string_sort_value(record, "createdAt"),
        "ENDS_AT" => discount_nullable_string_sort_value(record, "endsAt"),
        "STARTS_AT" => discount_string_sort_value(record, "startsAt"),
        "TITLE" => discount_normalized_string_sort_value(record, "title"),
        "UPDATED_AT" => discount_string_sort_value(record, "updatedAt"),
        "ID" | "RELEVANCE" => discount_gid_tail_sort_value(record),
        _ => discount_gid_tail_sort_value(record),
    };
    vec![primary, discount_gid_tail_sort_value(record)]
}

fn discount_string_sort_value(record: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(
        record
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    )
}

fn discount_normalized_string_sort_value(record: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(
        record
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn discount_nullable_string_sort_value(record: &Value, field: &str) -> StagedSortValue {
    record
        .get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_string()))
        .unwrap_or(StagedSortValue::Null)
}

fn discount_gid_tail_sort_value(record: &Value) -> StagedSortValue {
    resource_id_tail_sort_value(Some(discount_id(record)))
}

fn app_discount_payload_for_root(root: &str, node: Value, user_errors: Vec<Value>) -> Value {
    discount_payload_with_keys(
        root,
        node,
        user_errors,
        "automaticAppDiscount",
        "codeAppDiscount",
    )
}

fn discount_payload_for_root(root: &str, node: Value, user_errors: Vec<Value>) -> Value {
    discount_payload_with_keys(
        root,
        node,
        user_errors,
        "automaticDiscountNode",
        "codeDiscountNode",
    )
}

fn discount_payload_with_keys(
    root: &str,
    node: Value,
    user_errors: Vec<Value>,
    automatic_key: &'static str,
    code_key: &'static str,
) -> Value {
    let node_key = if root.starts_with("discountAutomatic") {
        automatic_key
    } else {
        code_key
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
    user_error_with_extra_info(vec!["id"], message, Some("INVALID"), Value::Null)
}

fn discount_id(record: &Value) -> &str {
    record["id"].as_str().unwrap_or_default()
}

fn discount_kind(record: &Value) -> &str {
    record["kind"].as_str().unwrap_or_default()
}

fn discount_kind_for_lifecycle_root(root: &str) -> &'static str {
    if root.starts_with("discountAutomatic") {
        "automatic"
    } else {
        "code"
    }
}

fn discount_record_accepts_redeem_code_bulk_add(record: &Value) -> bool {
    discount_kind(record) == "code"
        && record
            .get("typename")
            .and_then(Value::as_str)
            .map(|typename| typename != "DiscountCodeApp")
            .unwrap_or(true)
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

fn discount_transition_is_noop(record: &Value, activating: bool, now_epoch: i64) -> bool {
    let starts_at = record
        .get("startsAt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let current_status = discount_status_from_dates(starts_at, &record["endsAt"], now_epoch);
    // An idempotent transition -- activating an already-active discount, or
    // deactivating an already-expired one -- is a no-op: Shopify leaves
    // startsAt/endsAt/updatedAt exactly as they were. A SCHEDULED discount being
    // deactivated is a real transition (it gets an endsAt and becomes EXPIRED).
    if activating {
        current_status == "ACTIVE"
    } else {
        current_status == "EXPIRED"
    }
}

fn apply_discount_activate_deactivate(
    record: &mut Value,
    activating: bool,
    timestamp: &str,
    now_epoch: i64,
) {
    record["status"] = json!(if activating { "ACTIVE" } else { "EXPIRED" });
    record["updatedAt"] = json!(timestamp);
    if record
        .get("startsAt")
        .and_then(Value::as_str)
        .and_then(super::app_shipping_helpers::parse_rfc3339_epoch_seconds)
        .map(|starts_at| starts_at > now_epoch)
        .unwrap_or(true)
    {
        record["startsAt"] = json!(timestamp);
    }
    if activating {
        record["endsAt"] = Value::Null;
    } else if record
        .get("endsAt")
        .and_then(Value::as_str)
        .and_then(super::app_shipping_helpers::parse_rfc3339_epoch_seconds)
        .map(|ends_at| ends_at > now_epoch)
        .unwrap_or(true)
    {
        record["endsAt"] = json!(timestamp);
    }
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

fn discount_matches_query_with_status(record: &Value, query: &str, status: &str) -> bool {
    let normalized = query.to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }
    if normalized.contains("status:active") && status != "ACTIVE" {
        return false;
    }
    if normalized.contains("status:expired") && status != "EXPIRED" {
        return false;
    }
    if normalized.contains("status:scheduled") && status != "SCHEDULED" {
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

fn resolved_non_blank_string_field(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    resolved_string_field(input, field).filter(|value| !value.trim().is_empty())
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

fn discount_status_from_dates(starts_at: &str, ends_at: &Value, now_epoch: i64) -> &'static str {
    if super::app_shipping_helpers::parse_rfc3339_epoch_seconds(starts_at)
        .map(|starts_at| starts_at > now_epoch)
        .unwrap_or(false)
    {
        return "SCHEDULED";
    }
    if ends_at
        .as_str()
        .and_then(super::app_shipping_helpers::parse_rfc3339_epoch_seconds)
        .map(|ends_at| ends_at <= now_epoch)
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

fn discount_customer_gets_for_update(
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    shop_currency_code: &str,
) -> Value {
    let has_customer_gets = resolved_object_path(
        Some(&ResolvedValue::Object(input.clone())),
        &["customerGets"],
    )
    .is_some();
    if !has_customer_gets {
        if let Some(customer_gets) = existing.and_then(|record| record.get("customerGets")) {
            return customer_gets.clone();
        }
    }

    let mut customer_gets = discount_customer_gets_from_input(typename, input, shop_currency_code);
    if resolved_bool_path(input, &["customerGets", "appliesOnOneTimePurchase"]).is_none() {
        if let Some(applies_on_one_time_purchase) =
            existing.and_then(|record| record["customerGets"]["appliesOnOneTimePurchase"].as_bool())
        {
            customer_gets["appliesOnOneTimePurchase"] = json!(applies_on_one_time_purchase);
        }
    }
    if resolved_bool_path(input, &["customerGets", "appliesOnSubscription"]).is_none() {
        if let Some(applies_on_subscription) =
            existing.and_then(|record| record["customerGets"]["appliesOnSubscription"].as_bool())
        {
            customer_gets["appliesOnSubscription"] = json!(applies_on_subscription);
        }
    }
    customer_gets
}

fn discount_customer_gets_from_input(
    typename: &str,
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    let value = if typename.contains("Bxgy") {
        discount_on_quantity_value_from_input(input, shop_currency_code)
    } else if let Some(percentage) =
        resolved_f64_path(input, &["customerGets", "value", "percentage"])
    {
        json!({ "__typename": "DiscountPercentage", "percentage": percentage })
    } else if let Some(amount) = discount_amount_value_from_input(
        input,
        &["customerGets", "value", "discountAmount"],
        shop_currency_code,
    ) {
        amount
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
        "appliesOnOneTimePurchase": resolved_bool_path(input, &["customerGets", "appliesOnOneTimePurchase"]).unwrap_or(true),
        "appliesOnSubscription": resolved_bool_path(input, &["customerGets", "appliesOnSubscription"]).unwrap_or(false)
    })
}

fn discount_on_quantity_value_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
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
    } else if let Some(amount) = discount_amount_value_from_input(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "discountAmount",
        ],
        shop_currency_code,
    ) {
        amount
    } else if let Some(amount) = resolved_decimal_text_path(
        input,
        &[
            "customerGets",
            "value",
            "discountOnQuantity",
            "effect",
            "amount",
        ],
    ) {
        fixed_discount_amount_value(&amount, false, shop_currency_code)
    } else {
        json!({ "__typename": "DiscountPercentage", "percentage": 1.0 })
    };
    json!({
        "__typename": "DiscountOnQuantity",
        "quantity": { "quantity": quantity },
        "effect": effect
    })
}

fn discount_amount_value_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    base_path: &[&str],
    shop_currency_code: &str,
) -> Option<Value> {
    let mut amount_path = base_path.to_vec();
    amount_path.push("amount");
    let amount = resolved_decimal_text_path(input, &amount_path)?;
    Some(fixed_discount_amount_value(
        &amount,
        discount_amount_applies_on_each_item(input, base_path),
        shop_currency_code,
    ))
}

fn fixed_discount_amount_value(
    amount: &str,
    applies_on_each_item: bool,
    shop_currency_code: &str,
) -> Value {
    json!({
        "__typename": "DiscountAmount",
        "amount": money_value(amount, shop_currency_code),
        "appliesOnEachItem": applies_on_each_item
    })
}

fn discount_amount_applies_on_each_item(
    input: &BTreeMap<String, ResolvedValue>,
    base_path: &[&str],
) -> bool {
    for field in ["appliesOnEachItem", "each", "useEach"] {
        let mut path = base_path.to_vec();
        path.push(field);
        if let Some(value) = resolved_bool_path(input, &path) {
            return value;
        }
    }
    false
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

fn discount_minimum_requirement_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
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
                "currencyCode": shop_currency_code
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

fn discount_maximum_shipping_price_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    resolved_decimal_text_path(input, &["maximumShippingPrice"])
        .map(|amount| money_value(&amount, shop_currency_code))
        .unwrap_or(Value::Null)
}

fn discount_metafields_from_input(
    input: &BTreeMap<String, ResolvedValue>,
    timestamp: &str,
) -> Option<Value> {
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
                        "createdAt": timestamp,
                        "updatedAt": timestamp
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
    let root = ResolvedValue::Object(input.clone());
    resolved_decimal_text(resolved_object_path(Some(&root), path))
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

fn discount_amount_off_summary_value(input: &BTreeMap<String, ResolvedValue>) -> String {
    if let Some(percentage) = resolved_f64_path(input, &["customerGets", "value", "percentage"]) {
        return format!(
            "{}% off",
            discount_percentage_summary_number(percentage * 100.0)
        );
    }
    if let Some(amount) = resolved_decimal_text_path(
        input,
        &["customerGets", "value", "discountAmount", "amount"],
    ) {
        return format!("{} off", discount_money_summary_amount(&amount));
    }
    "10% off".to_string()
}

fn discount_purchase_scope_summary(
    input: &BTreeMap<String, ResolvedValue>,
    base_path: &[&str],
    default_scope: &str,
    subscription_capable_default: bool,
) -> String {
    let mut one_time_path = base_path.to_vec();
    one_time_path.push("appliesOnOneTimePurchase");
    let mut subscription_path = base_path.to_vec();
    subscription_path.push("appliesOnSubscription");
    let applies_on_one_time = resolved_bool_path(input, &one_time_path);
    let applies_on_subscription = resolved_bool_path(input, &subscription_path);
    match (applies_on_one_time, applies_on_subscription) {
        (Some(false), Some(true)) => "subscription products".to_string(),
        (Some(true), Some(false)) => "one-time purchase products".to_string(),
        (Some(true), Some(true)) => "all products".to_string(),
        (None, None) if subscription_capable_default => "one-time purchase products".to_string(),
        _ => default_scope.to_string(),
    }
}

fn discount_minimum_requirement_summary(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    if let Some(amount) = resolved_decimal_text_path(
        input,
        &[
            "minimumRequirement",
            "subtotal",
            "greaterThanOrEqualToSubtotal",
        ],
    ) {
        return Some(format!(
            "Minimum purchase of {}",
            discount_money_summary_amount(&amount)
        ));
    }
    resolved_i64_path(
        input,
        &[
            "minimumRequirement",
            "quantity",
            "greaterThanOrEqualToQuantity",
        ],
    )
    .map(|quantity| format!("Minimum quantity of {quantity}"))
}

fn discount_destination_summary(input: &BTreeMap<String, ResolvedValue>) -> String {
    let input_value = ResolvedValue::Object(input.clone());
    if resolved_object_path(Some(&input_value), &["destination", "countries"]).is_some() {
        let countries = resolved_string_list_path(input, &["destination", "countries", "add"]);
        return match countries.as_slice() {
            [country] => format!("For {}", discount_country_summary_name(country)),
            countries => format!("For {} countries", countries.len()),
        };
    }
    "For all countries".to_string()
}

fn discount_maximum_shipping_price_summary(
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<String> {
    resolved_decimal_text_path(input, &["maximumShippingPrice"]).map(|amount| {
        format!(
            "Applies to shipping rates under {}",
            discount_money_summary_amount(&amount)
        )
    })
}

fn discount_summary_with_parts(
    lead: String,
    parts: impl IntoIterator<Item = Option<String>>,
) -> String {
    std::iter::once(lead)
        .chain(parts.into_iter().flatten())
        .collect::<Vec<_>>()
        .join(" • ")
}

fn discount_percentage_summary_number(value: f64) -> String {
    let rounded = value.round();
    if (value - rounded).abs() < 0.000_000_1 {
        return format!("{rounded:.0}");
    }
    trim_decimal_zeros(&format!("{value:.2}"))
}

fn discount_money_summary_amount(amount: &str) -> String {
    let parsed = amount.trim().parse::<f64>().unwrap_or(0.0).abs();
    format!("${parsed:.2}")
}

fn trim_decimal_zeros(value: &str) -> String {
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn discount_country_summary_name(country_code: &str) -> String {
    match country_code {
        "AU" => "Australia",
        "CA" => "Canada",
        "DE" => "Germany",
        "DK" => "Denmark",
        "FR" => "France",
        "GB" => "United Kingdom",
        "JP" => "Japan",
        "US" => "United States",
        _ => country_code,
    }
    .to_string()
}

pub(in crate::proxy) fn gift_card_lifecycle_base_card(
    id: &str,
    _shop_currency_code: &str,
) -> Value {
    let timestamp = default_product_timestamp();
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": resource_id_path_tail(id),
        "lastCharacters": null,
        "maskedCode": null,
        "giftCardCode": null,
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": null,
        "note": null,
        "templateSuffix": null,
        "createdAt": timestamp.clone(),
        "updatedAt": timestamp,
        "initialValue": null,
        "balance": null,
        "customer": null,
        "recipientAttributes": null,
        "transactions": connection_json(Vec::new())
    })
}

pub(in crate::proxy) fn gift_card_configuration_record(shop_currency_code: &str) -> Value {
    json!({
        "issueLimit": money_value("3000.0", shop_currency_code),
        "purchaseLimit": money_value("14000.0", shop_currency_code)
    })
}

pub(in crate::proxy) fn push_gift_card_transaction(card: &mut Value, transaction: Value) {
    if !card.get("transactions").is_some_and(Value::is_object) {
        card["transactions"] = connection_json(Vec::new());
    } else {
        if !card["transactions"]
            .get("nodes")
            .is_some_and(Value::is_array)
        {
            card["transactions"]["nodes"] = json!([]);
        }
        if !card["transactions"]
            .get("edges")
            .is_some_and(Value::is_array)
        {
            card["transactions"]["edges"] = json!([]);
        }
        if !card["transactions"]
            .get("pageInfo")
            .is_some_and(Value::is_object)
        {
            card["transactions"]["pageInfo"] = empty_page_info();
        }
    }
    if let Some(nodes) = card["transactions"]["nodes"].as_array_mut() {
        nodes.push(transaction);
    }
}

pub(in crate::proxy) fn backup_region_country_code_coercion_error(
    message: &str,
    operation_path: &str,
    code: &str,
    location: SourceLocation,
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
            "locations": [{ "line": location.line, "column": location.column }],
            "path": [operation_path, "backupRegionUpdate", "region", "countryCode"],
            "extensions": extensions
        }]
    })
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
    None
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
    root_payload_json(fields, |field| {
        Some(match field.name.as_str() {
            "cashTrackingSession"
            | "pointOfSaleDevice"
            | "dispute"
            | "disputeEvidence"
            | "shopPayPaymentRequestReceipt" => Value::Null,
            "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" => {
                selected_empty_connection_json(&field.selection)
            }
            _ => Value::Null,
        })
    })
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
                return Some(user_error_with_extra_info(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit cannot be zero",
                    Some("VALUE_OUTSIDE_RANGE"),
                    Value::Null,
                ));
            }
            if n < 0 {
                return Some(user_error_with_extra_info(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be greater than 0",
                    Some("GREATER_THAN"),
                    Value::Null,
                ));
            }
            if n > 2_147_483_647 {
                return Some(user_error_with_extra_info(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be less than or equal to 2147483647",
                    Some("LESS_THAN_OR_EQUAL_TO"),
                    Value::Null,
                ));
            }
        }
    }

    if let Some(n) = resolved_i64_path(input, &["customerBuys", "value", "quantity"]) {
        if n == 0 {
            return Some(user_error_with_extra_info(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be greater than 0",
                Some("GREATER_THAN"),
                Value::Null,
            ));
        }
        if n >= 100_000 {
            return Some(user_error_with_extra_info(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be less than 100000",
                Some("LESS_THAN"),
                Value::Null,
            ));
        }
    }

    if let Some(n) = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    ) {
        if n == 0 {
            return Some(user_error_with_extra_info(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be greater than 0",
                Some("GREATER_THAN"),
                Value::Null,
            ));
        }
        if n >= 100_000 {
            return Some(user_error_with_extra_info(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be less than 100000",
                Some("LESS_THAN"),
                Value::Null,
            ));
        }
    }
    None
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
        "codes": connection_json_with_empty_edges(
            codes.iter().enumerate().map(|(index, code)| {
                discount_redeem_code_bulk_creation_node(code, codes, index, existing, pending)
            }).collect::<Vec<_>>()
        )
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
        return vec![user_error_with_extra_info(
            ["code"],
            "is too short (minimum is 1 character)",
            None,
            Value::Null,
        )];
    }
    if code.contains('\n') || code.contains('\r') {
        return vec![user_error_with_extra_info(
            ["code"],
            "cannot contain newline characters.",
            None,
            Value::Null,
        )];
    }
    if code.chars().count() > 255 {
        return vec![user_error_with_extra_info(
            ["code"],
            "is too long (maximum is 255 characters)",
            None,
            Value::Null,
        )];
    }
    let normalized = code.to_ascii_uppercase();
    // A second (or later) occurrence of the same code within this submission.
    if codes
        .iter()
        .take(index)
        .any(|candidate| candidate.to_ascii_uppercase() == normalized)
    {
        return vec![user_error_with_extra_info(
            ["code"],
            "Codes must be unique within BulkDiscountCodeCreation",
            None,
            Value::Null,
        )];
    }
    // The code is already assigned to some discount in the shop.
    if existing.contains(&normalized) {
        return vec![user_error_with_extra_info(
            ["code"],
            "must be unique. Please try a different code.",
            None,
            Value::Null,
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}
