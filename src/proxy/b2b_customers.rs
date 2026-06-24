use super::*;

mod b2b_companies;

/// Snapshot of a staged customer's inline-address context:
/// `(firstName, lastName, addressesV2.nodes, defaultAddress.id)`.
type CustomerAddressContext = (Option<String>, Option<String>, Vec<Value>, Option<String>);

// Shared with the parity capture scripts via include_str! so recorded `CustomerHydrate`
// cassettes byte-match what `hydrate_customer_for_mutation` forwards upstream. The leading
// newline is significant: the cassette matcher only trims trailing whitespace.
const CUSTOMER_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/customers/customer-mutation-hydrate.graphql");

// Shared with the parity capture scripts via include_str! so recorded
// `CustomerDuplicateHydrate` dedupe cassettes byte-match what the create path forwards
// upstream. The leading newline is significant: the cassette matcher only trims trailing
// whitespace.
const CUSTOMER_DUPLICATE_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/customers/customer-duplicate-hydrate.graphql");

// `customerMerge` resolves both referenced customers the real way (forward + observe) and
// must reconcile their *attached* resources — metafields, addresses, and orders — into the
// resulting customer. The general `CustomerHydrate` mutation hydrate only carries scalars +
// addresses, so the merge forwards this richer query instead and stages metafields/orders
// from it. Shared with the merge capture scripts via include_str! so the recorded
// `CustomerMergeHydrate` cassettes byte-match what `hydrate_customer_for_merge` forwards.
const CUSTOMER_MERGE_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/customers/customer-merge-hydrate.graphql");

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
            "customer" => match field.arguments.get("id") {
                Some(ResolvedValue::String(id)) => {
                    self.store.staged.customers.contains_key(id)
                        || self.store.staged.customers.is_tombstoned(id)
                        || self.store_credit_owner_has_accounts(id)
                }
                _ => false,
            },
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
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "customer" => Some(self.customer_read_field(field)),
                "customerByIdentifier" => Some(self.customer_by_identifier_field(field)),
                "customers" => Some(self.customers_list_field(field)),
                "customersCount" => Some(selected_json(
                    &json!({ "count": self.customers_count_value(), "precision": "EXACT" }),
                    &field.selection,
                )),
                "customerMergeJobStatus" => Some(self.customer_merge_job_status_field(field)),
                "job" => Some(self.customer_merge_job_node_field(field)),
                "node" if self.customer_merge_job_reference(field) => {
                    Some(self.customer_merge_job_node_field(field))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(field.response_key.clone(), value);
            }
        }
        Value::Object(data)
    }

    /// The store-wide total customer count: the seeded live baseline (or the
    /// legacy default) reduced by the number of customers deleted/merged-away in
    /// this scenario, so `customersCount` tracks merges generically.
    pub(in crate::proxy) fn customers_count_value(&self) -> u64 {
        self.store
            .staged
            .customers_count_base
            .unwrap_or(177)
            .saturating_sub(self.store.staged.customers.tombstones.len() as u64)
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
        let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
            return Value::Null;
        };
        if self.store.staged.customers.is_tombstoned(id) {
            return Value::Null;
        }
        self.store
            .staged
            .customers
            .get(id)
            .map(|customer| self.customer_with_order_connection(id, customer, &field.selection))
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
            "orders" => Some(match mapped_orders {
                Some(orders) => selected_connection_json_with_args(
                    orders.clone(),
                    &field.arguments,
                    &field.selection,
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
            // The `storeCreditAccounts` connection is resolved from the staged
            // store-credit accounts indexed by owner, so a customer read reflects
            // credit/debit mutations (and locally minted accounts) immediately.
            "storeCreditAccounts" => Some(self.store_credit_accounts_connection_for_owner(
                id,
                &field.arguments,
                &field.selection,
            )),
            _ => selected_json(customer, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    pub(in crate::proxy) fn store_credit_account_read_fields(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name != "storeCreditAccount" {
                continue;
            }
            let value = resolved_string_arg(&field.arguments, "id")
                .and_then(|id| self.store.staged.store_credit_accounts.get(&id))
                .map(|account| self.selected_store_credit_account(account, &field.selection))
                .unwrap_or(Value::Null);
            data.insert(field.response_key.clone(), value);
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn store_credit_account_mutation(
        &mut self,
        root_field: &str,
        _request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let Some(fields) = root_fields(query, variables) else {
            return MutationOutcome::response(json_error(400, "Could not parse GraphQL operation"));
        };
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for field in fields {
            if !matches!(
                field.name.as_str(),
                "storeCreditAccountCredit" | "storeCreditAccountDebit"
            ) {
                continue;
            }
            let outcome = self.store_credit_account_mutation_field(&field);
            if let Some(log_draft) = outcome.log_draft {
                log_drafts.push(log_draft);
            }
            data.insert(field.response_key.clone(), outcome.value);
        }
        if data.is_empty() {
            return MutationOutcome::response(json_error(501, "Unsupported store credit mutation"));
        }
        let response = ok_json(json!({ "data": Value::Object(data) }));
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

        if amount <= 0.0 {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &[input_name, amount_name, "amount"],
                    if is_credit {
                        "A positive amount must be used to credit a store credit account"
                    } else {
                        "A positive amount must be used to debit a store credit account"
                    },
                    "NEGATIVE_OR_ZERO_AMOUNT",
                ),
            );
        }
        if is_credit && !store_credit_supported_currency(&currency) {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &[input_name, amount_name, "currencyCode"],
                    "Currency is not supported",
                    "UNSUPPORTED_CURRENCY",
                ),
            );
        }
        if is_credit
            && resolved_string_field(&input, "expiresAt")
                .as_deref()
                .map(store_credit_expires_at_in_past)
                .unwrap_or(false)
        {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &[input_name, "expiresAt"],
                    "The expiry date must be in the future",
                    "EXPIRES_AT_IN_PAST",
                ),
            );
        }

        let id = resolved_string_arg(&field.arguments, "id").unwrap_or_default();
        let Some(account_id) =
            self.resolve_store_credit_account_id_for_mutation(&id, &currency, is_credit)
        else {
            return self.store_credit_error_outcome(
                field,
                store_credit_missing_id_user_error(&id, is_credit),
            );
        };

        let Some(existing) = self
            .store
            .staged
            .store_credit_accounts
            .get(&account_id)
            .cloned()
        else {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &["id"],
                    "Store credit account does not exist",
                    "ACCOUNT_NOT_FOUND",
                ),
            );
        };
        let account_currency = existing["balance"]["currencyCode"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if currency != account_currency {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &[input_name, amount_name, "currencyCode"],
                    "The currency provided does not match the currency of the store credit account",
                    "MISMATCHING_CURRENCY",
                ),
            );
        }

        let current_balance = existing["balance"]["amount"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);
        if is_credit && current_balance + amount >= STORE_CREDIT_LIMIT {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &[input_name, amount_name, "amount"],
                    "The operation would cause the account's credit limit to be exceeded",
                    "CREDIT_LIMIT_EXCEEDED",
                ),
            );
        }
        if !is_credit && amount > current_balance {
            return self.store_credit_error_outcome(
                field,
                store_credit_user_error(
                    &[input_name, amount_name, "amount"],
                    "The store credit account does not have sufficient funds to satisfy the request",
                    "INSUFFICIENT_FUNDS",
                ),
            );
        }

        let delta = if is_credit { amount } else { -amount };
        let balance_after = current_balance + delta;
        let amount_display = shopify_decimal_text(delta);
        let balance_display = shopify_decimal_text(balance_after);
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

    fn resolve_store_credit_account_id_for_mutation(
        &mut self,
        id: &str,
        currency: &str,
        allow_create: bool,
    ) -> Option<String> {
        match shopify_gid_resource_type(id) {
            Some("StoreCreditAccount") => self
                .store
                .staged
                .store_credit_accounts
                .contains_key(id)
                .then(|| id.to_string()),
            Some("Customer") | Some("CompanyLocation") => {
                if !self.store_credit_owner_exists(id) {
                    return None;
                }
                if let Some(account_id) =
                    self.store_credit_account_id_for_owner_currency(id, currency)
                {
                    return Some(account_id);
                }
                if allow_create {
                    Some(self.create_store_credit_account_for_owner(id, currency))
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
        selected_connection_json_with_args(accounts, arguments, selection, value_id_cursor)
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

    fn store_credit_owner_exists(&self, owner_id: &str) -> bool {
        match shopify_gid_resource_type(owner_id) {
            Some("Customer") => {
                self.store.staged.customers.contains_key(owner_id)
                    && !self.store.staged.customers.is_tombstoned(owner_id)
            }
            Some("CompanyLocation") => {
                b2b_company_location_exists(&self.store.staged.b2b_locations.records, owner_id)
            }
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
                .unwrap_or_else(|| b2b_synthetic_seed_company_location(owner_id)),
            _ => json!({ "id": owner_id }),
        }
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
    /// (excluding merged-away / deleted records) by the optional `query` (currently
    /// `tag:<value>` plus a generic substring fallback over email/display name) and
    /// projects each node through the shared customer renderer so nested
    /// `orders`/`addressesV2`/`metafields` connections resolve from store state
    /// exactly as the singular `customer`/`customerByIdentifier` reads do.
    pub(in crate::proxy) fn customers_list_field(&self, field: &RootFieldSelection) -> Value {
        let query = resolved_string_field(&field.arguments, "query");
        let mut records: Vec<Value> = self
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
            .filter(|customer| customer_matches_query(customer, query.as_deref()))
            .cloned()
            .collect();
        records.sort_by(|a, b| {
            a["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["id"].as_str().unwrap_or_default())
        });
        selected_typed_connection_with_args(
            &records,
            &field.arguments,
            &field.selection,
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
        let Some(ResolvedValue::Object(identifier)) = field.arguments.get("identifier") else {
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
        let customer = if let Some(raw_email) = resolved_string_field(identifier, "email")
            .or_else(|| resolved_string_field(identifier, "emailAddress"))
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
        } else if let Some(id) = resolved_string_field(identifier, "id") {
            self.store
                .staged
                .customers
                .get(&id)
                .filter(|_| !self.store.staged.customers.is_tombstoned(&id))
        } else if let Some(raw_phone) = resolved_string_field(identifier, "phone")
            .or_else(|| resolved_string_field(identifier, "phoneNumber"))
        {
            let needle = normalize_customer_phone(&raw_phone);
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
        let order_email = resolved_string_field(&order_input, "email").unwrap_or_default();
        let id = if order_email.ends_with("example.test") {
            let ordinal = self.next_synthetic_id.saturating_sub(1);
            format!("gid://shopify/Order/{}", ordinal.max(1))
        } else {
            synthetic_shopify_gid("Order", self.next_synthetic_id)
        };
        self.next_synthetic_id += 1;
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

        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            let (payload, staged_ids, field_errors) =
                self.customer_mutation_payload(request, &field);
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
                selected_json(&payload, &field.selection)
            };
            data.insert(field.response_key.clone(), rendered);
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !errors.is_empty() {
            body["errors"] = Value::Array(errors);
        }
        ok_json(body)
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
                    vec![customer_user_error(
                        Value::Null,
                        "Local staging for this customer mutation is not implemented.",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            ),
        }
    }

    /// Fabricated customers always receive a stable plain `gid://shopify/Customer/N`
    /// id. The local-runtime conformance fixtures compare these ids strictly and
    /// expect the plain form, while every live-hybrid scenario that surfaces a
    /// fabricated customer id matches it with the lenient `shopify-gid:Customer`
    /// matcher (which accepts any `gid://shopify/Customer/...`). Read routing keys
    /// on `staged.customers.contains_key(id)`, so the proxy stays internally
    /// consistent without needing the `?shopify-draft-proxy=synthetic` marker.
    fn next_customer_gid(&mut self, _normalized: &NormalizedCustomerInput) -> String {
        self.next_synthetic_gid("Customer")
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
                    vec![customer_user_error(
                        json!(["id"]),
                        "Cannot specify ID on creation",
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
            self.customer_input_validation_errors(request, &input, None, false);
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

        let id = self.next_customer_gid(&normalized);
        let timestamp = self.next_product_timestamp();
        let verified_email_default = customer_create_verified_email_default(request, &normalized);
        let mut customer =
            customer_record_from_parts(&id, None, &normalized, &timestamp, verified_email_default);
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
        // mutation payload projection. `amountSpent` needs the shop currency (not
        // known locally) and remains the one acknowledged representation gap.
        apply_customer_order_summary_defaults(&mut customer);
        // A freshly created customer also has no store-credit accounts. Bake the
        // empty connection so a create payload selecting `storeCreditAccounts`
        // matches; reads recompute it from staged store-credit state via
        // `customer_with_order_connection`.
        if customer
            .get("storeCreditAccounts")
            .is_none_or(Value::is_null)
        {
            customer["storeCreditAccounts"] = empty_orders_connection();
        }
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
    }

    /// Standalone `customerAddress*` / `customerUpdateDefaultAddress` mutations.
    ///
    /// HEAD stores customer addresses *inline* on the staged customer record at
    /// `addressesV2.nodes` / `defaultAddress`; these handlers operate directly on
    /// that inline model so reads (`customer`, `customerByIdentifier`) reflect
    /// every mutation via the same `selected_json` path. Address ids are minted
    /// from the shared synthetic counter (`next_proxy_synthetic_gid`) so they are
    /// globally unique across customers — this is what lets cross-owner address
    /// references resolve to "Address does not exist" rather than colliding with a
    /// different customer's per-customer index. The parity comparison matches
    /// these synthetic ids and cursors with `any-string`, so only their
    /// uniqueness and read-after-write consistency matter, never their values.
    pub(in crate::proxy) fn customer_address_mutation(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        let mut top_errors = Vec::new();
        for field in &fields {
            let (payload, staged_ids, field_top_errors) = match field.name.as_str() {
                "customerAddressCreate" => self.customer_address_create(field),
                "customerAddressUpdate" => self.customer_address_update(field),
                "customerAddressDelete" => self.customer_address_delete(field),
                "customerUpdateDefaultAddress" => self.customer_update_default_address(field),
                _ => (Value::Null, Vec::new(), Vec::new()),
            };
            top_errors.extend(field_top_errors);
            if !staged_ids.is_empty() {
                self.record_mutation_log_entry(request, query, variables, &field.name, staged_ids);
            }
            // A null payload signals a top-level RESOURCE_NOT_FOUND (the data
            // field itself is null); a non-null payload renders through the
            // selection set like every other mutation result.
            let rendered = if payload.is_null() {
                Value::Null
            } else {
                selected_json(&payload, &field.selection)
            };
            data.insert(field.response_key.clone(), rendered);
        }
        let mut body = json!({ "data": Value::Object(data) });
        if !top_errors.is_empty() {
            body["errors"] = Value::Array(top_errors);
        }
        ok_json(body)
    }

    fn customer_address_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let set_as_default = resolved_bool_field(&field.arguments, "setAsDefault");
        let Some((customer_first, customer_last, existing_nodes, current_default)) =
            self.customer_address_context(&customer_id)
        else {
            return (
                customer_address_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["customerId"]),
                        "Customer does not exist",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        let new_id = self.next_proxy_synthetic_gid("MailingAddress");
        let (node, errors) = customer_address_input_node(
            &address_input,
            None,
            customer_first.as_deref(),
            customer_last.as_deref(),
            &new_id,
        );
        if !errors.is_empty() {
            return (
                customer_address_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let node = node.unwrap_or(Value::Null);
        let new_key = customer_address_dedup_key(&node);
        if existing_nodes
            .iter()
            .any(|existing| customer_address_dedup_key(existing) == new_key)
        {
            return (
                customer_address_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["address"]),
                        "Address already exists",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let was_empty = existing_nodes.is_empty();
        let mut nodes = existing_nodes;
        nodes.push(node.clone());
        let default_id = if set_as_default == Some(true) || was_empty {
            Some(new_id.clone())
        } else {
            current_default
        };
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        (
            customer_address_payload(node, Vec::new()),
            vec![new_id],
            Vec::new(),
        )
    }

    fn customer_address_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let set_as_default = resolved_bool_field(&field.arguments, "setAsDefault");
        // A nested `address.id` that is present must equal the top-level
        // `addressId`. An explicit null (key present, value null) counts as a
        // mismatch, matching Shopify; an omitted key skips the check.
        if address_input.contains_key("id")
            && resolved_string_field(&address_input, "id").as_deref() != Some(address_id.as_str())
        {
            return (
                customer_address_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["addressId"]),
                        "The id of the address does not match the id in the input",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        let context = self.customer_address_context(&customer_id);
        let index = context
            .as_ref()
            .and_then(|(_, _, nodes, _)| customer_address_node_index(nodes, &address_id));
        let Some((customer_first, customer_last, existing_nodes, current_default)) = context else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| customer_address_payload(Value::Null, errors),
            );
        };
        let Some(index) = index else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| customer_address_payload(Value::Null, errors),
            );
        };
        let (node, errors) = customer_address_input_node(
            &address_input,
            Some(&existing_nodes[index]),
            customer_first.as_deref(),
            customer_last.as_deref(),
            &address_id,
        );
        if !errors.is_empty() {
            return (
                customer_address_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let node = node.unwrap_or(Value::Null);
        let mut nodes = existing_nodes;
        nodes[index] = node.clone();
        let default_id = if set_as_default == Some(true) {
            Some(address_id.clone())
        } else {
            current_default
        };
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        (
            customer_address_payload(node, Vec::new()),
            vec![address_id],
            Vec::new(),
        )
    }

    fn customer_address_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let context = self.customer_address_context(&customer_id);
        let index = context
            .as_ref()
            .and_then(|(_, _, nodes, _)| customer_address_node_index(nodes, &address_id));
        let Some((_, _, existing_nodes, current_default)) = context else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| json!({ "deletedAddressId": Value::Null, "userErrors": errors }),
            );
        };
        let Some(index) = index else {
            return self.customer_address_missing_result(
                &address_id,
                &field.response_key,
                |errors| json!({ "deletedAddressId": Value::Null, "userErrors": errors }),
            );
        };
        let was_default = current_default.as_deref() == Some(address_id.as_str());
        let mut nodes = existing_nodes;
        nodes.remove(index);
        // Deleting the default promotes the first remaining address; deleting a
        // non-default leaves the default untouched.
        let default_id = if was_default {
            nodes
                .first()
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            current_default
        };
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        (
            json!({ "deletedAddressId": address_id, "userErrors": [] }),
            Vec::new(),
            Vec::new(),
        )
    }

    fn customer_update_default_address(
        &mut self,
        field: &RootFieldSelection,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let customer_id = resolved_string_field(&field.arguments, "customerId").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let context = self.customer_address_context(&customer_id);
        let index = context
            .as_ref()
            .and_then(|(_, _, nodes, _)| customer_address_node_index(nodes, &address_id));
        // Return the full staged customer record; the field's `customer`
        // sub-selection is applied by `selected_json` at the call site.
        let render_customer = |me: &Self| {
            me.store
                .staged
                .customers
                .get(&customer_id)
                .cloned()
                .unwrap_or(Value::Null)
        };
        let Some((_, _, existing_nodes, _)) = context else {
            // Unknown customer: treat the address as not found.
            if self.customer_address_exists_anywhere(&address_id) {
                let customer = render_customer(self);
                return (
                    json!({
                        "customer": customer,
                        "userErrors": [customer_user_error(json!(["addressId"]), "Address does not exist")]
                    }),
                    Vec::new(),
                    Vec::new(),
                );
            }
            return (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(
                    &field.response_key,
                )],
            );
        };
        let Some(index) = index else {
            // Address belongs to another customer (exists somewhere) → userError,
            // but the customer record is still returned. Truly unknown ids return
            // a null payload with a RESOURCE_NOT_FOUND top-level error.
            if self.customer_address_exists_anywhere(&address_id) {
                let customer = render_customer(self);
                return (
                    json!({
                        "customer": customer,
                        "userErrors": [customer_user_error(json!(["addressId"]), "Address does not exist")]
                    }),
                    Vec::new(),
                    Vec::new(),
                );
            }
            return (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(
                    &field.response_key,
                )],
            );
        };
        let default_id = existing_nodes[index]
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            let nodes = existing_nodes;
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
        }
        let customer = render_customer(self);
        (
            json!({ "customer": customer, "userErrors": [] }),
            Vec::new(),
            Vec::new(),
        )
    }

    /// Snapshot the inline-address context for a staged customer:
    /// `(firstName, lastName, addressesV2.nodes, defaultAddress.id)`. Returns
    /// `None` when the customer is not staged locally. Extracting clones here
    /// ends the immutable borrow so callers can subsequently mint ids / take a
    /// mutable borrow of the same customer.
    fn customer_address_context(&self, customer_id: &str) -> Option<CustomerAddressContext> {
        let customer = self.store.staged.customers.get(customer_id)?;
        let first = customer
            .get("firstName")
            .and_then(Value::as_str)
            .map(str::to_string);
        let last = customer
            .get("lastName")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some((
            first,
            last,
            customer_address_nodes(customer),
            customer_default_address_id(customer),
        ))
    }

    fn customer_address_exists_anywhere(&self, address_id: &str) -> bool {
        self.store.staged.customers.values().any(|customer| {
            customer_address_node_index(&customer_address_nodes(customer), address_id).is_some()
        })
    }

    /// Shared "addressId not present on this customer" branch for update/delete.
    /// An address that exists on *another* customer yields an "Address does not
    /// exist" user error in the payload shape built by `build_payload`; an id
    /// that exists nowhere yields a null payload + RESOURCE_NOT_FOUND.
    fn customer_address_missing_result(
        &self,
        address_id: &str,
        response_key: &str,
        build_payload: impl Fn(Vec<Value>) -> Value,
    ) -> (Value, Vec<String>, Vec<Value>) {
        if self.customer_address_exists_anywhere(address_id) {
            (
                build_payload(vec![customer_user_error(
                    json!(["addressId"]),
                    "Address does not exist",
                )]),
                Vec::new(),
                Vec::new(),
            )
        } else {
            (
                Value::Null,
                Vec::new(),
                vec![customer_address_resource_not_found_error(response_key)],
            )
        }
    }

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
        let Some(existing) = self.customer_existing_for_update(request, &id) else {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["id"]),
                        "Customer does not exist",
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
        let mut payload = if id.is_empty() || !self.customer_exists_for_mutation(request, &id) {
            json!({
                "deletedCustomerId": null,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": [{ "field": ["id"], "message": "Customer can't be found" }]
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
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
                "userErrors": [{
                    "field": ["id"],
                    "message": "Customer can’t be deleted because they have associated orders"
                }]
            })
        } else {
            self.store.staged.customers.remove(&id);
            self.store.staged.customers.tombstone(id.clone());
            json!({
                "deletedCustomerId": id,
                "shop": { "id": "gid://shopify/Shop/1?shopify-draft-proxy=synthetic" },
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
                    vec![customer_user_error_with_code(
                        json!(["input"]),
                        "The id field is not allowed if identifier is provided.",
                        "ID_NOT_ALLOWED",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }

        if let Some(identifier) = identifier.as_ref() {
            if let Some(id) = resolved_string_field(identifier, "id") {
                let Some(existing) = self.customer_existing_for_update(request, &id) else {
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
                    find_customer_id_by_email,
                );
            }
            if let Some(phone) = resolved_string_field(identifier, "phone") {
                let normalized_phone = normalize_customer_phone(&phone).unwrap_or(phone);
                return self.customer_set_contact_identifier_payload(
                    request,
                    "phone",
                    &normalized_phone,
                    &input,
                    find_customer_id_by_phone,
                );
            }
            if identifier.contains_key("customId") {
                return (
                    Value::Null,
                    Vec::new(),
                    vec![json!({
                            "message": "Resource matching the identifier was not found.",
                            "path": ["customerSet"],
                            "extensions": { "code": "NOT_FOUND" }
                    })],
                );
            }
        }

        self.customer_set_create_payload(request, &input)
    }

    fn customer_set_contact_identifier_payload(
        &mut self,
        request: &Request,
        identifier_field: &str,
        identifier_value: &str,
        input: &BTreeMap<String, ResolvedValue>,
        find: fn(&BTreeMap<String, Value>, &str) -> Option<String>,
    ) -> (Value, Vec<String>, Vec<Value>) {
        let input_value = resolved_string_field(input, identifier_field);
        let Some(input_value) = input_value else {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["input"]),
                        "The input field corresponding to the identifier is required.",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        };
        let normalized_input_value = if identifier_field == "phone" {
            normalize_customer_phone(&input_value).unwrap_or(input_value)
        } else {
            normalize_customer_email(&input_value).unwrap_or(input_value)
        };
        if normalized_input_value != identifier_value {
            return (
                customer_payload(
                    Value::Null,
                    vec![customer_user_error(
                        json!(["input"]),
                        "The identifier value does not match the value of the corresponding field in the input.",
                    )],
                ),
                Vec::new(),
                Vec::new(),
            );
        }
        if let Some(id) = find(&self.store.staged.customers.records, identifier_value) {
            let Some(existing) = self.customer_existing_for_update(request, &id) else {
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
            let Some(existing) = self.customer_existing_for_update(request, &id) else {
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
            self.customer_input_validation_errors(request, input, None, true);
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
        let id = self.next_customer_gid(&normalized);
        let timestamp = self.next_product_timestamp();
        let customer = customer_record_from_parts(&id, None, &normalized, &timestamp, true);
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        (customer_payload(customer, Vec::new()), vec![id], Vec::new())
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
        let (errors, normalized) =
            self.customer_input_validation_errors(request, input, Some(id), customer_set);
        if !errors.is_empty() {
            return (
                customer_payload(Value::Null, errors),
                Vec::new(),
                Vec::new(),
            );
        }
        let timestamp = self.next_product_timestamp();
        let customer =
            customer_record_from_parts(id, Some(&existing), &normalized, &timestamp, customer_set);
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
        (
            customer_payload(customer, Vec::new()),
            vec![id.to_string()],
            Vec::new(),
        )
    }

    fn customer_existing_for_update(&mut self, request: &Request, id: &str) -> Option<Value> {
        if id.is_empty() || self.store.staged.customers.is_tombstoned(id) {
            return None;
        }
        self.store
            .staged
            .customers
            .get(id)
            .cloned()
            .or_else(|| self.hydrate_customer_for_mutation(request, id))
    }

    fn customer_exists_for_mutation(&mut self, request: &Request, id: &str) -> bool {
        self.customer_existing_for_update(request, id).is_some()
    }

    /// Ensure a customer referenced by `customerMerge` is present in staged state
    /// by forwarding a hydrate upstream and observing the result. Mirrors
    /// `customer_existing_for_update`'s forward-on-miss, but *stages* the observed
    /// record so both the existence validation (`customer_exists`) and the merge
    /// body read the same customer. No-op when the customer is already staged or
    /// has been deleted/merged away.
    fn ensure_customer_hydrated_for_merge(&mut self, request: &Request, id: &str) {
        if id.is_empty()
            || self.store.staged.customers.contains_staged(id)
            || self.store.staged.customers.is_tombstoned(id)
        {
            return;
        }
        if let Some(customer) = self.hydrate_customer_for_merge(request, id) {
            self.store.staged.customers.stage(id.to_string(), customer);
        }
    }

    /// Forward the richer `CustomerMergeHydrate` query and observe a customer the merge
    /// references, so the merge body reads consistent state for the customer's attached
    /// resources. Unlike `hydrate_customer_for_mutation`, this also lifts the customer's
    /// `orders` connection into the staged `customer_orders` index (preserving each order's
    /// opaque connection cursor) so the merge can transfer them to the resulting customer and
    /// downstream order reads window/cursor them like locally-created orders. Returns the
    /// staged customer record (metafields/addresses retained) or `None` for a missing
    /// customer / snapshot mode.
    fn hydrate_customer_for_merge(&mut self, request: &Request, id: &str) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_MERGE_HYDRATE_QUERY,
                "operationName": "CustomerMergeHydrate",
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
        let orders = customer_merge_extract_order_records(id, &customer["orders"]);
        if !orders.is_empty() {
            self.store
                .staged
                .customer_orders
                .insert(id.to_string(), orders);
        }
        let mut record = normalize_hydrated_customer_record(customer);
        // The orders connection is served from `customer_orders`; drop the raw hydrate edges
        // so the stored record keeps the canonical staged-customer shape.
        if let Some(object) = record.as_object_mut() {
            object.remove("orders");
        }
        Some(record)
    }

    fn customer_input_validation_errors(
        &self,
        request: &Request,
        input: &BTreeMap<String, ResolvedValue>,
        current_id: Option<&str>,
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
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "email"),
                        "Email has already been taken",
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
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "email"),
                        "Email is too long (maximum is 255 characters)",
                    ));
                }
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "email"),
                    "Email is invalid",
                ));
            }
        } else if input
            .get("email")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.email = Some(None);
        }

        if let Some(raw_phone) = resolved_string_field(input, "phone") {
            if raw_phone.trim().is_empty() {
                normalized.phone = Some(None);
            } else if let Some(phone) = normalize_customer_phone(&raw_phone) {
                if self.customer_phone_taken(request, current_id, &phone) {
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "phone"),
                        "Phone has already been taken",
                    ));
                }
                normalized.phone = Some(Some(phone));
            } else {
                if raw_phone.trim().chars().count() > 255 {
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, "phone"),
                        "Phone is too long (maximum is 255 characters)",
                    ));
                }
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "phone"),
                    "Phone is invalid",
                ));
            }
        } else if input
            .get("phone")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.phone = Some(None);
        }

        if let Some(raw_locale) = resolved_string_field(input, "locale") {
            if raw_locale.trim().is_empty() {
                normalized.locale = Some(None);
            } else if let Some(locale) = normalize_shopify_locale(raw_locale.trim()) {
                normalized.locale = Some(Some(locale));
            } else {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "locale"),
                    "Locale is invalid",
                ));
            }
        } else if input
            .get("locale")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
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
                    errors.push(customer_user_error(
                        customer_field_path(customer_set, field),
                        message,
                    ));
                }
                let normalized_value = blank_string_to_option(value.trim().to_string());
                if field == "firstName" {
                    normalized.first_name = Some(normalized_value);
                } else {
                    normalized.last_name = Some(normalized_value);
                }
            } else if input
                .get(field)
                .is_some_and(|value| matches!(value, ResolvedValue::Null))
            {
                if field == "firstName" {
                    normalized.first_name = Some(None);
                } else {
                    normalized.last_name = Some(None);
                }
            }
        }

        if let Some(note) = resolved_string_field(input, "note") {
            if note.chars().count() > 5000 {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "note"),
                    "Note is too long (maximum is 5000 characters)",
                ));
            }
            normalized.note = Some(Some(note));
        } else if input
            .get("note")
            .is_some_and(|value| matches!(value, ResolvedValue::Null))
        {
            normalized.note = Some(None);
        }

        if input.contains_key("tags") {
            let tags = raw_taggable_tags_argument(input.get("tags"));
            if tags.iter().any(|tag| tag.chars().count() > 255) {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "tags"),
                    "Tags is too long (maximum is 255 characters)",
                ));
            }
            let normalized_tags = normalize_taggable_tags(tags);
            if normalized_tags.len() > 250 {
                errors.push(customer_user_error(
                    customer_field_path(customer_set, "tags"),
                    "Tags cannot be more than 250",
                ));
            }
            normalized.tags = Some(normalized_tags);
        }

        if input.contains_key("taxExempt") {
            match input.get("taxExempt") {
                Some(ResolvedValue::Bool(value)) => normalized.tax_exempt = Some(*value),
                Some(ResolvedValue::Null) if customer_set => errors.push(customer_user_error(
                    json!(["input", "taxExempt"]),
                    "Tax exempt is of unexpected type NilClass",
                )),
                _ => {}
            }
        }
        if input.contains_key("taxExemptions") {
            normalized.tax_exemptions =
                Some(resolved_string_list_field_unsorted(input, "taxExemptions"));
        }
        if input.contains_key("metafields") {
            normalized.loyalty = Some(customer_loyalty_metafield(input));
        }
        if let Some(ResolvedValue::List(address_values)) = input.get("addresses") {
            let (addresses, address_errors) =
                customer_mailing_addresses(address_values, customer_set);
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
    ) -> Option<Value> {
        if self.config.read_mode == ReadMode::Snapshot {
            return None;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_HYDRATE_QUERY,
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
        for field_name in ["emailMarketingConsent", "smsMarketingConsent"] {
            let Some(consent) = resolved_object_field(input, field_name) else {
                continue;
            };
            if resolved_string_field(&consent, "marketingState").as_deref() == Some("REDACTED") {
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
                    vec![customer_user_error(
                        json!(["emailMarketingConsent"]),
                        "An email address is required to set the email marketing consent state.",
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
                    vec![customer_user_error(
                        json!(["smsMarketingConsent"]),
                        "A phone number is required to set the SMS consent state.",
                    )],
                ),
                Vec::new(),
            ));
        }
        None
    }
}

