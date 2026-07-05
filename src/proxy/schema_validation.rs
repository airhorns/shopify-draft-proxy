use super::*;

use std::sync::OnceLock;

mod builders;
mod engine;

pub(in crate::proxy) use builders::*;
pub(in crate::proxy) use engine::*;

#[derive(Debug, Clone)]
struct SchemaTypeRef {
    display: String,
    named_type: String,
    non_null: bool,
}

#[derive(Debug, Clone)]
struct SchemaField {
    type_ref: SchemaTypeRef,
    has_default: bool,
}

type SchemaArgument = SchemaField;
type SchemaInputField = SchemaField;

#[derive(Debug, Clone, Default)]
struct AdminInputSchema {
    mutation_fields: BTreeMap<String, BTreeMap<String, SchemaArgument>>,
    input_objects: BTreeMap<String, BTreeMap<String, SchemaInputField>>,
    strict_input_objects: BTreeSet<String>,
    enum_values: BTreeMap<String, Vec<String>>,
}

impl AdminInputSchema {
    fn insert_strict_input_object(
        &mut self,
        name: impl Into<String>,
        fields: BTreeMap<String, SchemaInputField>,
    ) {
        let name = name.into();
        self.strict_input_objects.insert(name.clone());
        self.input_objects.insert(name, fields);
    }

    fn input_object_is_strict(&self, name: &str) -> bool {
        self.strict_input_objects.contains(name)
    }
}

#[derive(Debug, Clone)]
struct OutputFieldType {
    named_type: String,
    composite: bool,
}

#[derive(Debug, Clone, Default)]
struct AdminOutputSchema {
    query_root_fields: BTreeMap<String, OutputFieldType>,
    mutation_root_fields: BTreeMap<String, OutputFieldType>,
    fields_by_parent: BTreeMap<String, BTreeMap<String, OutputFieldType>>,
}

impl AdminOutputSchema {
    fn insert_local_projection_field(
        &mut self,
        parent_type: &str,
        name: &str,
        named_type: &str,
        composite: bool,
    ) {
        self.fields_by_parent
            .entry(parent_type.to_string())
            .or_default()
            .insert(
                name.to_string(),
                OutputFieldType {
                    named_type: named_type.to_string(),
                    composite,
                },
            );
    }

    fn insert_local_scalar_field(&mut self, parent_type: &str, name: &str, named_type: &str) {
        self.insert_local_projection_field(parent_type, name, named_type, false);
    }

    fn insert_local_object_field(&mut self, parent_type: &str, name: &str, named_type: &str) {
        self.insert_local_projection_field(parent_type, name, named_type, true);
    }

    fn insert_local_connection_field(&mut self, parent_type: &str, name: &str, node_type: &str) {
        self.insert_local_projection_field(
            parent_type,
            name,
            &format!("{node_type}Connection"),
            true,
        );
    }

    fn apply_local_projection_extensions(&mut self) {
        // These fields are projected by existing local overlay-read handlers but
        // are absent from the captured bulk-query schema JSON. Keep them
        // explicit so generic validation still rejects arbitrary unknown fields.
        self.insert_local_connection_field("Catalog", "markets", "Market");
        self.insert_local_scalar_field("CompanyLocation", "billingSameAsShipping", "Boolean");
        self.insert_local_scalar_field("File", "contentType", "FileContentType");
        self.insert_local_scalar_field("File", "filename", "String");
        self.insert_local_projection_field("File", "mediaErrors", "MediaError", true);
        self.insert_local_projection_field("File", "mediaWarnings", "MediaWarning", true);
        self.insert_local_scalar_field("GenericFile", "filename", "String");
        self.insert_local_scalar_field("HasMetafields", "id", "ID");
        self.insert_local_scalar_field(
            "CustomerCreditCardBillingAddress",
            "countryCodeV2",
            "String",
        );
        self.insert_local_scalar_field("MarketingActivity", "remoteId", "String");
        self.insert_local_scalar_field("MarketRegion", "code", "String");
        self.insert_local_scalar_field("Model3d", "filename", "String");
        self.insert_local_projection_field("Model3d", "mediaErrors", "MediaError", true);
        self.insert_local_projection_field("Model3d", "mediaWarnings", "MediaWarning", true);
        self.insert_local_scalar_field("Model3d", "mimeType", "String");
        self.insert_local_scalar_field("OrderTransaction", "paymentReferenceId", "ID");
        self.insert_local_scalar_field("PaymentCustomization", "functionHandle", "String");
        self.insert_local_connection_field("RegionsCondition", "regions", "MarketRegion");
        self.insert_local_scalar_field(
            "ReverseFulfillmentOrderLineItem",
            "remainingQuantity",
            "Int",
        );
        self.insert_local_object_field(
            "ReverseFulfillmentOrderLineItem",
            "returnLineItem",
            "ReturnLineItemType",
        );
        self.insert_local_scalar_field("ScriptTag", "event", "String");
        self.insert_local_object_field("Segment", "author", "StaffMember");
        self.insert_local_scalar_field("Segment", "percentageSnapshot", "Float");
        self.insert_local_scalar_field("Segment", "percentageSnapshotUpdatedAt", "DateTime");
        self.insert_local_scalar_field("Segment", "tagMigrated", "Boolean");
        self.insert_local_scalar_field("Segment", "translation", "String");
        self.insert_local_scalar_field("Segment", "valid", "Boolean");
        self.insert_local_scalar_field("TaxAppConfiguration", "id", "ID");
        self.insert_local_scalar_field("TaxAppConfiguration", "ready", "Boolean");
        self.insert_local_scalar_field("TaxAppConfiguration", "updatedAt", "DateTime");
        self.insert_local_scalar_field("WebPixel", "status", "String");
        self.insert_local_scalar_field("WebPixel", "webhookEndpointAddress", "String");
    }
}

#[derive(Debug, Clone, Copy)]
enum AdminSchemaKind {
    Mutation,
    BulkQuery,
}

struct VersionedSchemaCache<T> {
    schema_2025_01: OnceLock<T>,
    schema_2025_10: OnceLock<T>,
    schema_2026_01: OnceLock<T>,
    schema_2026_04: OnceLock<T>,
}

impl<T> VersionedSchemaCache<T> {
    const fn new() -> Self {
        Self {
            schema_2025_01: OnceLock::new(),
            schema_2025_10: OnceLock::new(),
            schema_2026_01: OnceLock::new(),
            schema_2026_04: OnceLock::new(),
        }
    }

    fn get_or_init(
        &'static self,
        api_version: &str,
        init: impl FnOnce() -> T,
    ) -> Option<&'static T> {
        let cache = match api_version {
            "2025-01" => &self.schema_2025_01,
            "2025-10" => &self.schema_2025_10,
            "2026-01" => &self.schema_2026_01,
            "2026-04" => &self.schema_2026_04,
            _ => return None,
        };
        Some(cache.get_or_init(init))
    }
}

fn public_admin_schema_json(api_version: &str, kind: AdminSchemaKind) -> Value {
    let raw = match (api_version, kind) {
        ("2025-01", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2025-01/mutation-schema.json")
        }
        ("2025-10", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2025-10/mutation-schema.json")
        }
        ("2026-01", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2026-01/mutation-schema.json")
        }
        ("2026-04", AdminSchemaKind::Mutation) => {
            include_str!("../../config/admin-graphql/2026-04/mutation-schema.json")
        }
        ("2025-01", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2025-01/bulk-query-schema.json")
        }
        ("2025-10", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2025-10/bulk-query-schema.json")
        }
        ("2026-01", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2026-01/bulk-query-schema.json")
        }
        ("2026-04", AdminSchemaKind::BulkQuery) => {
            include_str!("../../config/admin-graphql/2026-04/bulk-query-schema.json")
        }
        _ => panic!("unsupported Admin API version has no captured schema: {api_version}"),
    };
    serde_json::from_str(raw).expect("checked-in Admin GraphQL schema should be valid JSON")
}

