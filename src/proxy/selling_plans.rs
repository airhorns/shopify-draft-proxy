use super::*;

const MAX_SELLING_PLANS_PER_GROUP: usize = 31;
const INT32_MIN: i64 = i32::MIN as i64;
const INT32_MAX: i64 = i32::MAX as i64;
const SELLING_PLAN_GROUP_HYDRATE_NODES_QUERY: &str = r#"
query sellingPlanGroupHydrateNodes($ids: [ID!]!) {
  nodes(ids: $ids) {
    __typename
    id
    ... on SellingPlanGroup {
      appId
      name
      merchantCode
      description
      options
      position
      createdAt
      products(first: 250) {
        edges {
          cursor
          node {
          __typename
          id
          title
          handle
          status
          createdAt
          updatedAt
          variants(first: 50) {
            edges {
              cursor
              node {
              __typename
              id
              title
              sku
              barcode
              price
              compareAtPrice
              taxable
              inventoryPolicy
              inventoryQuantity
              selectedOptions { name value }
              inventoryItem { id tracked requiresShipping }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      productVariants(first: 250) {
        edges {
          cursor
          node {
          __typename
          id
          title
          sku
          barcode
          price
          compareAtPrice
          taxable
          inventoryPolicy
          inventoryQuantity
          selectedOptions { name value }
          inventoryItem { id tracked requiresShipping }
          product { id title handle status createdAt updatedAt }
          }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      sellingPlans(first: 250) {
        edges {
          cursor
          node {
          __typename
          id
          name
          description
          options
          position
          category
          createdAt
          billingPolicy {
            __typename
            ... on SellingPlanRecurringBillingPolicy { interval intervalCount minCycles maxCycles }
          }
          deliveryPolicy {
            __typename
            ... on SellingPlanRecurringDeliveryPolicy { interval intervalCount cutoff intent preAnchorBehavior }
          }
          inventoryPolicy { reserve }
          pricingPolicies {
            __typename
            ... on SellingPlanFixedPricingPolicy {
              adjustmentType
              adjustmentValue {
                __typename
                ... on SellingPlanPricingPolicyPercentageValue { percentage }
                ... on MoneyV2 { amount currencyCode }
              }
            }
            ... on SellingPlanRecurringPricingPolicy {
              afterCycle
              createdAt
              adjustmentType
              adjustmentValue {
                __typename
                ... on SellingPlanPricingPolicyPercentageValue { percentage }
                ... on MoneyV2 { amount currencyCode }
              }
            }
          }
          }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
  }
}
"#;

const SELLING_PLAN_GROUPS_CONNECTION_BASELINE_QUERY: &str = r#"
query SellingPlanGroupsConnectionBaseline(
  $first: Int!
  $after: String
  $query: String
  $sortKey: SellingPlanGroupSortKeys
) {
  sellingPlanGroupsBaseline: sellingPlanGroups(
    first: $first
    after: $after
    query: $query
    sortKey: $sortKey
  ) {
    edges { cursor node { id } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
"#;

const SELLING_PLAN_GROUP_PRODUCTS_PAGE_QUERY: &str = r#"
query SellingPlanGroupProductsPage($id: ID!, $first: Int!, $after: String) {
  sellingPlanGroup(id: $id) {
    products(first: $first, after: $after) {
      edges {
        cursor
        node {
          __typename id title handle status createdAt updatedAt
          variants(first: 50) {
            edges {
              cursor
              node {
                __typename id title sku barcode price compareAtPrice taxable
                inventoryPolicy inventoryQuantity selectedOptions { name value }
                inventoryItem { id tracked requiresShipping }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
"#;

const SELLING_PLAN_GROUP_PRODUCT_VARIANTS_PAGE_QUERY: &str = r#"
query SellingPlanGroupProductVariantsPage($id: ID!, $first: Int!, $after: String) {
  sellingPlanGroup(id: $id) {
    productVariants(first: $first, after: $after) {
      edges {
        cursor
        node {
          __typename id title sku barcode price compareAtPrice taxable
          inventoryPolicy inventoryQuantity selectedOptions { name value }
          inventoryItem { id tracked requiresShipping }
          product { id title handle status createdAt updatedAt }
        }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
"#;

const SELLING_PLAN_PRODUCT_VARIANTS_PAGE_QUERY: &str = r#"
query SellingPlanProductVariantsPage($id: ID!, $first: Int!, $after: String) {
  product(id: $id) {
    variants(first: $first, after: $after) {
      edges {
        cursor
        node {
          __typename id title sku barcode price compareAtPrice taxable
          inventoryPolicy inventoryQuantity selectedOptions { name value }
          inventoryItem { id tracked requiresShipping }
        }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
"#;

pub(in crate::proxy) fn selling_plan_field_resolver_type_policies() -> Vec<FieldResolverTypePolicy>
{
    vec![
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "SellingPlanGroup",
            "field is not yet modeled by the canonical selling-plan resolver",
        ),
        FieldResolverTypePolicy::unsupported_remaining(
            ApiSurface::Admin,
            "SellingPlan",
            "field is not yet modeled by the canonical selling-plan resolver",
        ),
    ]
}

pub(in crate::proxy) fn selling_plan_field_resolver_registrations() -> Vec<FieldResolverRegistration>
{
    let mut registrations = Vec::new();
    for field in [
        "appId",
        "createdAt",
        "description",
        "id",
        "merchantCode",
        "name",
        "options",
        "position",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "SellingPlanGroup",
            field,
        ));
    }
    for (field, handler) in [
        (
            "appliesToProduct",
            selling_plan_group_applies_to_product_field
                as crate::resolver_registry::FieldResolverHandler,
        ),
        (
            "appliesToProductVariant",
            selling_plan_group_applies_to_product_variant_field,
        ),
        (
            "appliesToProductVariants",
            selling_plan_group_applies_to_product_variants_field,
        ),
        ("products", selling_plan_group_products_field),
        ("productsCount", selling_plan_group_products_count_field),
        ("productVariants", selling_plan_group_product_variants_field),
        (
            "productVariantsCount",
            selling_plan_group_product_variants_count_field,
        ),
        ("sellingPlans", selling_plan_group_selling_plans_field),
        ("summary", selling_plan_group_summary_field),
    ] {
        registrations.push(FieldResolverRegistration::explicit(
            ApiSurface::Admin,
            "SellingPlanGroup",
            field,
            handler,
        ));
    }
    for field in [
        "billingPolicy",
        "category",
        "createdAt",
        "deliveryPolicy",
        "description",
        "id",
        "inventoryPolicy",
        "name",
        "options",
        "position",
        "pricingPolicies",
    ] {
        registrations.push(FieldResolverRegistration::property(
            ApiSurface::Admin,
            "SellingPlan",
            field,
        ));
    }
    registrations
}

fn selling_plan_group_field_record(
    proxy: &DraftProxy,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Option<SellingPlanGroupRecord> {
    let id = invocation.parent.get("id").and_then(Value::as_str)?;
    proxy.store.selling_plan_group_by_id(id).cloned()
}

fn selling_plan_field_arguments(
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> BTreeMap<String, ResolvedValue> {
    resolved_arguments_from_json(&invocation.arguments)
}

fn selling_plan_group_applies_to_product_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let id = invocation
        .arguments
        .get("productId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!(selling_plan_group_field_record(proxy, invocation)
        .is_some_and(|group| group
            .product_ids
            .iter()
            .any(|product_id| product_id == id))))
}

fn selling_plan_group_applies_to_product_variant_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let id = invocation
        .arguments
        .get("productVariantId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!(selling_plan_group_field_record(proxy, invocation)
        .is_some_and(|group| group
            .product_variant_ids
            .iter()
            .any(|variant_id| variant_id == id))))
}

fn selling_plan_group_applies_to_product_variants_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let product_id = invocation
        .arguments
        .get("productId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let variant_ids = proxy
        .store
        .product_variants_for_product(product_id)
        .into_iter()
        .map(|variant| variant.id)
        .collect::<BTreeSet<_>>();
    Ok(json!(selling_plan_group_field_record(proxy, invocation)
        .is_some_and(|group| {
            group
                .product_variant_ids
                .iter()
                .any(|id| variant_ids.contains(id))
        })))
}

fn selling_plan_group_products_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(group) = selling_plan_group_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(connection_value_with_args(
        group
            .product_ids
            .iter()
            .filter_map(|id| proxy.store.product_by_id(id))
            .map(|product| proxy.product_canonical_value(product))
            .collect(),
        &selling_plan_field_arguments(invocation),
        |node| selling_plan_relationship_cursor(&group.product_cursors, node),
    ))
}

fn selling_plan_group_product_variants_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(group) = selling_plan_group_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    Ok(connection_value_with_args(
        group
            .product_variant_ids
            .iter()
            .filter_map(|id| proxy.store.product_variant_by_id(id))
            .map(|variant| proxy.product_variant_canonical_value(variant))
            .collect(),
        &selling_plan_field_arguments(invocation),
        |node| selling_plan_relationship_cursor(&group.product_variant_cursors, node),
    ))
}

fn selling_plan_group_selling_plans_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(group) = selling_plan_group_field_record(proxy, invocation) else {
        return Ok(connection_json(Vec::new()));
    };
    let plans = group
        .selling_plans
        .iter()
        .map(selling_plan_canonical_value)
        .collect();
    Ok(connection_value_with_args(
        plans,
        &selling_plan_field_arguments(invocation),
        |node| {
            let id = value_id_cursor(node);
            group
                .selling_plans
                .iter()
                .find(|plan| plan.id == id)
                .and_then(|plan| plan.cursor.clone())
                .unwrap_or(id)
        },
    ))
}

fn selling_plan_relationship_cursor(cursors: &BTreeMap<String, String>, node: &Value) -> String {
    let id = value_id_cursor(node);
    cursors.get(&id).cloned().unwrap_or(id)
}

fn selling_plan_group_products_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(count_object(
        selling_plan_group_field_record(proxy, invocation)
            .map(|group| group.product_ids.len())
            .unwrap_or(0),
    ))
}

fn selling_plan_group_product_variants_count_field(
    proxy: &mut DraftProxy,
    _request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    Ok(count_object(
        selling_plan_group_field_record(proxy, invocation)
            .map(|group| group.product_variant_ids.len())
            .unwrap_or(0),
    ))
}

fn selling_plan_group_summary_field(
    proxy: &mut DraftProxy,
    request: &Request,
    invocation: &crate::admin_graphql::FieldResolverInvocation<'_>,
) -> Result<Value, String> {
    let Some(group) = selling_plan_group_field_record(proxy, invocation) else {
        return Ok(Value::Null);
    };
    let needs_money_format = group
        .selling_plans
        .iter()
        .flat_map(|plan| plan.pricing_policies.iter())
        .any(|policy| policy.pointer("/adjustmentValue/amount").is_some());
    if needs_money_format && proxy.store.shop_money_format().is_none() {
        proxy.hydrate_shop_pricing_state_if_missing(request, true, false);
    }
    Ok(json!(selling_plan_group_summary(
        &group,
        proxy.store.shop_money_format().as_deref(),
    )))
}

fn selling_plan_group_cursor(group: &SellingPlanGroupRecord) -> String {
    group.cursor.clone().unwrap_or_else(|| group.id.clone())
}

fn selling_plan_groups_baseline_variables(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> serde_json::Map<String, Value> {
    let mut variables = serde_json::Map::from_iter([
        ("first".to_string(), json!(250)),
        ("after".to_string(), Value::Null),
        ("query".to_string(), Value::Null),
        ("sortKey".to_string(), Value::Null),
    ]);
    for name in ["query", "sortKey"] {
        if let Some(value) = arguments.get(name) {
            variables.insert(name.to_string(), resolved_value_json(value));
        }
    }
    variables
}

fn selling_plan_relationship_page_variables(
    id: &str,
    first: usize,
) -> serde_json::Map<String, Value> {
    serde_json::Map::from_iter([
        ("id".to_string(), json!(id)),
        ("first".to_string(), json!(first)),
        ("after".to_string(), Value::Null),
    ])
}

impl DraftProxy {
    pub(crate) fn selling_plan_group_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        self.hydrate_selling_plan_group_read(&invocation, false);
        let id = invocation
            .arguments
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ResolverOutcome::value(
            self.store
                .selling_plan_group_by_id(id)
                .map(|group| selling_plan_group_canonical_value(self, group))
                .unwrap_or(Value::Null),
        )
    }

    pub(crate) fn selling_plan_groups_root(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        if self.config.read_mode == ReadMode::LiveHybrid {
            let mut outcome = self.cached_or_forward_upstream_root_outcome(
                invocation.request,
                invocation.response_key,
            );
            if !outcome.errors.is_empty() || !self.store.staged.selling_plan_groups_overlay_dirty {
                return outcome;
            }
            let baseline = self
                .complete_upstream_connection(
                    invocation.request,
                    SELLING_PLAN_GROUPS_CONNECTION_BASELINE_QUERY,
                    "SellingPlanGroupsConnectionBaseline",
                    selling_plan_groups_baseline_variables(&arguments),
                    "/data/sellingPlanGroupsBaseline",
                    None,
                )
                .or_else(|| {
                    upstream_page_is_complete_baseline(&outcome.value, &arguments)
                        .then(|| outcome.value.clone())
                });
            let Some(baseline) = baseline else {
                return outcome;
            };
            let rows = observed_connection_rows(&baseline);
            let ids = rows
                .iter()
                .filter_map(|row| {
                    row.node
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
            self.hydrate_selling_plan_group_nodes_for_observation(invocation.request, ids);
            for row in rows {
                let Some(id) = row.node.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(group) = self.store.staged.selling_plan_groups.get_mut(id) {
                    group.cursor = row.cursor;
                }
            }
            outcome.value = staged_connection_value_with_args(
                self.store.selling_plan_groups(),
                &arguments,
                selling_plan_group_search_decision,
                selling_plan_group_staged_sort_key,
                |group| selling_plan_group_canonical_value(self, group),
                selling_plan_group_cursor,
            );
            outcome.value_source = crate::admin_graphql::ResolverValueSource::Local;
            return outcome;
        }
        ResolverOutcome::value(staged_connection_value_with_args(
            self.store.selling_plan_groups(),
            &arguments,
            selling_plan_group_search_decision,
            selling_plan_group_staged_sort_key,
            |group| selling_plan_group_canonical_value(self, group),
            selling_plan_group_cursor,
        ))
    }

    fn hydrate_selling_plan_group_read(
        &mut self,
        invocation: &RootInvocation<'_>,
        connection: bool,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        if !connection {
            let id = invocation
                .arguments
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            self.hydrate_selling_plan_group_nodes_for_observation(
                invocation.request,
                vec![id.to_string()],
            );
        }
    }

    pub(in crate::proxy) fn hydrate_selling_plan_mutation_targets(
        &mut self,
        request: &Request,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }

        let mut group_ids = Vec::new();
        let mut product_ids = Vec::new();
        let mut variant_ids = Vec::new();
        match root_field {
            "sellingPlanGroupCreate" => {
                let resources = resolved_object_field(arguments, "resources").unwrap_or_default();
                product_ids.extend(list_string_field(&resources, "productIds"));
                variant_ids.extend(list_string_field(&resources, "productVariantIds"));
            }
            "sellingPlanGroupUpdate"
            | "sellingPlanGroupDelete"
            | "sellingPlanGroupAddProducts"
            | "sellingPlanGroupRemoveProducts"
            | "sellingPlanGroupAddProductVariants"
            | "sellingPlanGroupRemoveProductVariants" => {
                push_unique_string(
                    &mut group_ids,
                    resolved_string_field(arguments, "id").unwrap_or_default(),
                );
                if matches!(
                    root_field,
                    "sellingPlanGroupAddProducts" | "sellingPlanGroupRemoveProducts"
                ) {
                    product_ids.extend(list_string_field(arguments, "productIds"));
                }
                if matches!(
                    root_field,
                    "sellingPlanGroupAddProductVariants" | "sellingPlanGroupRemoveProductVariants"
                ) {
                    variant_ids.extend(list_string_field(arguments, "productVariantIds"));
                }
            }
            "productJoinSellingPlanGroups" | "productLeaveSellingPlanGroups" => {
                push_unique_string(
                    &mut product_ids,
                    resolved_string_field(arguments, "id").unwrap_or_default(),
                );
                group_ids.extend(list_string_field(arguments, "sellingPlanGroupIds"));
            }
            "productVariantJoinSellingPlanGroups" | "productVariantLeaveSellingPlanGroups" => {
                push_unique_string(
                    &mut variant_ids,
                    resolved_string_field(arguments, "id").unwrap_or_default(),
                );
                group_ids.extend(list_string_field(arguments, "sellingPlanGroupIds"));
            }
            _ => {}
        }

        self.hydrate_product_nodes_for_observation_with_request(request, product_ids);
        self.hydrate_product_nodes_for_observation_with_request(request, variant_ids);
        self.hydrate_selling_plan_group_nodes_for_observation(request, group_ids);
    }

    fn hydrate_selling_plan_group_nodes_for_observation(
        &mut self,
        request: &Request,
        ids: Vec<String>,
    ) {
        let ids = ids
            .into_iter()
            .filter(|id| !id.is_empty())
            .filter(|id| !self.store.staged.selling_plan_groups.is_tombstoned(id))
            .filter(|id| self.store.selling_plan_group_by_id(id).is_none())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }

        let response = self.upstream_post(
            request,
            json!({
                "query": SELLING_PLAN_GROUP_HYDRATE_NODES_QUERY,
                "operationName": "sellingPlanGroupHydrateNodes",
                "variables": { "ids": ids }
            }),
        );
        self.observe_selling_plan_groups_response(request, &response, false);
    }

    fn observe_selling_plan_groups_response(
        &mut self,
        request: &Request,
        response: &Response,
        replace: bool,
    ) {
        for mut group in observed_selling_plan_group_values(response) {
            if !self.complete_selling_plan_group_relationships(request, &mut group) {
                continue;
            }
            self.observe_selling_plan_group_value(&group, replace);
        }
    }

    fn complete_selling_plan_group_relationships(
        &self,
        request: &Request,
        group: &mut Value,
    ) -> bool {
        let Some(group_id) = group.get("id").and_then(Value::as_str).map(str::to_string) else {
            return false;
        };
        let Some(products) = self.complete_upstream_connection(
            request,
            SELLING_PLAN_GROUP_PRODUCTS_PAGE_QUERY,
            "SellingPlanGroupProductsPage",
            selling_plan_relationship_page_variables(&group_id, 250),
            "/data/sellingPlanGroup/products",
            group.get("products"),
        ) else {
            return false;
        };
        let mut product_rows = observed_connection_rows(&products);
        for row in &mut product_rows {
            // Registered Shopify requests always select this connection. Older
            // injected transports may omit unselected nested data entirely;
            // absence is distinct from a bounded page whose pageInfo requires
            // another request.
            if row.node.get("variants").is_none() {
                row.node["variants"] = connection_json(Vec::new());
                continue;
            }
            let Some(product_id) = row
                .node
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            let Some(variants) = self.complete_upstream_connection(
                request,
                SELLING_PLAN_PRODUCT_VARIANTS_PAGE_QUERY,
                "SellingPlanProductVariantsPage",
                selling_plan_relationship_page_variables(&product_id, 250),
                "/data/product/variants",
                row.node.get("variants"),
            ) else {
                return false;
            };
            row.node["variants"] = variants;
        }
        group["products"] = complete_connection_value(product_rows);

        let Some(product_variants) = self.complete_upstream_connection(
            request,
            SELLING_PLAN_GROUP_PRODUCT_VARIANTS_PAGE_QUERY,
            "SellingPlanGroupProductVariantsPage",
            selling_plan_relationship_page_variables(&group_id, 250),
            "/data/sellingPlanGroup/productVariants",
            group.get("productVariants"),
        ) else {
            return false;
        };
        group["productVariants"] = product_variants;
        true
    }

    fn observe_selling_plan_group_value(&mut self, value: &Value, replace: bool) {
        let Some(group) = selling_plan_group_record_from_observed_json(value) else {
            return;
        };
        if self
            .store
            .staged
            .selling_plan_groups
            .is_tombstoned(&group.id)
        {
            return;
        }
        if !replace
            && (self.store.selling_plan_group_by_id(&group.id).is_some()
                || self
                    .store
                    .selling_plan_groups()
                    .iter()
                    .any(|existing| existing.merchant_code == group.merchant_code))
        {
            return;
        }

        for product in observed_connection_nodes(value.get("products")) {
            if let Some(product_id) = product.get("id").and_then(Value::as_str) {
                for variant in product
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
            self.store.stage_observed_product_json(&product);
        }
        for variant in observed_connection_nodes(value.get("productVariants")) {
            if let Some(variant) = product_variant_state_from_observed_json(&variant) {
                self.store.stage_product_variant(variant);
            }
        }
        self.store.observe_selling_plan_group(group);
    }

    pub(crate) fn selling_plan_outcome(
        &mut self,
        invocation: RootInvocation<'_>,
    ) -> ResolverOutcome<Value> {
        let arguments = resolved_arguments_from_json(&invocation.arguments);
        self.hydrate_selling_plan_mutation_targets(
            invocation.request,
            invocation.root_name,
            &arguments,
        );
        match invocation.root_name {
            "sellingPlanGroupCreate" => {
                self.selling_plan_group_create(invocation.root_name, &arguments)
            }
            "sellingPlanGroupUpdate" => {
                self.selling_plan_group_update(invocation.root_name, &arguments)
            }
            "sellingPlanGroupDelete" => {
                self.selling_plan_group_delete(invocation.root_name, &arguments)
            }
            "sellingPlanGroupAddProducts" => self.selling_plan_group_add_resources(
                invocation.root_name,
                &arguments,
                ResourceKind::Product,
            ),
            "sellingPlanGroupAddProductVariants" => self.selling_plan_group_add_resources(
                invocation.root_name,
                &arguments,
                ResourceKind::ProductVariant,
            ),
            "sellingPlanGroupRemoveProducts" => self.selling_plan_group_remove_resources(
                invocation.root_name,
                &arguments,
                ResourceKind::Product,
            ),
            "sellingPlanGroupRemoveProductVariants" => self.selling_plan_group_remove_resources(
                invocation.root_name,
                &arguments,
                ResourceKind::ProductVariant,
            ),
            "productJoinSellingPlanGroups" => self.resource_join_leave_selling_plan_groups(
                invocation.root_name,
                &arguments,
                ResourceKind::Product,
                true,
            ),
            "productLeaveSellingPlanGroups" => self.resource_join_leave_selling_plan_groups(
                invocation.root_name,
                &arguments,
                ResourceKind::Product,
                false,
            ),
            "productVariantJoinSellingPlanGroups" => self.resource_join_leave_selling_plan_groups(
                invocation.root_name,
                &arguments,
                ResourceKind::ProductVariant,
                true,
            ),
            "productVariantLeaveSellingPlanGroups" => self.resource_join_leave_selling_plan_groups(
                invocation.root_name,
                &arguments,
                ResourceKind::ProductVariant,
                false,
            ),
            root => ResolverOutcome::error(format!(
                "No selling-plan resolver implemented for root `{root}`"
            )),
        }
    }

    fn selling_plan_group_create(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let user_errors =
            selling_plan_group_input_user_errors(&input, SellingPlanInputMode::Create);
        if !user_errors.is_empty() {
            return self.selling_plan_group_failed_outcome(
                root_field,
                None,
                user_errors,
                "Selling plan group input validation failed; original raw mutation retained for observability.",
            );
        }

        let resources = resolved_object_field(arguments, "resources").unwrap_or_default();
        let product_ids = list_string_field(&resources, "productIds");
        let product_variant_ids = list_string_field(&resources, "productVariantIds");
        if let Some(error) = self.resource_existence_error(&product_ids, ResourceKind::Product) {
            return self.selling_plan_group_failed_outcome(
                root_field,
                None,
                vec![error],
                "Selling plan group resource validation failed; original raw mutation retained for observability.",
            );
        }
        if let Some(error) =
            self.resource_existence_error(&product_variant_ids, ResourceKind::ProductVariant)
        {
            return self.selling_plan_group_failed_outcome(
                root_field,
                None,
                vec![error],
                "Selling plan group resource validation failed; original raw mutation retained for observability.",
            );
        }

        let id = self.next_proxy_synthetic_gid("SellingPlanGroup");
        let created_at = self.next_product_timestamp();
        let shop_currency_code = self.store.shop_currency_code();
        let selling_plans = resolved_object_list_field(&input, "sellingPlansToCreate")
            .into_iter()
            .enumerate()
            .map(|(index, plan_input)| {
                let plan_id = self.next_proxy_synthetic_gid("SellingPlan");
                selling_plan_record_from_input(
                    plan_id,
                    &created_at,
                    index,
                    &plan_input,
                    &shop_currency_code,
                )
            })
            .collect::<Vec<_>>();
        let name = resolved_string_field(&input, "name").unwrap_or_default();
        let mut unique_product_ids = Vec::new();
        extend_unique_strings(&mut unique_product_ids, product_ids);
        let mut unique_product_variant_ids = Vec::new();
        extend_unique_strings(&mut unique_product_variant_ids, product_variant_ids);

        let group = SellingPlanGroupRecord {
            id: id.clone(),
            cursor: None,
            app_id: resolved_string_field(&input, "appId"),
            merchant_code: resolved_string_field(&input, "merchantCode")
                .unwrap_or_else(|| name.clone()),
            description: resolved_string_field(&input, "description").unwrap_or_default(),
            options: list_string_field(&input, "options"),
            position: resolved_int_field(&input, "position").unwrap_or(1),
            created_at: created_at.clone(),
            updated_at: created_at,
            name,
            selling_plans,
            product_ids: unique_product_ids,
            product_variant_ids: unique_product_variant_ids,
            product_cursors: BTreeMap::new(),
            product_variant_cursors: BTreeMap::new(),
        };
        self.store.stage_selling_plan_group(group.clone());

        ResolverOutcome::value(selling_plan_group_payload_value(
            self,
            Some(&group),
            None,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(root_field, "products", vec![id]))
    }

    fn selling_plan_group_update(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let input = resolved_object_field(arguments, "input").unwrap_or_default();
        let Some(mut group) = self.store.selling_plan_group_by_id(&id).cloned() else {
            return self.selling_plan_group_failed_outcome(
                root_field,
                Some(None),
                vec![group_does_not_exist_error()],
                "Selling plan group update targeted an unknown group; original raw mutation retained for observability.",
            );
        };

        let user_errors =
            selling_plan_group_input_user_errors(&input, SellingPlanInputMode::Update);
        if !user_errors.is_empty() {
            return self.selling_plan_group_failed_outcome(
                root_field,
                Some(None),
                user_errors,
                "Selling plan group input validation failed; original raw mutation retained for observability.",
            );
        }
        let user_errors = selling_plan_group_update_model_user_errors(&group, &input);
        if !user_errors.is_empty() {
            return self.selling_plan_group_failed_outcome(
                root_field,
                Some(None),
                user_errors,
                "Selling plan group update would delete every existing selling plan without a replacement; original raw mutation retained for observability.",
            );
        }

        if let Some(name) = resolved_string_field(&input, "name") {
            group.name = name;
        }
        if input.contains_key("appId") {
            group.app_id = resolved_string_field(&input, "appId");
        }
        if let Some(merchant_code) = resolved_string_field(&input, "merchantCode") {
            group.merchant_code = merchant_code;
        }
        if let Some(description) = resolved_string_field(&input, "description") {
            group.description = description;
        }
        if input.contains_key("options") {
            group.options = list_string_field(&input, "options");
        }
        if let Some(position) = resolved_int_field(&input, "position") {
            group.position = position;
        }

        let mut deleted_plan_ids = Vec::new();
        for id in list_string_field(&input, "sellingPlansToDelete") {
            if let Some(index) = group.selling_plans.iter().position(|plan| plan.id == id) {
                deleted_plan_ids.push(group.selling_plans.remove(index).id);
            }
        }
        let shop_currency_code = self.store.shop_currency_code();
        for plan_input in resolved_object_list_field(&input, "sellingPlansToUpdate") {
            let Some(plan_id) = resolved_string_field(&plan_input, "id") else {
                continue;
            };
            if let Some(plan) = group
                .selling_plans
                .iter_mut()
                .find(|plan| plan.id == plan_id)
            {
                apply_selling_plan_update(plan, &plan_input, &shop_currency_code);
            }
        }
        let created_at = group.created_at.clone();
        for plan_input in resolved_object_list_field(&input, "sellingPlansToCreate") {
            let plan_id = self.next_proxy_synthetic_gid("SellingPlan");
            let index = group.selling_plans.len();
            group.selling_plans.push(selling_plan_record_from_input(
                plan_id,
                &created_at,
                index,
                &plan_input,
                &shop_currency_code,
            ));
        }
        self.store.stage_selling_plan_group(group.clone());

        ResolverOutcome::value(selling_plan_group_payload_value(
            self,
            Some(&group),
            Some(Some(deleted_plan_ids)),
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(root_field, "products", vec![id]))
    }

    fn selling_plan_group_delete(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> ResolverOutcome<Value> {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let existed = self.store.delete_selling_plan_group(&id);
        let (deleted_id, user_errors, log_draft) = if existed {
            (
                Some(id.clone()),
                Vec::new(),
                LogDraft::staged(root_field, "products", vec![id]),
            )
        } else {
            (
                None,
                vec![group_does_not_exist_error()],
                LogDraft::failed(
                    root_field,
                    "products",
                    "Selling plan group delete targeted an unknown group; original raw mutation retained for observability.",
                ),
            )
        };
        ResolverOutcome::value(selling_plan_group_delete_payload_value(
            deleted_id.as_deref(),
            user_errors,
        ))
        .with_log_draft(log_draft)
    }

    fn selling_plan_group_add_resources(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        resource_kind: ResourceKind,
    ) -> ResolverOutcome<Value> {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let ids = list_string_field(arguments, resource_kind.ids_arg());
        let Some(mut group) = self.store.selling_plan_group_by_id(&id).cloned() else {
            return self.selling_plan_group_failed_outcome(
                root_field,
                None,
                vec![group_does_not_exist_error()],
                "Selling plan group add targeted an unknown group; original raw mutation retained for observability.",
            );
        };
        if let Some(error) = self.resource_existence_error(&ids, resource_kind) {
            return self.selling_plan_group_failed_outcome(
                root_field,
                None,
                vec![error],
                "Selling plan group membership validation failed; original raw mutation retained for observability.",
            );
        }
        let members = resource_members_mut(&mut group, resource_kind);
        if ids.iter().any(|resource_id| members.contains(resource_id)) {
            return self.selling_plan_group_failed_outcome(
                root_field,
                None,
                vec![user_error(
                    [resource_kind.ids_arg()],
                    "Resource has already been taken",
                    Some("TAKEN"),
                )],
                "Selling plan group membership validation failed; original raw mutation retained for observability.",
            );
        }
        extend_unique_strings(members, ids.clone());
        self.store.stage_selling_plan_group(group.clone());
        let mut staged_ids = vec![group.id.clone()];
        staged_ids.extend(ids);
        ResolverOutcome::value(selling_plan_group_payload_value(
            self,
            Some(&group),
            None,
            Vec::new(),
        ))
        .with_log_draft(LogDraft::staged(root_field, "products", staged_ids))
    }

    fn selling_plan_group_remove_resources(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        resource_kind: ResourceKind,
    ) -> ResolverOutcome<Value> {
        let id = resolved_string_field(arguments, "id").unwrap_or_default();
        let ids = list_string_field(arguments, resource_kind.ids_arg());
        let Some(mut group) = self.store.selling_plan_group_by_id(&id).cloned() else {
            return ResolverOutcome::value(group_remove_payload_value(
                None,
                vec![group_does_not_exist_error()],
                resource_kind,
            ))
            .with_log_draft(LogDraft::failed(
                root_field,
                "products",
                "Selling plan group remove targeted an unknown group; original raw mutation retained for observability.",
            ));
        };

        let members = resource_members_mut(&mut group, resource_kind);
        let mut removed = Vec::new();
        for resource_id in ids {
            if let Some(position) = members.iter().position(|member| member == &resource_id) {
                removed.push(members.remove(position));
            }
        }
        self.store.stage_selling_plan_group(group);
        ResolverOutcome::value(group_remove_payload_value(
            Some(removed.clone()),
            Vec::new(),
            resource_kind,
        ))
        .with_log_draft(LogDraft::staged(root_field, "products", removed))
    }

    fn resource_join_leave_selling_plan_groups(
        &mut self,
        root_field: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
        resource_kind: ResourceKind,
        is_join: bool,
    ) -> ResolverOutcome<Value> {
        let resource_id = resolved_string_field(arguments, "id").unwrap_or_default();
        let group_ids = list_string_field(arguments, "sellingPlanGroupIds");
        let value = |proxy: &Self, user_errors: Vec<Value>| {
            resource_join_leave_payload_value(proxy, resource_kind, &resource_id, user_errors)
        };

        let user_errors =
            self.join_leave_preflight_errors(resource_kind, &resource_id, &group_ids, is_join);
        if !user_errors.is_empty() {
            return ResolverOutcome::value(value(self, user_errors)).with_log_draft(
                LogDraft::failed(
                    root_field,
                    "products",
                "Selling plan group join/leave validation failed; original raw mutation retained for observability.",
                ),
            );
        }

        for group_id in &group_ids {
            let Some(mut group) = self.store.selling_plan_group_by_id(group_id).cloned() else {
                continue;
            };
            let members = resource_members_mut(&mut group, resource_kind);
            if is_join {
                push_unique_string(members, &resource_id);
            } else if let Some(position) = members.iter().position(|member| member == &resource_id)
            {
                members.remove(position);
            }
            self.store.stage_selling_plan_group(group);
        }

        let mut staged_ids = vec![resource_id.clone()];
        staged_ids.extend(group_ids);
        ResolverOutcome::value(value(self, Vec::new()))
            .with_log_draft(LogDraft::staged(root_field, "products", staged_ids))
    }

    fn selling_plan_group_failed_outcome(
        &self,
        root_field: &str,
        deleted_plan_ids: Option<Option<Vec<String>>>,
        user_errors: Vec<Value>,
        notes: &'static str,
    ) -> ResolverOutcome<Value> {
        ResolverOutcome::value(selling_plan_group_payload_value(
            self,
            None,
            deleted_plan_ids,
            user_errors,
        ))
        .with_log_draft(LogDraft::failed(root_field, "products", notes))
    }

    fn resource_existence_error(
        &mut self,
        ids: &[String],
        resource_kind: ResourceKind,
    ) -> Option<Value> {
        let missing = ids
            .iter()
            .filter(|id| !self.has_resource(id, resource_kind))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() && self.config.read_mode != ReadMode::Snapshot {
            self.hydrate_product_nodes_for_observation(missing);
        }
        ids.iter()
            .find(|id| !self.has_resource(id, resource_kind))
            .map(|id| {
                user_error(
                    [resource_kind.ids_arg()],
                    &format!("{} {} does not exist.", resource_kind.label(), id),
                    Some("NOT_FOUND"),
                )
            })
    }

    fn has_resource(&self, id: &str, resource_kind: ResourceKind) -> bool {
        match resource_kind {
            ResourceKind::Product => self.store.product_by_id(id).is_some(),
            ResourceKind::ProductVariant => self.store.product_variant_by_id(id).is_some(),
        }
    }

    fn join_leave_preflight_errors(
        &self,
        resource_kind: ResourceKind,
        resource_id: &str,
        group_ids: &[String],
        is_join: bool,
    ) -> Vec<Value> {
        if group_ids.is_empty() {
            return vec![presence_user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group IDs",
            )];
        }
        let mut seen = BTreeSet::new();
        if group_ids.iter().any(|id| !seen.insert(id)) {
            return vec![user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group IDs contains duplicate values.",
                None,
            )];
        }
        if !self.has_resource(resource_id, resource_kind) {
            return vec![user_error(
                ["id"],
                &format!("{} does not exist.", resource_kind.label()),
                Some("NOT_FOUND"),
            )];
        }
        if let Some(_missing_group) = group_ids
            .iter()
            .find(|id| self.store.selling_plan_group_by_id(id).is_none())
        {
            return vec![user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group does not exist.",
                Some("GROUP_DOES_NOT_EXIST"),
            )];
        }
        if !is_join
            && group_ids.iter().any(|group_id| {
                self.store
                    .selling_plan_group_by_id(group_id)
                    .is_some_and(|group| {
                        !resource_members(group, resource_kind).contains(&resource_id.to_string())
                    })
            })
        {
            return vec![user_error(
                ["sellingPlanGroupIds"],
                "Selling plan group is not a member.",
                None,
            )];
        }
        Vec::new()
    }

    fn direct_group_ids_for_resource(
        &self,
        resource_kind: ResourceKind,
        resource_id: &str,
    ) -> BTreeSet<String> {
        self.store
            .selling_plan_groups()
            .into_iter()
            .filter(|group| {
                resource_members(group, resource_kind)
                    .iter()
                    .any(|id| id == resource_id)
            })
            .map(|group| group.id)
            .collect()
    }

    fn selling_plan_groups_for_nodes_matching(
        &self,
        predicate: impl Fn(&SellingPlanGroupRecord) -> bool,
    ) -> Vec<SellingPlanGroupRecord> {
        let mut groups = Vec::new();
        let mut seen = BTreeSet::new();
        for group in self.store.selling_plan_groups() {
            if predicate(&group) && seen.insert(group.id.clone()) {
                groups.push(group);
            }
        }
        groups
    }

    pub(in crate::proxy) fn canonical_product_selling_plan_groups_value(
        &self,
        product_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let variant_ids = self
            .store
            .product_variants_for_product(product_id)
            .into_iter()
            .map(|variant| variant.id)
            .collect::<BTreeSet<_>>();
        let groups = self.selling_plan_groups_for_nodes_matching(|group| {
            group.product_ids.iter().any(|id| id == product_id)
                || group
                    .product_variant_ids
                    .iter()
                    .any(|id| variant_ids.contains(id))
        });
        canonical_selling_plan_group_connection(groups, arguments)
    }

    pub(in crate::proxy) fn canonical_product_selling_plan_groups_count_value(
        &self,
        product_id: &str,
    ) -> Value {
        count_object(
            self.direct_group_ids_for_resource(ResourceKind::Product, product_id)
                .len(),
        )
    }

    pub(in crate::proxy) fn canonical_product_variant_selling_plan_groups_value(
        &self,
        variant_id: &str,
        product_id: &str,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Value {
        let groups = self.selling_plan_groups_for_nodes_matching(|group| {
            group.product_ids.iter().any(|id| id == product_id)
                || group.product_variant_ids.iter().any(|id| id == variant_id)
        });
        canonical_selling_plan_group_connection(groups, arguments)
    }

    pub(in crate::proxy) fn canonical_product_variant_selling_plan_groups_count_value(
        &self,
        variant_id: &str,
    ) -> Value {
        count_object(
            self.direct_group_ids_for_resource(ResourceKind::ProductVariant, variant_id)
                .len(),
        )
    }
}

fn canonical_selling_plan_group_connection(
    groups: Vec<SellingPlanGroupRecord>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Value {
    connection_value_with_args(
        groups
            .into_iter()
            .map(|group| {
                json!({
                    "__typename": "SellingPlanGroup",
                    "id": group.id,
                    "name": group.name,
                    "merchantCode": group.merchant_code,
                })
            })
            .collect(),
        arguments,
        value_id_cursor,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SellingPlanInputMode {
    Create,
    Update,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceKind {
    Product,
    ProductVariant,
}

impl ResourceKind {
    fn ids_arg(self) -> &'static str {
        match self {
            Self::Product => "productIds",
            Self::ProductVariant => "productVariantIds",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Product => "Product",
            Self::ProductVariant => "Product variant",
        }
    }

    fn payload_resource_field(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::ProductVariant => "productVariant",
        }
    }
}

fn observed_selling_plan_group_values(response: &Response) -> Vec<Value> {
    let mut groups = Vec::new();
    if let Some(group) = response
        .body
        .pointer("/data/sellingPlanGroup")
        .filter(|group| group.is_object())
    {
        groups.push(group.clone());
    }
    groups.extend(observed_connection_nodes(
        response.body.pointer("/data/sellingPlanGroups"),
    ));
    groups.extend(
        response
            .body
            .pointer("/data/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|node| observed_value_is_selling_plan_group(node))
            .cloned(),
    );
    if let Some(node) = response
        .body
        .pointer("/data/node")
        .filter(|node| observed_value_is_selling_plan_group(node))
    {
        groups.push(node.clone());
    }
    groups
}

fn observed_value_is_selling_plan_group(value: &Value) -> bool {
    value
        .get("__typename")
        .and_then(Value::as_str)
        .is_some_and(|typename| typename == "SellingPlanGroup")
        || value
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| is_shopify_gid_of_type(id, "SellingPlanGroup"))
}

fn observed_connection_nodes(value: Option<&Value>) -> Vec<Value> {
    let mut nodes = value
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| node.is_object())
        .cloned()
        .collect::<Vec<_>>();
    nodes.extend(
        value
            .and_then(|connection| connection.get("edges"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|edge| edge.get("node"))
            .filter(|node| node.is_object())
            .cloned(),
    );
    nodes
}

fn selling_plan_group_record_from_observed_json(value: &Value) -> Option<SellingPlanGroupRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let created_at = value
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let product_rows = value
        .get("products")
        .map(observed_connection_rows)
        .unwrap_or_default();
    let product_ids = product_rows
        .iter()
        .filter_map(|product| {
            product
                .node
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let product_cursors = product_rows
        .iter()
        .filter_map(|row| {
            Some((
                row.node.get("id")?.as_str()?.to_string(),
                row.cursor.clone()?,
            ))
        })
        .collect();
    let product_variant_rows = value
        .get("productVariants")
        .map(observed_connection_rows)
        .unwrap_or_default();
    let product_variant_ids = product_variant_rows
        .iter()
        .filter_map(|variant| {
            variant
                .node
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let product_variant_cursors = product_variant_rows
        .iter()
        .filter_map(|row| {
            Some((
                row.node.get("id")?.as_str()?.to_string(),
                row.cursor.clone()?,
            ))
        })
        .collect();
    let selling_plans = value
        .get("sellingPlans")
        .map(observed_connection_rows)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, row)| {
            selling_plan_record_from_observed_json(&row.node, row.cursor, index)
        })
        .collect::<Vec<_>>();

    Some(SellingPlanGroupRecord {
        id,
        cursor: None,
        app_id: value
            .get("appId")
            .and_then(Value::as_str)
            .map(str::to_string),
        merchant_code: value
            .get("merchantCode")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| name.clone()),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        options: string_values_from_json(value.get("options")),
        position: value.get("position").and_then(Value::as_i64).unwrap_or(1),
        updated_at: value
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or(&created_at)
            .to_string(),
        created_at,
        name,
        selling_plans,
        product_ids,
        product_variant_ids,
        product_cursors,
        product_variant_cursors,
    })
}

fn selling_plan_record_from_observed_json(
    value: &Value,
    cursor: Option<String>,
    index: usize,
) -> Option<SellingPlanRecord> {
    let id = value.get("id")?.as_str()?.to_string();
    Some(SellingPlanRecord {
        id,
        cursor,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        options: string_values_from_json(value.get("options")),
        position: value
            .get("position")
            .and_then(Value::as_i64)
            .unwrap_or((index + 1) as i64),
        category: value
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or("OTHER")
            .to_string(),
        created_at: value
            .get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        billing_policy: value
            .get("billingPolicy")
            .filter(|policy| policy.is_object())
            .cloned()
            .unwrap_or_else(|| json!({ "__typename": "SellingPlanFixedBillingPolicy" })),
        delivery_policy: value
            .get("deliveryPolicy")
            .filter(|policy| policy.is_object())
            .cloned()
            .unwrap_or_else(|| json!({ "__typename": "SellingPlanFixedDeliveryPolicy" })),
        inventory_policy: value
            .get("inventoryPolicy")
            .filter(|policy| policy.is_object())
            .cloned()
            .unwrap_or_else(|| json!({ "reserve": "ON_FULFILLMENT" })),
        pricing_policies: value
            .get("pricingPolicies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|policy| policy.is_object())
            .cloned()
            .collect(),
    })
}

fn string_values_from_json(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn selling_plan_group_effective_updated_at(group: &SellingPlanGroupRecord) -> &str {
    if group.updated_at.is_empty() {
        &group.created_at
    } else {
        &group.updated_at
    }
}

fn selling_plan_group_gid_tail_sort_value(id: &str) -> StagedSortValue {
    resource_id_tail_sort_value(Some(id))
}

fn selling_plan_group_staged_sort_key(
    group: &SellingPlanGroupRecord,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let id_sort = selling_plan_group_gid_tail_sort_value(&group.id);
    let primary = match sort_key.unwrap_or("ID") {
        "CREATED_AT" => StagedSortValue::String(group.created_at.clone()),
        "NAME" => StagedSortValue::String(group.name.to_ascii_lowercase()),
        "UPDATED_AT" => {
            StagedSortValue::String(selling_plan_group_effective_updated_at(group).to_string())
        }
        _ => id_sort.clone(),
    };
    vec![primary, id_sort]
}

fn selling_plan_group_search_decision(
    group: &SellingPlanGroupRecord,
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
        let term = trim_selling_plan_group_search_value(term);
        if term.is_empty() || term.eq_ignore_ascii_case("AND") {
            continue;
        }
        match selling_plan_group_search_term_decision(group, term) {
            StagedSearchDecision::Match => {}
            StagedSearchDecision::NoMatch => return StagedSearchDecision::NoMatch,
            StagedSearchDecision::Unsupported => return StagedSearchDecision::Unsupported,
        }
    }

    StagedSearchDecision::Match
}

fn selling_plan_group_search_term_decision(
    group: &SellingPlanGroupRecord,
    term: &str,
) -> StagedSearchDecision {
    let Some((key, value)) = term.split_once(':') else {
        return StagedSearchDecision::from_bool(selling_plan_group_text_matches(group, term));
    };
    let key = key.to_ascii_lowercase();
    let value = trim_selling_plan_group_search_value(value);
    if value.is_empty() {
        return StagedSearchDecision::NoMatch;
    }

    match key.as_str() {
        "app_id" => {
            StagedSearchDecision::from_bool(selling_plan_group_app_id_matches(group, value))
        }
        "category" => {
            StagedSearchDecision::from_bool(selling_plan_group_categories_match(group, value))
        }
        "created_at" => {
            StagedSearchDecision::from_bool(timestamp_search_matches(&group.created_at, value))
        }
        "delivery_frequency" => StagedSearchDecision::from_bool(
            selling_plan_group_delivery_frequency_matches(group, value),
        ),
        "id" => StagedSearchDecision::from_bool(resource_id_search_matches(&group.id, value)),
        "name" => StagedSearchDecision::from_bool(search_text_matches(&group.name, value)),
        "percentage_off" => {
            StagedSearchDecision::from_bool(selling_plan_group_percentage_off_matches(group, value))
        }
        _ => StagedSearchDecision::Unsupported,
    }
}

fn trim_selling_plan_group_search_value(value: &str) -> &str {
    value
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim_matches('(')
        .trim_matches(')')
}

fn split_search_comparison(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<"] {
        if let Some(operand) = value.strip_prefix(operator) {
            return (operator, trim_selling_plan_group_search_value(operand));
        }
    }
    ("=", value)
}

fn search_text_matches(haystack: &str, value: &str) -> bool {
    let needle = trim_selling_plan_group_search_value(value)
        .trim_end_matches('*')
        .to_ascii_lowercase();
    !needle.is_empty() && haystack.to_ascii_lowercase().contains(&needle)
}

fn selling_plan_group_text_matches(group: &SellingPlanGroupRecord, term: &str) -> bool {
    let mut haystack = format!(
        "{} {} {} {}",
        group.name,
        group.merchant_code,
        group.description,
        group.options.join(" ")
    );
    for plan in &group.selling_plans {
        haystack.push(' ');
        haystack.push_str(&plan.name);
        haystack.push(' ');
        haystack.push_str(&plan.description);
        haystack.push(' ');
        haystack.push_str(&plan.category);
    }
    search_text_matches(&haystack, term)
}

fn selling_plan_group_app_id_matches(group: &SellingPlanGroupRecord, value: &str) -> bool {
    if value.eq_ignore_ascii_case("CURRENT") || value.eq_ignore_ascii_case("ALL") {
        return true;
    }
    group
        .app_id
        .as_deref()
        .map(|app_id| app_id == value || resource_id_tail(app_id) == value)
        .unwrap_or(false)
}

fn selling_plan_group_categories_match(group: &SellingPlanGroupRecord, value: &str) -> bool {
    let values = value
        .split(',')
        .map(trim_selling_plan_group_search_value)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    !values.is_empty()
        && group.selling_plans.iter().any(|plan| {
            values
                .iter()
                .any(|value| plan.category.eq_ignore_ascii_case(value))
        })
}

fn timestamp_search_matches(timestamp: &str, value: &str) -> bool {
    let (operator, operand) = split_search_comparison(value);
    match operator {
        ">" => timestamp > operand,
        ">=" => timestamp >= operand,
        "<" => timestamp < operand,
        "<=" => timestamp <= operand,
        _ => timestamp.starts_with(operand),
    }
}

fn resource_id_search_matches(id: &str, value: &str) -> bool {
    let (operator, operand) = split_search_comparison(value);
    let id_tail = resource_id_tail(id);
    if operator == "=" {
        return id == operand || id_tail == resource_id_tail(operand);
    }
    let Ok(id_number) = id_tail.parse::<i64>() else {
        return false;
    };
    let Ok(operand_number) = resource_id_tail(operand).parse::<i64>() else {
        return false;
    };
    numeric_comparison_matches(id_number, operator, operand_number, |value, operand| {
        value == operand
    })
}

fn numeric_comparison_matches<T>(
    value: T,
    operator: &str,
    operand: T,
    is_equal: impl Fn(T, T) -> bool,
) -> bool
where
    T: PartialOrd + Copy,
{
    match operator {
        ">" => value > operand,
        ">=" => value >= operand,
        "<" => value < operand,
        "<=" => value <= operand,
        _ => is_equal(value, operand),
    }
}

fn selling_plan_group_delivery_frequency_matches(
    group: &SellingPlanGroupRecord,
    value: &str,
) -> bool {
    let needle = value.to_ascii_lowercase();
    group.selling_plans.iter().any(|plan| {
        if plan
            .delivery_policy
            .to_string()
            .to_ascii_lowercase()
            .contains(&needle)
        {
            return true;
        }
        let interval = plan
            .delivery_policy
            .get("interval")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let interval_count = plan
            .delivery_policy
            .get("intervalCount")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        needle == interval.to_ascii_lowercase()
            || needle == format!("{} {}", interval_count, interval).to_ascii_lowercase()
    })
}

fn selling_plan_group_percentage_off_matches(group: &SellingPlanGroupRecord, value: &str) -> bool {
    let (operator, operand) = split_search_comparison(value);
    let Ok(operand) = operand.parse::<f64>() else {
        return false;
    };
    group
        .selling_plans
        .iter()
        .flat_map(|plan| plan.pricing_policies.iter())
        .filter(|policy| policy.get("adjustmentType").and_then(Value::as_str) == Some("PERCENTAGE"))
        .filter_map(|policy| {
            policy
                .pointer("/adjustmentValue/percentage")
                .and_then(Value::as_f64)
        })
        .any(|percentage| {
            numeric_comparison_matches(percentage, operator, operand, |value, operand| {
                (value - operand).abs() < f64::EPSILON
            })
        })
}

fn selling_plan_group_input_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
    mode: SellingPlanInputMode,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if list_string_field(input, "options").len() > 3 {
        errors.push(user_error(
            ["input", "options"],
            "Too many selling plan group options (maximum 3 options)",
            Some("TOO_LONG"),
        ));
    }
    if let Some(position) = resolved_int_field(input, "position") {
        if !is_int32(position) {
            errors.push(position_invalid_error(vec!["input", "position"]));
        }
    }
    let create_field = "sellingPlansToCreate";
    for (index, plan) in resolved_object_list_field(input, create_field)
        .iter()
        .enumerate()
    {
        errors.extend(selling_plan_input_user_errors(
            plan,
            mode,
            create_field,
            index,
        ));
    }
    if mode == SellingPlanInputMode::Update {
        for (index, plan) in resolved_object_list_field(input, "sellingPlansToUpdate")
            .iter()
            .enumerate()
        {
            if resolved_string_field(plan, "id").is_none() {
                errors.push(user_error(
                    vec![
                        "input".to_string(),
                        "sellingPlansToUpdate".to_string(),
                        index.to_string(),
                        "id".to_string(),
                    ],
                    "Id must be specificed to update a Selling Plan.",
                    Some("PLAN_ID_MUST_BE_SPECIFIED_TO_UPDATE"),
                ));
            }
            errors.extend(selling_plan_input_user_errors(
                plan,
                mode,
                "sellingPlansToUpdate",
                index,
            ));
        }
    }
    if errors.is_empty() && mode == SellingPlanInputMode::Create {
        errors.extend(selling_plan_group_create_model_user_errors(input));
    }
    errors
}

fn selling_plan_group_create_model_user_errors(
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if resolved_string_field(input, "name")
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
    {
        errors.push(presence_user_error(["input", "name"], "Name"));
    }

    let plans = resolved_object_list_field(input, "sellingPlansToCreate");
    if plans.len() > MAX_SELLING_PLANS_PER_GROUP {
        errors.push(user_error(
            ["input"],
            "Selling plan groups can't have more than 31 selling plans.",
            Some("SELLING_PLAN_COUNT_UPPER_BOUND"),
        ));
    } else if plans.is_empty() {
        errors.push(user_error(
            ["input"],
            "Selling plan groups must have at least 1 selling plan.",
            Some("SELLING_PLAN_COUNT_LOWER_BOUND"),
        ));
    }

    for (index, plan) in plans.iter().enumerate() {
        let index = index.to_string();
        if resolved_object_field(plan, "billingPolicy").is_none() {
            errors.push(user_error(
                [
                    "input",
                    "sellingPlansToCreate",
                    index.as_str(),
                    "billingPolicy",
                ],
                "Selling plans to create billing policy must be present.",
                Some("SELLING_PLAN_BILLING_POLICY_MISSING"),
            ));
        }
        if resolved_object_field(plan, "deliveryPolicy").is_none() {
            errors.push(user_error(
                [
                    "input",
                    "sellingPlansToCreate",
                    index.as_str(),
                    "deliveryPolicy",
                ],
                "Selling plans to create delivery policy must be present.",
                Some("SELLING_PLAN_DELIVERY_POLICY_MISSING"),
            ));
        }
    }

    errors
}

fn selling_plan_group_update_model_user_errors(
    group: &SellingPlanGroupRecord,
    input: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let plans_to_create = resolved_object_list_field(input, "sellingPlansToCreate");
    let plan_ids_to_delete = resolved_string_list_field(input, "sellingPlansToDelete");
    if plans_to_create.is_empty()
        && !plan_ids_to_delete.is_empty()
        && !group.selling_plans.is_empty()
    {
        let deletes_every_existing_plan = group
            .selling_plans
            .iter()
            .all(|plan| plan_ids_to_delete.iter().any(|id| id == &plan.id));
        if deletes_every_existing_plan {
            return vec![user_error(
                ["input", "sellingPlansToDelete"],
                "Selling plans to delete can't result in a selling plan group with no selling plan.",
                Some("SELLING_PLAN_COUNT_LOWER_BOUND"),
            )];
        }
    }

    Vec::new()
}

fn selling_plan_input_user_errors(
    plan: &BTreeMap<String, ResolvedValue>,
    mode: SellingPlanInputMode,
    list_field: &str,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let index = index.to_string();
    if list_string_field(plan, "options").len() > 3 {
        errors.push(user_error(
            selling_plan_input_field_path(list_field, &index, "options"),
            "Too many selling plan options (maximum 3 options)",
            Some("TOO_LONG"),
        ));
    }
    let pricing_policies = resolved_object_list_field(plan, "pricingPolicies");
    if pricing_policies.len() > 2 {
        let message =
            pricing_policy_user_error_message(mode, "can't have more than 2 pricing policies");
        errors.push(user_error(
            selling_plan_input_field_path(list_field, &index, "pricingPolicies"),
            &message,
            Some("SELLING_PLAN_PRICING_POLICIES_LIMIT"),
        ));
    }
    if pricing_policies.len() <= 2 && pricing_policies_must_contain_fixed_error(&pricing_policies) {
        let message =
            pricing_policy_user_error_message(mode, "must contain one fixed pricing policy");
        errors.push(user_error(
            selling_plan_input_field_path(list_field, &index, "pricingPolicies"),
            &message,
            Some("SELLING_PLAN_PRICING_POLICIES_MUST_CONTAIN_A_FIXED_PRICING_POLICY"),
        ));
    }
    if let Some(position) = resolved_int_field(plan, "position") {
        if !is_int32(position) {
            errors.push(position_invalid_error(selling_plan_input_field_path(
                list_field, &index, "position",
            )));
        }
    }

    let billing_policy = resolved_object_field(plan, "billingPolicy").unwrap_or_default();
    let delivery_policy = resolved_object_field(plan, "deliveryPolicy").unwrap_or_default();
    let billing_recurring = resolved_object_field(&billing_policy, "recurring");
    let delivery_recurring = resolved_object_field(&delivery_policy, "recurring");
    let delivery_fixed = resolved_object_field(&delivery_policy, "fixed");
    if billing_recurring.is_some() && delivery_fixed.is_some() {
        match mode {
            SellingPlanInputMode::Create => errors.push(user_error(
                vec!["input".to_string(), list_field.to_string(), index.clone()],
                "billing and delivery policy types must be the same.",
                Some("BILLING_AND_DELIVERY_POLICY_TYPES_MUST_BE_THE_SAME"),
            )),
            SellingPlanInputMode::Update => errors.push(user_error(
                vec!["input".to_string(), list_field.to_string(), index.clone()],
                "Only one of fixed or recurring delivery policy is allowed",
                Some("ONLY_ONE_OF_FIXED_OR_RECURRING_DELIVERY"),
            )),
        }
    }
    if let Some(delivery_recurring) = delivery_recurring.as_ref() {
        validate_recurring_interval_count(
            &mut errors,
            delivery_recurring,
            list_field,
            &index,
            "deliveryPolicy",
        );
        if let Some(cutoff) = resolved_int_field(delivery_recurring, "cutoff") {
            if !is_non_negative_int32(cutoff) {
                errors.push(range_invalid_error(
                    selling_plan_recurring_field_path(
                        list_field,
                        &index,
                        "deliveryPolicy",
                        "cutoff",
                    ),
                    "Cutoff must be within the range of 0 to 2,147,483,647",
                ));
            }
        }
    }
    if let Some(billing_recurring) = billing_recurring.as_ref() {
        validate_recurring_interval_count(
            &mut errors,
            billing_recurring,
            list_field,
            &index,
            "billingPolicy",
        );
        if let Some(min_cycles) = resolved_int_field(billing_recurring, "minCycles") {
            if !is_positive_int32(min_cycles) {
                errors.push(range_invalid_error(
                    selling_plan_recurring_field_path(
                        list_field,
                        &index,
                        "billingPolicy",
                        "minCycles",
                    ),
                    "Min cycles must be within the range of 1 to 2,147,483,647",
                ));
            }
        }
        if let Some(max_cycles) = resolved_int_field(billing_recurring, "maxCycles") {
            if !is_positive_int32(max_cycles) {
                errors.push(range_invalid_error(
                    selling_plan_recurring_field_path(
                        list_field,
                        &index,
                        "billingPolicy",
                        "maxCycles",
                    ),
                    "Max cycles must be within the range of 1 to 2,147,483,647",
                ));
            }
        }
    }
    if let (Some(billing_recurring), Some(delivery_recurring)) =
        (billing_recurring.as_ref(), delivery_recurring.as_ref())
    {
        if resolved_value_json(
            billing_recurring
                .get("anchors")
                .unwrap_or(&ResolvedValue::Null),
        ) != resolved_value_json(
            delivery_recurring
                .get("anchors")
                .unwrap_or(&ResolvedValue::Null),
        ) {
            errors.push(user_error(
                vec!["input".to_string(), list_field.to_string(), index],
                "Billing and delivery policy anchors must be the same",
                Some("SELLING_PLAN_BILLING_AND_DELIVERY_POLICY_ANCHORS_MUST_BE_EQUAL"),
            ));
        }
    }
    errors
}

fn pricing_policies_must_contain_fixed_error(
    pricing_policies: &[BTreeMap<String, ResolvedValue>],
) -> bool {
    let contains_recurring = pricing_policies
        .iter()
        .any(|policy| resolved_object_field(policy, "recurring").is_some());
    let contains_fixed = pricing_policies
        .iter()
        .any(|policy| resolved_object_field(policy, "fixed").is_some());
    contains_recurring && !contains_fixed
}

fn pricing_policy_user_error_message(mode: SellingPlanInputMode, suffix: &str) -> String {
    let action = match mode {
        SellingPlanInputMode::Create => "create",
        SellingPlanInputMode::Update => "update",
    };
    format!("Selling plans to {action} pricing policies {suffix}")
}

fn is_int32(value: i64) -> bool {
    (INT32_MIN..=INT32_MAX).contains(&value)
}

fn is_positive_int32(value: i64) -> bool {
    (1..=INT32_MAX).contains(&value)
}

fn is_non_negative_int32(value: i64) -> bool {
    (0..=INT32_MAX).contains(&value)
}

fn position_invalid_error(field: Vec<impl Into<String>>) -> Value {
    let field = field.into_iter().map(Into::into).collect::<Vec<String>>();
    user_error(
        field,
        "Position must be within the range of -2,147,483,648 to 2,147,483,647",
        Some("INVALID"),
    )
}

fn range_invalid_error(field: Vec<String>, message: &str) -> Value {
    user_error(field, message, Some("INVALID"))
}

fn selling_plan_input_field_path(list_field: &str, index: &str, field: &str) -> Vec<String> {
    vec![
        "input".to_string(),
        list_field.to_string(),
        index.to_string(),
        field.to_string(),
    ]
}

fn selling_plan_recurring_field_path(
    list_field: &str,
    index: &str,
    policy_field: &str,
    recurring_field: &str,
) -> Vec<String> {
    let mut field = selling_plan_input_field_path(list_field, index, policy_field);
    field.push("recurring".to_string());
    field.push(recurring_field.to_string());
    field
}

fn validate_recurring_interval_count(
    errors: &mut Vec<Value>,
    recurring: &BTreeMap<String, ResolvedValue>,
    list_field: &str,
    index: &str,
    policy_field: &str,
) {
    if resolved_int_field(recurring, "intervalCount") == Some(0) {
        errors.push(user_error(
            selling_plan_recurring_field_path(list_field, index, policy_field, "intervalCount"),
            "Interval count must be greater than 0",
            Some("GREATER_THAN"),
        ));
    }
}

fn selling_plan_record_from_input(
    id: String,
    created_at: &str,
    index: usize,
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> SellingPlanRecord {
    SellingPlanRecord {
        id,
        cursor: None,
        name: resolved_string_field(input, "name").unwrap_or_default(),
        description: resolved_string_field(input, "description").unwrap_or_default(),
        options: list_string_field(input, "options"),
        position: resolved_int_field(input, "position").unwrap_or((index + 1) as i64),
        category: resolved_string_field(input, "category").unwrap_or_else(|| "OTHER".to_string()),
        created_at: created_at.to_string(),
        billing_policy: billing_policy_json(&resolved_object_field(input, "billingPolicy")),
        delivery_policy: delivery_policy_json(&resolved_object_field(input, "deliveryPolicy")),
        inventory_policy: inventory_policy_json(&resolved_object_field(input, "inventoryPolicy")),
        pricing_policies: pricing_policies_json(
            &resolved_object_list_field(input, "pricingPolicies"),
            shop_currency_code,
            created_at,
        ),
    }
}

fn apply_selling_plan_update(
    plan: &mut SellingPlanRecord,
    input: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) {
    if let Some(name) = resolved_string_field(input, "name") {
        plan.name = name;
    }
    if let Some(description) = resolved_string_field(input, "description") {
        plan.description = description;
    }
    if input.contains_key("options") {
        plan.options = list_string_field(input, "options");
    }
    if let Some(position) = resolved_int_field(input, "position") {
        plan.position = position;
    }
    if input.contains_key("category") {
        plan.category = resolved_string_field(input, "category").unwrap_or_default();
    }
    if let Some(policy) = resolved_object_field(input, "billingPolicy") {
        plan.billing_policy = billing_policy_json(&Some(policy));
    }
    if let Some(policy) = resolved_object_field(input, "deliveryPolicy") {
        plan.delivery_policy = delivery_policy_json(&Some(policy));
    }
    if let Some(policy) = resolved_object_field(input, "inventoryPolicy") {
        plan.inventory_policy = inventory_policy_json(&Some(policy));
    }
    plan.pricing_policies = pricing_policies_json(
        &resolved_object_list_field(input, "pricingPolicies"),
        shop_currency_code,
        &plan.created_at,
    );
}

fn billing_policy_json(policy: &Option<BTreeMap<String, ResolvedValue>>) -> Value {
    let recurring = policy
        .as_ref()
        .and_then(|policy| resolved_object_field(policy, "recurring"));
    if let Some(recurring) = recurring {
        return json!({
            "__typename": "SellingPlanRecurringBillingPolicy",
            "interval": resolved_string_field(&recurring, "interval").unwrap_or_else(|| "MONTH".to_string()),
            "intervalCount": resolved_int_field(&recurring, "intervalCount").unwrap_or(1),
            "minCycles": resolved_int_field(&recurring, "minCycles"),
            "maxCycles": resolved_int_field(&recurring, "maxCycles")
        });
    }
    json!({ "__typename": "SellingPlanFixedBillingPolicy" })
}

fn delivery_policy_json(policy: &Option<BTreeMap<String, ResolvedValue>>) -> Value {
    let recurring = policy
        .as_ref()
        .and_then(|policy| resolved_object_field(policy, "recurring"));
    if let Some(recurring) = recurring {
        return json!({
            "__typename": "SellingPlanRecurringDeliveryPolicy",
            "interval": resolved_string_field(&recurring, "interval").unwrap_or_else(|| "MONTH".to_string()),
            "intervalCount": resolved_int_field(&recurring, "intervalCount").unwrap_or(1),
            "cutoff": resolved_int_field(&recurring, "cutoff").unwrap_or(0),
            "intent": resolved_string_field(&recurring, "intent").unwrap_or_else(|| "FULFILLMENT_BEGIN".to_string()),
            "preAnchorBehavior": resolved_string_field(&recurring, "preAnchorBehavior").unwrap_or_else(|| "ASAP".to_string())
        });
    }
    json!({ "__typename": "SellingPlanFixedDeliveryPolicy" })
}

fn inventory_policy_json(policy: &Option<BTreeMap<String, ResolvedValue>>) -> Value {
    json!({
        "reserve": policy
            .as_ref()
            .and_then(|policy| resolved_string_field(policy, "reserve"))
            .unwrap_or_else(|| "ON_FULFILLMENT".to_string())
    })
}

fn pricing_policies_json(
    policies: &[BTreeMap<String, ResolvedValue>],
    shop_currency_code: &str,
    created_at: &str,
) -> Vec<Value> {
    policies
        .iter()
        .filter_map(|policy| {
            if let Some(fixed) = resolved_object_field(policy, "fixed") {
                return Some(fixed_pricing_policy_json(&fixed, shop_currency_code));
            }
            let recurring = resolved_object_field(policy, "recurring")?;
            let after_cycle = resolved_int_field(&recurring, "afterCycle").unwrap_or(0);
            if after_cycle <= 0 {
                return Some(fixed_pricing_policy_json(&recurring, shop_currency_code));
            }
            Some(json!({
                "__typename": "SellingPlanRecurringPricingPolicy",
                "afterCycle": after_cycle,
                "createdAt": created_at,
                "adjustmentType": pricing_policy_adjustment_type(&recurring),
                "adjustmentValue": pricing_policy_adjustment_value_json(&recurring, shop_currency_code)
            }))
        })
        .collect()
}

fn fixed_pricing_policy_json(
    policy: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    json!({
        "__typename": "SellingPlanFixedPricingPolicy",
        "adjustmentType": pricing_policy_adjustment_type(policy),
        "adjustmentValue": pricing_policy_adjustment_value_json(policy, shop_currency_code)
    })
}

fn pricing_policy_adjustment_type(policy: &BTreeMap<String, ResolvedValue>) -> String {
    resolved_string_field(policy, "adjustmentType").unwrap_or_else(|| "PERCENTAGE".to_string())
}

fn pricing_policy_adjustment_value_json(
    policy: &BTreeMap<String, ResolvedValue>,
    shop_currency_code: &str,
) -> Value {
    let adjustment_value = resolved_object_field(policy, "adjustmentValue").unwrap_or_default();
    if let Some(fixed_value) = resolved_decimal_text_field(&adjustment_value, "fixedValue") {
        json!({
            "__typename": "MoneyV2",
            "amount": fixed_value,
            "currencyCode": shop_currency_code
        })
    } else {
        json!({
            "__typename": "SellingPlanPricingPolicyPercentageValue",
            "percentage": resolved_number_field(&adjustment_value, "percentage").unwrap_or(0.0)
        })
    }
}

fn selling_plan_group_summary(
    group: &SellingPlanGroupRecord,
    shop_money_format: Option<&str>,
) -> String {
    let plan_count = group.selling_plans.len();
    if plan_count == 0 {
        return String::new();
    }

    let mut percentages = Vec::new();
    let mut fixed_values = Vec::new();
    let mut fixed_currency_code = None::<String>;
    for policy in group
        .selling_plans
        .iter()
        .flat_map(|plan| plan.pricing_policies.iter())
    {
        if let Some(percentage) = policy
            .pointer("/adjustmentValue/percentage")
            .and_then(Value::as_f64)
        {
            percentages.push(percentage);
        }
        if let Some(fixed_value) = policy
            .pointer("/adjustmentValue/amount")
            .and_then(json_number_value)
        {
            fixed_values.push(fixed_value);
            if fixed_currency_code.is_none() {
                fixed_currency_code = policy
                    .pointer("/adjustmentValue/currencyCode")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
    }

    let mut discount_pieces = Vec::new();
    if let Some(piece) = percentage_summary_piece(&percentages) {
        discount_pieces.push(piece);
    }
    if let Some(piece) = fixed_value_summary_piece(
        &fixed_values,
        fixed_currency_code.as_deref(),
        shop_money_format,
    ) {
        discount_pieces.push(piece);
    }
    let discount = discount_pieces.join("·");
    let frequencies = if plan_count == 1 {
        "frequency"
    } else {
        "frequencies"
    };
    format!(
        "{} delivery {}, {} discount",
        plan_count, frequencies, discount
    )
}

fn json_number_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn percentage_summary_piece(values: &[f64]) -> Option<String> {
    let (min, max) = min_max(values)?;
    if (min - max).abs() < f64::EPSILON {
        Some(format!("{}%", format_summary_percentage(min)))
    } else {
        Some(format!(
            "{}-{}%",
            format_summary_percentage(min),
            format_summary_percentage(max)
        ))
    }
}

fn fixed_value_summary_piece(
    values: &[f64],
    currency_code: Option<&str>,
    shop_money_format: Option<&str>,
) -> Option<String> {
    let (min, max) = min_max(values)?;
    if (min - max).abs() < f64::EPSILON {
        Some(format_summary_currency(
            min,
            currency_code,
            shop_money_format,
        ))
    } else {
        Some(format!(
            "{}-{}",
            format_summary_currency(min, currency_code, shop_money_format),
            format_summary_currency(max, currency_code, shop_money_format)
        ))
    }
}

fn min_max(values: &[f64]) -> Option<(f64, f64)> {
    let mut values = values.iter().copied();
    let first = values.next()?;
    Some(values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    }))
}

fn format_summary_percentage(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let mut formatted = format!("{rounded:.2}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn format_summary_currency(
    value: f64,
    currency_code: Option<&str>,
    shop_money_format: Option<&str>,
) -> String {
    let formatted_amount = format!("{:.0}", value.round());
    if let Some(format) = shop_money_format {
        let rendered = format
            .replace("{{amount}}", &formatted_amount)
            .replace("{{ amount }}", &formatted_amount);
        if rendered != format {
            return rendered;
        }
    }
    match currency_code {
        Some("USD") => format!("${formatted_amount}"),
        Some(code) if !code.is_empty() => format!("{formatted_amount} {code}"),
        _ => formatted_amount,
    }
}

fn selling_plan_group_payload_value(
    proxy: &DraftProxy,
    group: Option<&SellingPlanGroupRecord>,
    deleted_plan_ids: Option<Option<Vec<String>>>,
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = json!({
        "sellingPlanGroup": group
            .map(|group| selling_plan_group_canonical_value(proxy, group))
            .unwrap_or(Value::Null),
        "userErrors": user_errors,
    });
    if let Some(deleted_plan_ids) = deleted_plan_ids {
        payload["deletedSellingPlanIds"] = deleted_plan_ids
            .map(|ids| json!(ids))
            .unwrap_or(Value::Null);
    }
    payload
}

fn selling_plan_group_delete_payload_value(
    deleted_id: Option<&str>,
    user_errors: Vec<Value>,
) -> Value {
    json!({
        "deletedSellingPlanGroupId": deleted_id.map_or(Value::Null, |id| json!(id)),
        "userErrors": user_errors,
    })
}

fn group_remove_payload_value(
    removed_ids: Option<Vec<String>>,
    user_errors: Vec<Value>,
    resource_kind: ResourceKind,
) -> Value {
    let removed_field = match resource_kind {
        ResourceKind::Product => "removedProductIds",
        ResourceKind::ProductVariant => "removedProductVariantIds",
    };
    json!({
        removed_field: removed_ids.map_or(Value::Null, |ids| json!(ids)),
        "userErrors": user_errors,
    })
}

fn resource_join_leave_payload_value(
    proxy: &DraftProxy,
    resource_kind: ResourceKind,
    resource_id: &str,
    user_errors: Vec<Value>,
) -> Value {
    let mut payload = json!({ "userErrors": user_errors });
    payload[resource_kind.payload_resource_field()] = match resource_kind {
        ResourceKind::Product => proxy
            .store
            .product_by_id(resource_id)
            .map(|product| proxy.product_canonical_value(product))
            .unwrap_or(Value::Null),
        ResourceKind::ProductVariant => proxy
            .store
            .product_variant_by_id(resource_id)
            .map(|variant| proxy.product_variant_canonical_value(variant))
            .unwrap_or(Value::Null),
    };
    payload
}

fn selling_plan_group_canonical_value(proxy: &DraftProxy, group: &SellingPlanGroupRecord) -> Value {
    json!({
        "__typename": "SellingPlanGroup",
        "appId": group.app_id,
        "createdAt": group.created_at,
        "description": group.description,
        "id": group.id,
        "merchantCode": group.merchant_code,
        "name": group.name,
        "options": group.options,
        "position": group.position,
        "summary": selling_plan_group_summary(group, proxy.store.shop_money_format().as_deref()),
        "productsCount": count_object(group.product_ids.len()),
        "productVariantsCount": count_object(group.product_variant_ids.len()),
        "products": connection_json(group.product_ids.iter().filter_map(|id| {
            proxy.store.product_by_id(id).map(|product| proxy.product_canonical_value(product))
        }).collect()),
        "productVariants": connection_json(group.product_variant_ids.iter().filter_map(|id| {
            proxy.store.product_variant_by_id(id).map(|variant| proxy.product_variant_canonical_value(variant))
        }).collect()),
        "sellingPlans": connection_json(group.selling_plans.iter().map(selling_plan_canonical_value).collect()),
    })
}

fn selling_plan_canonical_value(plan: &SellingPlanRecord) -> Value {
    json!({
        "__typename": "SellingPlan",
        "id": plan.id,
        "name": plan.name,
        "description": plan.description,
        "options": plan.options,
        "position": plan.position,
        "category": plan.category,
        "createdAt": plan.created_at,
        "billingPolicy": plan.billing_policy,
        "deliveryPolicy": plan.delivery_policy,
        "inventoryPolicy": plan.inventory_policy,
        "pricingPolicies": plan.pricing_policies,
    })
}

fn group_does_not_exist_error() -> Value {
    user_error(
        ["id"],
        "Selling plan group does not exist.",
        Some("GROUP_DOES_NOT_EXIST"),
    )
}

fn resource_members(group: &SellingPlanGroupRecord, resource_kind: ResourceKind) -> &Vec<String> {
    match resource_kind {
        ResourceKind::Product => &group.product_ids,
        ResourceKind::ProductVariant => &group.product_variant_ids,
    }
}

fn resource_members_mut(
    group: &mut SellingPlanGroupRecord,
    resource_kind: ResourceKind,
) -> &mut Vec<String> {
    match resource_kind {
        ResourceKind::Product => &mut group.product_ids,
        ResourceKind::ProductVariant => &mut group.product_variant_ids,
    }
}
