//// Snapshot-style coverage of Shopify Admin GraphQL root operations
//// against the proxy's operation registry. Decodes the captured 2025-01
//// root-operation introspection fixture and asserts on the names that
//// either have a registry entry with `implemented: false` (declared gaps)
//// or no registry entry at all (unregistered). The literal-list assertion
//// follows the existing snapshot pattern in this suite (see
//// `unsupported_node_implementors_match_introspection_snapshot_test`).
////
//// Restored after the TypeScript runtime cutover dropped
//// `tests/unit/graphql-operation-coverage.test.ts` without porting it.

import gleam/dynamic/decode
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/operation_registry.{
  type OperationType, type RegistryEntry, Mutation, Query,
}
import shopify_draft_proxy/proxy/operation_registry_data
import simplifile

const introspection_fixture_path: String = "fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json"

type RootOperationIntrospection {
  RootOperationIntrospection(
    query_type_name: String,
    mutation_type_name: String,
    query_root_name: String,
    mutation_root_name: String,
    query_field_names: List(String),
    mutation_field_names: List(String),
  )
}

pub fn formally_supported_root_operations_match_introspection_test() {
  let intro = read_introspection_fixture()
  let registry = operation_registry_data.default_registry()

  // The __schema mutationType/queryType names line up with the captured
  // queryRoot/mutationRoot type names — the fixture is internally consistent.
  assert intro.query_type_name == intro.query_root_name
  assert intro.mutation_type_name == intro.mutation_root_name

  let supported_queries =
    list.filter(intro.query_field_names, fn(name) {
      is_supported(registry, Query, name)
    })
  let supported_mutations =
    list.filter(intro.mutation_field_names, fn(name) {
      is_supported(registry, Mutation, name)
    })

  assert list.length(intro.query_field_names) > list.length(supported_queries)
  assert list.length(intro.mutation_field_names)
    > list.length(supported_mutations)

  assert list.contains(supported_queries, "product")
  assert list.contains(supported_queries, "products")
  assert list.contains(supported_queries, "productsCount")
  assert list.contains(supported_mutations, "productCreate")
  assert list.contains(supported_mutations, "productUpdate")
  assert list.contains(supported_mutations, "productDelete")
}

pub fn unsupported_introspected_mutations_match_snapshot_test() {
  let intro = read_introspection_fixture()
  let registry = operation_registry_data.default_registry()
  let unsupported =
    list.filter(intro.mutation_field_names, fn(name) {
      !is_supported(registry, Mutation, name)
    })

  assert unsupported
    == [
      "companyContactSendWelcomeEmail",
      "consentPolicyUpdate",
      "deliveryCustomizationActivation",
      "deliveryCustomizationCreate",
      "deliveryCustomizationDelete",
      "deliveryCustomizationUpdate",
      "deliveryPromiseParticipantsUpdate",
      "deliveryPromiseProviderUpsert",
      "deliverySettingUpdate",
      "disputeEvidenceUpdate",
      "fulfillmentConstraintRuleCreate",
      "fulfillmentConstraintRuleDelete",
      "fulfillmentConstraintRuleUpdate",
      "fulfillmentOrderClose",
      "fulfillmentOrderLineItemsPreparedForPickup",
      "fulfillmentOrderReschedule",
      "menuCreate",
      "menuDelete",
      "menuUpdate",
      "orderEditUpdateDiscount",
      "orderRiskAssessmentCreate",
      "privacyFeaturesDisable",
      "shopifyPaymentsPayoutAlternateCurrencyCreate",
      "subscriptionBillingAttemptCreate",
      "subscriptionBillingCycleBulkCharge",
      "subscriptionBillingCycleBulkSearch",
      "subscriptionBillingCycleCharge",
      "subscriptionBillingCycleContractDraftCommit",
      "subscriptionBillingCycleContractDraftConcatenate",
      "subscriptionBillingCycleContractEdit",
      "subscriptionBillingCycleEditDelete",
      "subscriptionBillingCycleEditsDelete",
      "subscriptionBillingCycleScheduleEdit",
      "subscriptionBillingCycleSkip",
      "subscriptionBillingCycleUnskip",
      "subscriptionContractActivate",
      "subscriptionContractAtomicCreate",
      "subscriptionContractCancel",
      "subscriptionContractCreate",
      "subscriptionContractExpire",
      "subscriptionContractFail",
      "subscriptionContractPause",
      "subscriptionContractProductChange",
      "subscriptionContractSetNextBillingDate",
      "subscriptionContractUpdate",
      "subscriptionDraftCommit",
      "subscriptionDraftDiscountAdd",
      "subscriptionDraftDiscountCodeApply",
      "subscriptionDraftDiscountRemove",
      "subscriptionDraftDiscountUpdate",
      "subscriptionDraftFreeShippingDiscountAdd",
      "subscriptionDraftFreeShippingDiscountUpdate",
      "subscriptionDraftLineAdd",
      "subscriptionDraftLineRemove",
      "subscriptionDraftLineUpdate",
      "subscriptionDraftUpdate",
      "urlRedirectBulkDeleteAll",
      "urlRedirectBulkDeleteByIds",
      "urlRedirectBulkDeleteBySavedSearch",
      "urlRedirectBulkDeleteBySearch",
      "urlRedirectCreate",
      "urlRedirectDelete",
      "urlRedirectImportCreate",
      "urlRedirectImportSubmit",
      "urlRedirectUpdate",
    ]
}

