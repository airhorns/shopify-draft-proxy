use super::*;

enum StoreCreditAccountMutationResolution {
    Existing(String),
    CreateForOwner(String),
}

#[derive(Clone)]
struct CustomerCustomId {
    namespace: String,
    key: String,
    value: String,
}

#[derive(Default)]
struct CustomerCustomIdUpstreamLookup {
    valid_definition: bool,
    found_id: Option<String>,
}

const CUSTOMER_HYDRATE_QUERY: &str = r#"
query CustomerHydrate($id: ID!) {
  customer(id: $id) {
    id
    firstName
    lastName
    displayName
    email
    phone
    locale
    note
    canDelete
    verifiedEmail
    dataSaleOptOut
    taxExempt
    taxExemptions
    state
    tags
    createdAt
    updatedAt
    defaultEmailAddress { emailAddress }
    defaultPhoneNumber { phoneNumber }
    defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }
  }
}
"#;
// Shared with the parity capture scripts via include_str! so recorded address-aware
// `CustomerHydrate` cassettes byte-match the request forwarded when address nodes
// are required for validation/output. The leading newline is significant: the
// cassette matcher only trims trailing whitespace.
const CUSTOMER_ADDRESS_HYDRATE_QUERY: &str =
    include_str!("../../../config/parity-requests/customers/customer-mutation-hydrate.graphql");

// Shared with the parity capture scripts via include_str! so recorded
// `CustomerDuplicateHydrate` dedupe cassettes byte-match what the create path forwards
// upstream. The leading newline is significant: the cassette matcher only trims trailing
// whitespace.
const CUSTOMER_DUPLICATE_HYDRATE_QUERY: &str =
    include_str!("../../../config/parity-requests/customers/customer-duplicate-hydrate.graphql");
const CUSTOMER_CUSTOM_ID_LOOKUP_QUERY: &str =
    include_str!("../../../config/parity-requests/customers/customer-custom-id-lookup.graphql");

const CUSTOMER_MERGE_HYDRATE_QUERY: &str = r#"
query CustomerMergeHydrate($ids: [ID!]!) {
  nodes(ids: $ids) {
    ... on Customer {
      id
      firstName
      lastName
      displayName
      email
      phone
      locale
      note
      canDelete
      verifiedEmail
      dataSaleOptOut
      taxExempt
      taxExemptions
      state
      tags
      numberOfOrders
      createdAt
      updatedAt
      defaultEmailAddress { emailAddress }
      defaultPhoneNumber { phoneNumber }
      defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }
      lastOrder { id name email createdAt }
    }
  }
}
"#;
const CUSTOMER_MERGE_ATTACHED_HYDRATE_QUERY: &str = r#"
query CustomerMergeAttachedHydrate($ids: [ID!]!) {
  nodes(ids: $ids) {
    ... on Customer {
      id
      defaultAddress { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea }
      addressesV2(first: 5) { nodes { id firstName lastName address1 address2 city company province provinceCode country countryCodeV2 zip phone name formattedArea } }
      metafields(first: 5) { nodes { id namespace key type value } }
      orders(first: 5, sortKey: CREATED_AT, reverse: true) {
        edges { cursor node { id name email createdAt } }
      }
      lastOrder { id name email createdAt }
    }
  }
}
"#;
const CUSTOMER_DELETE_SHOP_HYDRATE_QUERY: &str =
    include_str!("../../../config/parity-requests/customers/customer-delete-shop-hydrate.graphql");
const STORE_CREDIT_CUSTOMER_HYDRATE_QUERY: &str = include_str!(
    "../../../config/parity-requests/customers/storeCreditCustomerHydrate-parity.graphql"
);
const STORE_CREDIT_ACCOUNT_HYDRATE_QUERY: &str = include_str!(
    "../../../config/parity-requests/customers/storeCreditAccountHydrate-parity.graphql"
);
const CUSTOMER_ACCOUNT_ACTIVATION_TOKEN_FIELD: &str = "__proxyAccountActivationToken";
const CUSTOMER_ACCOUNT_INVITE_FIELD: &str = "__proxyAccountInvite";

impl DraftProxy {
    pub(in crate::proxy) fn dispatch_unknown_passthrough_or_legacy_error(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        operation_type: OperationType,
        root_fields: &[String],
        root_field: &str,
    ) -> Response {
        if operation_type == OperationType::Query
            && self.has_staged_url_redirects()
            && root_fields.iter().all(|field| {
                matches!(
                    field.as_str(),
                    "urlRedirect" | "urlRedirects" | "urlRedirectsCount"
                )
            })
        {
            let Some(fields) = crate::graphql::root_fields(query, variables) else {
                return json_error(400, "Could not parse GraphQL operation");
            };
            return ok_json(json!({ "data": self.url_redirect_query_data(&fields) }));
        }
        match operation_type {
            OperationType::Mutation
                if self.config.unsupported_mutation_mode
                    == Some(UnsupportedMutationMode::Reject) =>
            {
                json_error(
                    400,
                    &format!(
                        "Unsupported mutation rejected by configuration: {}",
                        root_field
                    ),
                )
            }
            OperationType::Query if self.config.read_mode == ReadMode::Snapshot => json_error(
                400,
                &format!(
                    "No domain dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            OperationType::Mutation if self.config.read_mode == ReadMode::Snapshot => json_error(
                400,
                &format!(
                    "No mutation dispatcher implemented for root field: {}",
                    root_field
                ),
            ),
            OperationType::Subscription if self.config.read_mode == ReadMode::Snapshot => {
                json_error(
                    400,
                    &format!(
                        "No domain dispatcher implemented for root field: {}",
                        root_field
                    ),
                )
            }
            _ => {
                if operation_type == OperationType::Mutation {
                    self.record_passthrough_log_entry(
                        request,
                        query,
                        variables,
                        root_fields,
                        root_field,
                    );
                }
                let response = (self.upstream_transport)(request.clone());
                if operation_type == OperationType::Mutation && root_field == "customerMerge" {
                    self.observe_customer_merge_passthrough_response(query, variables, &response);
                }
                if operation_type == OperationType::Query
                    && root_fields
                        .iter()
                        .all(|field| matches!(field.as_str(), "node" | "nodes"))
                {
                    self.observe_collection_passthrough_response(&response);
                }
                if operation_type == OperationType::Mutation
                    && matches!(
                        root_field,
                        "collectionAddProducts" | "collectionCreate" | "collectionReorderProducts"
                    )
                {
                    self.observe_collection_passthrough_response(&response);
                    let hydrate_ids =
                        collection_passthrough_hydration_ids(root_field, &response, variables);
                    self.hydrate_product_nodes_for_observation(hydrate_ids);
                }
                response
            }
        }
    }

    pub(in crate::proxy) fn observe_customer_merge_passthrough_response(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        response: &Response,
    ) {
        if !(200..300).contains(&response.status) {
            return;
        }
        let user_errors = response.body["data"]["customerMerge"]["userErrors"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !user_errors.is_empty() {
            return;
        }
        let Some(resulting_id) =
            response.body["data"]["customerMerge"]["resultingCustomerId"].as_str()
        else {
            return;
        };
        let arguments = root_field_arguments(query, variables).unwrap_or_default();
        for field_name in ["customerOneId", "customerTwoId"] {
            if let Some(id) = resolved_string_field(&arguments, field_name) {
                if id != resulting_id {
                    self.store.staged.customers.remove(&id);
                    self.store.staged.customers.tombstone(id);
                }
            }
        }
    }

    pub(in crate::proxy) fn should_handle_customer_overlay_read(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "customer" => resolved_string_field(&field.arguments, "id").is_some_and(|id| {
                self.store.staged.customers.contains_key(&id)
                    || self.store.staged.customers.is_tombstoned(&id)
                    || self.store_credit_owner_has_accounts(&id)
            }),
            "customerByIdentifier" => !self.store.staged.customers.is_empty(),
            // A standalone `customers(query:)` / `customersCount` list read is
            // served from the staged overlay once this scenario has staged at
            // least one customer (e.g. a customerCreate or a privacy
            // dataSaleOptOut synthetic). With no staged customers there is
            // nothing local to serve, so the read forwards upstream unchanged.
            "customers" | "customersCount" => !self.store.staged.customers.is_empty(),
            _ => false,
        })
    }