/// Hydration query for the store-wide `customersCount` baseline used by the
/// `customer*TaxExemptions` / marketing-consent downstream reads in LiveHybrid
/// mode. Mirrors the per-resource hydrate queries; the count is cached into
/// `customers_count_base` so subsequent reads track deletions generically.
const CUSTOMER_COUNT_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/customers/customer-count-hydrate.graphql");

impl DraftProxy {
    /// `customerAddTaxExemptions` / `customerRemoveTaxExemptions` /
    /// `customerReplaceTaxExemptions`: stage the resulting tax-exemption set onto
    /// the staged (or hydrated) customer and project the requested selection.
    /// Enum validation (`customer_tax_exemptions_invalid_enum_response`) runs in
    /// the dispatcher before this, so every field here carries valid exemptions.
    pub(in crate::proxy) fn customer_tax_exemptions_mutation_response(
        &mut self,
        fields: &[RootFieldSelection],
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in fields {
            let (payload, staged_id) = self.customer_tax_exemptions_field_payload(field, request);
            if let Some(id) = staged_id {
                self.record_mutation_log_entry(request, query, variables, &field.name, vec![id]);
            }
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
        ok_json(json!({ "data": Value::Object(data) }))
    }

    fn customer_tax_exemptions_field_payload(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
    ) -> (Value, Option<String>) {
        let customer_id = resolved_string_arg(&field.arguments, "customerId").unwrap_or_default();
        if customer_id.is_empty() || self.store.staged.customers.is_tombstoned(&customer_id) {
            return (
                customer_tax_exemptions_payload(
                    Value::Null,
                    vec![customer_tax_exemptions_user_error()],
                ),
                None,
            );
        }
        if !self.store.staged.customers.contains_key(&customer_id) {
            self.taggable_resource_staged_or_hydrated("Customer", &customer_id, request);
        }
        if !self.store.staged.customers.contains_key(&customer_id) {
            return (
                customer_tax_exemptions_payload(
                    Value::Null,
                    vec![customer_tax_exemptions_user_error()],
                ),
                None,
            );
        }

        let tax_exemptions = normalize_customer_tax_exemptions(
            resolved_string_list_field_unsorted(&field.arguments, "taxExemptions"),
        );
        let mut customer = self
            .store
            .staged
            .customers
            .get(&customer_id)
            .cloned()
            .unwrap_or(Value::Null);
        let existing = customer_tax_exemptions(&customer);
        let next = match field.name.as_str() {
            "customerAddTaxExemptions" => add_customer_tax_exemptions(existing, tax_exemptions),
            "customerRemoveTaxExemptions" => {
                remove_customer_tax_exemptions(existing, tax_exemptions)
            }
            "customerReplaceTaxExemptions" => tax_exemptions,
            _ => existing,
        };
        customer["taxExemptions"] = json!(next);
        customer["updatedAt"] = json!("2026-04-25T01:41:06Z");
        self.store
            .staged
            .customers
            .insert(customer_id.clone(), customer.clone());

        (
            customer_tax_exemptions_payload(customer, Vec::new()),
            Some(customer_id),
        )
    }

    /// In LiveHybrid mode, hydrate the store-wide `customersCount` baseline from
    /// upstream once (cached into `customers_count_base`) so a downstream
    /// `customersCount` read served from the staged overlay reports the live
    /// total. No-op in Snapshot mode or when the baseline is already known.
    pub(in crate::proxy) fn hydrate_customers_count_for_overlay_read(&mut self, request: &Request) {
        if self.config.read_mode != ReadMode::LiveHybrid
            || self.store.staged.customers_count_base.is_some()
        {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": CUSTOMER_COUNT_HYDRATE_QUERY,
                "operationName": "CustomerCountHydrate",
                "variables": {},
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if let Some(count) = response.body["data"]["customersCount"]["count"].as_u64() {
            self.store.staged.customers_count_base = Some(count);
        }
    }

    /// `customerEmailMarketingConsentUpdate` / `customerSmsMarketingConsentUpdate`:
    /// apply the resolved consent state onto the staged (or hydrated) customer and
    /// project the requested selection, mirroring Shopify's resolver-error shapes.
    pub(in crate::proxy) fn customer_marketing_consent_update(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let Some(fields) = root_fields(query, variables) else {
            return json_error(400, "Could not parse GraphQL operation");
        };
        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in fields {
            let outcome =
                self.customer_marketing_consent_update_field(&field, request, query, variables);
            if let Some(error) = outcome.top_level_error {
                errors.push(error);
                data.insert(field.response_key.clone(), Value::Null);
            } else {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&outcome.payload, &field.selection),
                );
            }
        }

        let mut response = serde_json::Map::new();
        if !errors.is_empty() {
            response.insert("errors".to_string(), Value::Array(errors));
        }
        response.insert("data".to_string(), Value::Object(data));
        ok_json(Value::Object(response))
    }

