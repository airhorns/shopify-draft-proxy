use super::*;

impl DraftProxy {
    /// Hydrate the staged store from a cassette-backed preflight before applying a
    /// web-presence mutation on a cold store, mirroring `fixed_price_mutation_preflight`.
    /// Gated on LiveHybrid so other read modes are untouched, and on a cold markets
    /// overlay so only the first mutation in a scenario seeds the baseline (later
    /// mutations operate on the already-staged records). The cassette serves recorded
    /// real Shopify `webPresences` data, which `stage_web_presence_preflight` loads
    /// into the local store — no fixture is hardcoded.
    pub(in crate::proxy) fn web_presence_mutation_preflight(
        &mut self,
        _variables: &BTreeMap<String, ResolvedValue>,
        request: &Request,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let shop_domain_missing = web_presence_shop_domain(&self.store).is_none();
        let web_presence_baseline_unknown = self.store.staged.web_presences.is_empty()
            && !self
                .store
                .staged
                .markets_dirty_families
                .contains("webPresences");
        if !shop_domain_missing && !web_presence_baseline_unknown {
            return;
        }
        self.run_markets_preflight(
            request,
            json!({
                "query": WEB_PRESENCE_PREFLIGHT_QUERY,
                "variables": { "first": WEB_PRESENCE_PREFLIGHT_FIRST },
                "operationName": "MarketsMutationPreflightHydrate",
            }),
            Self::stage_web_presence_preflight,
        );
    }

    /// Stage the baseline `webPresences` a preflight returns. Records insert only
    /// when absent so a multi-step lifecycle (create → update → delete) preserves
    /// records staged by earlier mutations instead of resetting to the baseline.
    pub(in crate::proxy) fn stage_web_presence_preflight(&mut self, body: &Value) {
        let Some(data) = body.get("data").filter(|data| data.is_object()) else {
            return;
        };
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            self.store.base.shop =
                shallow_merged_object(self.store.base.shop.clone(), shop.clone());
        }
        for record in markets_collect_records(data, "webPresences", "webPresence") {
            if let Some(id) = record_gid(&record, "MarketWebPresence") {
                self.store.staged.web_presences.entry(id).or_insert(record);
            }
        }
    }

    pub(crate) fn web_presence_mutation_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        self.web_presence_mutation_preflight(invocation.variables, invocation.request);
        let root_field = invocation.root_name;
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        let payload = match root_field {
            "webPresenceCreate" => {
                let input = resolved_object_field(&arguments, "input").unwrap_or_default();
                self.web_presence_helper_create_payload_inner(&input)
            }
            "webPresenceUpdate" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                let input = resolved_object_field(&arguments, "input").unwrap_or_default();
                self.web_presence_helper_update_payload_inner(&id, &input)
            }
            "webPresenceDelete" => {
                let id = resolved_string_field(&arguments, "id").unwrap_or_default();
                self.web_presence_delete_payload(&id)
            }
            _ => Value::Null,
        };
        let staged_id = match root_field {
            "webPresenceDelete" => payload.get("deletedId"),
            _ => payload.pointer("/webPresence/id"),
        }
        .and_then(Value::as_str)
        .map(str::to_string);
        let outcome = ResolverOutcome::value(payload);
        staged_id.map_or(outcome.clone(), |id| {
            outcome.with_log_draft(LogDraft::staged(root_field, "markets", vec![id]))
        })
    }

    /// Stage a `webPresenceDelete`. Shopify rejects deleting a presence that does
    /// not exist (`WEB_PRESENCE_NOT_FOUND`) and refuses to delete the presence that
    /// serves the shop's primary domain (`SHOP_MUST_HAVE_PRIMARY_DOMAIN_WEB_PRESENCE`).
    pub(in crate::proxy) fn web_presence_delete_payload(&mut self, id: &str) -> Value {
        let Some(record) = self.store.staged.web_presences.get(id) else {
            return market_id_payload_error(
                "deletedId",
                "The market web presence wasn't found.",
                "WEB_PRESENCE_NOT_FOUND",
            );
        };
        if web_presence_targets_shop_primary_host(&self.store, record) {
            return market_id_payload_error(
                "deletedId",
                "The shop must have a web presence that uses the primary domain.",
                "SHOP_MUST_HAVE_PRIMARY_DOMAIN_WEB_PRESENCE",
            );
        }
        self.store.staged.web_presences.remove(id);
        self.mark_markets_family_dirty("webPresences");
        json!({"deletedId": id, "userErrors": []})
    }

    fn web_presence_helper_create_payload_inner(
        &mut self,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let mut errors = Vec::new();
        let primary_locale = self.localization_primary_locale();
        let mut draft =
            web_presence_draft_from_input(input, None, &mut errors, true, &primary_locale);
        let linked_domain = draft
            .domain_id
            .as_deref()
            .and_then(|id| self.store.domain_by_id(id));
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.store.staged.web_presences,
            None,
            true,
            linked_domain.as_ref(),
            &mut errors,
        );
        if !errors.is_empty() {
            return payload_error("webPresence", errors);
        }
        let id = self.next_markets_family_synthetic_gid("MarketWebPresence");
        draft.id = id.clone();
        let shop_domain = web_presence_shop_domain(&self.store);
        if linked_domain.is_none() && shop_domain.is_none() {
            return web_presence_domain_context_unavailable_payload();
        }
        let record = market_web_presence_helper_record(
            &draft,
            shop_domain.as_deref().unwrap_or(""),
            linked_domain.as_ref(),
        );
        self.store
            .staged
            .web_presences
            .insert(id.clone(), record.clone());
        self.mark_markets_family_dirty("webPresences");
        json!({"webPresence": record, "userErrors": []})
    }

    fn web_presence_helper_update_payload_inner(
        &mut self,
        id: &str,
        input: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let Some(existing) = self.store.staged.web_presences.get(id).cloned() else {
            return market_id_payload_error(
                "webPresence",
                "The market web presence wasn't found.",
                "WEB_PRESENCE_NOT_FOUND",
            );
        };
        let mut errors = Vec::new();
        let primary_locale = self.localization_primary_locale();
        let draft = web_presence_draft_from_input(
            input,
            Some(&existing),
            &mut errors,
            false,
            &primary_locale,
        );
        let linked_domain = draft
            .domain_id
            .as_deref()
            .and_then(|id| self.store.domain_by_id(id));
        web_presence_validate_routing_and_uniqueness(
            &draft,
            input,
            &self.store.staged.web_presences,
            Some(id),
            false,
            linked_domain.as_ref(),
            &mut errors,
        );
        if !errors.is_empty() {
            return payload_error("webPresence", errors);
        }
        let shop_domain = web_presence_shop_domain(&self.store);
        if linked_domain.is_none() && shop_domain.is_none() {
            return web_presence_domain_context_unavailable_payload();
        }
        let record = market_web_presence_helper_record(
            &draft,
            shop_domain.as_deref().unwrap_or(""),
            linked_domain.as_ref(),
        );
        self.store
            .staged
            .web_presences
            .insert(id.to_string(), record.clone());
        self.mark_markets_family_dirty("webPresences");
        json!({"webPresence": record, "userErrors": []})
    }
}

fn web_presence_domain_context_unavailable_payload() -> Value {
    payload_user_error(
        "webPresence",
        market_user_error(
            vec!["input", "subfolderSuffix"],
            "Shop domain context is unavailable for subfolder web presence URL generation.",
            json!("GENERIC_ERROR"),
        ),
    )
}