    pub(in crate::proxy) fn customer_overlay_read_fields(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| match field.name.as_str() {
            "customer" => Some(self.customer_read_field(field)),
            "customerByIdentifier" => Some(self.customer_by_identifier_field(field)),
            "customers" => Some(self.customers_list_field(field)),
            "customersCount" => Some(self.customers_count_field(field)),
            "customerMergeJobStatus" => Some(self.customer_merge_job_status_field(field)),
            "job" => Some(self.customer_merge_job_node_field(field)),
            "node" if self.customer_merge_job_reference(field) => {
                Some(self.customer_merge_job_node_field(field))
            }
            _ => None,
        })
    }

    pub(in crate::proxy) fn customer_read_selects_amount_spent(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "customer" | "customerByIdentifier" | "customers"
            ) && selection_contains_any(&field.selection, &["amountSpent"])
        })
    }

    /// The store-wide total customer count: the seeded live baseline (or the
    /// legacy default) reduced by the number of customers deleted/merged-away in
    /// this scenario, so `customersCount` tracks merges generically.
    pub(in crate::proxy) fn customers_count_value(&self) -> u64 {
        let live_staged_count = self.store.staged.customers.len() as u64;
        let Some(base_count) = self.store.staged.customers_count_base else {
            return live_staged_count;
        };
        let locally_created = self
            .store
            .staged
            .locally_created_customer_ids
            .iter()
            .filter(|id| self.store.staged.customers.contains_key(id))
            .count() as u64;
        let deleted_base_customers = self
            .store
            .staged
            .customers
            .tombstones
            .iter()
            .filter(|id| !self.store.staged.locally_created_customer_ids.contains(*id))
            .count() as u64;
        base_count
            .saturating_add(locally_created)
            .saturating_sub(deleted_base_customers)
    }

    fn customers_count_field(&self, field: &RootFieldSelection) -> Value {
        if field.arguments.contains_key("query") {
            let query = resolved_string_field(&field.arguments, "query");
            let count = self
                .store
                .staged
                .customers
                .values()
                .filter(|customer| {
                    let id = customer
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    !self.store.staged.customers.is_tombstoned(id)
                })
                .filter(|customer| {
                    customer_overlay_search_decision(customer, query.as_deref())
                        == StagedSearchDecision::Match
                })
                .count();
            return selected_json(&count_object(count), &field.selection);
        }

        selected_json(
            &count_object(self.customers_count_value()),
            &field.selection,
        )
    }

    /// `customerMergeJobStatus(jobId:)` read: project the requested selection over
    /// the locally recorded merge request (keyed by the synthetic job id minted by
    /// `customerMerge`). Returns null for unknown job ids.
    pub(in crate::proxy) fn customer_merge_job_status_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(job_id) = resolved_string_field(&field.arguments, "jobId") else {
            return Value::Null;
        };
        self.store
            .staged
            .customer_merge_requests
            .get(&job_id)
            .map(|request| selected_json(request, &field.selection))
            .unwrap_or(Value::Null)
    }

    /// Resolve `job(id:)` / `node(id:)` for a synthetic merge job id minted by
    /// `customer_merge`. Returns a completed `Job` projection from the staged
    /// merge request, or null for ids the proxy did not mint.
    pub(in crate::proxy) fn customer_merge_job_node_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        self.store
            .staged
            .customer_merge_requests
            .get(&id)
            .map(customer_merge_job_from_request)
            .map(|job| selected_json(&job, &field.selection))
            .unwrap_or(Value::Null)
    }

    /// True iff `node(id:)` targets a `Job` id we minted for a staged merge
    /// request, so the overlay read may serve it instead of forwarding.
    pub(in crate::proxy) fn customer_merge_job_reference(
        &self,
        field: &RootFieldSelection,
    ) -> bool {
        resolved_string_field(&field.arguments, "id")
            .as_deref()
            .is_some_and(|id| self.store.staged.customer_merge_requests.contains_key(id))
    }

    pub(in crate::proxy) fn customer_read_field(&self, field: &RootFieldSelection) -> Value {
        let Some(id) = resolved_string_field(&field.arguments, "id") else {
            return Value::Null;
        };
        if self.store.staged.customers.is_tombstoned(&id) {
            return Value::Null;
        }
        self.store
            .staged
            .customers
            .get(&id)
            .map(|customer| self.customer_with_order_connection(&id, customer, &field.selection))
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_with_order_connection(
        &self,
        id: &str,
        customer: &Value,
        selection: &[SelectedField],
    ) -> Value {
        // The per-customer order connection is resolved from the staged
        // `customer_orders` index when present (orders created/transferred in the
        // scenario), windowing + cursoring generically. When a customer has no staged
        // orders but carries a recorded inline `orders` connection (a seeded read
        // baseline whose opaque cursors / pageInfo cannot be reconstructed locally),
        // that recorded page is projected verbatim instead.
        let mapped_orders = self.store.staged.customer_orders.get(id);
        selected_payload_json(selection, |field| match field.name.as_str() {
            "canDelete" => Some(json!(self.customer_can_delete_value(id, customer))),
            "amountSpent"
                if customer.get("amountSpent").is_none_or(Value::is_null)
                    && customer_order_count(customer) == Some(0) =>
            {
                let projected = json!({
                    "amountSpent": money_value("0.0", &self.store.shop_currency_code())
                });
                selected_json(&projected, std::slice::from_ref(field))
                    .as_object()
                    .and_then(|object| object.get(&field.response_key).cloned())
            }
            "orders" => Some(match mapped_orders {
                Some(orders) => selected_staged_connection_with_args(
                    orders.clone(),
                    &field.arguments,
                    &field.selection,
                    order_search_decision,
                    order_staged_sort_key,
                    selected_json,
                    order_connection_cursor,
                ),
                None if connection_has_nodes(&customer["orders"]) => project_seeded_connection(
                    &customer["orders"],
                    &field.arguments,
                    &field.selection,
                ),
                None => selected_connection_json_with_args(
                    Vec::new(),
                    &field.arguments,
                    &field.selection,
                    order_connection_cursor,
                ),
            }),
            "addressesV2" => Some(selected_customer_addresses_connection(customer, field)),
            // The `storeCreditAccounts` connection is resolved from the staged
            // store-credit accounts indexed by owner, so a customer read reflects
            // credit/debit mutations (and locally minted accounts) immediately.
            "storeCreditAccounts" => Some(self.store_credit_accounts_connection_for_owner(
                id,
                &field.arguments,
                &field.selection,
            )),
            "metafield" | "metafields" => {
                let base = selected_json(customer, std::slice::from_ref(field));
                let projected = self.owner_metafield_overlay_owner_json(
                    "customer",
                    id,
                    std::slice::from_ref(field),
                    base,
                );
                Some(
                    projected
                        .get(field.response_key.as_str())
                        .cloned()
                        .unwrap_or(Value::Null),
                )
            }
            _ => selected_json(customer, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn customer_can_delete_value(&self, id: &str, customer: &Value) -> bool {
        if self.customer_has_effective_orders(id, customer) {
            return false;
        }
        customer
            .get("canDelete")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    fn customer_has_effective_orders(&self, id: &str, customer: &Value) -> bool {
        self.store
            .staged
            .customer_orders
            .get(id)
            .is_some_and(|orders| !orders.is_empty())
            || connection_has_nodes(&customer["orders"])
            || customer_order_count(customer).is_some_and(|count| count > 0)
            || customer
                .get("lastOrder")
                .is_some_and(|last_order| !last_order.is_null())
    }

    pub(in crate::proxy) fn store_credit_account_read_fields(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        root_payload_json(fields, |field| {
            if field.name != "storeCreditAccount" {
                return None;
            }
            let value = resolved_string_field(&field.arguments, "id")
                .and_then(|id| self.store.staged.store_credit_accounts.get(&id))
                .map(|account| self.selected_store_credit_account(account, &field.selection))
                .unwrap_or(Value::Null);
            Some(value)
        })
    }

    pub(in crate::proxy) fn store_credit_account_mutation(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(fields) = root_fields(query, variables) else {
            return MutationOutcome::response(json_error(400, "Could not parse GraphQL operation"));
        };
        if let Some(response) = store_credit_result_only_currency_response(&fields) {
            return MutationOutcome::response(response);
        }
        let mut log_drafts = Vec::new();
        let data = root_payload_json(&fields, |field| {
            if !matches!(
                field.name.as_str(),
                "storeCreditAccountCredit" | "storeCreditAccountDebit"
            ) {
                return None;
            }
            let outcome = self.store_credit_account_mutation_field(field, request);
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            Some(outcome.value)
        });
        if data.as_object().is_none_or(serde_json::Map::is_empty) {
            return MutationOutcome::response(json_error(501, "Unsupported store credit mutation"));
        }
        let response = ok_json(json!({ "data": data }));
        if log_drafts.is_empty() {
            MutationOutcome::response(response)
        } else if root_field == "storeCreditAccountCredit"
            || root_field == "storeCreditAccountDebit"
        {
            MutationOutcome::with_log_drafts(response, log_drafts)
        } else {
            MutationOutcome::response(response)
        }
    }

    fn store_credit_account_mutation_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> MutationFieldOutcome {
        let is_credit = field.name == "storeCreditAccountCredit";
        let input_name = if is_credit {
            "creditInput"
        } else {
            "debitInput"
        };
        let amount_name = if is_credit {
            "creditAmount"
        } else {
            "debitAmount"
        };
        let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
        let amount_input = resolved_object_field(&input, amount_name).unwrap_or_default();
        let currency = resolved_string_field(&amount_input, "currencyCode").unwrap_or_default();
        let amount_text = resolved_money_amount_text(&amount_input, "amount");
        let amount = amount_text
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);

        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(account_resolution) =
            self.resolve_store_credit_account_for_mutation(request, &id, &currency, is_credit)
        else {
            return self.store_credit_error_outcome(
                field,
                store_credit_missing_id_user_error(&id, is_credit),
            );
        };

        if is_credit
            && resolved_string_field(&input, "expiresAt")
                .as_deref()
                .map(|expires_at| {
                    store_credit_expires_at_in_past(expires_at, self.current_epoch_seconds())
                })
                .unwrap_or(false)
        {
            return self.store_credit_error_outcome(
                field,
                user_error(
                    [input_name, "expiresAt"],
                    "The expiry date must be in the future",
                    Some("EXPIRES_AT_IN_PAST"),
                ),
            );
        }

        if amount <= 0.0 {
            return self.store_credit_error_outcome(
                field,
                user_error(
                    [input_name, amount_name, "amount"],
                    if is_credit {
                        "A positive amount must be used to credit a store credit account"
                    } else {
                        "A positive amount must be used to debit a store credit account"
                    },
                    Some("NEGATIVE_OR_ZERO_AMOUNT"),
                ),
            );
        }

        let (account_currency, current_balance) = match &account_resolution {
            StoreCreditAccountMutationResolution::Existing(account_id) => {
                let Some(existing) = self.store.staged.store_credit_accounts.get(account_id) else {
                    return self.store_credit_error_outcome(
                        field,
                        user_error(
                            ["id"],
                            "Store credit account does not exist",
                            Some("ACCOUNT_NOT_FOUND"),
                        ),
                    );
                };
                let account_currency = existing["balance"]["currencyCode"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let current_balance = existing["balance"]["amount"]
                    .as_str()
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(0.0);
                (account_currency, current_balance)
            }
            StoreCreditAccountMutationResolution::CreateForOwner(_) => (currency.clone(), 0.0),
        };
        if currency != account_currency {
            return self.store_credit_error_outcome(
                field,
                user_error(
                    [input_name, amount_name, "currencyCode"],
                    "The currency provided does not match the currency of the store credit account",
                    Some("MISMATCHING_CURRENCY"),
                ),
            );
        }

        if is_credit && current_balance + amount >= STORE_CREDIT_LIMIT {
            return self.store_credit_error_outcome(
                field,
                user_error(
                    [input_name, amount_name, "amount"],
                    "The operation would cause the account's credit limit to be exceeded",
                    Some("CREDIT_LIMIT_EXCEEDED"),
                ),
            );
        }
        if !is_credit && amount > current_balance {
            return self.store_credit_error_outcome(
                field,
                user_error(
                    [input_name, amount_name, "amount"],
                    "The store credit account does not have sufficient funds to satisfy the request",
                    Some("INSUFFICIENT_FUNDS"),
                ),
            );
        }

        let (account_id, existing) = match account_resolution {
            StoreCreditAccountMutationResolution::Existing(account_id) => {
                let Some(existing) = self
                    .store
                    .staged
                    .store_credit_accounts
                    .get(&account_id)
                    .cloned()
                else {
                    return self.store_credit_error_outcome(
                        field,
                        user_error(
                            ["id"],
                            "Store credit account does not exist",
                            Some("ACCOUNT_NOT_FOUND"),
                        ),
                    );
                };
                (account_id, existing)
            }
            StoreCreditAccountMutationResolution::CreateForOwner(owner_id) => {
                let account_id = self.create_store_credit_account_for_owner(&owner_id, &currency);
                let Some(existing) = self
                    .store
                    .staged
                    .store_credit_accounts
                    .get(&account_id)
                    .cloned()
                else {
                    return self.store_credit_error_outcome(
                        field,
                        user_error(
                            ["id"],
                            "Store credit account does not exist",
                            Some("ACCOUNT_NOT_FOUND"),
                        ),
                    );
                };
                (account_id, existing)
            }
        };

        let delta = if is_credit { amount } else { -amount };
        let balance_after = current_balance + delta;
        let amount_display = format_money_amount(delta);
        let balance_display = format_money_amount(balance_after);
        let transaction_id = self.next_store_credit_transaction_gid();
        let mut account = existing;
        account["balance"] = money_value(&balance_display, &currency);
        let transaction = json!({
            "id": transaction_id,
            "__typename": if is_credit { "StoreCreditAccountCreditTransaction" } else { "StoreCreditAccountDebitTransaction" },
            "amount": money_value(&amount_display, &currency),
            "balanceAfterTransaction": money_value(&balance_display, &currency),
            "createdAt": self.next_product_timestamp(),
            "event": "ADJUSTMENT",
            "origin": Value::Null,
            "notify": resolved_bool_field(&input, "notify").unwrap_or(false),
            "account": account.clone()
        });
        let transaction_order_id = transaction["id"].as_str().unwrap_or_default().to_string();
        if !self
            .store
            .staged
            .store_credit_transaction_order
            .iter()
            .any(|id| id == &transaction_order_id)
        {
            self.store
                .staged
                .store_credit_transaction_order
                .push(transaction_order_id.clone());
        }
        self.store
            .staged
            .store_credit_transactions
            .insert(transaction_order_id, transaction.clone());
        self.store
            .staged
            .store_credit_accounts
            .insert(account_id.clone(), account);

        let payload = self.store_credit_payload_for_selection(
            &field.selection,
            &field.name,
            Some(&transaction),
            Vec::new(),
        );
        MutationFieldOutcome::staged(
            payload,
            LogDraft::staged(&field.name, "customers", vec![account_id]),
        )
    }

    fn store_credit_error_outcome(
        &self,
        field: &RootFieldSelection,
        error: Value,
    ) -> MutationFieldOutcome {
        MutationFieldOutcome::unlogged(self.store_credit_payload_for_selection(
            &field.selection,
            &field.name,
            None,
            vec![error],
        ))
    }

    fn resolve_store_credit_account_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
        currency: &str,
        allow_create: bool,
    ) -> Option<StoreCreditAccountMutationResolution> {
        match shopify_gid_resource_type(id) {
            Some("StoreCreditAccount") => {
                if self.store.staged.store_credit_accounts.contains_key(id) {
                    Some(StoreCreditAccountMutationResolution::Existing(
                        id.to_string(),
                    ))
                } else {
                    self.hydrate_store_credit_account_for_mutation(request, id)
                        .map(StoreCreditAccountMutationResolution::Existing)
                }
            }
            Some("Customer") | Some("CompanyLocation") => {
                if !self.store_credit_owner_exists(request, id) {
                    return None;
                }
                if let Some(account_id) =
                    self.store_credit_account_id_for_owner_currency(id, currency)
                {
                    return Some(StoreCreditAccountMutationResolution::Existing(account_id));
                }
                if allow_create {
                    Some(StoreCreditAccountMutationResolution::CreateForOwner(
                        id.to_string(),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn create_store_credit_account_for_owner(&mut self, owner_id: &str, currency: &str) -> String {
        let account_id = self.next_store_credit_account_gid();
        let owner = self.store_credit_owner_json(owner_id);
        let account = json!({
            "id": account_id,
            "balance": money_value("0.0", currency),
            "owner": owner,
            "transactions": connection_json(Vec::new())
        });
        self.store
            .staged
            .store_credit_accounts
            .insert(account_id.clone(), account);
        account_id
    }

    fn store_credit_payload_for_selection(
        &self,
        selection: &[SelectedField],
        root_field: &str,
        transaction: Option<&Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        let payload = json!({
            "__typename": if root_field == "storeCreditAccountCredit" {
                "StoreCreditAccountCreditPayload"
            } else {
                "StoreCreditAccountDebitPayload"
            },
            "storeCreditAccountTransaction": transaction.cloned().unwrap_or(Value::Null),
            "userErrors": user_errors
        });
        selected_payload_json(selection, |field| match field.name.as_str() {
            "storeCreditAccountTransaction" => Some(
                transaction
                    .map(|transaction| {
                        self.selected_store_credit_transaction(transaction, &field.selection)
                    })
                    .unwrap_or(Value::Null),
            ),
            _ => selected_json(&payload, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn selected_store_credit_transaction(
        &self,
        transaction: &Value,
        selection: &[SelectedField],
    ) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "account" => transaction
                .get("account")
                .map(|account| self.selected_store_credit_account(account, &field.selection)),
            _ => selected_json(transaction, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn selected_store_credit_account(&self, account: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "transactions" => {
                let account_id = account
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let transactions = self
                    .store
                    .staged
                    .store_credit_transaction_order
                    .iter()
                    .filter_map(|id| self.store.staged.store_credit_transactions.get(id))
                    .filter(|transaction| transaction["account"]["id"].as_str() == Some(account_id))
                    .cloned()
                    .collect::<Vec<_>>();
                Some(selected_connection_json_with_args(
                    transactions,
                    &field.arguments,
                    &field.selection,
                    value_id_cursor,
                ))
            }
            _ => selected_json(account, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn store_credit_accounts_connection_for_owner(
        &self,
        owner_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let accounts = self
            .store
            .staged
            .store_credit_accounts
            .order
            .iter()
            .filter_map(|id| self.store.staged.store_credit_accounts.get(id))
            .filter(|account| account["owner"]["id"].as_str() == Some(owner_id))
            .cloned()
            .collect::<Vec<_>>();
        selected_staged_connection_with_args(
            accounts,
            arguments,
            selection,
            store_credit_account_search_decision,
            store_credit_account_sort_key,
            selected_json,
            value_id_cursor,
        )
    }

    fn store_credit_account_id_for_owner_currency(
        &self,
        owner_id: &str,
        currency: &str,
    ) -> Option<String> {
        self.store
            .staged
            .store_credit_accounts
            .order
            .iter()
            .filter_map(|id| self.store.staged.store_credit_accounts.get(id))
            .find(|account| {
                account["owner"]["id"].as_str() == Some(owner_id)
                    && account["balance"]["currencyCode"].as_str() == Some(currency)
            })
            .and_then(|account| account["id"].as_str().map(str::to_string))
    }

    fn store_credit_owner_has_accounts(&self, owner_id: &str) -> bool {
        self.store
            .staged
            .store_credit_accounts
            .values()
            .any(|account| account["owner"]["id"].as_str() == Some(owner_id))
    }

    fn store_credit_owner_exists(&mut self, request: &Request, owner_id: &str) -> bool {
        match shopify_gid_resource_type(owner_id) {
            Some("Customer") => {
                if self.store.staged.customers.contains_key(owner_id)
                    && !self.store.staged.customers.is_tombstoned(owner_id)
                {
                    true
                } else {
                    self.hydrate_store_credit_customer_for_mutation(request, owner_id)
                        .is_some()
                }
            }
            Some("CompanyLocation") => self
                .b2b_company_location_for_mutation(Some(request), owner_id)
                .is_some(),
            _ => false,
        }
    }

    fn store_credit_owner_json(&self, owner_id: &str) -> Value {
        match shopify_gid_resource_type(owner_id) {
            Some("Customer") => self
                .store
                .staged
                .customers
                .get(owner_id)
                .cloned()
                .unwrap_or_else(|| json!({ "id": owner_id })),
            Some("CompanyLocation") => self
                .store
                .staged
                .b2b_locations
                .get(owner_id)
                .cloned()
                .unwrap_or_else(|| json!({ "id": owner_id })),
            _ => json!({ "id": owner_id }),
        }
    }

    fn hydrate_store_credit_customer_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot
            || self.store.staged.customers.is_tombstoned(id)
        {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": STORE_CREDIT_CUSTOMER_HYDRATE_QUERY,
                "operationName": "StoreCreditCustomerHydrate",
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let customer = response.body["data"]["customer"].clone();
        if customer.is_null() {
            return None;
        }

        let account_nodes = connection_nodes(&customer["storeCreditAccounts"]);
        let customer = normalize_hydrated_customer_record(customer);
        for account in account_nodes {
            self.stage_store_credit_account_from_upstream(account, Some(&customer));
        }
        let staged_id = customer["id"].as_str().unwrap_or(id).to_string();
        self.store
            .staged
            .customers
            .stage(staged_id, customer.clone());
        Some(customer)
    }

    fn hydrate_store_credit_account_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": STORE_CREDIT_ACCOUNT_HYDRATE_QUERY,
                "operationName": "StoreCreditAccountHydrate",
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let account = response.body["data"]["storeCreditAccount"].clone();
        if account.is_null() {
            None
        } else {
            self.stage_store_credit_account_from_upstream(account, None)
        }
    }

    fn stage_store_credit_account_from_upstream(
        &mut self,
        mut account: Value,
        fallback_owner: Option<&Value>,
    ) -> Option<String> {
        let account_id = account["id"].as_str()?.to_string();
        account.get("balance")?;

        let owner = account
            .get("owner")
            .filter(|owner| !owner.is_null())
            .cloned()
            .or_else(|| fallback_owner.cloned());
        if let Some(owner) = owner {
            if let Some(owner_id) = owner.get("id").and_then(Value::as_str) {
                if shopify_gid_resource_type(owner_id) == Some("Customer")
                    && !self.store.staged.customers.is_tombstoned(owner_id)
                {
                    let customer = normalize_hydrated_customer_record(owner.clone());
                    self.store
                        .staged
                        .customers
                        .stage(owner_id.to_string(), customer);
                }
            }
            account["owner"] = owner;
        }
        if account.get("transactions").is_none_or(Value::is_null) {
            account["transactions"] = connection_json(Vec::new());
        }

        self.store
            .staged
            .store_credit_accounts
            .stage(account_id.clone(), account);
        Some(account_id)
    }

    fn next_store_credit_account_gid(&mut self) -> String {
        let id = self.store.staged.next_store_credit_account_id;
        self.store.staged.next_store_credit_account_id += 1;
        synthetic_shopify_gid("StoreCreditAccount", id)
    }

    fn next_store_credit_transaction_gid(&mut self) -> String {
        let id = self.store.staged.next_store_credit_transaction_id;
        self.store.staged.next_store_credit_transaction_id += 1;
        synthetic_shopify_gid("StoreCreditAccountTransaction", id)
    }

    /// `customers(first:, query:)` list root. Filters the live staged customers
    /// (excluding merged-away / deleted records) by supported local query terms,
    /// while unsupported search terms do not narrow staged customers. Projects
    /// each node through the shared customer renderer so nested
    /// `orders`/`addressesV2`/`metafields` connections resolve from store state
    /// exactly as the singular `customer`/`customerByIdentifier` reads do.
    pub(in crate::proxy) fn customers_list_field(&self, field: &RootFieldSelection) -> Value {
        let records: Vec<Value> = self
            .store
            .staged
            .customers
            .values()
            .filter(|customer| {
                let id = customer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                !self.store.staged.customers.is_tombstoned(id)
            })
            .cloned()
            .collect();
        selected_staged_connection_with_args(
            records,
            &field.arguments,
            &field.selection,
            customer_overlay_search_decision,
            customer_staged_sort_key,
            |customer, selection| {
                let id = customer["id"].as_str().unwrap_or_default().to_string();
                self.customer_with_order_connection(&id, customer, selection)
            },
            value_id_cursor,
        )
    }

    pub(in crate::proxy) fn customer_by_identifier_field(
        &self,
        field: &RootFieldSelection,
    ) -> Value {
        let Some(identifier) = resolved_object_field(&field.arguments, "identifier") else {
            return Value::Null;
        };
        // A merged-away / deleted customer must not resolve through identifier
        // lookups even though its record may briefly linger in the map.
        let is_live = |customer: &&Value| {
            customer
                .get("id")
                .and_then(Value::as_str)
                .map(|id| !self.store.staged.customers.is_tombstoned(id))
                .unwrap_or(true)
        };
        let customer = if let Some(raw_email) = resolved_string_field(&identifier, "email")
            .or_else(|| resolved_string_field(&identifier, "emailAddress"))
        {
            let needle = normalize_customer_email(&raw_email);
            self.store.staged.customers.values().find(|customer| {
                if !is_live(customer) {
                    return false;
                }
                let stored = customer.get("email").and_then(Value::as_str);
                let stored_default = customer["defaultEmailAddress"]["emailAddress"].as_str();
                match needle.as_deref() {
                    Some(needle) => stored == Some(needle) || stored_default == Some(needle),
                    None => {
                        stored == Some(raw_email.as_str())
                            || stored_default == Some(raw_email.as_str())
                    }
                }
            })
        } else if let Some(id) = resolved_string_field(&identifier, "id") {
            self.store
                .staged
                .customers
                .get(&id)
                .filter(|_| !self.store.staged.customers.is_tombstoned(&id))
        } else if let Some(raw_phone) = resolved_string_field(&identifier, "phone")
            .or_else(|| resolved_string_field(&identifier, "phoneNumber"))
        {
            let needle =
                normalize_customer_phone(&raw_phone, shop_country_code(&self.store.base.shop));
            self.store.staged.customers.values().find(|customer| {
                if !is_live(customer) {
                    return false;
                }
                let stored = customer.get("phone").and_then(Value::as_str);
                let stored_default = customer["defaultPhoneNumber"]["phoneNumber"].as_str();
                match needle.as_deref() {
                    Some(needle) => stored == Some(needle) || stored_default == Some(needle),
                    None => {
                        stored == Some(raw_phone.as_str())
                            || stored_default == Some(raw_phone.as_str())
                    }
                }
            })
        } else if let Some(custom_id) = customer_custom_id_from_identifier(&identifier, None) {
            if self.customer_custom_id_has_local_valid_definition(&custom_id) {
                self.customer_ids_matching_custom_id(&custom_id)
                    .first()
                    .and_then(|id| self.store.staged.customers.get(id))
            } else {
                None
            }
        } else {
            None
        };
        customer
            .map(|customer| {
                let id = customer["id"].as_str().unwrap_or_default().to_string();
                self.customer_with_order_connection(&id, customer, &field.selection)
            })
            .unwrap_or(Value::Null)
    }

    pub(in crate::proxy) fn customer_order_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection))
            .unwrap_or_else(|| ("orderCreate".to_string(), Vec::new()));
        let order_input = resolved_object_field(variables, "order").unwrap_or_default();
        let customer_id = resolved_string_field(&order_input, "customerId").unwrap_or_default();
        let customer = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(Value::Null);
        let id = self.next_proxy_synthetic_gid("Order");
        let mut customer = customer;
        if !customer.is_null() {
            customer["canDelete"] = json!(false);
        }
        let order = json!({ "id": id, "customer": customer });
        if !customer_id.is_empty() {
            self.store
                .staged
                .customer_orders
                .entry(customer_id.clone())
                .or_default()
                .push(order.clone());
        }
        self.record_mutation_log_entry(request, query, variables, "orderCreate", vec![id]);
        let payload = json!({ "order": order, "userErrors": [] });
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    pub(in crate::proxy) fn customer_mutation_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "customerCreate" | "customerUpdate" | "customerDelete" | "customerSet"
            )
        }) {
            return json_error(400, "Unsupported mixed customer mutation selection");
        }
        let selects_amount_spent = fields
            .iter()
            .any(|field| selection_contains_any(&field.selection, &["amountSpent"]));
        if selects_amount_spent {
            self.hydrate_shop_pricing_state_if_missing(request, true, false);
        }

        let mut errors = Vec::new();
        let data = root_payload_json(&fields, |field| {
            let (payload, staged_ids, field_errors) =
                self.customer_mutation_payload(request, field);
            // A top-level GraphQL error whose path points at this root field means
            // the field itself resolves to `null` in `data` (GraphQL error
            // propagation), not `{customer:null,userErrors:[]}`. This mirrors
            // Shopify's REDACTED inline-consent rejection, which surfaces a
            // top-level error AND `customerCreate: null`.
            let has_top_error = !field_errors.is_empty();
            errors.extend(field_errors);
            if !staged_ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
            }
            let rendered = if has_top_error {
                Value::Null
            } else {
                self.selected_customer_mutation_payload(&payload, &field.selection)
            };
            Some(rendered)
        });
        let mut body = json!({ "data": data });
        if !errors.is_empty() {
            body["errors"] = Value::Array(errors);
        }
        ok_json(body)
    }

    pub(in crate::proxy) fn customer_outbound_lifecycle_response(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        if !fields.iter().all(|field| {
            matches!(
                field.name.as_str(),
                "customerGenerateAccountActivationUrl"
                    | "customerSendAccountInviteEmail"
                    | "customerPaymentMethodSendUpdateEmail"
            )
        }) {
            return json_error(
                400,
                "Unsupported mixed customer outbound mutation selection",
            );
        }

        let data = root_payload_json(&fields, |field| {
            let (payload, staged_ids) = match field.name.as_str() {
                "customerGenerateAccountActivationUrl" => {
                    self.customer_generate_account_activation_url_payload(request, field)
                }
                "customerSendAccountInviteEmail" => {
                    self.customer_send_account_invite_email_payload(request, field)
                }
                // Kept unimplemented as a primary root. This projection lets the
                // existing mixed outbound-validation parity request continue to
                // compare its captured not-found branch without staging delivery.
                "customerPaymentMethodSendUpdateEmail" => (
                    customer_payment_method_send_update_email_not_found_payload(),
                    Vec::new(),
                ),
                _ => unreachable!("validated customer outbound root"),
            };
            if !staged_ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
            }
            Some(self.selected_customer_outbound_payload(&payload, &field.selection))
        });
        ok_json(json!({ "data": data }))
    }

    fn selected_customer_outbound_payload(
        &self,
        payload: &Value,
        selection: &[SelectedField],
    ) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "customer" => {
                let customer = &payload["customer"];
                if customer.is_null() {
                    return Some(Value::Null);
                }
                let id = customer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some(self.customer_with_order_connection(id, customer, &field.selection))
            }
            _ => selected_json(payload, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn customer_generate_account_activation_url_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let Some(mut customer) = self.customer_existing_for_update(request, &customer_id, false)
        else {
            return (
                customer_account_activation_url_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        ["customerId"],
                        "The customer can't be found.",
                        None,
                    )],
                ),
                Vec::new(),
            );
        };

        let state = customer_account_state(&customer);
        if !customer_account_allows_invite_or_activation(state) {
            return (
                customer_account_activation_url_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        ["customerId"],
                        "account_already_enabled",
                        None,
                    )],
                ),
                Vec::new(),
            );
        }

        let token = customer_account_activation_token(&mut customer, &customer_id);
        let activation_url = customer_account_activation_url(&token);
        self.store
            .staged
            .customers
            .stage(customer_id.clone(), customer);
        (
            customer_account_activation_url_payload(json!(activation_url), Vec::new()),
            vec![customer_id],
        )
    }

    fn customer_send_account_invite_email_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let hydrate_addresses = customer_payload_selection_needs_address_hydrate(&field.selection);
        let Some(mut customer) =
            self.customer_existing_for_update(request, &customer_id, hydrate_addresses)
        else {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error(
                        ["customerId"],
                        "Customer can't be found",
                        Some("INVALID"),
                    )],
                ),
                Vec::new(),
            );
        };

        if let Some(errors) = customer_invite_email_user_errors(&field.arguments) {
            return (customer_payload(Value::Null, vec![errors]), Vec::new());
        }

        let state = customer_account_state(&customer);
        if !customer_account_allows_invite_or_activation(state) {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error(
                        ["customerId"],
                        "Customer account is already enabled.",
                        Some("ACCOUNT_ALREADY_ENABLED"),
                    )],
                ),
                Vec::new(),
            );
        }

        customer["state"] = json!("INVITED");
        customer[CUSTOMER_ACCOUNT_INVITE_FIELD] = customer_account_invite_state(&field.arguments);
        self.store
            .staged
            .customers
            .stage(customer_id.clone(), customer.clone());
        (customer_payload(customer, Vec::new()), vec![customer_id])
    }

    fn selected_customer_mutation_payload(
        &self,
        payload: &Value,
        selection: &[SelectedField],
    ) -> Value {
        selected_payload_json(selection, |field| match field.name.as_str() {
            "customer" => {
                let customer = &payload["customer"];
                if customer.is_null() {
                    return Some(Value::Null);
                }
                let id = customer
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some(self.customer_with_order_connection(id, customer, &field.selection))
            }
            _ => selected_json(payload, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn customer_mutation_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        match field.name.as_str() {
            "customerCreate" => self.customer_create_payload(request, field),
            "customerUpdate" => self.customer_update_payload(request, field),
            "customerDelete" => self.customer_delete_payload(request, field),
            "customerSet" => self.customer_set_payload(request, field),
            _ => (
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        Value::Null,
                        "Local staging for this customer mutation is not implemented.",
                        None,
                    )],
                ),
                Vec::new(),
                Vec::new(),
            ),
        }
    }

    fn customer_create_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        if input.contains_key("id") {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        json!(["id"]),
                        "Cannot specify ID on creation",
                        None,
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        if let Some(error) = customer_create_nested_id_error(&input) {
            return (
                customer_payload(Value::Null, vec![error]),
                Vec::new(),
                Vec::new(),
            );
        }
        if let Some((response, errors)) =
            self.customer_create_inline_consent_response(field, &input)
        {
            return (response, Vec::new(), errors);
        }
        let (errors, normalized) =
            self.customer_input_validation_errors(request, &input, None, None, false);
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        if !customer_has_identity(&normalized) {
            return (
                customer_payload(Value::Null, vec![customer_identity_user_error(Value::Null)]),
                Vec::new(),
                Vec::new(),
            );
        }

        // Fabricated customers always receive a stable plain `gid://shopify/Customer/N`
        // id. Local-runtime fixtures compare these ids strictly, while live-hybrid
        // scenarios use the lenient `shopify-gid:Customer` matcher.
        let id = self.next_synthetic_gid("Customer");
        let timestamp = self.next_product_timestamp();
        let default_locale = self.localization_primary_locale();
        let verified_email_default = customer_create_verified_email_default(request, &normalized);
        let mut customer = customer_record_from_parts(
            &id,
            None,
            &normalized,
            &timestamp,
            &default_locale,
            verified_email_default,
        );
        let shop_currency_code = self.store.observed_shop_currency_code();
        // `customerCreate` accepts inline `emailMarketingConsent` /
        // `smsMarketingConsent` and immediately reflects them on the staged
        // record's compatibility consent fields and on
        // `defaultEmailAddress` / `defaultPhoneNumber`. Validation (missing
        // contact, REDACTED state) already ran above, so any consent present
        // here is applicable.
        apply_inline_consent_from_input(&mut customer, &input);
        // A freshly created customer has no orders yet. Surface Shopify's
        // order-summary defaults (string-zero `numberOfOrders`, null `lastOrder`,
        // empty `orders` connection) so create payloads and subsequent reads that
        // select the order summary match without inventing order state. The
        // per-customer `orders` connection on reads is recomputed from the staged
        // `customer_orders` index, so this stored empty connection only backs the
        // mutation payload projection.
        apply_customer_order_summary_defaults(&mut customer, shop_currency_code.as_deref());
        // A freshly created customer also has no store-credit accounts. Bake the
        // empty connection so a create payload selecting `storeCreditAccounts`
        // matches; reads recompute it from staged store-credit state via
        // `customer_with_order_connection`.
        if customer
            .get("storeCreditAccounts")
            .is_none_or(Value::is_null)
        {
            customer["storeCreditAccounts"] = connection_json_with_empty_edges(Vec::new());
        }
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        self.store
            .staged
            .locally_created_customer_ids
            .insert(id.clone());
        if input.contains_key("metafields") {
            self.stage_owner_metafields_from_input(&id, &input);
            self.sync_customer_metafields_from_owner_store(&id);
        }
        let customer = self
            .store
            .staged
            .customers
            .get(&id)
            .cloned()
            .unwrap_or(customer);
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
    }
}