    fn customer_marketing_consent_update_field(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> CustomerConsentOutcome {
        let is_email = field.name == "customerEmailMarketingConsentUpdate";
        let consent_key = if is_email {
            "emailMarketingConsent"
        } else {
            "smsMarketingConsent"
        };
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let customer_id = resolved_string_field(&input, "customerId").unwrap_or_default();
        let consent = resolved_object_field(&input, consent_key).unwrap_or_default();
        let marketing_state = resolved_string_field(&consent, "marketingState").unwrap_or_default();

        if matches!(marketing_state.as_str(), "NOT_SUBSCRIBED" | "REDACTED")
            || (is_email && marketing_state == "INVALID")
        {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            return CustomerConsentOutcome {
                payload: Value::Null,
                top_level_error: Some(customer_consent_invalid_state_error(
                    field,
                    &marketing_state,
                )),
            };
        }

        let Some(existing_customer) =
            self.taggable_resource_staged_or_hydrated("Customer", &customer_id, request)
        else {
            let user_error = if is_email {
                json!({
                    "field": ["input", "customerId"],
                    "message": "Customer not found",
                    "code": "INVALID"
                })
            } else {
                json!({
                    "field": Value::Null,
                    "message": "Customer not found",
                    "code": Value::Null
                })
            };
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            return CustomerConsentOutcome {
                payload: customer_consent_payload(Value::Null, vec![user_error]),
                top_level_error: None,
            };
        };

        let marketing_opt_in_level = resolved_string_field(&consent, "marketingOptInLevel")
            .unwrap_or_else(|| current_consent_opt_in_level(&existing_customer, is_email));
        let consent_updated_at = resolved_string_field(&consent, "consentUpdatedAt");

        if let Some(consent_updated_at) = consent_updated_at.as_deref() {
            if customer_consent_updated_at_is_future(consent_updated_at) {
                self.record_customer_consent_log(
                    request,
                    query,
                    variables,
                    &field.name,
                    Vec::new(),
                    "failed",
                );
                let customer = if is_email {
                    existing_customer.clone()
                } else {
                    Value::Null
                };
                return CustomerConsentOutcome {
                    payload: customer_consent_payload(
                        customer,
                        vec![customer_consent_user_error(
                            vec!["input", consent_key, "consentUpdatedAt"],
                            "Consent updated at must not be in the future",
                            "INVALID",
                        )],
                    ),
                    top_level_error: None,
                };
            }
        }

        if marketing_state == "PENDING" && marketing_opt_in_level != "CONFIRMED_OPT_IN" {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            let customer = if is_email {
                existing_customer.clone()
            } else {
                Value::Null
            };
            return CustomerConsentOutcome {
                payload: customer_consent_payload(
                    customer,
                    vec![customer_consent_user_error(
                        vec!["input", consent_key, "marketingOptInLevel"],
                        "Marketing opt in level must be confirmed opt-in for pending consent state",
                        "INVALID",
                    )],
                ),
                top_level_error: None,
            };
        }

        if !is_email && !customer_has_default_phone(&existing_customer) {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                Vec::new(),
                "failed",
            );
            return CustomerConsentOutcome {
                payload: customer_consent_payload(
                    Value::Null,
                    vec![customer_consent_user_error(
                        vec!["input", "smsMarketingConsent"],
                        "A phone number is required to set the SMS consent state.",
                        "INVALID",
                    )],
                ),
                top_level_error: None,
            };
        }

        if is_email && !customer_has_default_email(&existing_customer) {
            self.record_customer_consent_log(
                request,
                query,
                variables,
                &field.name,
                vec![customer_id],
                "staged",
            );
            return CustomerConsentOutcome {
                payload: customer_consent_payload(existing_customer, Vec::new()),
                top_level_error: None,
            };
        }

        let updated_at = consent_updated_at
            .or_else(|| current_consent_updated_at(&existing_customer, is_email))
            .unwrap_or_else(|| "2026-04-25T01:41:06Z".to_string());
        let mut customer = existing_customer;
        apply_customer_marketing_consent(
            &mut customer,
            is_email,
            &marketing_state,
            &marketing_opt_in_level,
            Some(updated_at.as_str()),
        );
        self.store
            .staged
            .customers
            .insert(customer_id.clone(), customer.clone());
        self.record_customer_consent_log(
            request,
            query,
            variables,
            &field.name,
            vec![customer_id],
            "staged",
        );
        CustomerConsentOutcome {
            payload: customer_consent_payload(customer, Vec::new()),
            top_level_error: None,
        }
    }

