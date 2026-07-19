use super::*;

pub(in crate::proxy) fn delivery_promise_field_resolver_registrations(
) -> Vec<FieldResolverRegistration> {
    vec![FieldResolverRegistration::explicit(
        ApiSurface::Admin,
        "DeliveryPromiseParticipant",
        "owner",
        delivery_promise_participant_owner_field,
    )]
}

fn delivery_promise_participant_owner_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let owner_id = delivery_promise_participant_owner_id(invocation.parent)
        .ok_or_else(|| "DeliveryPromiseParticipant parent has no canonical owner id".to_string())?;
    let state = proxy.request_entity_load_state(ApiSurface::Admin, owner_id, Some(request));
    Ok(match state {
        crate::node_resolver_inventory::NodeLoadState::Found(entity) => entity.value,
        crate::node_resolver_inventory::NodeLoadState::KnownMissing => Value::Null,
        crate::node_resolver_inventory::NodeLoadState::NeedsHydration => {
            let type_name = shopify_gid_resource_type(owner_id).unwrap_or("Node");
            json!({ "__typename": type_name, "id": owner_id })
        }
        crate::node_resolver_inventory::NodeLoadState::UnsupportedType => Value::Null,
    })
}

const DELIVERY_PROMISE_OWNER_LIMIT: usize = 250;
const DELIVERY_PROMISE_HANDLE_MAX_LENGTH: usize = 255;
const DELIVERY_PROMISE_TIME_ZONE_MAX_LENGTH: usize = 255;

#[derive(Clone)]
enum DeliveryPromisePreparedMutation {
    ProviderUpsert(DeliveryPromiseProviderUpsertPlan),
    ParticipantsUpdate(DeliveryPromiseParticipantsUpdatePlan),
}

impl DeliveryPromisePreparedMutation {
    fn has_user_errors(&self) -> bool {
        match self {
            Self::ProviderUpsert(plan) => !plan.user_errors.is_empty(),
            Self::ParticipantsUpdate(plan) => !plan.user_errors.is_empty(),
        }
    }
}

#[derive(Clone)]
struct DeliveryPromiseProviderUpsertPlan {
    response_key: String,
    location_id: String,
    location: Option<Value>,
    active: Option<bool>,
    fulfillment_delay: Option<i64>,
    time_zone: Option<String>,
    user_errors: Vec<Value>,
}

#[derive(Clone)]
struct DeliveryPromiseParticipantsUpdatePlan {
    response_key: String,
    branded_promise_handle: String,
    owners_to_add: Vec<String>,
    owners_to_remove: Vec<String>,
    user_errors: Vec<Value>,
}