impl DraftProxy {
    fn customer_update_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let inline_consent_errors = customer_update_inline_consent_errors(&input);
        if !inline_consent_errors.is_empty() {
            return (
                json!({
                    "customer": null,
                    "userErrors": inline_consent_errors,
                    "customerUpdateUserErrors": inline_consent_errors
                }),
                Vec::new(),
                Vec::new(),
            );
        }
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let hydrate_addresses = customer_update_needs_address_hydrate(&input, &field.selection);
        let Some(existing) = self.customer_existing_for_update(request, &id, hydrate_addresses)
        else {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        json!(["id"]),
                        "Customer does not exist",
                        None,
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        self.customer_update_existing_payload(
            request,
            "customerUpdate",
            &id,
            existing,
            &input,
            false,
        )
    }

    fn customer_delete_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let id = resolved_string_field(&input, "id").unwrap_or_default();
        let customer_exists = !id.is_empty() && self.customer_exists_for_mutation(request, &id);
        self.hydrate_customer_delete_shop_if_selected(request, &field.selection);
        let selected_shop = self.customer_delete_shop_payload(&field.selection);
        let mut payload = if !customer_exists {
            json!({
                "deletedCustomerId": null,
                "shop": selected_shop.clone(),
                "userErrors": [user_error_omit_code(["id"], "Customer can't be found", None)]
            })
        } else if self
            .store
            .staged
            .customer_orders
            .get(&id)
            .map(|orders| !orders.is_empty())
            .unwrap_or(false)
        {
            json!({
                "deletedCustomerId": null,
                "shop": selected_shop.clone(),
                "userErrors": [user_error_omit_code(["id"], "Customer can’t be deleted because they have associated orders", None)]
            })
        } else {
            self.store.staged.customers.remove(&id);
            self.store.staged.customers.tombstone(id.clone());
            json!({
                "deletedCustomerId": id,
                "shop": selected_shop,
                "userErrors": []
            })
        };
        if !field
            .selection
            .iter()
            .any(|selection| selection.name == "shop")
        {
            payload.as_object_mut().map(|object| object.remove("shop"));
        }
        let staged_ids = payload
            .get("deletedCustomerId")
            .and_then(Value::as_str)
            .map(|id| vec![id.to_string()])
            .unwrap_or_default();
        (payload, staged_ids, Vec::new())
    }

    fn customer_delete_shop_payload(&self, payload_selection: &[SelectedField]) -> Value {
        if selected_child_selection(payload_selection, "shop").is_none() {
            return Value::Null;
        }
        if !self.customer_delete_shop_has_real_identity() {
            return Value::Null;
        }
        self.store.effective_shop()
    }

    fn hydrate_customer_delete_shop_if_selected(
        &mut self,
        request: &Request,
        payload_selection: &[SelectedField],
    ) {
        let Some(shop_selection) = selected_child_selection(payload_selection, "shop") else {
            return;
        };
        if !self.customer_delete_shop_needs_hydration(&shop_selection) {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_DELETE_SHOP_HYDRATE_QUERY,
                "operationName": "CustomerDeleteShopHydrate",
                "variables": {},
            }),
        );
        if (200..300).contains(&response.status) {
            self.hydrate_shop_state_from_response_data(&response.body["data"]);
        }
    }

    fn customer_delete_shop_needs_hydration(&self, shop_selection: &[SelectedField]) -> bool {
        if self.config.read_mode == ReadMode::Snapshot || shop_selection.is_empty() {
            return false;
        }
        let has_default_shop = self
            .store
            .base
            .shop
            .get("myshopifyDomain")
            .and_then(Value::as_str)
            == Some("shopify-draft-proxy.local");
        !self.customer_delete_shop_has_real_identity()
            || has_default_shop
            || shop_selection
                .iter()
                .any(|field| self.store.base.shop.get(&field.name).is_none())
    }

    fn customer_delete_shop_has_real_identity(&self) -> bool {
        self.store
            .base
            .shop
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                let id = id.trim();
                !id.is_empty() && !id.contains("shopify-draft-proxy=synthetic")
            })
    }

    fn customer_set_payload(
        &mut self,
        request: &Request,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let identifier = resolved_object_field(&field.arguments, "identifier");
        if input.contains_key("id") && identifier.is_some() {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error(
                        json!(["input"]),
                        "The id field is not allowed if identifier is provided.",
                        Some("ID_NOT_ALLOWED"),
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }

        if let Some(identifier) = identifier.as_ref() {
            let hydrate_addresses = customer_update_needs_address_hydrate(&input, &field.selection);
            if let Some(id) = resolved_string_field(identifier, "id") {
                let Some(existing) =
                    self.customer_existing_for_update(request, &id, hydrate_addresses)
                else {
                    return (customer_set_not_found_payload(), Vec::new(), Vec::new());
                };
                return self.customer_update_existing_payload(
                    request,
                    "customerSet",
                    &id,
                    existing,
                    &input,
                    true,
                );
            }
            if let Some(email) = resolved_string_field(identifier, "email") {
                return self.customer_set_contact_identifier_payload(
                    request,
                    "email",
                    &email,
                    &input,
                    None,
                    hydrate_addresses,
                );
            }
            if let Some(phone) = resolved_string_field(identifier, "phone") {
                let phone_country_code = self.customer_phone_country_code(&input, None);
                let normalized_phone =
                    normalize_customer_phone(&phone, phone_country_code.as_deref())
                        .unwrap_or(phone);
                return self.customer_set_contact_identifier_payload(
                    request,
                    "phone",
                    &normalized_phone,
                    &input,
                    phone_country_code.as_deref(),
                    hydrate_addresses,
                );
            }
            if identifier.contains_key("customId") {
                let api_client_id = request_app_namespace_api_client_id(request);
                let Some(custom_id) =
                    customer_custom_id_from_identifier(identifier, api_client_id.as_deref())
                else {
                    return customer_set_custom_id_not_found_response();
                };
                return self.customer_set_custom_id_payload(
                    request,
                    &custom_id,
                    &input,
                    hydrate_addresses,
                );
            }
        }

        self.customer_set_create_payload(request, &input)
    }

    fn customer_set_custom_id_payload(
        &mut self,
        request: &Request,
        custom_id: &CustomerCustomId,
        input: &BTreeMap<String, ResolvedValue>,
        hydrate_addresses: bool,
    ) -> (Value, Vec<String>, Vec<Value>) {
        if !self.customer_custom_id_has_local_valid_definition(custom_id) {
            let lookup = self.customer_upstream_custom_id_lookup(custom_id, request);
            if let Some(id) = lookup.found_id {
                let Some(existing) =
                    self.customer_existing_for_update(request, &id, hydrate_addresses)
                else {
                    return customer_set_custom_id_not_found_response();
                };
                return self.customer_update_existing_payload_with_custom_id(
                    request, &id, existing, input, custom_id,
                );
            }
            if lookup.valid_definition {
                return self.customer_set_create_payload_with_custom_id(request, input, custom_id);
            }
            return customer_set_custom_id_not_found_response();
        }

        if let Some(error) = customer_custom_id_input_mismatch_error(input, custom_id) {
            return (
                customer_payload(Value::Null, vec![error]),
                Vec::new(),
                Vec::new(),
            );
        }

        let matches = self.customer_ids_matching_custom_id(custom_id);
        if matches.len() > 1 {
            return (
                customer_payload(Value::Null, vec![customer_custom_id_duplicate_user_error()]),
                Vec::new(),
                Vec::new(),
            );
        }
        if let Some(id) = matches.first() {
            let Some(existing) = self.customer_existing_for_update(request, id, hydrate_addresses)
            else {
                return (customer_set_not_found_payload(), Vec::new(), Vec::new());
            };
            return self.customer_update_existing_payload_with_custom_id(
                request, id, existing, input, custom_id,
            );
        }

        if let Some(id) = self
            .customer_upstream_custom_id_lookup(custom_id, request)
            .found_id
        {
            let Some(existing) = self.customer_existing_for_update(request, &id, hydrate_addresses)
            else {
                return (customer_set_not_found_payload(), Vec::new(), Vec::new());
            };
            return self.customer_update_existing_payload_with_custom_id(
                request, &id, existing, input, custom_id,
            );
        }

        self.customer_set_create_payload_with_custom_id(request, input, custom_id)
    }

    fn customer_set_contact_identifier_payload(
        &mut self,
        request: &Request,
        identifier_field: &str,
        identifier_value: &str,
        input: &BTreeMap<String, ResolvedValue>,
        phone_country_code: Option<&str>,
        hydrate_addresses: bool,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input_value = resolved_string_field(input, identifier_field);
        let Some(input_value) = input_value else {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        json!(["input"]),
                        "The input field corresponding to the identifier is required.",
                        None,
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        let normalized_input_value = if identifier_field == "phone" {
            normalize_customer_phone(&input_value, phone_country_code).unwrap_or(input_value)
        } else {
            normalize_customer_email(&input_value).unwrap_or(input_value)
        };
        if normalized_input_value != identifier_value {
            return (
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(json!(["input"]), "The identifier value does not match the value of the corresponding field in the input.", None)],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let staged_match = if identifier_field == "phone" {
            find_customer_id_by_phone(&self.store.staged.customers.records, identifier_value)
        } else {
            find_customer_id_by_email(&self.store.staged.customers.records, identifier_value)
        };
        if let Some(id) = staged_match {
            let Some(existing) = self.customer_existing_for_update(request, &id, hydrate_addresses)
            else {
                return (customer_set_not_found_payload(), Vec::new(), Vec::new());
            };
            self.customer_update_existing_payload(
                request,
                "customerSet",
                &id,
                existing,
                input,
                true,
            )
        } else if let Some(id) = self.customer_upstream_contact_identifier_id(
            identifier_field,
            identifier_value,
            request,
        ) {
            let Some(existing) = self.customer_existing_for_update(request, &id, hydrate_addresses)
            else {
                return (customer_set_not_found_payload(), Vec::new(), Vec::new());
            };
            self.customer_update_existing_payload(
                request,
                "customerSet",
                &id,
                existing,
                input,
                true,
            )
        } else {
            self.customer_set_create_payload(request, input)
        }
    }

    fn customer_set_create_payload(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let (errors, normalized) =
            self.customer_input_validation_errors(request, input, None, None, true);
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        if !customer_has_identity(&normalized) {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_identity_user_error(json!(["input"]))],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let id = self.next_synthetic_gid("Customer");
        let timestamp = self.next_product_timestamp();
        let default_locale = self.localization_primary_locale();
        let mut customer =
            customer_record_from_parts(&id, None, &normalized, &timestamp, &default_locale, true);
        let shop_currency_code = self.store.observed_shop_currency_code();
        apply_customer_order_summary_defaults(&mut customer, shop_currency_code.as_deref());
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        self.store
            .staged
            .locally_created_customer_ids
            .insert(id.clone());
        if input.contains_key("metafields") {
            self.stage_owner_metafields_from_input(&id, input);
            self.sync_customer_metafields_from_owner_store(&id);
        }
        let customer = self
            .store
            .staged
            .customers
            .get(&id)
            .cloned()
            .unwrap_or(customer);
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
    }

    fn customer_set_create_payload_with_custom_id(
        &mut self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        custom_id: &CustomerCustomId,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let (mut payload, staged_ids, errors) = self.customer_set_create_payload(request, input);
        self.apply_customer_custom_id_to_success_payload(&mut payload, custom_id);
        (payload, staged_ids, errors)
    }

    fn customer_update_existing_payload_with_custom_id(
        &mut self,
        request: &Request,
        id: &str,
        existing: Value,
        input: &BTreeMap<String, ResolvedValue>,
        custom_id: &CustomerCustomId,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let (mut payload, staged_ids, errors) = self.customer_update_existing_payload(
            request,
            "customerSet",
            id,
            existing,
            input,
            true,
        );
        self.apply_customer_custom_id_to_success_payload(&mut payload, custom_id);
        (payload, staged_ids, errors)
    }

    fn apply_customer_custom_id_to_success_payload(
        &mut self,
        payload: &mut Value,
        custom_id: &CustomerCustomId,
    ) {
        let Some(customer_id) = payload["customer"]["id"].as_str().map(str::to_string) else {
            return;
        };
        self.stage_owner_metafield_value(
            &customer_id,
            &custom_id.namespace,
            &custom_id.key,
            "id",
            &custom_id.value,
        );
        self.sync_customer_metafields_from_owner_store(&customer_id);
        if let Some(customer) = self.store.staged.customers.get(&customer_id) {
            payload["customer"] = customer.clone();
        }
    }

    fn customer_custom_id_has_local_valid_definition(&self, custom_id: &CustomerCustomId) -> bool {
        self.owner_metafield_definition("CUSTOMER", &custom_id.namespace, &custom_id.key)
            .as_ref()
            .is_some_and(customer_custom_id_definition_is_valid)
    }

    fn customer_ids_matching_custom_id(&self, custom_id: &CustomerCustomId) -> Vec<String> {
        self.store
            .staged
            .customers
            .iter()
            .filter_map(|(id, customer)| {
                if self.store.staged.customers.is_tombstoned(id) {
                    return None;
                }
                self.customer_matches_custom_id(id, customer, custom_id)
                    .then(|| id.clone())
            })
            .collect()
    }

    fn customer_matches_custom_id(
        &self,
        id: &str,
        customer: &Value,
        custom_id: &CustomerCustomId,
    ) -> bool {
        let key_filter = [(custom_id.namespace.clone(), custom_id.key.clone())];
        self.owner_metafields(id, Some(&custom_id.namespace), Some(&key_filter))
            .iter()
            .any(|metafield| customer_metafield_matches_custom_id(metafield, custom_id))
            || connection_nodes(&customer["metafields"])
                .iter()
                .any(|metafield| customer_metafield_matches_custom_id(metafield, custom_id))
    }

    fn customer_upstream_custom_id_lookup(
        &self,
        custom_id: &CustomerCustomId,
        request: &Request,
    ) -> CustomerCustomIdUpstreamLookup {
        if self.config.read_mode == ReadMode::Snapshot {
            return CustomerCustomIdUpstreamLookup::default();
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_CUSTOM_ID_LOOKUP_QUERY,
                "operationName": "CustomerCustomIdLookup",
                "variables": {
                    "identifier": {
                        "customId": {
                            "namespace": &custom_id.namespace,
                            "key": &custom_id.key,
                            "value": &custom_id.value
                        }
                    }
                },
            }),
        );
        if !(200..300).contains(&response.status) {
            return CustomerCustomIdUpstreamLookup::default();
        }
        let valid_definition = !response.body["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|error| {
                error["extensions"]["code"].as_str() == Some("NOT_FOUND")
                    && error["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("Metafield definition"))
            });
        if !valid_definition {
            return CustomerCustomIdUpstreamLookup::default();
        }
        let found_id = response.body["data"]["customerByIdentifier"]["id"]
            .as_str()
            .map(str::to_string);
        CustomerCustomIdUpstreamLookup {
            valid_definition: true,
            found_id,
        }
    }

    fn customer_update_existing_payload(
        &mut self,
        request: &Request,
        _root_field: &str,
        id: &str,
        existing: Value,
        input: &BTreeMap<String, ResolvedValue>,
        customer_set: bool,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let update_address_values = (!customer_set)
            .then(|| resolved_list_field(input, "addresses"))
            .flatten();
        let input_without_update_addresses = if !customer_set && input.contains_key("addresses") {
            let mut input = input.clone();
            input.remove("addresses");
            Some(input)
        } else {
            None
        };
        let validation_input = input_without_update_addresses.as_ref().unwrap_or(input);
        let (mut errors, mut normalized) = self.customer_input_validation_errors(
            request,
            validation_input,
            Some(id),
            Some(&existing),
            customer_set,
        );
        if let Some(address_values) = update_address_values {
            let (addresses, mut address_errors) =
                self.customer_update_mailing_addresses(&address_values, &existing);
            errors.append(&mut address_errors);
            normalized.addresses = Some(addresses);
        }
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let timestamp = self.next_product_timestamp();
        let default_locale = self.localization_primary_locale();
        let customer = customer_record_from_parts(
            id,
            Some(&existing),
            &normalized,
            &timestamp,
            &default_locale,
            customer_set,
        );
        if !customer_has_identity_json(&customer) {
            let field = if customer_set {
                json!(["input"])
            } else {
                Value::Null
            };
            return (
                customer_payload(Value::Null, vec![customer_identity_user_error(field)]),
                Vec::new(),
                Vec::new(),
            );
        }
        self.store
            .staged
            .customers
            .insert(id.to_string(), customer.clone());
        if input.contains_key("metafields") {
            self.stage_owner_metafields_from_input(id, input);
            self.sync_customer_metafields_from_owner_store(id);
        }
        let customer = self
            .store
            .staged
            .customers
            .get(id)
            .cloned()
            .unwrap_or(customer);
        (
            customer_payload(customer, Vec::new()),
            vec![id.to_string()],
            Vec::new(),
        )
    }

    fn customer_update_mailing_addresses(
        &mut self,
        values: &[ResolvedValue],
        existing_customer: &Value,
    ) -> (Vec<Value>, Vec<Value>) {
        let existing_nodes = customer_address_nodes(existing_customer);
        let existing_by_id = existing_nodes
            .iter()
            .filter_map(|node| {
                node.get("id")
                    .and_then(Value::as_str)
                    .map(|id| (id.to_string(), node.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let mut used_ids = existing_by_id.keys().cloned().collect::<BTreeSet<_>>();
        let mut errors = Vec::new();

        for (index, value) in values.iter().enumerate() {
            let Some(input) = resolved_value_object(value) else {
                continue;
            };
            let requested_id = input
                .contains_key("id")
                .then(|| resolved_string_field(&input, "id").unwrap_or_default());
            if let Some(requested_id) = requested_id.as_deref() {
                if !existing_by_id.contains_key(requested_id) {
                    errors.push(user_error_omit_code(
                        customer_address_field_path(false, index, Some("id")),
                        "Customer address does not exist",
                        None,
                    ));
                    continue;
                }
            }
            let existing = requested_id
                .as_deref()
                .and_then(|id| existing_by_id.get(id));
            let validation_id = requested_id
                .clone()
                .unwrap_or_else(|| synthetic_shopify_gid("MailingAddress", index + 1));
            let (_, mut address_errors) =
                customer_update_mailing_address(&input, index, existing, &validation_id);
            errors.append(&mut address_errors);
        }

        if !errors.is_empty() {
            return (Vec::new(), errors);
        }

        let mut addresses = Vec::new();
        let mut seen = BTreeSet::new();
        for (index, value) in values.iter().enumerate() {
            let Some(input) = resolved_value_object(value) else {
                continue;
            };
            let requested_id = input
                .contains_key("id")
                .then(|| resolved_string_field(&input, "id").unwrap_or_default());
            let existing = requested_id
                .as_deref()
                .and_then(|id| existing_by_id.get(id));
            let id = requested_id
                .unwrap_or_else(|| self.next_customer_update_mailing_address_id(&used_ids));
            let (address, _) = customer_update_mailing_address(&input, index, existing, &id);
            if seen.insert(customer_address_dedup_key(&address)) {
                used_ids.insert(id);
                addresses.push(address);
            }
        }
        (addresses, Vec::new())
    }

    fn next_customer_update_mailing_address_id(&mut self, used_ids: &BTreeSet<String>) -> String {
        loop {
            let id = self.next_proxy_synthetic_gid("MailingAddress");
            if !used_ids.contains(&id) {
                return id;
            }
        }
    }

    fn sync_customer_metafields_from_owner_store(&mut self, customer_id: &str) {
        let metafields = self
            .store
            .staged
            .owner_metafields
            .get(customer_id)
            .cloned()
            .unwrap_or_default();
        if let Some(customer) = self.store.staged.customers.get_mut(customer_id) {
            customer["metafields"] = nodes_connection(metafields.clone());
            let first = metafields.first().cloned().unwrap_or(Value::Null);
            customer["metafield"] = first;
        }
    }

    fn customer_existing_for_update(
        &mut self,
        request: &Request,
        id: &str,
        hydrate_addresses: bool,
    ) -> Option<Value> {
        if id.is_empty() || self.store.staged.customers.is_tombstoned(id) {
            return None;
        }
        self.store
            .staged
            .customers
            .get(id)
            .cloned()
            .or_else(|| self.hydrate_customer_for_mutation(request, id, hydrate_addresses))
    }

    pub(super) fn customer_exists_for_mutation(&mut self, request: &Request, id: &str) -> bool {
        self.customer_existing_for_update(request, id, false)
            .is_some()
    }

    /// Ensure cold customers referenced by `customerMerge` are present in staged
    /// state with the scalar fields needed for existence, blocker, and survivor
    /// validation. Attached resources are intentionally not fetched here.
    pub(super) fn ensure_customers_hydrated_for_merge(
        &mut self,
        request: &Request,
        ids: &[String],
    ) -> Vec<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return Vec::new();
        }
        let ids_to_hydrate = ids
            .iter()
            .filter(|id| {
                !id.is_empty()
                    && !self.store.staged.customers.contains_staged(id)
                    && !self.store.staged.customers.is_tombstoned(id)
            })
            .cloned()
            .collect::<Vec<_>>();
        if ids_to_hydrate.is_empty() {
            return Vec::new();
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_MERGE_HYDRATE_QUERY,
                "operationName": "CustomerMergeHydrate",
                "variables": { "ids": ids_to_hydrate },
            }),
        );
        if !(200..300).contains(&response.status) {
            return Vec::new();
        }
        let requested = ids_to_hydrate.into_iter().collect::<BTreeSet<_>>();
        let mut hydrated = Vec::new();
        let Some(nodes) = response.body["data"]["nodes"].as_array() else {
            return hydrated;
        };
        for customer in nodes {
            if customer.is_null() {
                continue;
            }
            let Some(id) = customer.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !requested.contains(id) {
                continue;
            }
            self.store.staged.customers.stage(
                id.to_string(),
                normalize_hydrated_customer_record(customer.clone()),
            );
            hydrated.push(id.to_string());
        }
        hydrated
    }

    /// Fetch the attached resources needed to apply the successful
    /// `customerMerge` branch. This stays separate from scalar validation so
    /// validation-only branches do not pay for address/metafield/order windows.
    pub(super) fn hydrate_customer_merge_attached_resources(
        &mut self,
        request: &Request,
        ids: &[String],
    ) {
        if self.config.read_mode == ReadMode::Snapshot || ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_MERGE_ATTACHED_HYDRATE_QUERY,
                "operationName": "CustomerMergeAttachedHydrate",
                "variables": { "ids": ids },
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(nodes) = response.body["data"]["nodes"].as_array() else {
            return;
        };
        for customer in nodes {
            if customer.is_null() {
                continue;
            }
            let Some(id) = customer.get("id").and_then(Value::as_str) else {
                continue;
            };
            if ids.iter().any(|requested| requested == id) {
                self.stage_customer_merge_attached_resources(id, customer);
            }
        }
    }

    fn stage_customer_merge_attached_resources(&mut self, id: &str, customer: &Value) {
        let orders = customer_merge_extract_order_records(id, &customer["orders"]);
        if !orders.is_empty() {
            self.store
                .staged
                .customer_orders
                .insert(id.to_string(), orders);
        }
        let metafields = customer
            .get("metafields")
            .map(|connection| nodes_connection(connection_nodes(connection)));
        let default_id = customer
            .get("defaultAddress")
            .and_then(|address| address.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(record) = self.store.staged.customers.get_mut(id) {
            if customer.get("addressesV2").is_some() {
                customer_rebuild_addresses(
                    record,
                    connection_nodes(&customer["addressesV2"]),
                    default_id.as_deref(),
                );
            } else if customer.get("defaultAddress").is_some() {
                record["defaultAddress"] = customer["defaultAddress"].clone();
            }
            if let Some(metafields) = metafields {
                record["metafields"] = metafields;
            }
            for key in ["lastOrder", "numberOfOrders"] {
                if let Some(value) = customer.get(key) {
                    record[key] = value.clone();
                }
            }
        }
    }

    fn customer_phone_country_code(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        existing: Option<&Value>,
    ) -> Option<String> {
        customer_input_address_country_code(input)
            .or_else(|| existing.and_then(customer_record_country_code))
            .or_else(|| shop_country_code(&self.store.base.shop).map(str::to_string))
    }

    fn customer_input_validation_errors(
        &self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        current_id: Option<&str>,
        existing: Option<&Value>,
        customer_set: bool,
    ) -> (Vec<Value>, NormalizedCustomerInput) {
        let mut errors = Vec::new();
        let mut normalized = NormalizedCustomerInput::default();

        if let Some(raw_email) = resolved_string_field(input, "email") {
            let email = normalize_customer_email(&raw_email);
            if raw_email.trim().is_empty() {
                normalized.email = Some(None);
            } else if let Some(email) = email {
                if self.customer_email_taken(request, current_id, &email) {
                    errors.push(user_error_omit_code(
                        customer_field_path(customer_set, "email"),
                        "Email has already been taken",
                        None,
                    ));
                }
                normalized.email = Some(Some(email));
            } else {
                if raw_email
                    .split_whitespace()
                    .collect::<String>()
                    .chars()
                    .count()
                    > 255
                {
                    errors.push(user_error_omit_code(
                        customer_field_path(customer_set, "email"),
                        "Email is too long (maximum is 255 characters)",
                        None,
                    ));
                }
                errors.push(user_error_omit_code(
                    customer_field_path(customer_set, "email"),
                    "Email is invalid",
                    None,
                ));
            }
        } else if resolved_field_is_null(input, "email") {
            normalized.email = Some(None);
        }

        let phone_country_code = self.customer_phone_country_code(input, existing);
        if let Some(raw_phone) = resolved_string_field(input, "phone") {
            if raw_phone.trim().is_empty() {
                normalized.phone = Some(None);
            } else if let Some(phone) =
                normalize_customer_phone(&raw_phone, phone_country_code.as_deref())
            {
                if self.customer_phone_taken(request, current_id, &phone) {
                    errors.push(user_error_omit_code(
                        customer_field_path(customer_set, "phone"),
                        "Phone has already been taken",
                        None,
                    ));
                }
                normalized.phone = Some(Some(phone));
            } else {
                if raw_phone.trim().chars().count() > 255 {
                    errors.push(user_error_omit_code(
                        customer_field_path(customer_set, "phone"),
                        "Phone is too long (maximum is 255 characters)",
                        None,
                    ));
                }
                errors.push(user_error_omit_code(
                    customer_field_path(customer_set, "phone"),
                    "Phone is invalid",
                    None,
                ));
            }
        } else if resolved_field_is_null(input, "phone") {
            normalized.phone = Some(None);
        }

        if let Some(raw_locale) = resolved_string_field(input, "locale") {
            if raw_locale.trim().is_empty() {
                normalized.locale = Some(None);
            } else if let Some(locale) = normalize_shopify_locale(raw_locale.trim()) {
                normalized.locale = Some(Some(locale));
            } else {
                errors.push(user_error_omit_code(
                    customer_field_path(customer_set, "locale"),
                    "Locale is invalid",
                    None,
                ));
            }
        } else if resolved_field_is_null(input, "locale") {
            normalized.locale = Some(None);
        }

        for field in ["firstName", "lastName"] {
            if let Some(value) = resolved_string_field(input, field) {
                if value.chars().count() > 255 {
                    let message = if field == "firstName" {
                        "First name is too long (maximum is 255 characters)"
                    } else {
                        "Last name is too long (maximum is 255 characters)"
                    };
                    errors.push(user_error_omit_code(
                        customer_field_path(customer_set, field),
                        message,
                        None,
                    ));
                }
                let normalized_value = blank_string_to_option(value.trim().to_string());
                if field == "firstName" {
                    normalized.first_name = Some(normalized_value);
                } else {
                    normalized.last_name = Some(normalized_value);
                }
            } else if resolved_field_is_null(input, field) {
                if field == "firstName" {
                    normalized.first_name = Some(None);
                } else {
                    normalized.last_name = Some(None);
                }
            }
        }

        if let Some(note) = resolved_string_field(input, "note") {
            if note.chars().count() > 5000 {
                errors.push(user_error_omit_code(
                    customer_field_path(customer_set, "note"),
                    "Note is too long (maximum is 5000 characters)",
                    None,
                ));
            }
            normalized.note = Some(Some(note));
        } else if resolved_field_is_null(input, "note") {
            normalized.note = Some(None);
        }

        if input.contains_key("tags") {
            let tags = raw_taggable_tags_argument(input.get("tags"));
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                errors.push(user_error_omit_code(
                    customer_field_path(customer_set, "tags"),
                    "Tags is too long (maximum is 255 characters)",
                    None,
                ));
            }
            let normalized_tags = normalize_taggable_tags(tags);
            if normalized_tags.len() > 250 {
                errors.push(user_error_omit_code(
                    customer_field_path(customer_set, "tags"),
                    "Tags cannot be more than 250",
                    None,
                ));
            }
            normalized.tags = Some(normalized_tags);
        }

        if input.contains_key("taxExempt") {
            if let Some(value) = resolved_bool_field(input, "taxExempt") {
                normalized.tax_exempt = Some(value);
            } else if customer_set && resolved_field_is_null(input, "taxExempt") {
                errors.push(user_error_omit_code(
                    json!(["input", "taxExempt"]),
                    "Tax exempt is of unexpected type NilClass",
                    None,
                ));
            }
        }
        if input.contains_key("taxExemptions") {
            normalized.tax_exemptions = Some(list_string_field(input, "taxExemptions"));
        }
        if let Some(address_values) = resolved_list_field(input, "addresses") {
            let (addresses, address_errors) =
                customer_mailing_addresses(&address_values, customer_set);
            errors.extend(address_errors);
            normalized.addresses = Some(addresses);
        }
        (errors, normalized)
    }

    fn customer_email_taken(
        &self,
        request: &Request,
        current_id: Option<&str>,
        email: &str,
    ) -> bool {
        self.store.staged.customers.iter().any(|(id, customer)| {
            current_id != Some(id.as_str())
                && customer
                    .get("email")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| {
                        customer_email_key(existing) == customer_email_key(email)
                    })
        }) || self.customer_upstream_contact_taken(request, current_id, "email", email)
    }

    fn customer_phone_taken(
        &self,
        request: &Request,
        current_id: Option<&str>,
        phone: &str,
    ) -> bool {
        self.store.staged.customers.iter().any(|(id, customer)| {
            current_id != Some(id.as_str())
                && customer
                    .get("phone")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing == phone)
        }) || self.customer_upstream_contact_taken(request, current_id, "phone", phone)
    }

    pub(in crate::proxy) fn hydrate_customer_for_mutation(
        &mut self,
        request: &Request,
        id: &str,
        hydrate_addresses: bool,
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let query = if hydrate_addresses {
            CUSTOMER_ADDRESS_HYDRATE_QUERY
        } else {
            CUSTOMER_HYDRATE_QUERY
        };
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "operationName": "CustomerHydrate",
                "variables": { "id": id },
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        let customer = response.body["data"]["customer"].clone();
        if customer.is_null() {
            None
        } else {
            Some(normalize_hydrated_customer_record(customer))
        }
    }

    fn customer_upstream_contact_taken(
        &self,
        request: &Request,
        current_id: Option<&str>,
        field: &str,
        value: &str,
    ) -> bool {
        self.customer_upstream_contact_identifier_id(field, value, request)
            .is_some_and(|id| current_id != Some(id.as_str()))
    }

    fn customer_upstream_contact_identifier_id(
        &self,
        field: &str,
        value: &str,
        request: &Request,
    ) -> Option<String> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let query_value = format!("{field}:{value}");
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_DUPLICATE_HYDRATE_QUERY,
                "operationName": "CustomerDuplicateHydrate",
                "variables": { "query": query_value },
            }),
        );
        if !(200..300).contains(&response.status) {
            return None;
        }
        response.body["data"]["customers"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .and_then(|node| node["id"].as_str())
            .map(str::to_string)
    }

    fn customer_create_inline_consent_response(
        &self,
        field: &RootFieldSelection,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Option<(Value, Vec<Value>)> {
        for field_name in [
            "emailMarketingConsent",
            "smsMarketingConsent",
            "whatsAppMarketingConsent",
        ] {
            let Some(consent) = resolved_object_field(input, field_name) else {
                continue;
            };
            if resolved_inline_consent_state(&consent, field_name).as_deref() == Some("REDACTED") {
                return Some((
                    customer_payload(Value::Null, Vec::new()),
                    vec![json!({
                        "message": "Cannot specify REDACTED as a marketing state input",
                        "path": [field.response_key.clone()],
                        "extensions": { "code": "INVALID" }
                    })],
                ));
            }
        }
        if input.contains_key("emailMarketingConsent")
            && resolved_string_field(input, "email").is_none_or(|email| email.trim().is_empty())
        {
            return Some((
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        json!(["emailMarketingConsent"]),
                        "An email address is required to set the email marketing consent state.",
                        None,
                    )],
                ),
                Vec::new(),
            ));
        }
        if input.contains_key("smsMarketingConsent")
            && resolved_string_field(input, "phone").is_none_or(|phone| phone.trim().is_empty())
        {
            return Some((
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        json!(["smsMarketingConsent"]),
                        "A phone number is required to set the SMS consent state.",
                        None,
                    )],
                ),
                Vec::new(),
            ));
        }
        if input.contains_key("whatsAppMarketingConsent")
            && resolved_string_field(input, "phone").is_none_or(|phone| phone.trim().is_empty())
        {
            return Some((
                customer_payload(
                    Value::Null,
                    vec![user_error_omit_code(
                        json!(["whatsAppMarketingConsent"]),
                        "A phone number is required to set the WhatsApp consent state.",
                        None,
                    )],
                ),
                Vec::new(),
            ));
        }
        None
    }
}