fn public_admin_input_schema(api_version: &str) -> Option<&'static AdminInputSchema> {
    static CACHE: VersionedSchemaCache<AdminInputSchema> = VersionedSchemaCache::new();
    CACHE.get_or_init(api_version, || {
        let mut schema = AdminInputSchema::default();
        extend_captured_admin_input_schema(&mut schema, api_version);
        extend_gift_card_input_schema(&mut schema);
        extend_discount_basic_input_schema(&mut schema);
        extend_app_input_schema(&mut schema);
        extend_customer_merge_input_schema(&mut schema);
        extend_customer_input_schema(&mut schema);
        extend_orders_input_schema(&mut schema);
        extend_marketing_engagement_input_schema(&mut schema);
        extend_media_input_schema(&mut schema, api_version);
        extend_functions_input_schema(&mut schema);
        extend_online_store_input_schema(&mut schema);
        extend_markets_input_schema(&mut schema, api_version);
        extend_webhook_input_schema(&mut schema, api_version);
        extend_metafield_definition_input_schema(&mut schema);
        extend_product_input_schema(&mut schema, api_version);
        extend_product_variant_input_schema(&mut schema);
        extend_publication_input_schema(&mut schema);
        extend_saved_search_input_schema(&mut schema, api_version);
        extend_payments_input_schema(&mut schema);
        extend_shipping_input_schema(&mut schema);
        extend_fulfillment_event_input_schema(&mut schema);
        extend_store_credit_input_schema(&mut schema);
        schema
    })
}

fn extend_captured_admin_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);
    for mutation in parsed
        .get("mutations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = mutation.get("name").and_then(Value::as_str) else {
            continue;
        };
        let arguments = mutation
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(schema_argument)
            .collect::<BTreeMap<_, _>>();
        schema.mutation_fields.insert(name.to_string(), arguments);
    }
    for input_object in parsed
        .get("inputObjects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = input_object.get("name").and_then(Value::as_str) else {
            continue;
        };
        let fields = input_object
            .get("inputFields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(schema_input_field)
            .collect::<BTreeMap<_, _>>();
        schema.input_objects.insert(name.to_string(), fields);
    }
    for enum_type in parsed
        .get("enums")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = enum_type.get("name").and_then(Value::as_str) else {
            continue;
        };
        let values = enum_type
            .get("values")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| value.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        schema.enum_values.insert(name.to_string(), values);
    }
}