pub fn mutation_coverage_classifies_declared_gaps_and_unregistered_test() {
  let intro = read_introspection_fixture()
  let registry = operation_registry_data.default_registry()
  let declared_gaps =
    list.filter(intro.mutation_field_names, fn(name) {
      has_unimplemented_entry(registry, Mutation, name)
    })
  let unregistered =
    list.filter(intro.mutation_field_names, fn(name) {
      !has_any_entry(registry, Mutation, name)
    })

  assert declared_gaps
    == [
      "companyContactSendWelcomeEmail",
      "consentPolicyUpdate",
      "deliveryCustomizationActivation",
      "deliveryCustomizationCreate",
      "deliveryCustomizationDelete",
      "deliveryCustomizationUpdate",
      "deliveryPromiseParticipantsUpdate",
      "deliveryPromiseProviderUpsert",
      "deliverySettingUpdate",
      "disputeEvidenceUpdate",
      "fulfillmentConstraintRuleCreate",
      "fulfillmentConstraintRuleDelete",
      "fulfillmentConstraintRuleUpdate",
      "fulfillmentOrderClose",
      "fulfillmentOrderLineItemsPreparedForPickup",
      "fulfillmentOrderReschedule",
      "menuCreate",
      "menuDelete",
      "menuUpdate",
      "orderRiskAssessmentCreate",
      "privacyFeaturesDisable",
      "shopifyPaymentsPayoutAlternateCurrencyCreate",
      "urlRedirectBulkDeleteAll",
      "urlRedirectBulkDeleteByIds",
      "urlRedirectBulkDeleteBySavedSearch",
      "urlRedirectBulkDeleteBySearch",
      "urlRedirectCreate",
      "urlRedirectDelete",
      "urlRedirectImportCreate",
      "urlRedirectImportSubmit",
      "urlRedirectUpdate",
    ]
  assert unregistered
    == [
      "orderEditUpdateDiscount",
      "subscriptionBillingAttemptCreate",
      "subscriptionBillingCycleBulkCharge",
      "subscriptionBillingCycleBulkSearch",
      "subscriptionBillingCycleCharge",
      "subscriptionBillingCycleContractDraftCommit",
      "subscriptionBillingCycleContractDraftConcatenate",
      "subscriptionBillingCycleContractEdit",
      "subscriptionBillingCycleEditDelete",
      "subscriptionBillingCycleEditsDelete",
      "subscriptionBillingCycleScheduleEdit",
      "subscriptionBillingCycleSkip",
      "subscriptionBillingCycleUnskip",
      "subscriptionContractActivate",
      "subscriptionContractAtomicCreate",
      "subscriptionContractCancel",
      "subscriptionContractCreate",
      "subscriptionContractExpire",
      "subscriptionContractFail",
      "subscriptionContractPause",
      "subscriptionContractProductChange",
      "subscriptionContractSetNextBillingDate",
      "subscriptionContractUpdate",
      "subscriptionDraftCommit",
      "subscriptionDraftDiscountAdd",
      "subscriptionDraftDiscountCodeApply",
      "subscriptionDraftDiscountRemove",
      "subscriptionDraftDiscountUpdate",
      "subscriptionDraftFreeShippingDiscountAdd",
      "subscriptionDraftFreeShippingDiscountUpdate",
      "subscriptionDraftLineAdd",
      "subscriptionDraftLineRemove",
      "subscriptionDraftLineUpdate",
      "subscriptionDraftUpdate",
    ]
}