    fn record_customer_consent_log(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        root_field: &str,
        staged_ids: Vec<String>,
        status: &str,
    ) {
        self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        if status != "staged" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
    }
}

/// Validates the `taxExemptions` argument of the `customer*TaxExemptions`
/// mutations before any staging, mirroring Shopify's enum coercion errors:
/// invalid literals raise `argumentLiteralsIncompatible`, invalid variable
/// values raise `INVALID_VARIABLE`. Returns `None` when every value is known.
pub(in crate::proxy) fn customer_tax_exemptions_invalid_enum_response(
    query: &str,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    for field in fields {
        if !matches!(
            field.name.as_str(),
            "customerAddTaxExemptions"
                | "customerRemoveTaxExemptions"
                | "customerReplaceTaxExemptions"
        ) {
            continue;
        }
        let Some(raw_value) = field.raw_arguments.get("taxExemptions") else {
            continue;
        };
        if let Some(literal) = raw_tax_exemption_literal(raw_value) {
            return Some(ok_json(json!({
                "errors": [{
                    "message": format!("Argument 'taxExemptions' has an invalid value [{literal}]. Expected type '[TaxExemption!]'. Did you mean CA_STATUS_CARD_EXEMPTION?"),
                    "extensions": {
                        "code": "argumentLiteralsIncompatible",
                        "argumentName": "taxExemptions"
                    }
                }]
            })));
        }
        if let Some(invalid) = tax_exemption_invalid_variable(raw_value) {
            return Some(tax_exemption_invalid_variable_response(query, &invalid));
        }
    }
    None
}

fn customer_tax_exemptions_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_tax_exemptions_user_error() -> Value {
    user_error_omit_code(["customerId"], "Customer does not exist.", None)
}

fn customer_tax_exemptions(customer: &Value) -> Vec<String> {
    customer
        .get("taxExemptions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_customer_tax_exemptions(exemptions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for exemption in exemptions {
        if seen.insert(exemption.clone()) {
            normalized.push(exemption);
        }
    }
    normalized
}

fn add_customer_tax_exemptions(existing: Vec<String>, additions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut merged = Vec::new();
    for exemption in existing.into_iter().chain(additions) {
        if seen.insert(exemption.clone()) {
            merged.push(exemption);
        }
    }
    merged
}

fn remove_customer_tax_exemptions(existing: Vec<String>, removals: Vec<String>) -> Vec<String> {
    let removals = removals.into_iter().collect::<BTreeSet<_>>();
    existing
        .into_iter()
        .filter(|exemption| !removals.contains(exemption))
        .collect()
}

/// Outcome of a single `customer*MarketingConsentUpdate` root field: either a
/// projected payload (with field-level `userErrors`) or a top-level GraphQL
/// error (Shopify raises these for disallowed marketing states).
struct CustomerConsentOutcome {
    payload: Value,
    top_level_error: Option<Value>,
}

fn customer_consent_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "customer": customer,
        "userErrors": user_errors
    })
}

fn customer_consent_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn customer_consent_invalid_state_error(field: &RootFieldSelection, state: &str) -> Value {
    json!({
        "message": format!("Cannot specify {state} as a marketing state input"),
        "locations": [{ "line": field.location.line, "column": field.location.column }],
        "extensions": { "code": "INVALID" },
        "path": [field.response_key.clone()]
    })
}

fn customer_has_default_email(customer: &Value) -> bool {
    customer
        .get("defaultEmailAddress")
        .and_then(|contact| contact.get("emailAddress"))
        .and_then(Value::as_str)
        .is_some_and(|email| !email.trim().is_empty())
}

fn customer_has_default_phone(customer: &Value) -> bool {
    customer
        .get("defaultPhoneNumber")
        .and_then(|contact| contact.get("phoneNumber"))
        .and_then(Value::as_str)
        .is_some_and(|phone| !phone.trim().is_empty())
}

fn current_consent_opt_in_level(customer: &Value, is_email: bool) -> String {
    let contact_key = if is_email {
        "defaultEmailAddress"
    } else {
        "defaultPhoneNumber"
    };
    customer
        .get(contact_key)
        .and_then(|contact| contact.get("marketingOptInLevel"))
        .and_then(Value::as_str)
        .unwrap_or("SINGLE_OPT_IN")
        .to_string()
}