fn customer_update_needs_address_hydrate(
    input: &BTreeMap<String, ResolvedValue>,
    payload_selection: &[SelectedField],
) -> bool {
    input.contains_key("addresses")
        || customer_payload_selection_needs_address_hydrate(payload_selection)
}

fn customer_payload_selection_needs_address_hydrate(payload_selection: &[SelectedField]) -> bool {
    selected_child_selection(payload_selection, "customer").is_some_and(|customer_selection| {
        selection_contains_any(&customer_selection, &["addressesV2"])
    })
}

#[derive(Default)]
struct NormalizedCustomerInput {
    first_name: Option<Option<String>>,
    last_name: Option<Option<String>>,
    email: Option<Option<String>>,
    phone: Option<Option<String>>,
    locale: Option<Option<String>>,
    note: Option<Option<String>>,
    tags: Option<Vec<String>>,
    tax_exempt: Option<bool>,
    tax_exemptions: Option<Vec<String>>,
    addresses: Option<Vec<Value>>,
}

fn customer_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({ "customer": customer, "userErrors": user_errors })
}

fn customer_account_activation_url_payload(
    account_activation_url: Value,
    user_errors: Vec<Value>,
) -> Value {
    json!({ "accountActivationUrl": account_activation_url, "userErrors": user_errors })
}