impl DraftProxy {
    pub(crate) fn delivery_promise_query_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        if self.config.read_mode == ReadMode::LiveHybrid && !self.has_delivery_promise_state() {
            let result = self.cached_or_forward_upstream_graphql_result(
                invocation.request,
                invocation.response_key,
            );
            if result.transport_succeeded {
                self.observe_delivery_promise_data(&result.data);
            }
            return result.outcome;
        }
        ResolverOutcome::value(self.delivery_promise_read_value(
            invocation.root_name,
            &resolved_arguments_from_json(&invocation.arguments),
        ))
    }

    fn has_delivery_promise_state(&self) -> bool {
        !self
            .store
            .base
            .delivery_promise_providers
            .records
            .is_empty()
            || !self
                .store
                .base
                .delivery_promise_participants
                .records
                .is_empty()
            || !self.store.staged.delivery_promise_providers.is_empty()
            || !self
                .store
                .staged
                .delivery_promise_providers
                .order
                .is_empty()
            || !self
                .store
                .staged
                .delivery_promise_providers
                .tombstones
                .is_empty()
            || !self.store.staged.delivery_promise_participants.is_empty()
            || !self
                .store
                .staged
                .delivery_promise_participants
                .order
                .is_empty()
            || !self
                .store
                .staged
                .delivery_promise_participants
                .tombstones
                .is_empty()
    }

    fn observe_delivery_promise_data(&mut self, data: &Value) {
        let mut providers = Vec::new();
        let mut participants = Vec::new();
        collect_delivery_promise_response_values(data, &mut providers, &mut participants);
        for provider in providers {
            if let Some(provider) = normalized_delivery_promise_provider_read_model(provider) {
                if let Some(id) = provider.get("id").and_then(Value::as_str) {
                    self.store
                        .base
                        .delivery_promise_providers
                        .insert(id.to_string(), provider);
                }
            }
        }
        for participant in participants {
            if let Some(participant) =
                normalized_delivery_promise_participant_read_model(participant)
            {
                if let Some(id) = participant.get("id").and_then(Value::as_str) {
                    self.store
                        .base
                        .delivery_promise_participants
                        .insert(id.to_string(), participant);
                }
            }
        }
    }

    fn delivery_promise_read_value(
        &self,
        root_name: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        match root_name {
            "deliveryPromiseProvider" => {
                let location_id =
                    resolved_string_field(arguments, "locationId").unwrap_or_default();
                self.delivery_promise_provider_by_location(&location_id)
                    .map(|provider| self.delivery_promise_provider_value(&provider))
                    .unwrap_or(Value::Null)
            }
            "deliveryPromiseParticipants" => {
                self.delivery_promise_participants_connection_value(arguments)
            }
            _ => Value::Null,
        }
    }

    pub(in crate::proxy) fn delivery_promise_mutation(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
        response_key: &str,
    ) -> BTreeMap<String, ResolverOutcome<Value>> {
        let Some(fields) = root_fields(query, variables) else {
            return BTreeMap::from([(
                response_key.to_string(),
                resolver_http_error_outcome(400, "Invalid delivery promise mutation"),
            )]);
        };
        self.delivery_promise_mutation_fields(fields, request, response_key)
    }

    pub(crate) fn delivery_promise_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let RootInvocation {
            root_name,
            response_key,
            arguments,
            request,
            ..
        } = invocation;
        let arguments = resolved_arguments_from_json(&arguments);
        let plan = match root_name {
            "deliveryPromiseProviderUpsert" => DeliveryPromisePreparedMutation::ProviderUpsert(
                self.prepare_delivery_promise_provider_upsert(
                    response_key.to_string(),
                    arguments,
                    request,
                ),
            ),
            "deliveryPromiseParticipantsUpdate" => {
                DeliveryPromisePreparedMutation::ParticipantsUpdate(
                    self.prepare_delivery_promise_participants_update(
                        response_key.to_string(),
                        arguments,
                    ),
                )
            }
            _ => {
                return resolver_http_error_outcome(
                    501,
                    format!("Unsupported delivery promise mutation {root_name}"),
                );
            }
        };
        self.execute_delivery_promise_mutations(vec![plan], response_key)
            .remove(response_key)
            .unwrap_or_else(|| ResolverOutcome::value(Value::Null))
    }

    pub(in crate::proxy) fn delivery_promise_mutation_fields(
        &mut self,
        fields: Vec<RootFieldSelection>,
        request: &Request,
        response_key: &str,
    ) -> BTreeMap<String, ResolverOutcome<Value>> {
        let mut prepared = Vec::new();
        for field in fields {
            let plan = match field.name.as_str() {
                "deliveryPromiseProviderUpsert" => DeliveryPromisePreparedMutation::ProviderUpsert(
                    self.prepare_delivery_promise_provider_upsert(
                        field.response_key,
                        field.arguments,
                        request,
                    ),
                ),
                "deliveryPromiseParticipantsUpdate" => {
                    DeliveryPromisePreparedMutation::ParticipantsUpdate(
                        self.prepare_delivery_promise_participants_update(
                            field.response_key,
                            field.arguments,
                        ),
                    )
                }
                _ => continue,
            };
            prepared.push(plan);
        }
        if prepared.is_empty() {
            return BTreeMap::from([(
                response_key.to_string(),
                resolver_http_error_outcome(501, "Unsupported delivery promise mutation"),
            )]);
        }

        self.execute_delivery_promise_mutations(prepared, response_key)
    }

    fn execute_delivery_promise_mutations(
        &mut self,
        prepared: Vec<DeliveryPromisePreparedMutation>,
        response_key: &str,
    ) -> BTreeMap<String, ResolverOutcome<Value>> {
        let has_user_errors = prepared
            .iter()
            .any(DeliveryPromisePreparedMutation::has_user_errors);
        let mut data = serde_json::Map::new();
        let mut log_drafts = Vec::new();
        for plan in prepared {
            match plan {
                DeliveryPromisePreparedMutation::ProviderUpsert(plan) => {
                    let payload = if has_user_errors {
                        delivery_promise_provider_payload_json(Value::Null, plan.user_errors)
                    } else {
                        let (provider, staged_id) =
                            self.apply_delivery_promise_provider_upsert(&plan);
                        log_drafts.push(LogDraft::staged(
                            "deliveryPromiseProviderUpsert",
                            "shipping-fulfillments",
                            vec![staged_id],
                        ));
                        delivery_promise_provider_payload_json(provider, Vec::new())
                    };
                    data.insert(plan.response_key, payload);
                }
                DeliveryPromisePreparedMutation::ParticipantsUpdate(plan) => {
                    let payload = if has_user_errors {
                        self.delivery_promise_participants_payload_json(
                            Vec::new(),
                            plan.user_errors,
                        )
                    } else {
                        let (participants, staged_ids) =
                            self.apply_delivery_promise_participants_update(&plan);
                        log_drafts.push(LogDraft::staged(
                            "deliveryPromiseParticipantsUpdate",
                            "shipping-fulfillments",
                            staged_ids,
                        ));
                        self.delivery_promise_participants_payload_json(participants, Vec::new())
                    };
                    data.insert(plan.response_key, payload);
                }
            }
        }

        let mut outcomes = data
            .into_iter()
            .map(|(response_key, value)| (response_key, ResolverOutcome::value(value)))
            .collect::<BTreeMap<_, _>>();
        outcomes
            .entry(response_key.to_string())
            .or_insert_with(|| ResolverOutcome::value(Value::Null))
            .log_drafts = log_drafts;
        outcomes
    }

    fn prepare_delivery_promise_provider_upsert(
        &mut self,
        response_key: String,
        arguments: BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) -> DeliveryPromiseProviderUpsertPlan {
        let location_id = resolved_string_field(&arguments, "locationId").unwrap_or_default();
        let location_ids = if location_id.is_empty() {
            Vec::new()
        } else {
            vec![location_id.clone()]
        };
        self.hydrate_delivery_profile_locations(&location_ids, request);
        let location = self.location_for_read(&location_id);
        let active = arguments.get("active").and_then(resolved_value_bool);
        let fulfillment_delay = resolved_int_field(&arguments, "fulfillmentDelay");
        let time_zone = resolved_string_field(&arguments, "timeZone");
        let mut user_errors = Vec::new();

        if location_id.is_empty() || location.is_none() {
            user_errors.push(delivery_promise_provider_user_error(
                ["locationId"],
                "Location does not exist.",
                "NOT_FOUND",
            ));
        } else if !location.as_ref().is_some_and(location_belongs_to_app) {
            user_errors.push(delivery_promise_provider_user_error(
                ["locationId"],
                "Location must belong to the app.",
                "MUST_BELONG_TO_APP",
            ));
        }
        if let Some(time_zone) = time_zone.as_deref() {
            if time_zone.len() > DELIVERY_PROMISE_TIME_ZONE_MAX_LENGTH {
                user_errors.push(delivery_promise_provider_user_error(
                    ["timeZone"],
                    "Time zone is too long (maximum is 255 characters)",
                    "TOO_LONG",
                ));
            } else if !delivery_promise_time_zone_is_valid(time_zone) {
                user_errors.push(delivery_promise_provider_user_error(
                    ["timeZone"],
                    "Invalid time zone.",
                    "INVALID_TIME_ZONE",
                ));
            }
        }

        DeliveryPromiseProviderUpsertPlan {
            response_key,
            location_id,
            location,
            active,
            fulfillment_delay,
            time_zone,
            user_errors,
        }
    }

    fn apply_delivery_promise_provider_upsert(
        &mut self,
        plan: &DeliveryPromiseProviderUpsertPlan,
    ) -> (Value, String) {
        let existing = self.delivery_promise_provider_by_location(&plan.location_id);
        let id = existing
            .as_ref()
            .and_then(|provider| provider.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| self.next_proxy_synthetic_gid("DeliveryPromiseProvider"));
        let active = plan
            .active
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|provider| provider.get("active").and_then(Value::as_bool))
            })
            .unwrap_or(false);
        let fulfillment_delay = plan
            .fulfillment_delay
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|provider| provider.get("fulfillmentDelay").and_then(Value::as_i64))
            })
            .unwrap_or(0);
        let time_zone = plan
            .time_zone
            .clone()
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|provider| provider.get("timeZone").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Etc/UTC".to_string());
        let location = plan
            .location
            .clone()
            .unwrap_or_else(|| json!({ "id": plan.location_id }));
        let provider = json!({
            "__typename": "DeliveryPromiseProvider",
            "id": id,
            "active": active,
            "fulfillmentDelay": fulfillment_delay,
            "timeZone": time_zone,
            "location": location
        });
        self.store
            .staged
            .delivery_promise_providers
            .stage(id.clone(), provider.clone());
        (provider, id)
    }

    fn prepare_delivery_promise_participants_update(
        &self,
        response_key: String,
        arguments: BTreeMap<String, ResolvedValue>,
    ) -> DeliveryPromiseParticipantsUpdatePlan {
        let branded_promise_handle =
            resolved_string_field(&arguments, "brandedPromiseHandle").unwrap_or_default();
        let owners_to_add =
            dedup_preserve_order(resolved_string_list_arg(&arguments, "ownersToAdd"));
        let owners_to_remove =
            dedup_preserve_order(resolved_string_list_arg(&arguments, "ownersToRemove"));
        let mut user_errors = Vec::new();

        if branded_promise_handle.trim().is_empty() {
            user_errors.push(user_error_omit_code(
                ["brandedPromiseHandle"],
                "Branded promise handle can't be blank",
                None,
            ));
        } else if branded_promise_handle.len() > DELIVERY_PROMISE_HANDLE_MAX_LENGTH {
            user_errors.push(user_error_omit_code(
                ["brandedPromiseHandle"],
                "Branded promise handle is too long (maximum is 255 characters)",
                None,
            ));
        }
        if owners_to_add.len() > DELIVERY_PROMISE_OWNER_LIMIT {
            user_errors.push(user_error_omit_code(
                ["ownersToAdd"],
                "ownersToAdd cannot contain more than 250 IDs",
                None,
            ));
        }
        if owners_to_remove.len() > DELIVERY_PROMISE_OWNER_LIMIT {
            user_errors.push(user_error_omit_code(
                ["ownersToRemove"],
                "ownersToRemove cannot contain more than 250 IDs",
                None,
            ));
        }
        for (index, owner_id) in owners_to_add.iter().enumerate() {
            if shopify_gid_resource_type(owner_id) != Some("ProductVariant")
                || self.store.product_variant_by_id(owner_id).is_none()
            {
                user_errors.push(user_error_omit_code(
                    vec![json!("ownersToAdd"), json!(index)],
                    "Owner must be an existing ProductVariant.",
                    None,
                ));
            }
        }

        DeliveryPromiseParticipantsUpdatePlan {
            response_key,
            branded_promise_handle,
            owners_to_add,
            owners_to_remove,
            user_errors,
        }
    }

    fn apply_delivery_promise_participants_update(
        &mut self,
        plan: &DeliveryPromiseParticipantsUpdatePlan,
    ) -> (Vec<Value>, Vec<String>) {
        let mut touched_ids = Vec::new();
        for owner_id in &plan.owners_to_remove {
            if let Some(participant) =
                self.delivery_promise_participant_for_owner(&plan.branded_promise_handle, owner_id)
            {
                if let Some(id) = participant.get("id").and_then(Value::as_str) {
                    self.store
                        .staged
                        .delivery_promise_participants
                        .remove_staged(id);
                    self.store
                        .staged
                        .delivery_promise_participants
                        .tombstone(id.to_string());
                    touched_ids.push(id.to_string());
                }
            }
        }
        for owner_id in &plan.owners_to_add {
            if self
                .delivery_promise_participant_for_owner(&plan.branded_promise_handle, owner_id)
                .is_some()
            {
                continue;
            }
            let id = self.next_proxy_synthetic_gid("DeliveryPromiseParticipant");
            let participant =
                delivery_promise_participant_record(&id, &plan.branded_promise_handle, owner_id);
            self.store
                .staged
                .delivery_promise_participants
                .stage(id.clone(), participant);
            touched_ids.push(id);
        }
        let participants =
            self.delivery_promise_participants_for_handle(&plan.branded_promise_handle, None);
        (participants, touched_ids)
    }

    fn delivery_promise_provider_value(&self, provider: &Value) -> Value {
        let mut provider = provider.clone();
        if let Some(location_id) = delivery_promise_provider_location_id(&provider) {
            if let Some(location) = self.location_for_read(location_id) {
                provider["location"] = location;
            }
        }
        provider
    }

    fn delivery_promise_participants_payload_json(
        &self,
        participants: Vec<Value>,
        user_errors: Vec<Value>,
    ) -> Value {
        json!({
            "promiseParticipants": participants,
            "userErrors": user_errors,
        })
    }

    fn delivery_promise_participants_connection_value(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let branded_promise_handle =
            resolved_string_field(arguments, "brandedPromiseHandle").unwrap_or_default();
        let owner_ids = resolved_string_list_arg(arguments, "ownerIds");
        let mut participants = self.delivery_promise_participants_for_handle(
            &branded_promise_handle,
            (!owner_ids.is_empty()).then_some(owner_ids.as_slice()),
        );
        if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
            participants.reverse();
        }
        let (participants, page_info) =
            connection_window(&participants, arguments, value_id_cursor);
        typed_connection_value(&participants, Value::clone, value_id_cursor, page_info)
    }

    fn delivery_promise_participants_for_handle(
        &self,
        branded_promise_handle: &str,
        owner_ids: Option<&[String]>,
    ) -> Vec<Value> {
        let owner_filter = owner_ids.map(|ids| ids.iter().collect::<BTreeSet<_>>());
        effective_records(
            &self.store.base.delivery_promise_participants,
            &self.store.staged.delivery_promise_participants,
        )
        .into_iter()
        .filter(|participant| {
            participant
                .get("brandedPromiseHandle")
                .and_then(Value::as_str)
                == Some(branded_promise_handle)
        })
        .filter(|participant| {
            owner_filter.as_ref().is_none_or(|owner_ids| {
                delivery_promise_participant_owner_id(participant)
                    .is_some_and(|owner_id| owner_ids.contains(&owner_id.to_string()))
            })
        })
        .collect()
    }

    fn delivery_promise_provider_by_id(&self, id: &str) -> Option<Value> {
        effective_get(
            &self.store.base.delivery_promise_providers,
            &self.store.staged.delivery_promise_providers,
            id,
        )
        .cloned()
    }

    fn delivery_promise_provider_by_location(&self, location_id: &str) -> Option<Value> {
        effective_find(
            &self.store.base.delivery_promise_providers,
            &self.store.staged.delivery_promise_providers,
            |provider| delivery_promise_provider_location_id(provider) == Some(location_id),
        )
        .cloned()
    }

    fn delivery_promise_participant_by_id(&self, id: &str) -> Option<Value> {
        effective_get(
            &self.store.base.delivery_promise_participants,
            &self.store.staged.delivery_promise_participants,
            id,
        )
        .cloned()
    }

    fn delivery_promise_participant_for_owner(
        &self,
        branded_promise_handle: &str,
        owner_id: &str,
    ) -> Option<Value> {
        effective_find(
            &self.store.base.delivery_promise_participants,
            &self.store.staged.delivery_promise_participants,
            |participant| {
                participant
                    .get("brandedPromiseHandle")
                    .and_then(Value::as_str)
                    == Some(branded_promise_handle)
                    && delivery_promise_participant_owner_id(participant) == Some(owner_id)
            },
        )
        .cloned()
    }

    pub(in crate::proxy) fn delivery_promise_node_value_by_id(&self, id: &str) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("DeliveryPromiseProvider") => Some(
                self.delivery_promise_provider_by_id(id)
                    .unwrap_or(Value::Null),
            ),
            Some("DeliveryPromiseParticipant") => Some(
                self.delivery_promise_participant_by_id(id)
                    .unwrap_or(Value::Null),
            ),
            _ => None,
        }
    }
}