fn current_consent_updated_at(customer: &Value, is_email: bool) -> Option<String> {
    let contact_key = if is_email {
        "defaultEmailAddress"
    } else {
        "defaultPhoneNumber"
    };
    customer
        .get(contact_key)
        .and_then(|contact| contact.get("marketingUpdatedAt"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn customer_consent_updated_at_is_future(value: &str) -> bool {
    let Some(updated_at) = parse_rfc3339_epoch_seconds(value) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    updated_at > now.as_secs() as i64
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
    loyalty: Option<Value>,
    addresses: Option<Vec<Value>>,
}

fn customer_payload(customer: Value, user_errors: Vec<Value>) -> Value {
    json!({ "customer": customer, "userErrors": user_errors })
}

fn customer_user_error(field: Value, message: &str) -> Value {
    user_error_omit_code(field, message, None)
}

fn customer_user_error_with_code(field: Value, message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn customer_identity_user_error(field: Value) -> Value {
    customer_user_error(
        field,
        "A name, phone number, or email address must be present",
    )
}

fn customer_set_not_found_payload() -> Value {
    customer_payload(
        Value::Null,
        vec![customer_user_error_with_code(
            json!(["input"]),
            "Resource matching the identifier was not found.",
            "NOT_FOUND",
        )],
    )
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

fn normalize_customer_phone(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 255 {
        return None;
    }
    if trimmed.contains('*') {
        return Some(trimmed.to_string());
    }
    let has_plus = trimmed.starts_with('+');
    let digits = trimmed
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    if digits.len() < 10 || digits.len() > 15 {
        return None;
    }
    if has_plus {
        return Some(format!("+{digits}"));
    }
    if !has_plus && digits.len() == 10 {
        Some(format!("+1{digits}"))
    } else {
        Some(format!("+{digits}"))
    }
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
    verified_email_default: bool,
) -> Value {
    let first = customer_string_value(input.first_name.as_ref(), existing, "firstName");
    let last = customer_string_value(input.last_name.as_ref(), existing, "lastName");
    let email = customer_string_value(input.email.as_ref(), existing, "email");
    let phone = customer_string_value(input.phone.as_ref(), existing, "phone");
    let locale = customer_string_value(input.locale.as_ref(), existing, "locale")
        .or_else(|| Some("en".to_string()));
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
    let loyalty = input
        .loyalty
        .clone()
        .or_else(|| existing.and_then(|customer| customer.get("loyalty").cloned()))
        .unwrap_or(Value::Null);
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
    customer_record(CustomerRecordInput {
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
        loyalty,
        addresses,
        created_at,
        updated_at: timestamp,
    })
}

/// `customerCreate` rejects nested resource ids on creation: an `id` key inside
/// any `addresses[]` or `metafields[]` input object yields a user error and a
/// null customer. Addresses are checked before metafields so the surfaced error
/// path matches Shopify's ordering when both are present.
fn customer_create_nested_id_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    for (collection, label) in [("addresses", "address"), ("metafields", "metafield")] {
        if let Some(ResolvedValue::List(entries)) = input.get(collection) {
            for (index, entry) in entries.iter().enumerate() {
                if let ResolvedValue::Object(object) = entry {
                    if object.contains_key("id") {
                        return Some(customer_user_error(
                            json!([collection, index.to_string(), "id"]),
                            &format!("Cannot specify {label} ID on creation"),
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
    loyalty: Value,
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
fn apply_customer_marketing_consent(
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
    let metafields = if input.loyalty.is_null() {
        json!({ "nodes": [], "pageInfo": empty_page_info() })
    } else {
        json!({
            "nodes": [input.loyalty.clone()],
            "pageInfo": connection_page_info(
                false,
                false,
                Some("cursor:customer-metafield:1".to_string()),
                Some("cursor:customer-metafield:1".to_string())
            )
        })
    };
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
        "canDelete": true,
        "loyalty": input.loyalty.clone(),
        "metafield": input.loyalty,
        "metafields": metafields,
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

fn customer_address_cursor(address: &Value) -> Option<String> {
    address
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"))
}

fn customer_mailing_addresses(
    values: &[ResolvedValue],
    customer_set: bool,
) -> (Vec<Value>, Vec<Value>) {
    let mut addresses = Vec::new();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let ResolvedValue::Object(input) = value else {
            continue;
        };
        let (address, mut address_errors) = customer_mailing_address(input, index, customer_set);
        if !address_errors.is_empty() {
            errors.append(&mut address_errors);
            continue;
        }
        let mut address_key = address.clone();
        if let Some(object) = address_key.as_object_mut() {
            object.remove("id");
        }
        let key = serde_json::to_string(&address_key).unwrap_or_default();
        if seen.insert(key) {
            addresses.push(address);
        }
    }
    (addresses, errors)
}

fn customer_mailing_address(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
    customer_set: bool,
) -> (Value, Vec<Value>) {
    let mut errors = Vec::new();
    for field in [
        "firstName",
        "lastName",
        "address1",
        "address2",
        "city",
        "company",
        "zip",
        "phone",
    ] {
        if let Some(value) = customer_address_string(input, field) {
            let label = customer_address_field_label(field);
            if value.chars().count() > 255 {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} is too long (maximum is 255 characters)"),
                ));
            }
            if customer_address_contains_html(&value) {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} cannot contain HTML tags"),
                ));
            }
            if matches!(field, "city" | "zip" | "phone") && customer_address_contains_url(&value) {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} cannot contain URL"),
                ));
            }
            if customer_address_contains_emoji(&value) {
                errors.push(customer_user_error(
                    customer_address_field_path(customer_set, index, Some(field)),
                    &format!("{label} cannot contain emojis"),
                ));
            }
        }
    }

    let country_input = customer_address_string(input, "countryCode")
        .or_else(|| customer_address_string(input, "countryCodeV2"))
        .or_else(|| customer_address_string(input, "country"));
    let province_input = customer_address_string(input, "provinceCode")
        .or_else(|| customer_address_string(input, "province"));
    let country = match country_input
        .as_deref()
        .and_then(customer_country_from_input)
    {
        Some(country) => Some(country),
        None if country_input.is_some() => {
            errors.push(customer_user_error(
                customer_address_field_path(customer_set, index, Some("country")),
                "Country is invalid",
            ));
            None
        }
        None => None,
    };
    let province = match (&country, province_input.as_deref()) {
        (Some(country), Some(raw_province)) => {
            match customer_province_from_input(country.code, raw_province) {
                Some(province) => province,
                None => {
                    errors.push(customer_user_error(
                        customer_address_field_path(customer_set, index, Some("province")),
                        "Province is invalid",
                    ));
                    None
                }
            }
        }
        _ => None,
    };
    let country = country.cloned();
    let province = province.cloned();

    if !errors.is_empty() {
        return (Value::Null, errors);
    }

    let first_name = customer_address_string(input, "firstName");
    let last_name = customer_address_string(input, "lastName");
    let address1 = customer_address_string(input, "address1");
    let address2 = customer_address_string(input, "address2");
    let city = customer_address_string(input, "city");
    let company = customer_address_string(input, "company");
    let zip = customer_address_string(input, "zip");
    let phone = customer_address_string(input, "phone");
    let is_blank = [
        first_name.as_deref(),
        last_name.as_deref(),
        address1.as_deref(),
        address2.as_deref(),
        city.as_deref(),
        company.as_deref(),
        zip.as_deref(),
        phone.as_deref(),
        country.as_ref().map(|country| country.code),
        province.as_ref().map(|province| province.code),
    ]
    .into_iter()
    .flatten()
    .all(str::is_empty);
    if is_blank && !customer_set {
        return (
            Value::Null,
            vec![customer_user_error(
                customer_address_field_path(customer_set, index, None),
                "Customer address cannot be blank.",
            )],
        );
    }

    let name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let formatted_area =
        customer_formatted_area(city.as_deref(), country.as_ref(), province.as_ref());
    let id = synthetic_shopify_gid("MailingAddress", index + 1);
    (
        json!({
            "id": id,
            "firstName": first_name,
            "lastName": last_name,
            "address1": address1,
            "address2": address2,
            "city": city,
            "company": company,
            "province": province.as_ref().map(|province| province.name),
            "provinceCode": province.as_ref().map(|province| province.code),
            "country": country.as_ref().map(|country| country.name),
            "countryCodeV2": country.as_ref().map(|country| country.code),
            "zip": zip,
            "phone": phone,
            "name": if name.is_empty() { Value::Null } else { json!(name) },
            "formattedArea": formatted_area,
        }),
        Vec::new(),
    )
}

fn customer_address_payload(address: Value, user_errors: Vec<Value>) -> Value {
    json!({ "address": address, "userErrors": user_errors })
}

fn customer_address_resource_not_found_error(response_key: &str) -> Value {
    json!({
        "message": "invalid id",
        "extensions": { "code": "RESOURCE_NOT_FOUND" },
        "path": [response_key]
    })
}

fn customer_address_nodes(customer: &Value) -> Vec<Value> {
    customer
        .get("addressesV2")
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn customer_default_address_id(customer: &Value) -> Option<String> {
    customer
        .get("defaultAddress")
        .and_then(|address| address.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn customer_address_node_index(nodes: &[Value], address_id: &str) -> Option<usize> {
    nodes
        .iter()
        .position(|node| node.get("id").and_then(Value::as_str) == Some(address_id))
}

/// Identity key for duplicate detection: the full node minus its synthetic id.
/// Derived fields (`name`, `formattedArea`, `country`/`province` names) are a
/// deterministic function of the inputs, so comparing the whole node is
/// equivalent to comparing the input field-set.
fn customer_address_dedup_key(node: &Value) -> String {
    let mut node = node.clone();
    if let Some(object) = node.as_object_mut() {
        object.remove("id");
    }
    serde_json::to_string(&node).unwrap_or_default()
}

/// Rebuild a customer's inline `addressesV2` connection (nodes/edges/pageInfo)
/// and `defaultAddress` from the given ordered node list. `default_id` selects
/// which node (if any) is the default. Cursors are the deterministic
/// `cursor:<id>` form, matched leniently as `any-string` by the parity rules.
fn customer_rebuild_addresses(customer: &mut Value, nodes: Vec<Value>, default_id: Option<&str>) {
    let edges = nodes
        .iter()
        .map(|node| json!({ "cursor": customer_address_cursor(node), "node": node.clone() }))
        .collect::<Vec<_>>();
    let start_cursor = nodes.first().and_then(customer_address_cursor);
    let end_cursor = nodes.last().and_then(customer_address_cursor);
    let default_address = default_id
        .and_then(|id| {
            nodes
                .iter()
                .find(|node| node.get("id").and_then(Value::as_str) == Some(id))
        })
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(object) = customer.as_object_mut() {
        object.insert("defaultAddress".to_string(), default_address);
        object.insert(
            "addressesV2".to_string(),
            json!({
                "nodes": nodes,
                "edges": edges,
                "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
            }),
        );
    }
}

/// Build a single mailing-address node for the standalone address mutations.
///
/// Unlike `customer_mailing_address` (used for inline `customerCreate`/`Set`
/// address arrays, which key errors on `addresses[i]` and never blank-defaults),
/// this:
///   * keys validation errors on `["address", field]`,
///   * never rejects a blank address (Shopify accepts `{}`),
///   * defaults `firstName`/`lastName` to the owning customer's name when absent,
///   * merges over an `existing` node for updates (input fields override; absent
///     fields keep the stored value).
///
/// Returns `(Some(node), [])` on success or `(None, errors)` on validation
/// failure.
fn customer_address_input_node(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    customer_first: Option<&str>,
    customer_last: Option<&str>,
    id: &str,
) -> (Option<Value>, Vec<Value>) {
    let mut errors = Vec::new();
    for field in [
        "firstName",
        "lastName",
        "address1",
        "address2",
        "city",
        "company",
        "zip",
        "phone",
    ] {
        if let Some(value) = customer_address_string(input, field) {
            let label = customer_address_field_label(field);
            if value.chars().count() > 255 {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} is too long (maximum is 255 characters)"),
                ));
            }
            if customer_address_contains_html(&value) {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} cannot contain HTML tags"),
                ));
            }
            if matches!(field, "city" | "zip" | "phone") && customer_address_contains_url(&value) {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} cannot contain URL"),
                ));
            }
            if customer_address_contains_emoji(&value) {
                errors.push(customer_user_error(
                    json!(["address", field]),
                    &format!("{label} cannot contain emojis"),
                ));
            }
        }
    }

    // Effective string value for a field: input value when the key is present
    // (trimmed; empty → None), otherwise the existing node's stored value.
    let field_value = |key: &str| -> Option<String> {
        if input.contains_key(key) {
            customer_address_string(input, key)
        } else {
            existing
                .and_then(|node| node.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        }
    };

    let country_present = input.contains_key("countryCode")
        || input.contains_key("countryCodeV2")
        || input.contains_key("country");
    let country_raw = if country_present {
        customer_address_string(input, "countryCode")
            .or_else(|| customer_address_string(input, "countryCodeV2"))
            .or_else(|| customer_address_string(input, "country"))
    } else {
        existing
            .and_then(|node| node.get("countryCodeV2"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let country = match country_raw.as_deref().and_then(customer_country_from_input) {
        Some(country) => Some(country),
        None if country_raw.is_some() => {
            errors.push(customer_user_error(
                json!(["address", "country"]),
                "Country is invalid",
            ));
            None
        }
        None => None,
    };

    let province_present = input.contains_key("provinceCode") || input.contains_key("province");
    let province_raw = if province_present {
        customer_address_string(input, "provinceCode")
            .or_else(|| customer_address_string(input, "province"))
    } else {
        existing
            .and_then(|node| node.get("provinceCode"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let province = match (&country, province_raw.as_deref()) {
        (Some(country), Some(raw_province)) => {
            match customer_province_from_input(country.code, raw_province) {
                Some(province) => province,
                None => {
                    errors.push(customer_user_error(
                        json!(["address", "province"]),
                        "Province is invalid",
                    ));
                    None
                }
            }
        }
        _ => None,
    };
    let country = country.cloned();
    let province = province.cloned();

    if !errors.is_empty() {
        return (None, errors);
    }

    let first_name = field_value("firstName").or_else(|| customer_first.map(str::to_string));
    let last_name = field_value("lastName").or_else(|| customer_last.map(str::to_string));
    let address1 = field_value("address1");
    let address2 = field_value("address2");
    let city = field_value("city");
    let company = field_value("company");
    let zip = field_value("zip");
    let phone = field_value("phone");
    let name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let formatted_area =
        customer_formatted_area(city.as_deref(), country.as_ref(), province.as_ref());
    (
        Some(json!({
            "id": id,
            "firstName": first_name,
            "lastName": last_name,
            "address1": address1,
            "address2": address2,
            "city": city,
            "company": company,
            "province": province.as_ref().map(|province| province.name),
            "provinceCode": province.as_ref().map(|province| province.code),
            "country": country.as_ref().map(|country| country.name),
            "countryCodeV2": country.as_ref().map(|country| country.code),
            "zip": zip,
            "phone": phone,
            "name": if name.is_empty() { Value::Null } else { json!(name) },
            "formattedArea": formatted_area,
        })),
        Vec::new(),
    )
}

#[derive(Clone, Copy)]
struct CustomerCountry {
    code: &'static str,
    name: &'static str,
}

#[derive(Clone, Copy)]
struct CustomerProvince {
    code: &'static str,
    name: &'static str,
}

fn customer_address_string(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Option<String> {
    resolved_string_field(input, field).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn customer_address_field_label(field: &str) -> &'static str {
    match field {
        "firstName" => "First name",
        "lastName" => "Last name",
        "address1" => "Address1",
        "address2" => "Address2",
        "city" => "City",
        "company" => "Company",
        "zip" => "Zip",
        "phone" => "Phone",
        "country" | "countryCode" | "countryCodeV2" => "Country",
        "province" | "provinceCode" => "Province",
        _ => "Address",
    }
}

fn customer_address_field_path(customer_set: bool, index: usize, field: Option<&str>) -> Value {
    let mut path = if customer_set {
        vec![
            "input".to_string(),
            "addresses".to_string(),
            index.to_string(),
        ]
    } else {
        vec!["addresses".to_string(), index.to_string()]
    };
    if let Some(field) = field {
        let field = match field {
            "countryCode" | "countryCodeV2" => "country",
            "provinceCode" => "province",
            other => other,
        };
        path.push(field.to_string());
    }
    json!(path)
}

fn customer_address_contains_html(value: &str) -> bool {
    value.contains('<') || value.contains('>')
}

fn customer_address_contains_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("www.")
}

fn customer_address_contains_emoji(value: &str) -> bool {
    value
        .chars()
        .any(|c| matches!(c as u32, 0x1F300..=0x1FAFF | 0x2600..=0x27BF))
}

fn customer_country_from_input(value: &str) -> Option<&'static CustomerCountry> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }
    CUSTOMER_COUNTRIES.iter().find(|country| {
        country.code.eq_ignore_ascii_case(normalized)
            || country.name.eq_ignore_ascii_case(normalized)
    })
}

fn customer_province_from_input(
    country_code: &str,
    value: &str,
) -> Option<Option<&'static CustomerProvince>> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Some(None);
    }
    let provinces = customer_country_provinces(country_code);
    if provinces.is_empty() {
        return Some(None);
    }
    provinces
        .iter()
        .find(|province| {
            province.code.eq_ignore_ascii_case(normalized)
                || province.name.eq_ignore_ascii_case(normalized)
        })
        .map(Some)
}

fn customer_country_provinces(country_code: &str) -> &'static [CustomerProvince] {
    match country_code {
        "CA" => CUSTOMER_CANADIAN_PROVINCES,
        "US" => CUSTOMER_US_PROVINCES,
        _ => &[],
    }
}

fn customer_formatted_area(
    city: Option<&str>,
    country: Option<&CustomerCountry>,
    province: Option<&CustomerProvince>,
) -> Value {
    let Some(country) = country else {
        return Value::Null;
    };
    let city = city.filter(|city| !city.is_empty());
    let province_code = province.map(|province| province.code);
    let value = match (city, province_code) {
        (Some(city), Some(province_code)) => format!("{city} {province_code}, {}", country.name),
        (Some(city), None) if country.code == "SG" => city.to_string(),
        (Some(city), None) => format!("{city}, {}", country.name),
        (None, Some(province_code)) => format!("{province_code}, {}", country.name),
        (None, None) => country.name.to_string(),
    };
    if value.is_empty() {
        Value::Null
    } else {
        json!(value)
    }
}

const CUSTOMER_COUNTRIES: &[CustomerCountry] = &[
    CustomerCountry {
        code: "CA",
        name: "Canada",
    },
    CustomerCountry {
        code: "SG",
        name: "Singapore",
    },
    CustomerCountry {
        code: "US",
        name: "United States",
    },
];

const CUSTOMER_CANADIAN_PROVINCES: &[CustomerProvince] = &[
    CustomerProvince {
        code: "AB",
        name: "Alberta",
    },
    CustomerProvince {
        code: "BC",
        name: "British Columbia",
    },
    CustomerProvince {
        code: "MB",
        name: "Manitoba",
    },
    CustomerProvince {
        code: "NB",
        name: "New Brunswick",
    },
    CustomerProvince {
        code: "NL",
        name: "Newfoundland and Labrador",
    },
    CustomerProvince {
        code: "NS",
        name: "Nova Scotia",
    },
    CustomerProvince {
        code: "NT",
        name: "Northwest Territories",
    },
    CustomerProvince {
        code: "NU",
        name: "Nunavut",
    },
    CustomerProvince {
        code: "ON",
        name: "Ontario",
    },
    CustomerProvince {
        code: "PE",
        name: "Prince Edward Island",
    },
    CustomerProvince {
        code: "QC",
        name: "Quebec",
    },
    CustomerProvince {
        code: "SK",
        name: "Saskatchewan",
    },
    CustomerProvince {
        code: "YT",
        name: "Yukon",
    },
];

const CUSTOMER_US_PROVINCES: &[CustomerProvince] = &[
    CustomerProvince {
        code: "CA",
        name: "California",
    },
    CustomerProvince {
        code: "IL",
        name: "Illinois",
    },
    CustomerProvince {
        code: "NY",
        name: "New York",
    },
];

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

/// The full `TaxExemption` enum exposed by the Shopify Admin GraphQL schema. This is the
/// authoritative set of accepted values for `companyLocationTaxSettingsUpdate`'s exemption
/// arguments, and is also what Shopify echoes back (verbatim, comma-joined) inside the
/// `INVALID_VARIABLE` coercion error when an unknown value is supplied.
const TAX_EXEMPTION_VALUES: &[&str] = &[
    "CA_STATUS_CARD_EXEMPTION",
    "CA_BC_RESELLER_EXEMPTION",
    "CA_MB_RESELLER_EXEMPTION",
    "CA_SK_RESELLER_EXEMPTION",
    "CA_SK_VPT_RESELLER_EXEMPTION",
    "CA_NL_VPT_RESELLER_EXEMPTION",
    "CA_DIPLOMAT_EXEMPTION",
    "CA_BC_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_MB_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_NS_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_PE_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_SK_COMMERCIAL_FISHERY_EXEMPTION",
    "CA_BC_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_SK_PRODUCTION_AND_MACHINERY_EXEMPTION",
    "CA_BC_SUB_CONTRACTOR_EXEMPTION",
    "CA_SK_SUB_CONTRACTOR_EXEMPTION",
    "CA_BC_CONTRACTOR_EXEMPTION",
    "CA_SK_CONTRACTOR_EXEMPTION",
    "CA_ON_PURCHASE_EXEMPTION",
    "CA_MB_FARMER_EXEMPTION",
    "CA_NS_FARMER_EXEMPTION",
    "CA_SK_FARMER_EXEMPTION",
    "EU_REVERSE_CHARGE_EXEMPTION_RULE",
    "US_AL_RESELLER_EXEMPTION",
    "US_AK_RESELLER_EXEMPTION",
    "US_AZ_RESELLER_EXEMPTION",
    "US_AR_RESELLER_EXEMPTION",
    "US_CA_RESELLER_EXEMPTION",
    "US_CO_RESELLER_EXEMPTION",
    "US_CT_RESELLER_EXEMPTION",
    "US_DE_RESELLER_EXEMPTION",
    "US_FL_RESELLER_EXEMPTION",
    "US_GA_RESELLER_EXEMPTION",
    "US_HI_RESELLER_EXEMPTION",
    "US_ID_RESELLER_EXEMPTION",
    "US_IL_RESELLER_EXEMPTION",
    "US_IN_RESELLER_EXEMPTION",
    "US_IA_RESELLER_EXEMPTION",
    "US_KS_RESELLER_EXEMPTION",
    "US_KY_RESELLER_EXEMPTION",
    "US_LA_RESELLER_EXEMPTION",
    "US_ME_RESELLER_EXEMPTION",
    "US_MD_RESELLER_EXEMPTION",
    "US_MA_RESELLER_EXEMPTION",
    "US_MI_RESELLER_EXEMPTION",
    "US_MN_RESELLER_EXEMPTION",
    "US_MS_RESELLER_EXEMPTION",
    "US_MO_RESELLER_EXEMPTION",
    "US_MT_RESELLER_EXEMPTION",
    "US_NE_RESELLER_EXEMPTION",
    "US_NV_RESELLER_EXEMPTION",
    "US_NH_RESELLER_EXEMPTION",
    "US_NJ_RESELLER_EXEMPTION",
    "US_NM_RESELLER_EXEMPTION",
    "US_NY_RESELLER_EXEMPTION",
    "US_NC_RESELLER_EXEMPTION",
    "US_ND_RESELLER_EXEMPTION",
    "US_OH_RESELLER_EXEMPTION",
    "US_OK_RESELLER_EXEMPTION",
    "US_OR_RESELLER_EXEMPTION",
    "US_PA_RESELLER_EXEMPTION",
    "US_RI_RESELLER_EXEMPTION",
    "US_SC_RESELLER_EXEMPTION",
    "US_SD_RESELLER_EXEMPTION",
    "US_TN_RESELLER_EXEMPTION",
    "US_TX_RESELLER_EXEMPTION",
    "US_UT_RESELLER_EXEMPTION",
    "US_VT_RESELLER_EXEMPTION",
    "US_VA_RESELLER_EXEMPTION",
    "US_WA_RESELLER_EXEMPTION",
    "US_WV_RESELLER_EXEMPTION",
    "US_WI_RESELLER_EXEMPTION",
    "US_WY_RESELLER_EXEMPTION",
    "US_DC_RESELLER_EXEMPTION",
];

/// An invalid `[TaxExemption!]` variable value detected during request validation.
struct InvalidTaxExemptionVariable {
    variable_name: String,
    /// The full provided value, echoed back in `extensions.value`.
    provided: Value,
    /// `(list index, invalid value)` for every element that is not a known exemption.
    problems: Vec<(usize, String)>,
}

pub(in crate::proxy) fn b2b_tax_settings_invalid_enum_response(
    query: &str,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    for field in fields {
        if field.name != "companyLocationTaxSettingsUpdate" {
            continue;
        }
        for argument_name in ["exemptionsToAssign", "exemptionsToRemove"] {
            let Some(raw_value) = field.raw_arguments.get(argument_name) else {
                continue;
            };
            if raw_tax_exemption_literal(raw_value).is_some() {
                return Some(ok_json(json!({
                    "errors": [{
                        "message": format!("Argument '{argument_name}' has an invalid value [NOT_A_REAL_EXEMPTION]. Expected type '[TaxExemption!]'. Did you mean CA_STATUS_CARD_EXEMPTION?"),
                        "extensions": {
                            "code": "argumentLiteralsIncompatible",
                            "argumentName": argument_name
                        }
                    }]
                })));
            }
            if let Some(invalid) = tax_exemption_invalid_variable(raw_value) {
                return Some(tax_exemption_invalid_variable_response(query, &invalid));
            }
        }
    }
    None
}

fn raw_tax_exemption_literal(value: &RawArgumentValue) -> Option<&str> {
    match value {
        RawArgumentValue::Enum(value) if !is_known_tax_exemption(value) => Some(value.as_str()),
        RawArgumentValue::List(values) => values.iter().find_map(raw_tax_exemption_literal),
        _ => None,
    }
}

fn tax_exemption_invalid_variable(value: &RawArgumentValue) -> Option<InvalidTaxExemptionVariable> {
    let RawArgumentValue::Variable {
        name,
        value: Some(resolved),
    } = value
    else {
        return None;
    };
    let mut problems = Vec::new();
    if let ResolvedValue::List(items) = resolved {
        for (index, item) in items.iter().enumerate() {
            if let ResolvedValue::String(item) = item {
                if !is_known_tax_exemption(item) {
                    problems.push((index, item.clone()));
                }
            }
        }
    }
    if problems.is_empty() {
        return None;
    }
    Some(InvalidTaxExemptionVariable {
        variable_name: name.clone(),
        provided: resolved_value_json(resolved),
        problems,
    })
}

fn tax_exemption_invalid_variable_response(
    query: &str,
    invalid: &InvalidTaxExemptionVariable,
) -> Response {
    let one_of = TAX_EXEMPTION_VALUES.join(", ");
    let problems: Vec<Value> = invalid
        .problems
        .iter()
        .map(|(index, value)| {
            json!({
                "path": [index],
                "explanation": format!("Expected \"{value}\" to be one of: {one_of}"),
            })
        })
        .collect();
    let (first_index, first_value) = &invalid.problems[0];
    let declared_type = graphql_variable_definition_type(query, &invalid.variable_name)
        .unwrap_or_else(|| "[TaxExemption!]".to_string());
    let message = format!(
        "Variable ${} of type {declared_type} was provided invalid value for {first_index} (Expected \"{first_value}\" to be one of: {one_of})",
        invalid.variable_name
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) =
        graphql_variable_definition_location(query, &invalid.variable_name)
    {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": invalid.provided,
            "problems": problems,
        }),
    );
    ok_json(json!({ "errors": [Value::Object(error)] }))
}