fn customer_payment_method_send_update_email_not_found_payload() -> Value {
    customer_payload(
        Value::Null,
        vec![user_error_omit_code(
            ["customerPaymentMethodId"],
            "Customer payment method does not exist",
            None,
        )],
    )
}

fn customer_account_state(customer: &Value) -> &str {
    customer
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("DISABLED")
}

fn customer_account_allows_invite_or_activation(state: &str) -> bool {
    matches!(state, "DISABLED" | "INVITED")
}

fn customer_account_activation_token(customer: &mut Value, customer_id: &str) -> String {
    if let Some(token) = customer
        .get(CUSTOMER_ACCOUNT_ACTIVATION_TOKEN_FIELD)
        .and_then(Value::as_str)
    {
        return token.to_string();
    }
    let id_tail = resource_id_tail(customer_id);
    let stable_tail = id_tail
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    let token = if stable_tail.is_empty() {
        "sdp-activation-token".to_string()
    } else {
        format!("sdp-activation-{stable_tail}")
    };
    customer[CUSTOMER_ACCOUNT_ACTIVATION_TOKEN_FIELD] = json!(token);
    token
}

fn customer_account_activation_url(token: &str) -> String {
    format!("https://shopify-draft-proxy.local/customer-account/activate/{token}")
}

fn customer_account_invite_state(arguments: &BTreeMap<String, ResolvedValue>) -> Value {
    json!({
        "status": "staged",
        "email": arguments
            .get("email")
            .map(resolved_value_json)
            .unwrap_or(Value::Null),
    })
}