fn delivery_promise_provider_payload_json(provider: Value, user_errors: Vec<Value>) -> Value {
    json!({
        "deliveryPromiseProvider": provider,
        "userErrors": user_errors
    })
}

fn delivery_promise_provider_user_error(
    field: impl Into<UserErrorField>,
    message: &str,
    code: &str,
) -> Value {
    user_error_typed(
        "DeliveryPromiseProviderUpsertUserError",
        field,
        message,
        Some(code),
    )
}

fn delivery_promise_participant_record(
    id: &str,
    branded_promise_handle: &str,
    owner_id: &str,
) -> Value {
    json!({
        "__typename": "DeliveryPromiseParticipant",
        "id": id,
        "brandedPromiseHandle": branded_promise_handle,
        "ownerId": owner_id,
        "ownerType": "PRODUCTVARIANT"
    })
}

fn delivery_promise_provider_location_id(provider: &Value) -> Option<&str> {
    provider
        .get("location")
        .and_then(|location| location.get("id"))
        .and_then(Value::as_str)
        .or_else(|| provider.get("locationId").and_then(Value::as_str))
}

fn delivery_promise_participant_owner_id(participant: &Value) -> Option<&str> {
    participant
        .get("ownerId")
        .and_then(Value::as_str)
        .or_else(|| {
            participant
                .get("owner")
                .and_then(|owner| owner.get("id"))
                .and_then(Value::as_str)
        })
}