/// Members of the `CustomerSmsMarketingState` GraphQL enum. Values outside this set
/// (e.g. `INVALID`) fail variable coercion *before* the resolver checks for
/// valid-but-disallowed input states (`NOT_SUBSCRIBED`, `REDACTED`). `INVALID` is a
/// real member of the *email* enum but not the SMS one, hence the channel-specific set.
const SMS_MARKETING_STATES: &[&str] = &[
    "NOT_SUBSCRIBED",
    "PENDING",
    "SUBSCRIBED",
    "UNSUBSCRIBED",
    "REDACTED",
];

/// Validates the `smsMarketingConsent.marketingState` enum value of
/// `customerSmsMarketingConsentUpdate` before any staging. Shopify rejects values
/// outside `CustomerSmsMarketingState` at variable-coercion time with an
/// `INVALID_VARIABLE` error, returning `None` when the state is a known member.
pub(in crate::proxy) fn customer_sms_consent_invalid_enum_response(
    query: &str,
    fields: &[RootFieldSelection],
) -> Option<Response> {
    for field in fields {
        if field.name != "customerSmsMarketingConsentUpdate" {
            continue;
        }
        let Some(RawArgumentValue::Variable {
            name,
            value: Some(resolved),
        }) = field.raw_arguments.get("input")
        else {
            continue;
        };
        let ResolvedValue::Object(input) = resolved else {
            continue;
        };
        let Some(ResolvedValue::Object(consent)) = input.get("smsMarketingConsent") else {
            continue;
        };
        let Some(ResolvedValue::String(state)) = consent.get("marketingState") else {
            continue;
        };
        if SMS_MARKETING_STATES.contains(&state.as_str()) {
            continue;
        }
        return Some(sms_consent_invalid_variable_response(
            query, name, resolved, state,
        ));
    }
    None
}

fn sms_consent_invalid_variable_response(
    query: &str,
    variable_name: &str,
    input: &ResolvedValue,
    state: &str,
) -> Response {
    let one_of = SMS_MARKETING_STATES.join(", ");
    let declared_type = graphql_variable_definition_type(query, variable_name)
        .unwrap_or_else(|| "CustomerSmsMarketingConsentUpdateInput!".to_string());
    let explanation = format!("Expected \"{state}\" to be one of: {one_of}");
    let message = format!(
        "Variable ${variable_name} of type {declared_type} was provided invalid value for smsMarketingConsent.marketingState ({explanation})"
    );
    let mut error = serde_json::Map::new();
    error.insert("message".to_string(), json!(message));
    if let Some((line, column)) = graphql_variable_definition_location(query, variable_name) {
        error.insert(
            "locations".to_string(),
            json!([{ "line": line, "column": column }]),
        );
    }
    error.insert(
        "extensions".to_string(),
        json!({
            "code": "INVALID_VARIABLE",
            "value": resolved_value_json(input),
            "problems": [{
                "path": ["smsMarketingConsent", "marketingState"],
                "explanation": explanation,
            }],
        }),
    );
    ok_json(json!({ "errors": [Value::Object(error)] }))
}

fn is_known_tax_exemption(value: &str) -> bool {
    TAX_EXEMPTION_VALUES.contains(&value)
}