pub fn query_coverage_classifies_declared_gaps_and_unregistered_test() {
  let intro = read_introspection_fixture()
  let registry = operation_registry_data.default_registry()
  let declared_gaps =
    list.filter(intro.query_field_names, fn(name) {
      has_unimplemented_entry(registry, Query, name)
    })
  let unregistered =
    list.filter(intro.query_field_names, fn(name) {
      !has_any_entry(registry, Query, name)
    })

  assert declared_gaps
    == [
      "app",
      "appByHandle",
      "appByKey",
      "appInstallation",
      "appInstallations",
      "consentPolicy",
      "consentPolicyRegions",
      "deliveryCustomization",
      "deliveryCustomizations",
      "deliveryPromiseParticipants",
      "deliveryPromiseProvider",
      "disputeEvidence",
      "draftOrderTag",
      "financeAppAccessPolicy",
      "financeKycInformation",
      "fulfillmentConstraintRules",
      "menu",
      "menus",
      "orderPaymentStatus",
      "privacySettings",
      "returnCalculate",
      "returnableFulfillment",
      "returnableFulfillments",
      "standardMetafieldDefinitionTemplates",
      "tenderTransactions",
      "urlRedirectSavedSearches",
      "urlRedirects",
      "urlRedirectsCount",
    ]
  assert unregistered
    == [
      "appDiscountType",
      "appDiscountTypes",
      "appDiscountTypesNodes",
      "availableBackupRegions",
      "catalogOperations",
      "collectionRulesConditions",
      "collectionsCount",
      "currentStaffMember",
      "discountCodesCount",
      "locationsCount",
      "metafieldDefinitionTypes",
      "onlineStore",
      "orderByIdentifier",
      "pendingOrdersCount",
      "refund",
      "shopBillingPreferences",
      "subscriptionBillingAttempt",
      "subscriptionBillingAttempts",
      "subscriptionBillingCycle",
      "subscriptionBillingCycleBulkResults",
      "subscriptionBillingCycles",
      "subscriptionContract",
      "subscriptionContracts",
      "subscriptionDraft",
      "urlRedirect",
      "urlRedirectImport",
    ]
}

fn is_supported(
  registry: List(RegistryEntry),
  type_: OperationType,
  name: String,
) -> Bool {
  list.any(registry, fn(entry) {
    entry.type_ == type_ && entry.name == name && entry.implemented
  })
}

fn has_unimplemented_entry(
  registry: List(RegistryEntry),
  type_: OperationType,
  name: String,
) -> Bool {
  list.any(registry, fn(entry) {
    entry.type_ == type_ && entry.name == name && !entry.implemented
  })
}

fn has_any_entry(
  registry: List(RegistryEntry),
  type_: OperationType,
  name: String,
) -> Bool {
  list.any(registry, fn(entry) { entry.type_ == type_ && entry.name == name })
}

fn read_introspection_fixture() -> RootOperationIntrospection {
  let assert Ok(source) = simplifile.read(introspection_fixture_path)
  let assert Ok(value) = json.parse(source, root_decoder())
  value
}

fn root_decoder() -> decode.Decoder(RootOperationIntrospection) {
  use intro <- decode.field("introspection", introspection_decoder())
  decode.success(intro)
}

fn introspection_decoder() -> decode.Decoder(RootOperationIntrospection) {
  use value <- decode.field("data", schema_data_decoder())
  decode.success(value)
}

fn schema_data_decoder() -> decode.Decoder(RootOperationIntrospection) {
  use schema_pair <- decode.field("__schema", schema_decoder())
  use query_root <- decode.field("queryRoot", root_type_decoder())
  use mutation_root <- decode.field("mutationRoot", root_type_decoder())
  let #(query_type_name, mutation_type_name) = schema_pair
  let #(query_root_name, query_field_names) = query_root
  let #(mutation_root_name, mutation_field_names) = mutation_root
  decode.success(RootOperationIntrospection(
    query_type_name: query_type_name,
    mutation_type_name: mutation_type_name,
    query_root_name: query_root_name,
    mutation_root_name: mutation_root_name,
    query_field_names: list.sort(query_field_names, by: string.compare),
    mutation_field_names: list.sort(mutation_field_names, by: string.compare),
  ))
}

fn schema_decoder() -> decode.Decoder(#(String, String)) {
  use query_type_name <- decode.field("queryType", named_decoder())
  use mutation_type_name <- decode.field("mutationType", named_decoder())
  decode.success(#(query_type_name, mutation_type_name))
}

fn named_decoder() -> decode.Decoder(String) {
  use name <- decode.field("name", decode.string)
  decode.success(name)
}

fn root_type_decoder() -> decode.Decoder(#(String, List(String))) {
  use name <- decode.field("name", decode.string)
  use fields <- decode.field("fields", decode.list(of: named_decoder()))
  decode.success(#(name, fields))
}