fn customer_invite_email_user_errors(arguments: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let email = resolved_object_field(arguments, "email")?;
    if resolved_string_field(&email, "subject").is_some_and(|subject| subject.trim().is_empty()) {
        return Some(user_error(
            ["email", "subject"],
            "Subject can't be blank",
            Some("INVALID"),
        ));
    }
    if resolved_string_field(&email, "to")
        .as_deref()
        .is_some_and(|to| normalize_customer_email(to).is_none())
    {
        return Some(user_error(
            ["email", "to"],
            "To is invalid",
            Some("INVALID"),
        ));
    }
    if resolved_string_field(&email, "from")
        .as_deref()
        .is_some_and(|from| normalize_customer_email(from).is_none())
    {
        return Some(user_error(
            ["email", "from"],
            "From Sender is invalid",
            Some("INVALID"),
        ));
    }
    let bcc = resolved_string_list_field_unsorted(&email, "bcc");
    if bcc
        .iter()
        .any(|address| normalize_customer_email(address).is_none())
    {
        let message = bcc
            .iter()
            .map(|address| format!("{address} is not a valid bcc address"))
            .collect::<Vec<_>>()
            .join(" and ");
        return Some(user_error(["email", "bcc"], &message, Some("INVALID")));
    }
    if resolved_string_field(&email, "subject")
        .is_some_and(|subject| subject.chars().count() > 1000)
        || resolved_string_field(&email, "customMessage").is_some_and(|message| {
            message.chars().count() > 5000 || message.contains('<') || message.contains('>')
        })
    {
        return Some(user_error(
            ["customerId"],
            "Error sending account invite to customer.",
            Some("INVALID"),
        ));
    }
    None
}

fn customer_identity_user_error(field: Value) -> Value {
    user_error_omit_code(
        field,
        "A name, phone number, or email address must be present",
        None,
    )
}

fn customer_set_not_found_payload() -> Value {
    customer_payload(
        Value::Null,
        vec![user_error(
            json!(["input"]),
            "Resource matching the identifier was not found.",
            Some("NOT_FOUND"),
        )],
    )
}

fn customer_set_custom_id_not_found_response() -> (Value, Vec<String>, Vec<Value>) {
    (
        Value::Null,
        Vec::new(),
        vec![customer_custom_id_not_found_error("customerSet")],
    )
}

fn customer_custom_id_not_found_error(path: &str) -> Value {
    json!({
        "message": "Metafield definition of type 'id' is required when using custom ids.",
        "path": [path],
        "extensions": { "code": "NOT_FOUND" }
    })
}

fn customer_custom_id_from_identifier(
    identifier: &BTreeMap<String, ResolvedValue>,
    api_client_id: Option<&str>,
) -> Option<CustomerCustomId> {
    let custom_id = resolved_object_field(identifier, "customId")?;
    let namespace = canonical_app_metafield_namespace(
        resolved_string_field(&custom_id, "namespace").as_deref(),
        api_client_id,
    );
    let key = resolved_string_field(&custom_id, "key")?;
    let value = resolved_string_field(&custom_id, "value")?;
    (!key.is_empty() && !value.is_empty()).then_some(CustomerCustomId {
        namespace,
        key,
        value,
    })
}

fn customer_custom_id_definition_is_valid(definition: &Value) -> bool {
    definition["ownerType"].as_str() == Some("CUSTOMER")
        && definition["type"]["name"].as_str() == Some("id")
        && definition["capabilities"]["uniqueValues"]["enabled"].as_bool() == Some(true)
}

fn customer_custom_id_input_mismatch_error(
    input: &BTreeMap<String, ResolvedValue>,
    custom_id: &CustomerCustomId,
) -> Option<Value> {
    resolved_object_list_field(input, "metafields")
        .into_iter()
        .filter(|metafield| {
            let namespace = resolved_string_field(metafield, "namespace").unwrap_or_default();
            let key = resolved_string_field(metafield, "key").unwrap_or_default();
            namespace == custom_id.namespace && key == custom_id.key
        })
        .find_map(|metafield| {
            let value = resolved_string_field(&metafield, "value").unwrap_or_default();
            (value != custom_id.value).then(|| {
                user_error_omit_code(
                    json!(["input"]),
                    "The identifier value does not match the value of the corresponding field in the input.",
                    None,
                )
            })
        })
}

fn customer_custom_id_duplicate_user_error() -> Value {
    user_error(
        json!(["input"]),
        "Value is already assigned to another metafield. Choose a different value to ensure it remains unique.",
        Some("TAKEN"),
    )
}

fn customer_metafield_matches_custom_id(metafield: &Value, custom_id: &CustomerCustomId) -> bool {
    metafield.get("namespace").and_then(Value::as_str) == Some(custom_id.namespace.as_str())
        && metafield.get("key").and_then(Value::as_str) == Some(custom_id.key.as_str())
        && metafield.get("type").and_then(Value::as_str) == Some("id")
        && metafield.get("value").and_then(Value::as_str) == Some(custom_id.value.as_str())
}

fn customer_field_path(customer_set: bool, field: &str) -> Value {
    if customer_set {
        json!(["input", field])
    } else {
        json!([field])
    }
}

fn normalize_customer_email(raw: &str) -> Option<String> {
    let email = raw.split_whitespace().collect::<String>().to_lowercase();
    if email.len() > 255 || email.is_empty() {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return None;
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return None;
    }
    Some(email)
}

fn customer_email_key(email: &str) -> String {
    email.split_whitespace().collect::<String>().to_lowercase()
}

fn customer_input_address_country_code(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    resolved_object_field(input, "defaultAddress")
        .and_then(|address| customer_input_country_code(&address))
        .or_else(|| {
            input.get("addresses").and_then(|value| match value {
                ResolvedValue::List(values) => values.iter().find_map(|value| {
                    resolved_value_object(value)
                        .as_ref()
                        .and_then(customer_input_country_code)
                }),
                _ => None,
            })
        })
}

fn customer_input_country_code(input: &BTreeMap<String, ResolvedValue>) -> Option<String> {
    customer_address_string(input, "countryCode")
        .or_else(|| customer_address_string(input, "countryCodeV2"))
        .or_else(|| customer_address_string(input, "country"))
        .and_then(|country| customer_country_from_input(&country).map(|resolved| resolved.code))
}

fn customer_record_country_code(customer: &Value) -> Option<String> {
    customer
        .get("defaultAddress")
        .and_then(customer_address_value_country_code)
        .or_else(|| {
            customer
                .pointer("/addressesV2/nodes")
                .and_then(Value::as_array)
                .and_then(|nodes| nodes.iter().find_map(customer_address_value_country_code))
        })
        .or_else(|| {
            customer
                .get("addresses")
                .and_then(Value::as_array)
                .and_then(|nodes| nodes.iter().find_map(customer_address_value_country_code))
        })
}

fn customer_address_value_country_code(address: &Value) -> Option<String> {
    value_country_code(address).map(str::to_string).or_else(|| {
        address
            .get("country")
            .and_then(Value::as_str)
            .and_then(|country| customer_country_from_input(country).map(|resolved| resolved.code))
    })
}

fn normalize_customer_phone(raw: &str, country_code: Option<&str>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 255 {
        return None;
    }
    normalize_phone_with_country_context(trimmed, country_code, true)
}