fn customer_update_inline_consent_errors(input: &BTreeMap<String, ResolvedValue>) -> Vec<Value> {
    let mut errors = Vec::new();
    if input.contains_key("smsMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "smsMarketingConsent",
            "customerSmsMarketingConsentUpdate",
        ));
    }
    if input.contains_key("emailMarketingConsent") {
        errors.push(customer_update_inline_consent_error(
            "emailMarketingConsent",
            "customerEmailMarketingConsentUpdate",
        ));
    }
    errors
}

fn customer_update_inline_consent_error(field: &str, mutation: &str) -> Value {
    json!({
        "field": [field],
        "message": format!("To update {field}, please use the {mutation} Mutation instead")
    })
}

impl DraftProxy {
    pub(in crate::proxy) fn customer_merge(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> Response {
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| ("customerMerge".to_string(), Vec::new(), BTreeMap::new()));
        let one_id = resolved_string_field(&arguments, "customerOneId")
            .or_else(|| resolved_string_field(variables, "customerOneId"))
            .unwrap_or_default();
        let two_id = resolved_string_field(&arguments, "customerTwoId")
            .or_else(|| resolved_string_field(variables, "customerTwoId"))
            .unwrap_or_default();

        // Pre-existing customers referenced by a merge are resolved the real way:
        // forward a hydrate upstream and stage the observed record so both the
        // existence checks and the merge body read consistent state. Already-staged
        // or deleted/merged-away customers are left untouched (a deleted source must
        // still surface DOES_NOT_EXIST rather than be re-hydrated).
        self.ensure_customer_hydrated_for_merge(request, &one_id);
        self.ensure_customer_hydrated_for_merge(request, &two_id);

        // Compute the payload generically from staged state. State only mutates on
        // the success branch; each early return mirrors a live customerMerge
        // userError branch (self-merge, unknown customer, merge blockers).
        let (payload, staged_ids) = self.customer_merge_payload(&arguments, &one_id, &two_id);
        self.record_mutation_log_entry(request, query, variables, "customerMerge", staged_ids);
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    /// Stage a `customerRequestDataErasure` / `customerCancelDataErasure`
    /// privacy side effect locally. `request_erasure == true` is the request
    /// root; `false` is the cancel root. Records the raw mutation in the log
    /// (status `staged` on success, `failed` on userError) and never forwards
    /// upstream. Returns `{ <responseKey>: { customerId, userErrors } }`.
    pub(in crate::proxy) fn customer_data_erasure(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        root_field: &str,
        request_erasure: bool,
    ) -> Response {
        let (response_key, payload_selection, arguments) = primary_root_field(query, variables)
            .map(|field| (field.response_key, field.selection, field.arguments))
            .unwrap_or_else(|| (root_field.to_string(), Vec::new(), BTreeMap::new()));
        let customer_id = resolved_string_field(&arguments, "customerId")
            .or_else(|| resolved_string_field(variables, "customerId"))
            .unwrap_or_default();

        let (payload, status, staged_ids) =
            self.customer_data_erasure_payload(&customer_id, request_erasure);
        self.record_mutation_log_entry(request, query, variables, root_field, staged_ids);
        if status != "staged" {
            if let Some(entry) = self.log_entries.last_mut() {
                set_log_status(entry, status);
            }
        }
        ok_json(json!({ "data": { response_key: selected_json(&payload, &payload_selection) } }))
    }

    fn customer_data_erasure_payload(
        &mut self,
        customer_id: &str,
        request_erasure: bool,
    ) -> (Value, &'static str, Vec<String>) {
        if !self.customer_exists(customer_id) {
            return (
                customer_data_erasure_payload_json(
                    None,
                    vec![customer_data_erasure_user_error(
                        "Customer does not exist",
                        "DOES_NOT_EXIST",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        if request_erasure {
            self.store.staged.customer_data_erasure_requests.insert(
                customer_id.to_string(),
                json!({ "customerId": customer_id, "status": "REQUESTED" }),
            );
            return (
                customer_data_erasure_payload_json(Some(customer_id), Vec::new()),
                "staged",
                vec![customer_id.to_string()],
            );
        }
        let is_pending = self
            .store
            .staged
            .customer_data_erasure_requests
            .get(customer_id)
            .and_then(|request| request["status"].as_str())
            == Some("REQUESTED");
        if !is_pending {
            return (
                customer_data_erasure_payload_json(
                    None,
                    vec![customer_data_erasure_user_error(
                        "Customer's data is not scheduled for erasure",
                        "NOT_BEING_ERASED",
                    )],
                ),
                "failed",
                Vec::new(),
            );
        }
        self.store.staged.customer_data_erasure_requests.insert(
            customer_id.to_string(),
            json!({ "customerId": customer_id, "status": "CANCELED" }),
        );
        (
            customer_data_erasure_payload_json(Some(customer_id), Vec::new()),
            "staged",
            vec![customer_id.to_string()],
        )
    }

    fn customer_merge_payload(
        &mut self,
        arguments: &BTreeMap<String, ResolvedValue>,
        one_id: &str,
        two_id: &str,
    ) -> (Value, Vec<String>) {
        if one_id.is_empty() || two_id.is_empty() {
            return (
                customer_merge_payload_json(
                    None,
                    None,
                    vec![customer_merge_user_error(
                        Value::Null,
                        "Both customerOneId and customerTwoId are required",
                        "INVALID_CUSTOMER_ID",
                    )],
                ),
                Vec::new(),
            );
        }
        if one_id == two_id {
            return (
                customer_merge_payload_json(
                    None,
                    None,
                    vec![customer_merge_user_error(
                        Value::Null,
                        "Customers IDs should not match",
                        "INVALID_CUSTOMER_ID",
                    )],
                ),
                Vec::new(),
            );
        }
        // Shopify validates customerOneId then customerTwoId.
        if let Some(error) = self.customer_merge_unknown_error(one_id, "customerOneId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                Vec::new(),
            );
        }
        if let Some(error) = self.customer_merge_unknown_error(two_id, "customerTwoId") {
            return (
                customer_merge_payload_json(None, None, vec![error]),
                Vec::new(),
            );
        }
        let blockers = self.customer_merge_blocker_errors(one_id, two_id);
        if !blockers.is_empty() {
            return (
                customer_merge_payload_json(None, None, blockers),
                Vec::new(),
            );
        }

        let override_fields =
            resolved_object_field(arguments, "overrideFields").unwrap_or_default();
        let one = self
            .store
            .staged
            .customers
            .get(one_id)
            .cloned()
            .unwrap_or(Value::Null);
        let two = self
            .store
            .staged
            .customers
            .get(two_id)
            .cloned()
            .unwrap_or(Value::Null);
        let (result_id, source_id) =
            customer_merge_result_source_ids(one_id, &one, two_id, &two, &override_fields);
        let mut result = if result_id == one_id {
            one.clone()
        } else {
            two.clone()
        };
        let source = if source_id == one_id { one } else { two };
        apply_customer_merge_overrides(&mut result, &source, &override_fields);
        merge_customer_attached_resources(&mut result, &source);
        normalize_merged_customer_defaults(&mut result);
        // The resulting customer inherits the earliest creation date of the two
        // merged customers (it represents the older identity). ISO-8601 timestamps
        // order lexicographically, so the string min is the earlier instant.
        if let Some(source_created) = source["createdAt"].as_str() {
            let earliest = match result["createdAt"].as_str() {
                Some(result_created) => source_created.min(result_created),
                None => source_created,
            }
            .to_string();
            result["createdAt"] = json!(earliest);
        }
        result["updatedAt"] = json!(self.next_product_timestamp());

        // The resulting customer's final email (post-override) is stamped onto every
        // order transferred from the merged-away source, mirroring Shopify reparenting
        // the source's orders under the resulting customer's identity.
        let result_email = result["email"].as_str().map(str::to_string);

        self.store
            .staged
            .customers
            .insert(result_id.clone(), result);
        self.store.staged.customers.remove(&source_id);
        self.store.staged.customers.tombstone(source_id.clone());
        self.store
            .staged
            .merged_customer_ids
            .insert(source_id.clone(), result_id.clone());
        if let Some(mut source_orders) = self.store.staged.customer_orders.remove(&source_id) {
            if let Some(email) = &result_email {
                for order in &mut source_orders {
                    if order.get("email").is_some() {
                        order["email"] = json!(email);
                    }
                }
            }
            self.store
                .staged
                .customer_orders
                .entry(result_id.clone())
                .or_default()
                .extend(source_orders);
        }

        let job_id = self.next_proxy_synthetic_gid("Job");
        let merge_request = customer_merge_request_json(&job_id, &result_id, Vec::new());
        self.store
            .staged
            .customer_merge_requests
            .insert(job_id.clone(), merge_request);
        (
            customer_merge_payload_json(Some(&result_id), Some(&job_id), Vec::new()),
            vec![source_id, result_id, job_id],
        )
    }

    fn customer_merge_unknown_error(&self, id: &str, field: &str) -> Option<Value> {
        if self.customer_exists(id) {
            return None;
        }
        Some(customer_merge_user_error(
            json!([field]),
            &format!("Customer does not exist with ID {}", resource_id_tail(id)),
            "INVALID_CUSTOMER_ID",
        ))
    }

    fn customer_exists(&self, id: &str) -> bool {
        !id.is_empty()
            && self.store.staged.customers.contains_key(id)
            && !self.store.staged.customers.is_tombstoned(id)
    }

    fn customer_merge_blocker_errors(&self, one_id: &str, two_id: &str) -> Vec<Value> {
        let one = self.store.staged.customers.get(one_id);
        let two = self.store.staged.customers.get(two_id);
        let mut errors = Vec::new();
        let combined_tags = one
            .into_iter()
            .chain(two)
            .flat_map(customer_tags)
            .collect::<BTreeSet<_>>();
        if combined_tags.len() > 250 {
            errors.push(customer_merge_user_error(
                json!(["customerOneId"]),
                "Customers must have 250 tags or less.",
                "INVALID_CUSTOMER",
            ));
            errors.push(customer_merge_user_error(
                json!(["customerTwoId"]),
                "Customers must have 250 tags or less.",
                "INVALID_CUSTOMER",
            ));
        }
        let combined_note_len = one
            .and_then(|customer| customer["note"].as_str())
            .unwrap_or_default()
            .chars()
            .count()
            + two
                .and_then(|customer| customer["note"].as_str())
                .unwrap_or_default()
                .chars()
                .count();
        if combined_note_len > 5000 {
            errors.push(customer_merge_user_error(
                json!(["customerOneId"]),
                "Customer notes must be 5,000 characters or less.",
                "INVALID_CUSTOMER",
            ));
            errors.push(customer_merge_user_error(
                json!(["customerTwoId"]),
                "Customer notes must be 5,000 characters or less.",
                "INVALID_CUSTOMER",
            ));
        }
        for (id, field_name) in [(one_id, "customerOneId"), (two_id, "customerTwoId")] {
            if self.customer_has_assigned_gift_card(id) {
                let name = self
                    .store
                    .staged
                    .customers
                    .get(id)
                    .and_then(|customer| customer["displayName"].as_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Customer");
                errors.push(customer_merge_user_error(
                    json!([field_name]),
                    &format!("{name} has gift cards and can\u{2019}t be merged."),
                    "INVALID_CUSTOMER",
                ));
            }
        }
        errors
    }

    fn customer_has_assigned_gift_card(&self, customer_id: &str) -> bool {
        self.store.staged.gift_cards.values().any(|card| {
            card["customer"]["id"].as_str() == Some(customer_id)
                || card["customerId"].as_str() == Some(customer_id)
        })
    }
}

fn customer_merge_payload_json(
    resulting_customer_id: Option<&str>,
    job_id: Option<&str>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "resultingCustomerId": resulting_customer_id.map(Value::from).unwrap_or(Value::Null),
        "job": job_id
            .map(|id| json!({ "__typename": "Job", "id": id, "done": false, "query": Value::Null }))
            .unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn customer_merge_request_json(
    job_id: &str,
    resulting_customer_id: &str,
    errors: Vec<Value>,
) -> Value {
    json!({
        "__typename": "CustomerMergeRequest",
        "jobId": job_id,
        "resultingCustomerId": resulting_customer_id,
        "status": "COMPLETED",
        "customerMergeErrors": errors
    })
}

fn customer_merge_job_from_request(request: &Value) -> Value {
    json!({
        "__typename": "Job",
        "id": request["jobId"].clone(),
        "done": true,
        "query": { "__typename": "QueryRoot" }
    })
}

fn customer_merge_user_error(field: Value, message: &str, code: &str) -> Value {
    json!({
        "field": field.clone(),
        "message": message,
        "code": code,
        "errorFields": field,
        "block_type": code
    })
}

fn customer_data_erasure_payload_json(customer_id: Option<&str>, user_errors: Vec<Value>) -> Value {
    json!({
        "customerId": customer_id.map(Value::from).unwrap_or(Value::Null),
        "userErrors": user_errors
    })
}

fn customer_data_erasure_user_error(message: &str, code: &str) -> Value {
    user_error(["customerId"], message, Some(code))
}

fn customer_tags(customer: &Value) -> Vec<String> {
    customer["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.as_str().map(str::to_string))
        .collect()
}

fn customer_merge_result_source_ids(
    one_id: &str,
    one: &Value,
    two_id: &str,
    two: &Value,
    override_fields: &BTreeMap<String, ResolvedValue>,
) -> (String, String) {
    if let Some(email_customer_id) =
        resolved_string_field(override_fields, "customerIdOfEmailToKeep")
    {
        if email_customer_id == one_id {
            return (one_id.to_string(), two_id.to_string());
        }
        if email_customer_id == two_id {
            return (two_id.to_string(), one_id.to_string());
        }
    }

    let one_has_email = customer_merge_has_email(one);
    let two_has_email = customer_merge_has_email(two);
    match (one_has_email, two_has_email) {
        (true, false) => return (one_id.to_string(), two_id.to_string()),
        (false, true) => return (two_id.to_string(), one_id.to_string()),
        (false, false) => return (two_id.to_string(), one_id.to_string()),
        (true, true) => {}
    }

    let one_consent = customer_merge_email_consent_priority(one);
    let two_consent = customer_merge_email_consent_priority(two);
    match one_consent.cmp(&two_consent) {
        std::cmp::Ordering::Greater => return (one_id.to_string(), two_id.to_string()),
        std::cmp::Ordering::Less => return (two_id.to_string(), one_id.to_string()),
        std::cmp::Ordering::Equal => {}
    }

    let one_state = customer_merge_account_state_priority(one);
    let two_state = customer_merge_account_state_priority(two);
    match one_state.cmp(&two_state) {
        std::cmp::Ordering::Greater => (one_id.to_string(), two_id.to_string()),
        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
            (two_id.to_string(), one_id.to_string())
        }
    }
}

fn customer_merge_has_email(customer: &Value) -> bool {
    customer
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| {
            customer
                .pointer("/defaultEmailAddress/emailAddress")
                .and_then(Value::as_str)
        })
        .is_some_and(|email| !email.trim().is_empty())
}

fn customer_merge_email_consent_priority(customer: &Value) -> u8 {
    let state = customer
        .pointer("/defaultEmailAddress/marketingState")
        .and_then(Value::as_str)
        .or_else(|| {
            customer
                .pointer("/emailMarketingConsent/marketingState")
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    if state.eq_ignore_ascii_case("SUBSCRIBED") {
        2
    } else if state.eq_ignore_ascii_case("PENDING") {
        1
    } else {
        0
    }
}

fn customer_merge_account_state_priority(customer: &Value) -> u8 {
    let state = customer
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if state.eq_ignore_ascii_case("ENABLED") {
        2
    } else if state.eq_ignore_ascii_case("INVITED") {
        1
    } else {
        0
    }
}

/// Apply `customerMerge` override selections onto the resulting customer record.
/// `customerIdOf<Field>ToKeep` picks the source/result value for that field; note
/// and tags follow the captured precedence (explicit override, else union); the
/// display name and default contact projections are rebuilt from the resolved
/// scalar fields so downstream reads observe a consistent merged identity.
fn apply_customer_merge_overrides(
    result: &mut Value,
    source: &Value,
    override_fields: &BTreeMap<String, ResolvedValue>,
) {
    for (override_key, target_field) in [
        ("customerIdOfEmailToKeep", "email"),
        ("customerIdOfPhoneNumberToKeep", "phone"),
        ("customerIdOfFirstNameToKeep", "firstName"),
        ("customerIdOfLastNameToKeep", "lastName"),
    ] {
        let Some(target_id) = resolved_string_field(override_fields, override_key) else {
            continue;
        };
        let target = if result["id"].as_str() == Some(target_id.as_str()) {
            result.clone()
        } else if source["id"].as_str() == Some(target_id.as_str()) {
            source.clone()
        } else {
            continue;
        };
        if let Some(value) = target.get(target_field).cloned() {
            result[target_field] = value;
        }
    }
    if let Some(note) = resolved_string_field(override_fields, "note") {
        result["note"] = json!(note);
    } else if result["note"].is_null() && !source["note"].is_null() {
        result["note"] = source["note"].clone();
    }
    if let Some(ResolvedValue::List(tags)) = override_fields.get("tags") {
        let mut tags = tags
            .iter()
            .filter_map(|tag| match tag {
                ResolvedValue::String(tag) => Some(tag.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        tags.sort();
        result["tags"] = json!(tags);
    } else {
        let mut tags = customer_tags(result)
            .into_iter()
            .chain(customer_tags(source))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        tags.sort();
        result["tags"] = json!(tags);
    }
    let first = result["firstName"].as_str().unwrap_or_default();
    let last = result["lastName"].as_str().unwrap_or_default();
    result["displayName"] = json!([first, last]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" "));
    if let Some(email) = result["email"].as_str() {
        result["defaultEmailAddress"] = json!({ "emailAddress": email });
    }
    if let Some(phone) = result["phone"].as_str() {
        result["defaultPhoneNumber"] = json!({ "phoneNumber": phone });
    }
}

/// Merge the source customer's inline attached resources (addresses / metafields)
/// into the resulting customer. Addresses concatenate source-first then result;
/// metafields union by `namespace`+`key` with the resulting customer winning
/// conflicts. No-op when the source carries no such resources.
fn merge_customer_attached_resources(result: &mut Value, source: &Value) {
    let source_addresses = connection_nodes(&source["addressesV2"]);
    if !source_addresses.is_empty() {
        let mut nodes = source_addresses;
        nodes.extend(connection_nodes(&result["addressesV2"]));
        result["addressesV2"] = nodes_connection(nodes);
        if result["defaultAddress"].is_null() && !source["defaultAddress"].is_null() {
            result["defaultAddress"] = source["defaultAddress"].clone();
        }
    }
    let source_metafields = connection_nodes(&source["metafields"]);
    if !source_metafields.is_empty() {
        let existing_keys = connection_nodes(&result["metafields"])
            .iter()
            .map(metafield_identity)
            .collect::<BTreeSet<_>>();
        let mut nodes = connection_nodes(&result["metafields"]);
        for node in source_metafields {
            if !existing_keys.contains(&metafield_identity(&node)) {
                nodes.push(node);
            }
        }
        result["metafields"] = nodes_connection(nodes);
    }
}

fn connection_has_nodes(connection: &Value) -> bool {
    connection
        .get("nodes")
        .and_then(Value::as_array)
        .map(|nodes| !nodes.is_empty())
        .unwrap_or(false)
}

fn metafield_identity(node: &Value) -> String {
    format!(
        "{}:{}",
        node["namespace"].as_str().unwrap_or_default(),
        node["key"].as_str().unwrap_or_default()
    )
}

fn nodes_connection(nodes: Vec<Value>) -> Value {
    // A non-empty connection reports opaque (non-null) boundary cursors; Shopify's
    // are base64 blobs the local engine can't reconstruct, but downstream parity
    // matchers treat connection cursors as opaque (`any-string`), so a deterministic
    // per-node string (the node id) is a faithful stand-in. An empty connection
    // reports null boundary cursors, matching Shopify.
    let start_cursor = nodes.first().map(node_connection_cursor);
    let end_cursor = nodes.last().map(node_connection_cursor);
    json!({
        "nodes": nodes,
        "pageInfo": connection_page_info(false, false, start_cursor, end_cursor)
    })
}

fn node_connection_cursor(node: &Value) -> String {
    node.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Lift a customer's hydrated `orders` connection (an `edges { cursor node { … } }` page)
/// into the per-customer order records the staged `customer_orders` index expects: each node
/// carries its opaque connection `__cursor` (so downstream order reads reproduce Shopify's
/// cursors verbatim) and a `customer { id }` back-reference (so a transferred order re-stamps
/// the resulting customer's email like a locally-created order).
fn customer_merge_extract_order_records(customer_id: &str, orders: &Value) -> Vec<Value> {
    let Some(edges) = orders.get("edges").and_then(Value::as_array) else {
        return Vec::new();
    };
    edges
        .iter()
        .filter_map(|edge| {
            let node = edge.get("node")?;
            if node.is_null() {
                return None;
            }
            let mut record = node.clone();
            if let Some(object) = record.as_object_mut() {
                if let Some(cursor) = edge.get("cursor").and_then(Value::as_str) {
                    object.insert("__cursor".to_string(), json!(cursor));
                }
                if !object.contains_key("customer") {
                    object.insert("customer".to_string(), json!({ "id": customer_id }));
                }
            }
            Some(record)
        })
        .collect()
}

/// Cursor for an order node within a customer's `orders` connection. Prefers a
/// seeded opaque `__cursor` (the live Shopify connection cursor a scenario captured
/// and re-seeded, which downstream reads compare verbatim) and otherwise falls back
/// to the order id.
fn order_connection_cursor(record: &Value) -> String {
    record
        .get("__cursor")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| value_id_cursor(record))
}

/// Evaluate a (small subset of a) customer search `query` against a staged customer.
/// Supports `tag:<value>` exact tag membership and a generic case-insensitive
/// substring fallback over email / display name / first name. An absent or blank
/// query matches every customer.
fn customer_matches_query(customer: &Value, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if let Some(tag) = query.strip_prefix("tag:") {
        let tag = tag.trim().trim_matches('\'').trim_matches('"');
        return customer["tags"]
            .as_array()
            .map(|tags| tags.iter().any(|value| value.as_str() == Some(tag)))
            .unwrap_or(false);
    }
    let needle = query.to_lowercase();
    let haystack = format!(
        "{} {} {}",
        customer["email"].as_str().unwrap_or_default(),
        customer["displayName"].as_str().unwrap_or_default(),
        customer["firstName"].as_str().unwrap_or_default()
    )
    .to_lowercase();
    haystack.contains(&needle)
}

/// Surface Shopify's order-summary defaults on a freshly staged customer record:
/// `numberOfOrders` is the string `"0"`, `lastOrder` is explicitly null, and
/// `orders` is an empty connection (with the `pageInfo` shape a `first:`/`last:`
/// page selection expects). Only fills fields that are absent/null so a record
/// that already carries real order state (e.g. a seeded customer) is untouched.
fn apply_customer_order_summary_defaults(customer: &mut Value) {
    if customer.get("numberOfOrders").is_none_or(Value::is_null) {
        customer["numberOfOrders"] = json!("0");
    }
    if customer.get("lastOrder").is_none() {
        customer["lastOrder"] = Value::Null;
    }
    if customer.get("orders").is_none_or(Value::is_null) {
        customer["orders"] = empty_orders_connection();
    }
}

/// An empty `Customer.orders` connection page: no nodes/edges and null boundary
/// cursors, matching how Shopify renders the summary connection for a customer
/// with zero orders.
fn empty_orders_connection() -> Value {
    json!({
        "nodes": [],
        "edges": [],
        "pageInfo": empty_page_info()
    })
}

/// Shopify rejects a credit/debit that would push an account past this hard cap.
const STORE_CREDIT_LIMIT: f64 = 100000.0;

fn store_credit_user_error(field: &[&str], message: &str, code: &str) -> Value {
    user_error(field, message, Some(code))
}

fn store_credit_missing_id_user_error(id: &str, is_credit: bool) -> Value {
    if is_credit
        && matches!(
            shopify_gid_resource_type(id),
            Some("Customer" | "CompanyLocation")
        )
    {
        store_credit_user_error(&["id"], "Owner does not exist", "OWNER_NOT_FOUND")
    } else {
        store_credit_user_error(
            &["id"],
            "Store credit account does not exist",
            "ACCOUNT_NOT_FOUND",
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

/// Render an `f64` money amount the way Shopify serializes `MoneyV2.amount`:
/// whole values keep a single trailing zero (`"10.0"`), fractional values drop
/// trailing zeros beyond two decimal places (`"6.12"`, `"2.5"`).
fn shopify_decimal_text(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{rounded:.1}")
    } else {
        let text = format!("{rounded:.2}");
        text.trim_end_matches('0').to_string()
    }
}

fn store_credit_supported_currency(currency: &str) -> bool {
    matches!(
        currency,
        "USD" | "CAD" | "AUD" | "EUR" | "GBP" | "JPY" | "NZD"
    )
}

fn store_credit_expires_at_in_past(expires_at: &str) -> bool {
    !expires_at.is_empty() && expires_at < "2026-06-15T00:00:00Z"
}

fn normalize_merged_customer_defaults(customer: &mut Value) {
    if customer["numberOfOrders"].is_null() {
        customer["numberOfOrders"] = json!("0");
    }
    if customer["lastOrder"].is_null() {
        customer["lastOrder"] = Value::Null;
    }
    if customer["addressesV2"].is_null() {
        customer["addressesV2"] = empty_nodes_connection();
    }
    if customer["metafields"].is_null() {
        customer["metafields"] = empty_nodes_connection();
    }
}

fn empty_nodes_connection() -> Value {
    nodes_connection(Vec::new())
}

/// Basic email format validation matching Shopify's rules:
/// must contain exactly one @, with non-empty local and domain parts,
/// domain must contain a dot.
pub(in crate::proxy) fn is_valid_customer_email(email: &str) -> bool {
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    // Domain must contain a dot and not start/end with a dot
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    // No spaces allowed
    if email.contains(' ') {
        return false;
    }
    true
}