fn location_belongs_to_app(location: &Value) -> bool {
    location
        .get("isFulfillmentService")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn delivery_promise_time_zone_is_valid(time_zone: &str) -> bool {
    if matches!(time_zone, "UTC" | "Etc/UTC") {
        return true;
    }
    let Some((area, name)) = time_zone.split_once('/') else {
        return false;
    };
    !area.is_empty()
        && !name.is_empty()
        && time_zone
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '+'))
}

fn dedup_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            deduped.push(value);
        }
    }
    deduped
}

fn normalized_delivery_promise_provider_read_model(provider: &Value) -> Option<Value> {
    if provider.is_null() {
        return None;
    }
    let id = provider.get("id").and_then(Value::as_str)?;
    if shopify_gid_resource_type(id) != Some("DeliveryPromiseProvider") {
        return None;
    }
    let location = provider.get("location").filter(|value| value.is_object())?;
    location.get("id").and_then(Value::as_str)?;
    Some(json!({
        "__typename": "DeliveryPromiseProvider",
        "id": id,
        "active": provider.get("active").and_then(Value::as_bool).unwrap_or(false),
        "fulfillmentDelay": provider.get("fulfillmentDelay").and_then(Value::as_i64).unwrap_or(0),
        "timeZone": provider.get("timeZone").and_then(Value::as_str).unwrap_or("Etc/UTC"),
        "location": location
    }))
}