fn captured_mutation_arguments(
    parsed: &Value,
    mutation_name: &str,
) -> Option<(String, BTreeMap<String, SchemaArgument>)> {
    let mutation = parsed
        .get("mutations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|mutation| mutation.get("name").and_then(Value::as_str) == Some(mutation_name))?;
    let arguments = mutation
        .get("args")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(schema_argument)
        .collect::<BTreeMap<_, _>>();
    Some((mutation_name.to_string(), arguments))
}

fn captured_input_object_fields(
    parsed: &Value,
    input_object_name: &str,
) -> Option<(String, BTreeMap<String, SchemaInputField>)> {
    let input_object = parsed
        .get("inputObjects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|input_object| {
            input_object.get("name").and_then(Value::as_str) == Some(input_object_name)
        })?;
    let fields = input_object
        .get("inputFields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(schema_input_field)
        .collect::<BTreeMap<_, _>>();
    Some((input_object_name.to_string(), fields))
}

fn schema_argument(argument: &Value) -> Option<(String, SchemaArgument)> {
    let name = argument.get("name").and_then(Value::as_str)?;
    let type_ref = schema_type_ref(argument.get("type")?)?;
    Some((
        name.to_string(),
        SchemaField {
            type_ref,
            has_default: has_default_value(argument),
        },
    ))
}

fn schema_input_field(field: &Value) -> Option<(String, SchemaInputField)> {
    let name = field.get("name").and_then(Value::as_str)?;
    let type_ref = schema_type_ref(field.get("type")?)?;
    Some((
        name.to_string(),
        SchemaField {
            type_ref,
            has_default: has_default_value(field),
        },
    ))
}

fn has_default_value(field_or_argument: &Value) -> bool {
    field_or_argument
        .get("defaultValue")
        .is_some_and(|default_value| !default_value.is_null())
}

fn schema_type_ref(value: &Value) -> Option<SchemaTypeRef> {
    let (display, named_type, non_null) = schema_type_ref_parts(value)?;
    Some(SchemaTypeRef {
        display,
        named_type,
        non_null,
    })
}

fn schema_type_ref_parts(value: &Value) -> Option<(String, String, bool)> {
    let kind = value.get("kind").and_then(Value::as_str)?;
    match kind {
        "NON_NULL" => {
            let (display, named_type, _) = schema_type_ref_parts(value.get("ofType")?)?;
            Some((format!("{display}!"), named_type, true))
        }
        "LIST" => {
            let (display, named_type, _) = schema_type_ref_parts(value.get("ofType")?)?;
            Some((format!("[{display}]"), named_type, false))
        }
        _ => {
            let name = value.get("name").and_then(Value::as_str)?;
            Some((name.to_string(), name.to_string(), false))
        }
    }
}

fn extend_product_variant_input_schema(schema: &mut AdminInputSchema) {
    // The public Admin schema for `ProductVariantsBulkInput` exposes
    // `optionValues`, not the legacy/internal `options` key. Registering the
    // bulk input object keeps unsupported keys as GraphQL coercion errors before
    // the local product variant handler stages anything.
    schema.insert_strict_input_object(
        "ProductVariantsBulkInput".to_string(),
        BTreeMap::from([
            ("barcode".to_string(), input_field(named("String"))),
            ("compareAtPrice".to_string(), input_field(named("Money"))),
            ("id".to_string(), input_field(named("ID"))),
            (
                "mediaSrc".to_string(),
                input_field(list_of_non_null("String")),
            ),
            (
                "inventoryPolicy".to_string(),
                input_field(named("ProductVariantInventoryPolicy")),
            ),
            (
                "inventoryQuantities".to_string(),
                input_field(list_of_non_null("InventoryLevelInput")),
            ),
            (
                "quantityAdjustments".to_string(),
                input_field(list_of_non_null("InventoryQuantityAdjustmentInput")),
            ),
            (
                "inventoryItem".to_string(),
                input_field(named("InventoryItemInput")),
            ),
            ("mediaId".to_string(), input_field(named("ID"))),
            (
                "metafields".to_string(),
                input_field(list_of_non_null("MetafieldInput")),
            ),
            (
                "optionValues".to_string(),
                input_field(list_of_non_null("VariantOptionValueInput")),
            ),
            ("price".to_string(), input_field(named("Money"))),
            ("taxable".to_string(), input_field(named("Boolean"))),
            ("taxCode".to_string(), input_field(named("String"))),
            (
                "unitPriceMeasurement".to_string(),
                input_field(named("UnitPriceMeasurementInput")),
            ),
            ("showUnitPrice".to_string(), input_field(named("Boolean"))),
            (
                "requiresComponents".to_string(),
                input_field(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "productVariantsBulkCreate".to_string(),
        BTreeMap::from([
            ("productId".to_string(), mutation_arg(non_null("ID"))),
            (
                "variants".to_string(),
                mutation_arg(non_null_list_of_non_null("ProductVariantsBulkInput")),
            ),
            (
                "strategy".to_string(),
                mutation_arg(named("ProductVariantsBulkCreateStrategy")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "productVariantsBulkUpdate".to_string(),
        BTreeMap::from([
            ("productId".to_string(), mutation_arg(non_null("ID"))),
            (
                "variants".to_string(),
                mutation_arg(non_null_list_of_non_null("ProductVariantsBulkInput")),
            ),
            (
                "allowPartialUpdates".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
}

fn extend_product_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    if let Some((name, fields)) = captured_input_object_fields(&parsed, "ProductDeleteInput") {
        schema.insert_strict_input_object(name, fields);
    }
    if let Some((name, fields)) = captured_input_object_fields(&parsed, "ProductFeedInput") {
        schema.insert_strict_input_object(name, fields);
    }
    schema.mutation_fields.insert(
        "productDelete".to_string(),
        BTreeMap::from([
            (
                "input".to_string(),
                mutation_arg(named("ProductDeleteInput")),
            ),
            (
                "product".to_string(),
                mutation_arg(named("ProductDeleteInput")),
            ),
            ("synchronous".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
}

fn extend_publication_input_schema(schema: &mut AdminInputSchema) {
    schema.insert_strict_input_object(
        "PublicationCreateInput".to_string(),
        BTreeMap::from([
            ("catalogId".to_string(), input_field(named("ID"))),
            (
                "defaultState".to_string(),
                input_field(named("PublicationCreateInputPublicationDefaultState")),
            ),
            ("autoPublish".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.insert_strict_input_object(
        "PublicationUpdateInput".to_string(),
        BTreeMap::from([
            (
                "publishablesToAdd".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "publishablesToRemove".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            ("autoPublish".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.mutation_fields.insert(
        "publicationCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("PublicationCreateInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "publicationUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("PublicationUpdateInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "publicationDelete".to_string(),
        BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
    );
    for root in ["publishablePublish", "publishableUnpublish"] {
        schema.mutation_fields.insert(
            root.to_string(),
            BTreeMap::from([
                ("id".to_string(), mutation_arg(non_null("ID"))),
                (
                    "input".to_string(),
                    mutation_arg(non_null_list_of_non_null("PublicationInput")),
                ),
            ]),
        );
    }
    for root in [
        "publishablePublishToCurrentChannel",
        "publishableUnpublishToCurrentChannel",
    ] {
        schema.mutation_fields.insert(
            root.to_string(),
            BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
        );
    }
}

fn extend_saved_search_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    for input_object_name in ["SavedSearchCreateInput", "SavedSearchUpdateInput"] {
        if let Some((name, fields)) = captured_input_object_fields(&parsed, input_object_name) {
            schema.insert_strict_input_object(name, fields);
        }
    }
    for mutation_name in ["savedSearchCreate", "savedSearchUpdate"] {
        if let Some((name, arguments)) = captured_mutation_arguments(&parsed, mutation_name) {
            let arguments = arguments
                .into_iter()
                .map(|(argument_name, mut argument)| {
                    if argument_name == "input" {
                        argument.type_ref.non_null = false;
                        argument.type_ref.display = argument.type_ref.named_type.clone();
                    }
                    (argument_name, argument)
                })
                .collect();
            schema.mutation_fields.insert(name, arguments);
        }
    }
}

fn extend_fulfillment_event_input_schema(schema: &mut AdminInputSchema) {
    // `fulfillmentEventCreate(fulfillmentEvent: FulfillmentEventInput!)` on the
    // active public Admin schema (2026-04). `status` is a non-null
    // `FulfillmentEventStatus` enum, so an out-of-range value must surface a
    // top-level `INVALID_VARIABLE` coercion error (anchored at the variable
    // definition) before the local handler runs. Every other accepted field is
    // registered nullable so the validator only rejects an out-of-range `status`
    // or an unknown field, and never fabricates a missing-required error for the
    // happy-path mutation that omits the optional geolocation fields.
    schema.insert_strict_input_object(
        "FulfillmentEventInput".to_string(),
        BTreeMap::from([
            ("fulfillmentId".to_string(), input_field(named("ID"))),
            (
                "status".to_string(),
                input_field(non_null("FulfillmentEventStatus")),
            ),
            ("message".to_string(), input_field(named("String"))),
            ("happenedAt".to_string(), input_field(named("DateTime"))),
            (
                "estimatedDeliveryAt".to_string(),
                input_field(named("DateTime")),
            ),
            ("city".to_string(), input_field(named("String"))),
            ("province".to_string(), input_field(named("String"))),
            ("country".to_string(), input_field(named("String"))),
            ("zip".to_string(), input_field(named("String"))),
            ("address1".to_string(), input_field(named("String"))),
            ("latitude".to_string(), input_field(named("Float"))),
            ("longitude".to_string(), input_field(named("Float"))),
        ]),
    );
    schema.mutation_fields.insert(
        "fulfillmentEventCreate".to_string(),
        BTreeMap::from([(
            "fulfillmentEvent".to_string(),
            mutation_arg(non_null("FulfillmentEventInput")),
        )]),
    );
}

fn extend_store_credit_input_schema(schema: &mut AdminInputSchema) {
    // `storeCreditAccountCredit` / `storeCreditAccountDebit` on Admin API 2026-04
    // are staged locally, but their input objects are registered here so an
    // unsupported field (e.g. `attribution`, or `notify` on a *debit* input where
    // it is not defined) surfaces a top-level `INVALID_VARIABLE` coercion error
    // before the resolver runs — exactly as the live schema rejects fields it does
    // not define. `MoneyInput` is intentionally left unregistered so the resolver
    // owns money-field validation and the nested `amount`/`currencyCode` fields are
    // never flagged as unknown.
    schema.insert_strict_input_object(
        "StoreCreditAccountCreditInput".to_string(),
        BTreeMap::from([
            (
                "creditAmount".to_string(),
                input_field(non_null("MoneyInput")),
            ),
            ("expiresAt".to_string(), input_field(named("DateTime"))),
            ("notify".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.insert_strict_input_object(
        "StoreCreditAccountDebitInput".to_string(),
        BTreeMap::from([(
            "debitAmount".to_string(),
            input_field(non_null("MoneyInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "storeCreditAccountCredit".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "creditInput".to_string(),
                mutation_arg(non_null("StoreCreditAccountCreditInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "storeCreditAccountDebit".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "debitInput".to_string(),
                mutation_arg(non_null("StoreCreditAccountDebitInput")),
            ),
        ]),
    );
}

fn extend_shipping_input_schema(schema: &mut AdminInputSchema) {
    // `fulfillmentServiceCreate` on the active public Admin schema (2026-04) accepts
    // only these field arguments. `permitsSkuSharing`, `inventorySyncEnabled`, and
    // `fulfillmentOrdersOptIn` are not exposed, so supplying one must raise a top-level
    // `argumentNotAccepted` GraphQL error (anchored at the argument name token) before
    // the resolver runs. Every accepted argument is registered nullable so the validator
    // only rejects unaccepted arguments and never fabricates a missing-required error for
    // the create docs that omit `callbackUrl`.
    schema.mutation_fields.insert(
        "fulfillmentServiceCreate".to_string(),
        BTreeMap::from([
            ("name".to_string(), mutation_arg(named("String"))),
            ("callbackUrl".to_string(), mutation_arg(named("URL"))),
            (
                "trackingSupport".to_string(),
                mutation_arg(named("Boolean")),
            ),
            (
                "inventoryManagement".to_string(),
                mutation_arg(named("Boolean")),
            ),
            (
                "requiresShippingMethod".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
}

fn extend_payments_input_schema(schema: &mut AdminInputSchema) {
    schema.insert_strict_input_object(
        "PaymentTermsCreateInput".to_string(),
        BTreeMap::from([
            (
                "paymentTermsTemplateId".to_string(),
                input_field(non_null("ID")),
            ),
            (
                "paymentSchedules".to_string(),
                input_field(list_of_non_null("PaymentScheduleInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "PaymentTermsInput".to_string(),
        BTreeMap::from([
            (
                "paymentTermsTemplateId".to_string(),
                input_field(named("ID")),
            ),
            (
                "paymentSchedules".to_string(),
                input_field(list_of_non_null("PaymentScheduleInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "PaymentTermsUpdateInput".to_string(),
        BTreeMap::from([
            ("paymentTermsId".to_string(), input_field(non_null("ID"))),
            (
                "paymentTermsAttributes".to_string(),
                input_field(non_null("PaymentTermsInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "paymentTermsCreate".to_string(),
        BTreeMap::from([
            ("referenceId".to_string(), mutation_arg(non_null("ID"))),
            (
                "paymentTermsAttributes".to_string(),
                mutation_arg(non_null("PaymentTermsCreateInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "paymentTermsUpdate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("PaymentTermsUpdateInput")),
        )]),
    );

    // customerPaymentMethodCreditCardCreate on Admin API 2026-04 takes three
    // required (non-null) field arguments: `customerId`, `billingAddress`, and
    // `sessionId`. Omitting any of them must surface a top-level
    // `missingRequiredArguments` error before the local payment-method handler
    // runs (the field-vault handler only owns billing-address blank checks once
    // the arguments are structurally present). `MailingAddressInput` is left
    // unregistered so the resolver continues to own per-field blank validation.
    schema.mutation_fields.insert(
        "customerPaymentMethodCreditCardCreate".to_string(),
        BTreeMap::from([
            ("customerId".to_string(), mutation_arg(non_null("ID"))),
            (
                "billingAddress".to_string(),
                mutation_arg(non_null("MailingAddressInput")),
            ),
            ("sessionId".to_string(), mutation_arg(non_null("String"))),
        ]),
    );
}

fn input_field(type_ref: SchemaTypeRef) -> SchemaInputField {
    SchemaField {
        type_ref,
        has_default: false,
    }
}

fn mutation_arg(type_ref: SchemaTypeRef) -> SchemaArgument {
    SchemaField {
        type_ref,
        has_default: false,
    }
}

fn extend_gift_card_input_schema(schema: &mut AdminInputSchema) {
    schema.insert_strict_input_object(
        "GiftCardCreateInput".to_string(),
        BTreeMap::from([
            ("initialValue".to_string(), input_field(non_null("Decimal"))),
            ("code".to_string(), input_field(named("String"))),
            ("customerId".to_string(), input_field(named("ID"))),
            ("expiresOn".to_string(), input_field(named("Date"))),
            ("note".to_string(), input_field(named("String"))),
            (
                "recipientAttributes".to_string(),
                input_field(named("GiftCardRecipientInput")),
            ),
            ("templateSuffix".to_string(), input_field(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "giftCardCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("GiftCardCreateInput")),
        )]),
    );
}

fn extend_markets_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    for input_object_name in [
        "MarketCurrencySettingsUpdateInput",
        "MarketCreateInput",
        "MarketUpdateInput",
    ] {
        if let Some((name, fields)) = captured_input_object_fields(&parsed, input_object_name) {
            schema.insert_strict_input_object(name, fields);
        }
    }
    for input_object_name in ["MarketCreateInput", "MarketUpdateInput"] {
        if let Some(fields) = schema.input_objects.get_mut(input_object_name) {
            if let Some(field) = fields.get_mut("priceInclusions") {
                field.type_ref = named("MarketPriceInclusionsInputLocal");
            }
        }
    }
    for mutation_name in ["marketCreate", "marketUpdate"] {
        if let Some((name, arguments)) = captured_mutation_arguments(&parsed, mutation_name) {
            schema.mutation_fields.insert(name, arguments);
        }
    }

    // CatalogCreateInput on Admin API 2026-04: `context` is a required
    // (non-null) input field. Omitting it must surface a top-level
    // INVALID_VARIABLE coercion error before the local catalog handler runs.
    schema.insert_strict_input_object(
        "CatalogCreateInput".to_string(),
        BTreeMap::from([
            ("title".to_string(), input_field(named("String"))),
            ("status".to_string(), input_field(named("String"))),
            (
                "context".to_string(),
                input_field(non_null("CatalogContextInput")),
            ),
            ("priceListId".to_string(), input_field(named("ID"))),
            ("publicationId".to_string(), input_field(named("ID"))),
        ]),
    );
    schema.mutation_fields.insert(
        "catalogCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("CatalogCreateInput")),
        )]),
    );

    // PriceListCreateInput on Admin API 2026-04: `currency` (a CurrencyCode
    // enum) and `parent` are both required. An out-of-range currency plus a
    // missing parent yields two ordered problems ([currency, parent]).
    schema.insert_strict_input_object(
        "PriceListCreateInput".to_string(),
        BTreeMap::from([
            ("name".to_string(), input_field(named("String"))),
            (
                "currency".to_string(),
                input_field(non_null("CurrencyCode")),
            ),
            (
                "parent".to_string(),
                input_field(non_null("PriceListParentCreateInputLocal")),
            ),
            ("catalogId".to_string(), input_field(named("ID"))),
        ]),
    );
    schema.mutation_fields.insert(
        "priceListCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("PriceListCreateInput")),
        )]),
    );

    // PriceListUpdateInput on Admin API 2026-04: every field is optional on
    // update. `catalogId` is an ID; a blank string fails global-id coercion
    // (INVALID_VARIABLE) before the local handler runs. `parent`'s type is
    // intentionally left unregistered in `input_objects` so adjustment-range
    // checks stay with the local handler (which emits INVALID_ADJUSTMENT_VALUE
    // as a userError, not a coercion error).
    schema.insert_strict_input_object(
        "PriceListUpdateInput".to_string(),
        BTreeMap::from([
            ("name".to_string(), input_field(named("String"))),
            ("currency".to_string(), input_field(named("CurrencyCode"))),
            (
                "parent".to_string(),
                input_field(named("PriceListParentUpdateInputLocal")),
            ),
            ("catalogId".to_string(), input_field(named("ID"))),
        ]),
    );
    schema.mutation_fields.insert(
        "priceListUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("PriceListUpdateInput")),
            ),
        ]),
    );
}

fn extend_metafield_definition_input_schema(schema: &mut AdminInputSchema) {
    if let Some(args) = schema
        .mutation_fields
        .get_mut("standardMetafieldDefinitionEnable")
    {
        args.insert(
            "useAsAdminFilter".to_string(),
            mutation_arg(named("Boolean")),
        );
        args.insert("forceEnable".to_string(), mutation_arg(named("Boolean")));
    }
}

fn extend_marketing_engagement_input_schema(schema: &mut AdminInputSchema) {
    // Marketing activity and engagement money inputs default an omitted
    // currencyCode from the shop currency in the local model. Keep the shared
    // MoneyInput strict for order-edit/payment paths while letting these
    // marketing-specific fields reach the resolver for that defaulting branch.
    schema.insert_strict_input_object(
        "MarketingMoneyInputLocal".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(non_null("Decimal"))),
            (
                "currencyCode".to_string(),
                input_field(named("CurrencyCode")),
            ),
        ]),
    );
    if let Some(fields) = schema.input_objects.get_mut("MarketingActivityBudgetInput") {
        if let Some(field) = fields.get_mut("total") {
            field.type_ref = marketing_money_input_ref();
        }
    }
    for input_object_name in [
        "MarketingActivityCreateExternalInput",
        "MarketingActivityUpdateExternalInput",
        "MarketingActivityUpsertExternalInput",
        "MarketingActivityUpdateInput",
    ] {
        if let Some(fields) = schema.input_objects.get_mut(input_object_name) {
            if let Some(field) = fields.get_mut("adSpend") {
                field.type_ref = marketing_money_input_ref();
            }
        }
    }

    // MarketingEngagementInput on Admin API 2026-04: occurredOn, utcOffset, and
    // isCumulative are required (non-null) schema fields. Omitting any of them must
    // produce top-level coercion errors before the local handler stages anything.
    schema.insert_strict_input_object(
        "MarketingEngagementInput".to_string(),
        BTreeMap::from([
            ("occurredOn".to_string(), input_field(non_null("Date"))),
            ("utcOffset".to_string(), input_field(non_null("UtcOffset"))),
            ("isCumulative".to_string(), input_field(non_null("Boolean"))),
            ("impressionsCount".to_string(), input_field(named("Int"))),
            ("viewsCount".to_string(), input_field(named("Int"))),
            ("clicksCount".to_string(), input_field(named("Int"))),
            ("sharesCount".to_string(), input_field(named("Int"))),
            ("favoritesCount".to_string(), input_field(named("Int"))),
            ("commentsCount".to_string(), input_field(named("Int"))),
            ("unsubscribesCount".to_string(), input_field(named("Int"))),
            ("complaintsCount".to_string(), input_field(named("Int"))),
            ("failsCount".to_string(), input_field(named("Int"))),
            ("sendsCount".to_string(), input_field(named("Int"))),
            ("uniqueViewsCount".to_string(), input_field(named("Int"))),
            ("uniqueClicksCount".to_string(), input_field(named("Int"))),
            (
                "adSpend".to_string(),
                input_field(marketing_money_input_ref()),
            ),
            (
                "sales".to_string(),
                input_field(marketing_money_input_ref()),
            ),
            ("sessionsCount".to_string(), input_field(named("Int"))),
            ("orders".to_string(), input_field(named("Decimal"))),
            (
                "firstTimeCustomers".to_string(),
                input_field(named("Decimal")),
            ),
            (
                "returningCustomers".to_string(),
                input_field(named("Decimal")),
            ),
            (
                "primaryConversions".to_string(),
                input_field(named("Decimal")),
            ),
            ("allConversions".to_string(), input_field(named("Decimal"))),
        ]),
    );
    schema.mutation_fields.insert(
        "marketingEngagementCreate".to_string(),
        BTreeMap::from([
            ("marketingActivityId".to_string(), mutation_arg(named("ID"))),
            ("remoteId".to_string(), mutation_arg(named("String"))),
            ("channelHandle".to_string(), mutation_arg(named("String"))),
            (
                "marketingEngagement".to_string(),
                mutation_arg(non_null("MarketingEngagementInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "marketingActivityDeleteExternal".to_string(),
        BTreeMap::from([
            ("marketingActivityId".to_string(), mutation_arg(named("ID"))),
            ("remoteId".to_string(), mutation_arg(named("String"))),
            ("id".to_string(), mutation_arg(named("ID"))),
        ]),
    );
}

fn marketing_money_input_ref() -> SchemaTypeRef {
    let mut type_ref = named("MarketingMoneyInputLocal");
    type_ref.display = "MoneyInput".to_string();
    type_ref
}

fn extend_media_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    if let Some((name, fields)) = captured_input_object_fields(&parsed, "StagedUploadInput") {
        schema.insert_strict_input_object(name, fields);
    }
    if let Some((name, fields)) = captured_input_object_fields(&parsed, "FileUpdateInput") {
        schema.insert_strict_input_object(name, fields);
    }
}

fn extend_webhook_input_schema(schema: &mut AdminInputSchema, api_version: &str) {
    let parsed = public_admin_schema_json(api_version, AdminSchemaKind::Mutation);

    if let Some((name, fields)) =
        captured_input_object_fields(&parsed, "PubSubWebhookSubscriptionInput")
    {
        schema.insert_strict_input_object(name, fields);
    }
}

fn extend_functions_input_schema(schema: &mut AdminInputSchema) {
    // ValidationUpdateInput on Admin API 2026-04 accepts only enable,
    // blockOnFailure, metafields, and title. Rebinding a validation to a
    // different function is not supported, so functionId / functionHandle are
    // not fields on the input object — supplying them must raise a schema error
    // (argumentNotAccepted for a literal, INVALID_VARIABLE for a variable)
    // before the validationUpdate resolver runs.
    schema.insert_strict_input_object(
        "ValidationUpdateInput".to_string(),
        BTreeMap::from([
            ("enable".to_string(), input_field(named("Boolean"))),
            ("blockOnFailure".to_string(), input_field(named("Boolean"))),
            (
                "metafields".to_string(),
                input_field(list_of_non_null("MetafieldInput")),
            ),
            ("title".to_string(), input_field(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "validationUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "validation".to_string(),
                mutation_arg(non_null("ValidationUpdateInput")),
            ),
        ]),
    );
    // cartTransformCreate takes scalar root arguments only; the function is
    // selected by functionId or functionHandle. There is no `cartTransform`
    // wrapper input and no `title` argument, so supplying either must raise a
    // top-level argumentNotAccepted error.
    schema.mutation_fields.insert(
        "cartTransformCreate".to_string(),
        BTreeMap::from([
            ("functionId".to_string(), mutation_arg(named("ID"))),
            ("functionHandle".to_string(), mutation_arg(named("String"))),
            ("blockOnFailure".to_string(), mutation_arg(named("Boolean"))),
            (
                "metafields".to_string(),
                mutation_arg(list_of_non_null("MetafieldInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "fulfillmentConstraintRuleUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "deliveryMethodTypes".to_string(),
                mutation_arg(non_null_list_of_non_null("DeliveryMethodType")),
            ),
            ("functionId".to_string(), mutation_arg(named("ID"))),
            ("functionHandle".to_string(), mutation_arg(named("String"))),
        ]),
    );
}

fn extend_online_store_input_schema(schema: &mut AdminInputSchema) {
    // OnlineStoreThemeInput on Admin API 2025-01 accepts only `name`. A theme's role is
    // set at creation (themeCreate(role:)) and changed via themePublish, never through
    // themeUpdate's input, so supplying `role` must raise a top-level argumentNotAccepted
    // schema error before the themeUpdate resolver runs.
    schema.insert_strict_input_object(
        "OnlineStoreThemeInput".to_string(),
        BTreeMap::from([("name".to_string(), input_field(named("String")))]),
    );
    schema.mutation_fields.insert(
        "themeUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("OnlineStoreThemeInput")),
            ),
        ]),
    );
    schema.input_objects.insert(
        "ScriptTagInput".to_string(),
        BTreeMap::from([
            ("src".to_string(), input_field(named("String"))),
            ("displayScope".to_string(), input_field(named("String"))),
            ("cache".to_string(), input_field(named("Boolean"))),
        ]),
    );
}

fn extend_customer_merge_input_schema(schema: &mut AdminInputSchema) {
    // customerMerge requires both customerOneId and customerTwoId as non-null IDs
    // overrideFields is optional
    // Mirror the live Admin schema's CustomerMergeOverrideFields so a valid call
    // that picks which customer's scalar fields / addresses survive the merge is
    // not flagged as `argumentNotAccepted` before the resolver runs.
    schema.insert_strict_input_object(
        "CustomerMergeOverrideFields".to_string(),
        BTreeMap::from([
            (
                "customerIdOfFirstNameToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfLastNameToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfEmailToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfPhoneNumberToKeep".to_string(),
                input_field(named("ID")),
            ),
            (
                "customerIdOfDefaultAddressToKeep".to_string(),
                input_field(named("ID")),
            ),
            ("note".to_string(), input_field(named("String"))),
            ("tags".to_string(), input_field(list_of_non_null("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "customerMerge".to_string(),
        BTreeMap::from([
            ("customerOneId".to_string(), mutation_arg(non_null("ID"))),
            ("customerTwoId".to_string(), mutation_arg(non_null("ID"))),
            (
                "overrideFields".to_string(),
                mutation_arg(named("CustomerMergeOverrideFields")),
            ),
        ]),
    );
}

fn extend_app_input_schema(schema: &mut AdminInputSchema) {
    // The local app uninstall handler accepts the legacy nullable `input` shape
    // used by existing conformance coverage while also supporting the no-arg
    // public shape captured from newer Admin schemas.
    schema.insert_strict_input_object(
        "AppUninstallInput".to_string(),
        BTreeMap::from([("id".to_string(), input_field(named("ID")))]),
    );
    schema.mutation_fields.insert(
        "appUninstall".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(named("AppUninstallInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "appSubscriptionLineItemUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "cappedAmount".to_string(),
                mutation_arg(non_null("MoneyInput")),
            ),
            (
                "requireApproval".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "appUsageRecordCreate".to_string(),
        BTreeMap::from([
            (
                "subscriptionLineItemId".to_string(),
                mutation_arg(non_null("ID")),
            ),
            ("price".to_string(), mutation_arg(non_null("MoneyInput"))),
            ("description".to_string(), mutation_arg(named("String"))),
            ("idempotencyKey".to_string(), mutation_arg(named("String"))),
        ]),
    );
}

fn extend_customer_input_schema(schema: &mut AdminInputSchema) {
    // customerCreate(input: CustomerInput!) on Admin API 2025-01. Only the
    // top-level `input` argument is required; the CustomerInput object itself is
    // intentionally left unregistered so the local customerCreate handler keeps
    // ownership of field-level validation (it emits payload userErrors, not
    // top-level coercion errors). Registering the field alone is enough to
    // surface the missing-argument / null-literal / unbound-variable envelopes
    // (missingRequiredArguments, argumentLiteralsIncompatible, INVALID_VARIABLE)
    // before the resolver runs.
    schema.mutation_fields.insert(
        "customerCreate".to_string(),
        BTreeMap::from([("input".to_string(), mutation_arg(non_null("CustomerInput")))]),
    );

    // dataSaleOptOut(email: String!) on Admin API 2026-04. The single `email`
    // argument is non-null, so a missing or explicitly-null email must surface a
    // top-level `missingRequiredArguments` / null-coercion envelope before the
    // local privacy handler runs (rather than the handler's own FAILED userError).
    schema.mutation_fields.insert(
        "dataSaleOptOut".to_string(),
        BTreeMap::from([("email".to_string(), mutation_arg(non_null("String")))]),
    );
}

fn extend_orders_input_schema(schema: &mut AdminInputSchema) {
    // This local-runtime abandonment helper models the internal delivery activity
    // state map exposed by captured fixtures, whose transition values include
    // states not present in the public introspected AbandonmentDeliveryState enum.
    // Keep the public argument names from the captured schema, but let the handler
    // own delivery-status transition validation.
    schema.mutation_fields.insert(
        "abandonmentUpdateActivitiesDeliveryStatuses".to_string(),
        BTreeMap::from([
            ("abandonmentId".to_string(), mutation_arg(non_null("ID"))),
            (
                "marketingActivityId".to_string(),
                mutation_arg(non_null("ID")),
            ),
            (
                "deliveryStatus".to_string(),
                mutation_arg(non_null("String")),
            ),
            ("deliveredAt".to_string(), mutation_arg(named("DateTime"))),
            (
                "deliveryStatusChangeReason".to_string(),
                mutation_arg(named("String")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "draftOrderInvoiceSend".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("email".to_string(), mutation_arg(named("EmailInput"))),
            (
                "presentmentCurrencyCode".to_string(),
                mutation_arg(named("CurrencyCode")),
            ),
            ("templateName".to_string(), mutation_arg(named("String"))),
        ]),
    );
    schema.insert_strict_input_object(
        "ReturnDeclineRequestInput".to_string(),
        BTreeMap::from([
            ("id".to_string(), input_field(non_null("ID"))),
            (
                "declineReason".to_string(),
                input_field(non_null("ReturnDeclineReason")),
            ),
            ("notifyCustomer".to_string(), input_field(named("Boolean"))),
            ("declineNote".to_string(), input_field(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "returnDeclineRequest".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("ReturnDeclineRequestInput")),
        )]),
    );

    schema.insert_strict_input_object(
        "DraftOrderAppliedDiscountInput".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(named("Money"))),
            (
                "amountWithCurrency".to_string(),
                input_field(named("MoneyInput")),
            ),
            ("description".to_string(), input_field(named("String"))),
            ("title".to_string(), input_field(named("String"))),
            ("value".to_string(), input_field(non_null("Float"))),
            (
                "valueType".to_string(),
                input_field(non_null("DraftOrderAppliedDiscountType")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DraftOrderLineItemInput".to_string(),
        BTreeMap::from([
            (
                "appliedDiscount".to_string(),
                input_field(named("DraftOrderAppliedDiscountInput")),
            ),
            (
                "customAttributes".to_string(),
                input_field(list_of_non_null("AttributeInput")),
            ),
            ("grams".to_string(), input_field(named("Int"))),
            ("originalUnitPrice".to_string(), input_field(named("Money"))),
            (
                "originalUnitPriceWithCurrency".to_string(),
                input_field(named("MoneyInput")),
            ),
            ("quantity".to_string(), input_field(non_null("Int"))),
            (
                "requiresShipping".to_string(),
                input_field(named("Boolean")),
            ),
            ("sku".to_string(), input_field(named("String"))),
            ("taxable".to_string(), input_field(named("Boolean"))),
            ("title".to_string(), input_field(named("String"))),
            ("variantId".to_string(), input_field(named("ID"))),
            ("weight".to_string(), input_field(named("WeightInput"))),
            ("uuid".to_string(), input_field(named("String"))),
            (
                "bundleComponents".to_string(),
                input_field(list_of_non_null(
                    "BundlesDraftOrderBundleLineItemComponentInput",
                )),
            ),
            (
                "components".to_string(),
                input_field(list_of_non_null("DraftOrderLineItemComponentInput")),
            ),
            (
                "generatePriceOverride".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "priceOverride".to_string(),
                input_field(named("MoneyInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DraftOrderInput".to_string(),
        BTreeMap::from([
            (
                "appliedDiscount".to_string(),
                input_field(named("DraftOrderAppliedDiscountInput")),
            ),
            (
                "discountCodes".to_string(),
                input_field(list_of_non_null("String")),
            ),
            (
                "acceptAutomaticDiscounts".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "billingAddress".to_string(),
                input_field(named("MailingAddressInput")),
            ),
            ("customerId".to_string(), input_field(named("ID"))),
            (
                "customAttributes".to_string(),
                input_field(list_of_non_null("AttributeInput")),
            ),
            ("email".to_string(), input_field(named("String"))),
            (
                "lineItems".to_string(),
                input_field(list_of_non_null("DraftOrderLineItemInput")),
            ),
            (
                "metafields".to_string(),
                input_field(list_of_non_null("MetafieldInput")),
            ),
            (
                "localizationExtensions".to_string(),
                input_field(list_of_non_null("LocalizationExtensionInput")),
            ),
            (
                "localizedFields".to_string(),
                input_field(list_of_non_null("LocalizedFieldInput")),
            ),
            ("note".to_string(), input_field(named("String"))),
            (
                "shippingAddress".to_string(),
                input_field(named("MailingAddressInput")),
            ),
            (
                "shippingLine".to_string(),
                input_field(named("ShippingLineInput")),
            ),
            ("tags".to_string(), input_field(list_of_non_null("String"))),
            ("taxExempt".to_string(), input_field(named("Boolean"))),
            (
                "useCustomerDefaultAddress".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "visibleToCustomer".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "reserveInventoryUntil".to_string(),
                input_field(named("DateTime")),
            ),
            (
                "presentmentCurrencyCode".to_string(),
                input_field(named("CurrencyCode")),
            ),
            (
                "marketRegionCountryCode".to_string(),
                input_field(named("CountryCode")),
            ),
            ("phone".to_string(), input_field(named("String"))),
            (
                "paymentTerms".to_string(),
                input_field(named("DraftOrderPaymentTermsInput")),
            ),
            (
                "purchasingEntity".to_string(),
                input_field(named("PurchasingEntityInput")),
            ),
            ("sourceName".to_string(), input_field(named("String"))),
            (
                "allowDiscountCodesInCheckout".to_string(),
                input_field(named("Boolean")),
            ),
            ("poNumber".to_string(), input_field(named("String"))),
            ("sessionToken".to_string(), input_field(named("String"))),
            (
                "transformerFingerprint".to_string(),
                input_field(named("String")),
            ),
        ]),
    );

    // The order/draft-order create + edit mutations require their primary
    // argument (a non-null input object or id). Each is registered with its full
    // set of accepted root arguments so that valid calls (which pass optional
    // arguments like paymentGatewayId / notifyCustomer) are not flagged as
    // "argument not accepted". Draft-order input objects are registered for the
    // public GraphQL coercion branches the local resolver never sees, while
    // domain-level userErrors stay with the local draft-order resolver.
    schema.mutation_fields.insert(
        "draftOrderCalculate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("DraftOrderInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "draftOrderCreate".to_string(),
        BTreeMap::from([(
            "input".to_string(),
            mutation_arg(non_null("DraftOrderInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "draftOrderComplete".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("paymentGatewayId".to_string(), mutation_arg(named("ID"))),
            ("paymentPending".to_string(), mutation_arg(named("Boolean"))),
            ("sourceName".to_string(), mutation_arg(named("String"))),
        ]),
    );
    schema.mutation_fields.insert(
        "draftOrderUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "input".to_string(),
                mutation_arg(non_null("DraftOrderInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderCreate".to_string(),
        BTreeMap::from([
            (
                "order".to_string(),
                mutation_arg(non_null("OrderCreateOrderInput")),
            ),
            (
                "options".to_string(),
                mutation_arg(named("OrderCreateOptionsInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditBegin".to_string(),
        BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
    );
    schema.mutation_fields.insert(
        "orderEditCommit".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("notifyCustomer".to_string(), mutation_arg(named("Boolean"))),
            ("staffNote".to_string(), mutation_arg(named("String"))),
        ]),
    );

    // Fulfillment lifecycle mutations. Routed locally now, so the proxy owns the
    // top-level missing-argument / null-literal / unbound-variable envelopes for
    // their required `id` / `fulfillmentId` arguments. The full accepted argument
    // set is registered so valid calls (which pass optional notifyCustomer /
    // trackingInfoInput) are not flagged "argument not accepted".
    schema.mutation_fields.insert(
        "fulfillmentCancel".to_string(),
        BTreeMap::from([("id".to_string(), mutation_arg(non_null("ID")))]),
    );
    schema.mutation_fields.insert(
        "fulfillmentTrackingInfoUpdate".to_string(),
        BTreeMap::from([
            ("fulfillmentId".to_string(), mutation_arg(non_null("ID"))),
            (
                "trackingInfoInput".to_string(),
                mutation_arg(non_null("FulfillmentTrackingInput")),
            ),
            ("notifyCustomer".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
    schema.insert_strict_input_object(
        "ReverseFulfillmentOrderDisposeInput".to_string(),
        BTreeMap::from([
            (
                "reverseFulfillmentOrderLineItemId".to_string(),
                input_field(non_null("ID")),
            ),
            ("quantity".to_string(), input_field(non_null("Int"))),
            ("locationId".to_string(), input_field(named("ID"))),
            (
                "dispositionType".to_string(),
                input_field(non_null("ReverseFulfillmentOrderDispositionType")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "reverseFulfillmentOrderDispose".to_string(),
        BTreeMap::from([(
            "dispositionInputs".to_string(),
            mutation_arg(non_null_list_of_non_null(
                "ReverseFulfillmentOrderDisposeInput",
            )),
        )]),
    );

    // Order-edit calculated-session mutations. Each is registered with its full
    // accepted argument set (so valid edits are not flagged "argument not
    // accepted") plus the required arguments / input-object attributes Shopify
    // enforces during variable coercion. Routing these locally means the proxy
    // owns the top-level coercion / missing-argument / missing-input-attribute
    // envelopes that previously only surfaced when the call passed through to a
    // recorded response — the local edit engine never sees a malformed input.
    //
    // `MoneyInput` requires both `amount` (Decimal!) and `currencyCode`
    // (CurrencyCode!); the order-edit money arguments (custom-item price, applied
    // discount fixedValue, shipping-line price) descend into it so an inline
    // money object missing `currencyCode` raises `missingRequiredInputObjectAttribute`.
    schema.insert_strict_input_object(
        "MoneyInput".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(non_null("Decimal"))),
            (
                "currencyCode".to_string(),
                input_field(non_null("CurrencyCode")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddVariant".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("variantId".to_string(), mutation_arg(non_null("ID"))),
            ("quantity".to_string(), mutation_arg(non_null("Int"))),
            ("locationId".to_string(), mutation_arg(named("ID"))),
            (
                "allowDuplicates".to_string(),
                mutation_arg(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditSetQuantity".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("lineItemId".to_string(), mutation_arg(non_null("ID"))),
            ("quantity".to_string(), mutation_arg(non_null("Int"))),
            ("restock".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddCustomItem".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("title".to_string(), mutation_arg(non_null("String"))),
            ("quantity".to_string(), mutation_arg(non_null("Int"))),
            ("price".to_string(), mutation_arg(non_null("MoneyInput"))),
            (
                "requiresShipping".to_string(),
                mutation_arg(named("Boolean")),
            ),
            ("taxable".to_string(), mutation_arg(named("Boolean"))),
        ]),
    );
    schema.insert_strict_input_object(
        "OrderEditAppliedDiscountInput".to_string(),
        BTreeMap::from([
            ("description".to_string(), input_field(named("String"))),
            ("fixedValue".to_string(), input_field(named("MoneyInput"))),
            ("percentage".to_string(), input_field(named("Float"))),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddLineItemDiscount".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("lineItemId".to_string(), mutation_arg(non_null("ID"))),
            (
                "discount".to_string(),
                mutation_arg(non_null("OrderEditAppliedDiscountInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditRemoveDiscount".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "discountApplicationId".to_string(),
                mutation_arg(non_null("ID")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "OrderEditAddShippingLineInput".to_string(),
        BTreeMap::from([
            ("title".to_string(), input_field(named("String"))),
            ("price".to_string(), input_field(non_null("MoneyInput"))),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditAddShippingLine".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "shippingLine".to_string(),
                mutation_arg(non_null("OrderEditAddShippingLineInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditUpdateShippingLine".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("shippingLineId".to_string(), mutation_arg(non_null("ID"))),
            (
                "shippingLine".to_string(),
                mutation_arg(non_null("OrderEditUpdateShippingLineInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "orderEditRemoveShippingLine".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            ("shippingLineId".to_string(), mutation_arg(non_null("ID"))),
        ]),
    );

    // RefundInput on Admin API 2026-04. Refund *attribution* fields
    // (pointOfSaleDeviceId, locationId, userId, transactionGroupId) are not part
    // of the public RefundInput — they belong to POS/internal refund flows —
    // so supplying any of them must raise a schema error before the refundCreate
    // resolver runs (argumentNotAccepted for inline literals, INVALID_VARIABLE
    // for a coerced variable). The accepted fields below are registered so valid
    // refunds (with refundLineItems / transactions / shipping / allowOverRefunding
    // / note / notify / currency) pass through; their nested input objects are
    // left unregistered so refund-line/transaction validation stays with the
    // local refund engine.
    schema.insert_strict_input_object(
        "RefundInput".to_string(),
        BTreeMap::from([
            ("orderId".to_string(), input_field(non_null("ID"))),
            ("currency".to_string(), input_field(named("CurrencyCode"))),
            ("note".to_string(), input_field(named("String"))),
            ("notify".to_string(), input_field(named("Boolean"))),
            (
                "allowOverRefunding".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "shipping".to_string(),
                input_field(named("ShippingRefundInput")),
            ),
            (
                "refundLineItems".to_string(),
                input_field(list_of_non_null("RefundLineItemInput")),
            ),
            (
                "refundDuties".to_string(),
                input_field(list_of_non_null("RefundDutyInput")),
            ),
            (
                "transactions".to_string(),
                input_field(list_of_non_null("OrderTransactionInput")),
            ),
            (
                "discrepancyReason".to_string(),
                input_field(named("OrderAdjustmentInputDiscrepancyReason")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "refundCreate".to_string(),
        BTreeMap::from([("input".to_string(), mutation_arg(non_null("RefundInput")))]),
    );
}

fn extend_discount_basic_input_schema(schema: &mut AdminInputSchema) {
    let mut code_basic_fields = discount_basic_input_fields();
    code_basic_fields.extend([
        (
            "appliesOncePerCustomer".to_string(),
            input_field(named("Boolean")),
        ),
        ("code".to_string(), input_field(named("String"))),
        (
            "customerSelection".to_string(),
            input_field(named("DiscountCustomerSelectionInput")),
        ),
        ("usageLimit".to_string(), input_field(named("Int"))),
    ]);
    schema.insert_strict_input_object("DiscountCodeBasicInput".to_string(), code_basic_fields);
    schema.insert_strict_input_object(
        "DiscountAutomaticBasicInput".to_string(),
        discount_basic_input_fields(),
    );
    schema.insert_strict_input_object(
        "DiscountCombinesWithInput".to_string(),
        BTreeMap::from([
            (
                "productDiscounts".to_string(),
                input_field(named("Boolean")),
            ),
            ("orderDiscounts".to_string(), input_field(named("Boolean"))),
            (
                "shippingDiscounts".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "productDiscountsWithTagsOnSameCartLine".to_string(),
                input_field(named("ProductDiscountsWithTagsOnSameCartLineInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "ProductDiscountsWithTagsOnSameCartLineInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("String"))),
            (
                "remove".to_string(),
                input_field(list_of_non_null("String")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountCustomerSelectionInput".to_string(),
        BTreeMap::from([
            ("all".to_string(), input_field(named("Boolean"))),
            (
                "customers".to_string(),
                input_field(named("DiscountCustomersInput")),
            ),
            (
                "customerSegments".to_string(),
                input_field(named("DiscountCustomerSegmentsInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountContextInput".to_string(),
        BTreeMap::from([
            (
                "all".to_string(),
                input_field(named("DiscountBuyerSelection")),
            ),
            (
                "customers".to_string(),
                input_field(named("DiscountCustomersInput")),
            ),
            (
                "customerSegments".to_string(),
                input_field(named("DiscountCustomerSegmentsInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountCustomersInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountCustomerSegmentsInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountMinimumRequirementInput".to_string(),
        BTreeMap::from([
            (
                "quantity".to_string(),
                input_field(named("DiscountMinimumQuantityInput")),
            ),
            (
                "subtotal".to_string(),
                input_field(named("DiscountMinimumSubtotalInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountMinimumQuantityInput".to_string(),
        BTreeMap::from([(
            "greaterThanOrEqualToQuantity".to_string(),
            input_field(named("UnsignedInt64")),
        )]),
    );
    schema.insert_strict_input_object(
        "DiscountMinimumSubtotalInput".to_string(),
        BTreeMap::from([(
            "greaterThanOrEqualToSubtotal".to_string(),
            input_field(named("Decimal")),
        )]),
    );
    schema.insert_strict_input_object(
        "DiscountCustomerGetsInput".to_string(),
        BTreeMap::from([
            (
                "value".to_string(),
                input_field(named("DiscountCustomerGetsValueInput")),
            ),
            (
                "items".to_string(),
                input_field(named("DiscountItemsInput")),
            ),
            (
                "appliesOnOneTimePurchase".to_string(),
                input_field(named("Boolean")),
            ),
            (
                "appliesOnSubscription".to_string(),
                input_field(named("Boolean")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountCustomerGetsValueInput".to_string(),
        BTreeMap::from([
            (
                "discountOnQuantity".to_string(),
                input_field(named("DiscountOnQuantityInput")),
            ),
            ("percentage".to_string(), input_field(named("Float"))),
            (
                "discountAmount".to_string(),
                input_field(named("DiscountAmountInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountItemsInput".to_string(),
        BTreeMap::from([
            (
                "products".to_string(),
                input_field(named("DiscountProductsInput")),
            ),
            (
                "collections".to_string(),
                input_field(named("DiscountCollectionsInput")),
            ),
            ("all".to_string(), input_field(named("Boolean"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountProductsInput".to_string(),
        BTreeMap::from([
            (
                "productsToAdd".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "productsToRemove".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "productVariantsToAdd".to_string(),
                input_field(list_of_non_null("ID")),
            ),
            (
                "productVariantsToRemove".to_string(),
                input_field(list_of_non_null("ID")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountCollectionsInput".to_string(),
        BTreeMap::from([
            ("add".to_string(), input_field(list_of_non_null("ID"))),
            ("remove".to_string(), input_field(list_of_non_null("ID"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountOnQuantityInput".to_string(),
        BTreeMap::from([
            ("quantity".to_string(), input_field(named("UnsignedInt64"))),
            (
                "effect".to_string(),
                input_field(named("DiscountEffectInput")),
            ),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountEffectInput".to_string(),
        BTreeMap::from([
            ("percentage".to_string(), input_field(named("Float"))),
            ("amount".to_string(), input_field(named("Decimal"))),
        ]),
    );
    schema.insert_strict_input_object(
        "DiscountAmountInput".to_string(),
        BTreeMap::from([
            ("amount".to_string(), input_field(named("Decimal"))),
            (
                "appliesOnEachItem".to_string(),
                input_field(named("Boolean")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "discountCodeBasicCreate".to_string(),
        BTreeMap::from([(
            "basicCodeDiscount".to_string(),
            mutation_arg(non_null("DiscountCodeBasicInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "discountCodeBasicUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "basicCodeDiscount".to_string(),
                mutation_arg(non_null("DiscountCodeBasicInput")),
            ),
        ]),
    );
    schema.mutation_fields.insert(
        "discountAutomaticBasicCreate".to_string(),
        BTreeMap::from([(
            "automaticBasicDiscount".to_string(),
            mutation_arg(non_null("DiscountAutomaticBasicInput")),
        )]),
    );
    schema.mutation_fields.insert(
        "discountAutomaticBasicUpdate".to_string(),
        BTreeMap::from([
            ("id".to_string(), mutation_arg(non_null("ID"))),
            (
                "automaticBasicDiscount".to_string(),
                mutation_arg(non_null("DiscountAutomaticBasicInput")),
            ),
        ]),
    );
}

fn discount_basic_input_fields() -> BTreeMap<String, SchemaInputField> {
    BTreeMap::from([
        (
            "combinesWith".to_string(),
            input_field(named("DiscountCombinesWithInput")),
        ),
        ("title".to_string(), input_field(named("String"))),
        ("startsAt".to_string(), input_field(named("DateTime"))),
        ("endsAt".to_string(), input_field(named("DateTime"))),
        (
            "context".to_string(),
            input_field(named("DiscountContextInput")),
        ),
        ("tags".to_string(), input_field(list_of_non_null("String"))),
        (
            "minimumRequirement".to_string(),
            input_field(named("DiscountMinimumRequirementInput")),
        ),
        (
            "customerGets".to_string(),
            input_field(named("DiscountCustomerGetsInput")),
        ),
        ("recurringCycleLimit".to_string(), input_field(named("Int"))),
    ])
}

fn named(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: name.to_string(),
        named_type: name.to_string(),
        non_null: false,
    }
}

fn non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("{name}!"),
        named_type: name.to_string(),
        non_null: true,
    }
}

fn non_null_list_of_non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("[{name}!]!"),
        named_type: name.to_string(),
        non_null: true,
    }
}

fn list_of_non_null(name: &str) -> SchemaTypeRef {
    SchemaTypeRef {
        display: format!("[{name}!]"),
        named_type: name.to_string(),
        non_null: false,
    }
}

fn type_ref_is_list(type_ref: &SchemaTypeRef) -> bool {
    type_ref.display.starts_with('[')
}

fn type_ref_has_non_null_list_items(type_ref: &SchemaTypeRef) -> bool {
    type_ref.display.contains("!]")
}