fn blank_string_to_option(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn raw_taggable_tags_argument(value: Option<&ResolvedValue>) -> Vec<String> {
    match value {
        Some(ResolvedValue::String(value)) => value.split(',').map(str::to_string).collect(),
        Some(ResolvedValue::List(values)) => values
            .iter()
            .flat_map(|value| match value {
                ResolvedValue::String(value) => value.split(',').map(str::to_string).collect(),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn customer_has_identity(input: &NormalizedCustomerInput) -> bool {
    input
        .first_name
        .as_ref()
        .and_then(|value| value.as_ref())
        .is_some_and(|value| !value.trim().is_empty())
        || input
            .last_name
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| !value.trim().is_empty())
        || input
            .email
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| !value.trim().is_empty())
        || input
            .phone
            .as_ref()
            .and_then(|value| value.as_ref())
            .is_some_and(|value| !value.trim().is_empty())
}

fn customer_has_identity_json(customer: &Value) -> bool {
    ["firstName", "lastName", "email", "phone"]
        .iter()
        .any(|field| {
            customer
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn customer_record_from_parts(
    id: &str,
    existing: Option<&Value>,
    input: &NormalizedCustomerInput,
    timestamp: &str,
    default_locale: &str,
    verified_email_default: bool,
) -> Value {
    let first = customer_string_value(input.first_name.as_ref(), existing, "firstName");
    let last = customer_string_value(input.last_name.as_ref(), existing, "lastName");
    let email = customer_string_value(input.email.as_ref(), existing, "email");
    let phone = customer_string_value(input.phone.as_ref(), existing, "phone");
    let locale = match input.locale.as_ref() {
        Some(value) => value.clone(),
        None => existing
            .and_then(|customer| customer.get("locale"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(default_locale.to_string())),
    };
    let note = customer_string_value(input.note.as_ref(), existing, "note");
    let tags = input
        .tags
        .clone()
        .or_else(|| {
            existing.and_then(|customer| {
                customer["tags"].as_array().map(|tags| {
                    tags.iter()
                        .filter_map(|tag| tag.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
            })
        })
        .unwrap_or_default();
    let tax_exempt = input
        .tax_exempt
        .or_else(|| existing.and_then(|customer| customer["taxExempt"].as_bool()))
        .unwrap_or(false);
    let tax_exemptions = input
        .tax_exemptions
        .clone()
        .or_else(|| {
            existing.and_then(|customer| {
                customer["taxExemptions"].as_array().map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
            })
        })
        .unwrap_or_default();
    let addresses = input
        .addresses
        .clone()
        .or_else(|| {
            existing.and_then(|customer| {
                customer["addressesV2"]["nodes"]
                    .as_array()
                    .map(|addresses| addresses.to_vec())
            })
        })
        .unwrap_or_default();
    let created_at = existing
        .and_then(|customer| customer["createdAt"].as_str())
        .unwrap_or(timestamp);
    let verified_email = existing
        .and_then(|customer| customer["verifiedEmail"].as_bool())
        .unwrap_or(verified_email_default);
    let mut customer = customer_record(CustomerRecordInput {
        id,
        first: first.as_deref(),
        last: last.as_deref(),
        email: email.as_deref(),
        phone: phone.as_deref(),
        locale: locale.as_deref(),
        note: note.as_deref(),
        verified_email,
        tax_exempt,
        tax_exemptions,
        tags,
        addresses,
        created_at,
        updated_at: timestamp,
    });
    if let Some(existing) = existing {
        preserve_existing_customer_fields(&mut customer, existing, input);
    }
    customer
}

fn preserve_existing_customer_fields(
    customer: &mut Value,
    existing: &Value,
    input: &NormalizedCustomerInput,
) {
    let (Some(customer), Some(existing)) = (customer.as_object_mut(), existing.as_object()) else {
        return;
    };

    for (key, value) in existing {
        if !customer.contains_key(key) {
            customer.insert(key.clone(), value.clone());
        }
    }

    for key in [
        "state",
        "canDelete",
        "dataSaleOptOut",
        "numberOfOrders",
        "amountSpent",
        "lastOrder",
        "orders",
        "storeCreditAccounts",
        "metafield",
        "metafields",
    ] {
        if let Some(value) = existing.get(key) {
            customer.insert(key.to_string(), value.clone());
        }
    }

    if input.email.is_none() {
        for key in ["defaultEmailAddress", "emailMarketingConsent"] {
            if let Some(value) = existing.get(key) {
                customer.insert(key.to_string(), value.clone());
            }
        }
    }
    if input.phone.is_none() {
        for key in ["defaultPhoneNumber", "smsMarketingConsent"] {
            if let Some(value) = existing.get(key) {
                customer.insert(key.to_string(), value.clone());
            }
        }
    }
    if input.addresses.is_none() {
        for key in ["defaultAddress", "addressesV2"] {
            if let Some(value) = existing.get(key) {
                customer.insert(key.to_string(), value.clone());
            }
        }
    }
}

/// `customerCreate` rejects nested resource ids on creation: an `id` key inside
/// any `addresses[]` or `metafields[]` input object yields a user error and a
/// null customer. Addresses are checked before metafields so the surfaced error
/// path matches Shopify's ordering when both are present.
fn customer_create_nested_id_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    for (collection, label) in [("addresses", "address"), ("metafields", "metafield")] {
        if let Some(entries) = resolved_list_field(input, collection) {
            for (index, entry) in entries.iter().enumerate() {
                if let Some(object) = resolved_value_object(entry) {
                    if object.contains_key("id") {
                        return Some(user_error_omit_code(
                            json!([collection, index.to_string(), "id"]),
                            &format!("Cannot specify {label} ID on creation"),
                            None,
                        ));
                    }
                }
            }
        }
    }
    None
}

fn customer_create_verified_email_default(
    request: &Request,
    input: &NormalizedCustomerInput,
) -> bool {
    if input
        .email
        .as_ref()
        .and_then(|value| value.as_ref())
        .is_none()
    {
        return false;
    }
    admin_graphql_version(&request.path).is_some_and(|version| !version_at_least(version, 2026, 4))
}

fn customer_string_value(
    input: Option<&Option<String>>,
    existing: Option<&Value>,
    field: &str,
) -> Option<String> {
    match input {
        Some(value) => value.clone(),
        None => existing
            .and_then(|customer| customer.get(field))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

struct CustomerRecordInput<'a> {
    id: &'a str,
    first: Option<&'a str>,
    last: Option<&'a str>,
    email: Option<&'a str>,
    phone: Option<&'a str>,
    locale: Option<&'a str>,
    note: Option<&'a str>,
    verified_email: bool,
    tax_exempt: bool,
    tax_exemptions: Vec<String>,
    tags: Vec<String>,
    addresses: Vec<Value>,
    created_at: &'a str,
    updated_at: &'a str,
}

/// Default `Customer.defaultEmailAddress` shape. Real Shopify always returns a
/// `CustomerEmailAddress` (with `NOT_SUBSCRIBED` marketing defaults) whenever an
/// email is present, and `null` otherwise. Inline consent overwrites the
/// marketing fields via [`apply_customer_marketing_consent`].
fn default_email_address_value(email: Option<&str>) -> Value {
    match email.filter(|value| !value.is_empty()) {
        Some(email) => json!({
            "emailAddress": email,
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "marketingUpdatedAt": Value::Null
        }),
        None => Value::Null,
    }
}

/// Default `Customer.defaultPhoneNumber` shape (see [`default_email_address_value`]).
fn default_phone_number_value(phone: Option<&str>) -> Value {
    match phone.filter(|value| !value.is_empty()) {
        Some(phone) => json!({
            "phoneNumber": phone,
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "marketingUpdatedAt": Value::Null,
            "marketingCollectedFrom": Value::Null
        }),
        None => Value::Null,
    }
}

/// Default `Customer.emailMarketingConsent` compatibility object.
fn email_marketing_consent_value(email: Option<&str>) -> Value {
    match email.filter(|value| !value.is_empty()) {
        Some(_) => json!({
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "consentUpdatedAt": Value::Null
        }),
        None => Value::Null,
    }
}

/// Default `Customer.smsMarketingConsent` compatibility object.
fn sms_marketing_consent_value(phone: Option<&str>) -> Value {
    match phone.filter(|value| !value.is_empty()) {
        Some(_) => json!({
            "marketingState": "NOT_SUBSCRIBED",
            "marketingOptInLevel": "SINGLE_OPT_IN",
            "consentUpdatedAt": Value::Null,
            "consentCollectedFrom": Value::Null
        }),
        None => Value::Null,
    }
}

/// Overwrite the marketing-consent fields of a staged customer record from a
/// resolved consent state. `is_email` selects email vs SMS; the latter also
/// carries `consentCollectedFrom` / `marketingCollectedFrom` defaulting to
/// `"OTHER"` (the value Shopify reports for API-set consent).
pub(super) fn apply_customer_marketing_consent(
    customer: &mut Value,
    is_email: bool,
    marketing_state: &str,
    marketing_opt_in_level: &str,
    updated_at: Option<&str>,
) {
    let Some(object) = customer.as_object_mut() else {
        return;
    };
    if is_email {
        if let Some(contact) = object
            .get_mut("defaultEmailAddress")
            .and_then(Value::as_object_mut)
        {
            contact.insert("marketingState".to_string(), json!(marketing_state));
            contact.insert(
                "marketingOptInLevel".to_string(),
                json!(marketing_opt_in_level),
            );
            contact.insert("marketingUpdatedAt".to_string(), json!(updated_at));
        }
        object.insert(
            "emailMarketingConsent".to_string(),
            json!({
                "marketingState": marketing_state,
                "marketingOptInLevel": marketing_opt_in_level,
                "consentUpdatedAt": updated_at
            }),
        );
    } else {
        if let Some(contact) = object
            .get_mut("defaultPhoneNumber")
            .and_then(Value::as_object_mut)
        {
            contact.insert("marketingState".to_string(), json!(marketing_state));
            contact.insert(
                "marketingOptInLevel".to_string(),
                json!(marketing_opt_in_level),
            );
            contact.insert("marketingUpdatedAt".to_string(), json!(updated_at));
            contact.insert("marketingCollectedFrom".to_string(), json!("OTHER"));
        }
        object.insert(
            "smsMarketingConsent".to_string(),
            json!({
                "marketingState": marketing_state,
                "marketingOptInLevel": marketing_opt_in_level,
                "consentUpdatedAt": updated_at,
                "consentCollectedFrom": "OTHER"
            }),
        );
    }
}

/// Apply inline `emailMarketingConsent` / `smsMarketingConsent` from a
/// `CustomerInput` onto a freshly built customer record. Callers must have
/// already validated that the matching contact (email/phone) is present and
/// that the marketing state is not `REDACTED`.
fn apply_inline_consent_from_input(customer: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    for (key, is_email) in [
        ("emailMarketingConsent", true),
        ("smsMarketingConsent", false),
    ] {
        let Some(consent) = resolved_object_field(input, key) else {
            continue;
        };
        let Some(marketing_state) = resolved_string_field(&consent, "marketingState") else {
            continue;
        };
        if marketing_state.is_empty() {
            continue;
        }
        let opt_in = resolved_string_field(&consent, "marketingOptInLevel")
            .unwrap_or_else(|| "SINGLE_OPT_IN".to_string());
        let updated_at = resolved_string_field(&consent, "consentUpdatedAt");
        apply_customer_marketing_consent(
            customer,
            is_email,
            &marketing_state,
            &opt_in,
            updated_at.as_deref(),
        );
    }
}

fn customer_record(input: CustomerRecordInput<'_>) -> Value {
    let first_value = input.first.filter(|value| !value.is_empty());
    let last_value = input.last.filter(|value| !value.is_empty());
    let display_name = customer_display_name(first_value, last_value, input.email);
    let default_address = input.addresses.first().cloned().unwrap_or(Value::Null);
    let start_cursor = input.addresses.first().and_then(customer_address_cursor);
    let end_cursor = input.addresses.last().and_then(customer_address_cursor);
    let address_edges = input
        .addresses
        .iter()
        .map(|address| json!({ "cursor": customer_address_cursor(address), "node": address }))
        .collect::<Vec<_>>();
    json!({
        "id": input.id,
        "firstName": first_value,
        "lastName": last_value,
        "displayName": display_name,
        "email": input.email,
        "phone": input.phone,
        "locale": input.locale,
        "note": input.note,
        "verifiedEmail": input.verified_email,
        "taxExempt": input.tax_exempt,
        "taxExemptions": input.tax_exemptions,
        "tags": input.tags,
        "state": "DISABLED",
        "dataSaleOptOut": false,
        "canDelete": true,
        "metafield": Value::Null,
        "metafields": nodes_connection(Vec::new()),
        "defaultEmailAddress": default_email_address_value(input.email),
        "defaultPhoneNumber": default_phone_number_value(input.phone),
        "emailMarketingConsent": email_marketing_consent_value(input.email),
        "smsMarketingConsent": sms_marketing_consent_value(input.phone),
        "defaultAddress": default_address,
        "addressesV2": {
            "nodes": input.addresses,
            "edges": address_edges,
            "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
        },
        "createdAt": input.created_at,
        "updatedAt": input.updated_at
    })
}

fn normalize_hydrated_customer_record(mut customer: Value) -> Value {
    if let Some(object) = customer.as_object_mut() {
        if !object.contains_key("phone") {
            let phone = object
                .get("defaultPhoneNumber")
                .and_then(|default| default.get("phoneNumber"))
                .cloned()
                .unwrap_or(Value::Null);
            object.insert("phone".to_string(), phone);
        }
        if !object.contains_key("firstName") {
            object.insert("firstName".to_string(), Value::Null);
        }
        if !object.contains_key("lastName") {
            object.insert("lastName".to_string(), Value::Null);
        }
        if !object.contains_key("note") {
            object.insert("note".to_string(), Value::Null);
        }
        if !object.contains_key("tags") {
            object.insert("tags".to_string(), json!([]));
        }
        if !object.contains_key("taxExemptions") {
            object.insert("taxExemptions".to_string(), json!([]));
        }
    }
    // The hydrate query returns `addressesV2 { nodes }` with no edges/pageInfo, but a real
    // connection read always reports them. Rebuild the connection into the full
    // nodes/edges/pageInfo shape so reads that select `addressesV2.pageInfo` (e.g. the merge
    // downstream read) match Shopify instead of observing an undefined pageInfo. Cursors are the
    // deterministic per-node form, matched leniently as `any-string` downstream.
    if customer.get("addressesV2").is_some() {
        let nodes = connection_nodes(&customer["addressesV2"]);
        let default_id = customer
            .get("defaultAddress")
            .and_then(|address| address.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        customer_rebuild_addresses(&mut customer, nodes, default_id.as_deref());
    }
    // The hydrate query likewise returns `metafields { nodes }` with no pageInfo, so rebuild it
    // into the full nodes/pageInfo connection shape. Otherwise an (often empty) metafields
    // connection reports an undefined pageInfo for reads that select it — e.g. the merge
    // downstream read, which expects the empty-connection `{hasNextPage:false, …, endCursor:null}`.
    if customer.get("metafields").is_some() {
        let nodes = connection_nodes(&customer["metafields"]);
        customer["metafields"] = nodes_connection(nodes);
    }
    customer
}

fn customer_display_name(first: Option<&str>, last: Option<&str>, email: Option<&str>) -> String {
    let name = [first, last]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !name.is_empty() {
        name
    } else {
        email.unwrap_or_default().to_string()
    }
}

fn find_customer_id_by_email(customers: &BTreeMap<String, Value>, email: &str) -> Option<String> {
    customers.iter().find_map(|(id, customer)| {
        customer
            .get("email")
            .and_then(Value::as_str)
            .is_some_and(|existing| customer_email_key(existing) == customer_email_key(email))
            .then(|| id.clone())
    })
}

fn find_customer_id_by_phone(customers: &BTreeMap<String, Value>, phone: &str) -> Option<String> {
    customers.iter().find_map(|(id, customer)| {
        customer
            .get("phone")
            .and_then(Value::as_str)
            .is_some_and(|existing| existing == phone)
            .then(|| id.clone())
    })
}

fn customer_value_string<'a>(customer: &'a Value, field: &str) -> &'a str {
    customer
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn customer_normalized_string(customer: &Value, field: &str) -> StagedSortValue {
    StagedSortValue::String(customer_value_string(customer, field).to_ascii_lowercase())
}

fn customer_gid_tail_sort_value(customer: &Value) -> StagedSortValue {
    let id = customer_value_string(customer, "id");
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn customer_name_sort_key(customer: &Value) -> StagedSortKey {
    vec![
        customer_normalized_string(customer, "lastName"),
        customer_normalized_string(customer, "firstName"),
        customer_normalized_string(customer, "displayName"),
        customer_gid_tail_sort_value(customer),
    ]
}

fn customer_address_sort_value(customer: &Value, field: &str) -> StagedSortValue {
    customer
        .get("defaultAddress")
        .and_then(|address| address.get(field))
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn customer_staged_sort_key(customer: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let primary = match sort_key.unwrap_or("ID") {
        "NAME" => return customer_name_sort_key(customer),
        "UPDATED_AT" => {
            StagedSortValue::String(customer_value_string(customer, "updatedAt").to_string())
        }
        "CREATED_AT" => {
            StagedSortValue::String(customer_value_string(customer, "createdAt").to_string())
        }
        "LOCATION" => {
            return vec![
                customer_address_sort_value(customer, "country"),
                customer_address_sort_value(customer, "province"),
                customer_address_sort_value(customer, "city"),
                customer_gid_tail_sort_value(customer),
            ];
        }
        "ID" | "RELEVANCE" => customer_gid_tail_sort_value(customer),
        _ => customer_gid_tail_sort_value(customer),
    };
    vec![primary, customer_gid_tail_sort_value(customer)]
}

/// Evaluate a customer search `query` against a staged customer.
///
/// The local slice intentionally stays query-language-small, but it covers the
/// documented customer fields exercised by captured parity (`state:`, grouped
/// `tag:` OR/negation) and common keyed fields used by staged read-after-write
/// tests. Unknown keyed predicates remain explicit unsupported terms.
fn customer_search_decision(customer: &Value, query: Option<&str>) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }
    let tokens = customer_search_tokens(query);
    if tokens.is_empty() {
        return StagedSearchDecision::Match;
    }
    let mut parser = CustomerSearchParser::new(tokens);
    parser
        .parse()
        .map(|expression| expression.decision(customer))
        .unwrap_or(StagedSearchDecision::Unsupported)
}

fn customer_overlay_search_decision(customer: &Value, query: Option<&str>) -> StagedSearchDecision {
    match customer_search_decision(customer, query) {
        StagedSearchDecision::Unsupported => StagedSearchDecision::Match,
        decision => decision,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CustomerSearchToken {
    Term { value: String, quoted: bool },
    LParen,
    RParen,
    Minus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CustomerSearchExpression {
    Term(CustomerSearchTerm),
    Not(Box<CustomerSearchExpression>),
    And(Vec<CustomerSearchExpression>),
    Or(Vec<CustomerSearchExpression>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CustomerSearchTerm {
    field: Option<String>,
    value: String,
}

struct CustomerSearchParser {
    tokens: Vec<CustomerSearchToken>,
    index: usize,
}

impl CustomerSearchParser {
    fn new(tokens: Vec<CustomerSearchToken>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse(&mut self) -> Option<CustomerSearchExpression> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<CustomerSearchExpression> {
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
            CustomerSearchExpression::Or(expressions)
        })
    }

    fn parse_and(&mut self) -> Option<CustomerSearchExpression> {
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
            CustomerSearchExpression::And(expressions)
        })
    }

    fn parse_unary(&mut self) -> Option<CustomerSearchExpression> {
        if matches!(
            self.tokens.get(self.index),
            Some(CustomerSearchToken::Minus)
        ) {
            self.index += 1;
            return self
                .parse_unary()
                .map(|expression| CustomerSearchExpression::Not(Box::new(expression)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<CustomerSearchExpression> {
        match self.tokens.get(self.index).cloned()? {
            CustomerSearchToken::Term { value, quoted } => {
                self.index += 1;
                Some(CustomerSearchExpression::Term(CustomerSearchTerm::new(
                    value, quoted,
                )))
            }
            CustomerSearchToken::LParen => {
                self.index += 1;
                let expression = self.parse_or()?;
                if self.peek_rparen() {
                    self.index += 1;
                }
                Some(expression)
            }
            CustomerSearchToken::RParen | CustomerSearchToken::Minus => None,
        }
    }

    fn peek_rparen(&self) -> bool {
        matches!(
            self.tokens.get(self.index),
            Some(CustomerSearchToken::RParen)
        )
    }

    fn peek_operator(&self, operator: &str) -> bool {
        matches!(
            self.tokens.get(self.index),
            Some(CustomerSearchToken::Term { value, quoted: false })
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

impl CustomerSearchExpression {
    fn decision(&self, customer: &Value) -> StagedSearchDecision {
        match self {
            CustomerSearchExpression::Term(term) => term.decision(customer),
            CustomerSearchExpression::Not(expression) => match expression.decision(customer) {
                StagedSearchDecision::Match => StagedSearchDecision::NoMatch,
                StagedSearchDecision::NoMatch => StagedSearchDecision::Match,
                StagedSearchDecision::Unsupported => StagedSearchDecision::Unsupported,
            },
            CustomerSearchExpression::And(expressions) => {
                let mut saw_supported = false;
                let mut saw_unsupported = false;
                for expression in expressions {
                    match expression.decision(customer) {
                        StagedSearchDecision::Match => saw_supported = true,
                        StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
                        StagedSearchDecision::Unsupported => saw_unsupported = true,
                    }
                }
                if saw_unsupported {
                    StagedSearchDecision::Unsupported
                } else {
                    StagedSearchDecision::from_bool(saw_supported)
                }
            }
            CustomerSearchExpression::Or(expressions) => {
                let mut saw_unsupported = false;
                for expression in expressions {
                    match expression.decision(customer) {
                        StagedSearchDecision::Match => return StagedSearchDecision::Match,
                        StagedSearchDecision::NoMatch => {}
                        StagedSearchDecision::Unsupported => saw_unsupported = true,
                    }
                }
                if saw_unsupported {
                    StagedSearchDecision::Unsupported
                } else {
                    StagedSearchDecision::NoMatch
                }
            }
        }
    }
}

impl CustomerSearchTerm {
    fn new(value: String, quoted: bool) -> Self {
        if !quoted {
            if let Some((field, value)) = value.split_once(':') {
                if !field.is_empty() && !value.is_empty() {
                    return Self {
                        field: Some(field.replace('-', "_").to_ascii_lowercase()),
                        value: value.trim_matches('"').trim_matches('\'').to_string(),
                    };
                }
            }
        }
        Self { field: None, value }
    }

    fn decision(&self, customer: &Value) -> StagedSearchDecision {
        let value = self.value.trim();
        if value.is_empty() {
            return StagedSearchDecision::Match;
        }
        match self.field.as_deref() {
            Some("email") => StagedSearchDecision::from_bool(customer_search_string_matches(
                customer_value_string(customer, "email"),
                value,
            )),
            Some("first_name") | Some("firstname") => StagedSearchDecision::from_bool(
                customer_search_string_matches(customer_value_string(customer, "firstName"), value),
            ),
            Some("last_name") | Some("lastname") => StagedSearchDecision::from_bool(
                customer_search_string_matches(customer_value_string(customer, "lastName"), value),
            ),
            Some("id") => {
                StagedSearchDecision::from_bool(customer_matches_search_id(customer, value))
            }
            Some("phone") => {
                StagedSearchDecision::from_bool(customer_matches_phone(customer, value))
            }
            Some("state") => {
                StagedSearchDecision::from_bool(customer_matches_state(customer, value))
            }
            Some("country") => StagedSearchDecision::from_bool(customer_address_matches_any(
                customer,
                value,
                &["country", "countryCode", "countryCodeV2"],
            )),
            Some("province") | Some("province_code") => {
                StagedSearchDecision::from_bool(customer_address_matches_any(
                    customer,
                    value,
                    &["province", "provinceCode", "provinceCodeV2"],
                ))
            }
            Some("city") => StagedSearchDecision::from_bool(customer_address_matches_any(
                customer,
                value,
                &["city"],
            )),
            Some("tag") => {
                StagedSearchDecision::from_bool(customer_matches_any_search_tag(customer, value))
            }
            Some("tag_not") => {
                StagedSearchDecision::from_bool(!customer_matches_any_search_tag(customer, value))
            }
            Some("verified_email") | Some("verifiedemail") => StagedSearchDecision::from_bool(
                customer_bool_field_matches(customer, "verifiedEmail", value),
            ),
            Some("created_at") | Some("createdat") => StagedSearchDecision::from_bool(
                customer_timestamp_matches(customer_value_string(customer, "createdAt"), value),
            ),
            Some("updated_at") | Some("updatedat") => StagedSearchDecision::from_bool(
                customer_timestamp_matches(customer_value_string(customer, "updatedAt"), value),
            ),
            Some("orders_count") | Some("orderscount") => {
                StagedSearchDecision::from_bool(customer_number_of_orders_matches(customer, value))
            }
            Some(_) => StagedSearchDecision::Unsupported,
            None => StagedSearchDecision::from_bool(customer_matches_free_text(customer, value)),
        }
    }
}

fn customer_search_tokens(query: &str) -> Vec<CustomerSearchToken> {
    let mut tokens = Vec::new();
    let chars = query.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ch if ch.is_whitespace() => index += 1,
            '(' => {
                tokens.push(CustomerSearchToken::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(CustomerSearchToken::RParen);
                index += 1;
            }
            '-' => {
                tokens.push(CustomerSearchToken::Minus);
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
                tokens.push(CustomerSearchToken::Term {
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
                    tokens.push(CustomerSearchToken::Term {
                        value,
                        quoted: false,
                    });
                }
            }
        }
    }
    tokens
}

fn customer_matches_free_text(customer: &Value, value: &str) -> bool {
    let haystack = format!(
        "{} {} {} {} {} {} {} {}",
        customer_value_string(customer, "email"),
        customer_value_string(customer, "displayName"),
        customer_value_string(customer, "firstName"),
        customer_value_string(customer, "lastName"),
        customer_value_string(customer, "phone"),
        customer_default_address_string(customer, "city"),
        customer_default_address_string(customer, "province"),
        customer_default_address_string(customer, "country")
    )
    .to_ascii_lowercase();
    customer_search_string_matches(&haystack, value)
        || customer_matches_any_search_tag(customer, value)
        || customer_matches_phone(customer, value)
}

fn customer_search_string_matches(haystack: &str, value: &str) -> bool {
    let needle = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches('*')
        .to_ascii_lowercase();
    !needle.is_empty() && haystack.to_ascii_lowercase().contains(&needle)
}

fn customer_default_address_string<'a>(customer: &'a Value, field: &str) -> &'a str {
    customer
        .get("defaultAddress")
        .and_then(|address| address.get(field))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn customer_address_matches_any(customer: &Value, value: &str, fields: &[&str]) -> bool {
    fields.iter().any(|field| {
        customer_search_string_matches(customer_default_address_string(customer, field), value)
    })
}

fn customer_matches_search_id(customer: &Value, value: &str) -> bool {
    let id = customer_value_string(customer, "id");
    let value = value.trim_matches('"').trim_matches('\'');
    id == value || resource_id_tail(id) == value || resource_id_path_tail(id) == value
}

fn customer_matches_phone(customer: &Value, value: &str) -> bool {
    [
        customer_value_string(customer, "phone"),
        customer
            .get("defaultPhoneNumber")
            .and_then(|phone| phone.get("phoneNumber"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        customer_default_address_string(customer, "phone"),
    ]
    .iter()
    .any(|phone| customer_search_string_matches(phone, value))
}

fn customer_matches_state(customer: &Value, value: &str) -> bool {
    customer_value_string(customer, "state").eq_ignore_ascii_case(value.trim())
        || customer_address_matches_any(
            customer,
            value,
            &["province", "provinceCode", "provinceCodeV2"],
        )
}

fn customer_matches_any_search_tag(customer: &Value, value: &str) -> bool {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .any(|needle| {
            customer["tags"]
                .as_array()
                .map(|tags| {
                    tags.iter().any(|tag| {
                        tag.as_str()
                            .is_some_and(|tag| customer_search_string_matches(tag, needle))
                    })
                })
                .unwrap_or(false)
        })
}

fn customer_bool_field_matches(customer: &Value, field: &str, value: &str) -> bool {
    let expected = match value.trim().to_ascii_lowercase().as_str() {
        "true" => true,
        "false" => false,
        _ => return false,
    };
    customer.get(field).and_then(Value::as_bool) == Some(expected)
}

fn customer_timestamp_matches(timestamp: &str, value: &str) -> bool {
    let value = value.trim();
    let (operator, operand) = if let Some(stripped) = value.strip_prefix(">=") {
        (">=", stripped.trim())
    } else if let Some(stripped) = value.strip_prefix("<=") {
        ("<=", stripped.trim())
    } else if let Some(stripped) = value.strip_prefix('>') {
        (">", stripped.trim())
    } else if let Some(stripped) = value.strip_prefix('<') {
        ("<", stripped.trim())
    } else {
        (":", value)
    };
    if timestamp.is_empty() || operand.is_empty() {
        return false;
    }
    match operator {
        ">=" => timestamp >= operand,
        ">" => timestamp > operand,
        "<=" => timestamp <= operand,
        "<" => timestamp < operand,
        _ => timestamp.starts_with(operand),
    }
}

fn customer_number_of_orders_matches(customer: &Value, value: &str) -> bool {
    let Some(expected) = value.trim().parse::<i64>().ok() else {
        return false;
    };
    customer.get("numberOfOrders").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    }) == Some(expected)
}

fn customer_order_count(customer: &Value) -> Option<u64> {
    customer
        .get("numberOfOrders")
        .and_then(|count| match count {
            Value::String(value) => value.parse::<u64>().ok(),
            Value::Number(value) => value.as_u64(),
            _ => None,
        })
}

/// Surface Shopify's order-summary defaults on a freshly staged customer record:
/// `numberOfOrders` is the string `"0"`, `lastOrder` is explicitly null, and
/// `orders` is an empty connection (with the `pageInfo` shape a `first:`/`last:`
/// page selection expects). Only fills fields that are absent/null so a record
/// that already carries real order state (e.g. a seeded customer) is untouched.
fn apply_customer_order_summary_defaults(customer: &mut Value, shop_currency_code: Option<&str>) {
    if customer.get("numberOfOrders").is_none_or(Value::is_null) {
        customer["numberOfOrders"] = json!("0");
    }
    if let Some(shop_currency_code) = shop_currency_code {
        if customer.get("amountSpent").is_none_or(Value::is_null) {
            customer["amountSpent"] = money_value("0.0", shop_currency_code);
        }
    }
    if customer.get("lastOrder").is_none() {
        customer["lastOrder"] = Value::Null;
    }
    if customer.get("orders").is_none_or(Value::is_null) {
        customer["orders"] = connection_json_with_empty_edges(Vec::new());
    }
}

fn store_credit_account_currency(account: &Value) -> &str {
    account["balance"]["currencyCode"]
        .as_str()
        .unwrap_or_default()
}

fn store_credit_account_matches_id(account: &Value, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    account
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| resource_id_tail(id) == value || resource_id_path_tail(id) == value)
}

fn store_credit_account_search_decision(
    account: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query else {
        return StagedSearchDecision::Match;
    };
    let query = query.trim();
    if query.is_empty() {
        return StagedSearchDecision::Match;
    }

    for term in query.split_whitespace() {
        if term.eq_ignore_ascii_case("AND") {
            continue;
        }
        let term = term.trim().trim_matches('"').trim_matches('\'');
        if term.is_empty() {
            continue;
        }
        let decision = if let Some((key, value)) = term.split_once(':') {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key {
                "id" => {
                    StagedSearchDecision::from_bool(store_credit_account_matches_id(account, value))
                }
                "currency"
                | "currency_code"
                | "currencyCode"
                | "balance.currency_code"
                | "balance.currencyCode" => StagedSearchDecision::from_bool(
                    store_credit_account_currency(account).eq_ignore_ascii_case(value),
                ),
                _ => StagedSearchDecision::Unsupported,
            }
        } else {
            let needle = term.to_ascii_lowercase();
            let currency = store_credit_account_currency(account).to_ascii_lowercase();
            StagedSearchDecision::from_bool(
                currency.contains(&needle) || store_credit_account_matches_id(account, term),
            )
        };
        match decision {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }
    StagedSearchDecision::Match
}

fn store_credit_account_sort_key(account: &Value, _sort_key: Option<&str>) -> StagedSortKey {
    vec![
        resource_id_tail_sort_value(account.get("id").and_then(Value::as_str)),
        StagedSortValue::String(store_credit_account_currency(account).to_ascii_lowercase()),
    ]
}

/// Shopify rejects a credit/debit that would push an account past this hard cap.
const STORE_CREDIT_LIMIT: f64 = 100000.0;

fn store_credit_missing_id_user_error(id: &str, is_credit: bool) -> Value {
    if is_credit
        && matches!(
            shopify_gid_resource_type(id),
            Some("Customer" | "CompanyLocation")
        )
    {
        user_error(["id"], "Owner does not exist", Some("OWNER_NOT_FOUND"))
    } else {
        user_error(
            ["id"],
            "Store credit account does not exist",
            Some("ACCOUNT_NOT_FOUND"),
        )
    }
}

/// Read a money `amount` field from a resolved input map, accepting either the
/// canonical string form or a numeric literal (GraphQL `Decimal` is serialized
/// as a string but some callers send numbers).
fn resolved_money_amount_text(
    input: &BTreeMap<String, ResolvedValue>,
    field: &str,
) -> Option<String> {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        Some(ResolvedValue::Int(value)) => Some(value.to_string()),
        Some(ResolvedValue::Float(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn store_credit_expires_at_in_past(expires_at: &str, now_epoch: i64) -> bool {
    parse_rfc3339_epoch_seconds(expires_at)
        .map(|expires_at| expires_at <= now_epoch)
        .unwrap_or(false)
}

fn store_credit_result_only_currency_response(fields: &[RootFieldSelection]) -> Option<Response> {
    let field = fields.iter().find(|field| {
        matches!(
            field.name.as_str(),
            "storeCreditAccountCredit" | "storeCreditAccountDebit"
        )
    })?;
    let (input_name, amount_name) = if field.name == "storeCreditAccountCredit" {
        ("creditInput", "creditAmount")
    } else {
        ("debitInput", "debitAmount")
    };
    let input = resolved_object_field(&field.arguments, input_name).unwrap_or_default();
    let amount_input = resolved_object_field(&input, amount_name).unwrap_or_default();
    let currency = resolved_string_field(&amount_input, "currencyCode")?;
    if !matches!(currency.as_str(), "USDC" | "XXX") {
        return None;
    }

    let mut data = serde_json::Map::new();
    data.insert(field.response_key.clone(), Value::Null);
    Some(ok_json(json!({
        "errors": [{
            "message": format!("CurrencyCode \"{currency}\" is invalid. It can only be used as a result and not as an input value."),
            "locations": [{
                "line": field.location.line,
                "column": field.location.column
            }],
            "extensions": { "code": "CURRENCY_CODE_INVALID" },
            "path": [field.response_key.clone()]
        }],
        "data": Value::Object(data)
    })))
}

#[cfg(test)]
mod customer_search_tests {
    use super::*;

    fn search_customer() -> Value {
        json!({
            "id": "gid://shopify/Customer/42",
            "email": "search-customer@example.test",
            "firstName": "Search",
            "lastName": "Customer",
            "displayName": "Search Customer",
            "state": "DISABLED",
            "verifiedEmail": true,
            "tags": ["VIP"],
            "defaultAddress": {
                "city": "Toronto",
                "province": "Ontario",
                "provinceCode": "ON",
                "country": "Canada",
                "countryCodeV2": "CA"
            },
            "defaultPhoneNumber": {
                "phoneNumber": "+16135550135"
            },
            "createdAt": "2026-07-01T00:00:00Z",
            "updatedAt": "2026-07-02T00:00:00Z",
            "numberOfOrders": "0"
        })
    }

    #[test]
    fn customer_search_distinguishes_supported_no_match_from_unsupported_key() {
        let customer = search_customer();
        assert_eq!(
            customer_search_decision(&customer, Some("country:Canada")),
            StagedSearchDecision::Match
        );
        assert_eq!(
            customer_search_decision(&customer, Some("country:United States")),
            StagedSearchDecision::NoMatch
        );
        assert_eq!(
            customer_search_decision(&customer, Some("made_up_filter:Canada")),
            StagedSearchDecision::Unsupported
        );
        assert_eq!(
            customer_overlay_search_decision(&customer, Some("made_up_filter:Canada")),
            StagedSearchDecision::Match
        );
        assert_eq!(
            customer_overlay_search_decision(
                &customer,
                Some("country:United States made_up_filter:Canada")
            ),
            StagedSearchDecision::NoMatch
        );
        assert_eq!(
            customer_overlay_search_decision(
                &customer,
                Some("country:United States OR made_up_filter:Canada")
            ),
            StagedSearchDecision::Match
        );
        assert_eq!(
            customer_search_decision(&customer, Some("state:DISABLED -tag:VIP")),
            StagedSearchDecision::NoMatch
        );
    }
}