fn normalized_delivery_promise_participant_read_model(participant: &Value) -> Option<Value> {
    if participant.is_null() {
        return None;
    }
    let id = participant.get("id").and_then(Value::as_str)?;
    if shopify_gid_resource_type(id) != Some("DeliveryPromiseParticipant") {
        return None;
    }
    let owner_id = delivery_promise_participant_owner_id(participant)?;
    let branded_promise_handle = participant
        .get("brandedPromiseHandle")
        .and_then(Value::as_str)?;
    Some(delivery_promise_participant_record(
        id,
        branded_promise_handle,
        owner_id,
    ))
}

fn collect_delivery_promise_response_values<'a>(
    value: &'a Value,
    providers: &mut Vec<&'a Value>,
    participants: &mut Vec<&'a Value>,
) {
    match value {
        Value::Object(object) => {
            if value.get("__typename").and_then(Value::as_str) == Some("DeliveryPromiseProvider")
                || (value.get("id").and_then(Value::as_str).is_some_and(|id| {
                    shopify_gid_resource_type(id) == Some("DeliveryPromiseProvider")
                }))
            {
                providers.push(value);
            }
            if value.get("__typename").and_then(Value::as_str) == Some("DeliveryPromiseParticipant")
                || (value.get("id").and_then(Value::as_str).is_some_and(|id| {
                    shopify_gid_resource_type(id) == Some("DeliveryPromiseParticipant")
                }))
            {
                participants.push(value);
            }
            for child in object.values() {
                collect_delivery_promise_response_values(child, providers, participants);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_delivery_promise_response_values(child, providers, participants);
            }
        }
        _ => {}
    }
}
