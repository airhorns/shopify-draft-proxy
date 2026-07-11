import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { loadNodeResolverInventory, loadOperationRegistry } from '../../scripts/conformance-scenario-registry.js';
import { parseJsonFileWithSchema } from '../../scripts/support/json-schemas.js';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const operationRegistryEntries = loadOperationRegistry(repoRoot);
const nodeResolverInventoryEntries = loadNodeResolverInventory(repoRoot);

const capturedMutationSchema = z
  .strictObject({
    mutations: z.array(z.strictObject({ name: z.string().min(1) }).passthrough()),
  })
  .passthrough();

const capturedNodeInterfaceSchema = z
  .strictObject({
    introspection: z
      .strictObject({
        nodeInterface: z
          .strictObject({
            possibleTypes: z.array(z.strictObject({ name: z.string().min(1) }).passthrough()),
          })
          .passthrough(),
      })
      .passthrough(),
  })
  .passthrough();

const mutationSchemaPath = resolve(repoRoot, 'config/admin-graphql/2026-04/mutation-schema.json');
const adminPlatformNodeCapturePath = resolve(
  repoRoot,
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform/admin-platform-utility-roots.json',
);

function listOperationRegistryEntries() {
  return operationRegistryEntries.map((entry) => ({
    ...entry,
    matchNames: [...entry.matchNames],
    runtimeTests: [...entry.runtimeTests],
  }));
}

function listImplementedOperationRegistryEntries() {
  return listOperationRegistryEntries().filter((entry) => entry.implemented);
}

function listRuntimeTestedOperationRegistryEntries() {
  return listOperationRegistryEntries().filter((entry) => entry.runtimeTests.length > 0);
}

function sortedStrings(values: Iterable<string>): string[] {
  return [...values].sort((left, right) => left.localeCompare(right));
}

function capturedMutationNames() {
  const schema = parseJsonFileWithSchema(mutationSchemaPath, capturedMutationSchema);
  return sortedStrings(schema.mutations.map((mutation) => mutation.name));
}

function adminMutationCoverageAudit() {
  const schemaMutationNames = capturedMutationNames();
  const registryMutations = listOperationRegistryEntries().filter((entry) => entry.type === 'mutation');
  const registryMutationByName = new Map(registryMutations.map((entry) => [entry.name, entry]));
  const implementedMutationNames = schemaMutationNames.filter(
    (name) => registryMutationByName.get(name)?.implemented === true,
  );
  const implementedWithRuntimeTests = implementedMutationNames.filter(
    (name) => (registryMutationByName.get(name)?.runtimeTests.length ?? 0) > 0,
  );
  const implementedWithoutRuntimeTests = implementedMutationNames.filter(
    (name) => (registryMutationByName.get(name)?.runtimeTests.length ?? 0) === 0,
  );

  return {
    capturedMutationCount: schemaMutationNames.length,
    registeredMutationCount: schemaMutationNames.filter((name) => registryMutationByName.has(name)).length,
    implementedMutationCount: implementedMutationNames.length,
    implementedMutationRuntimeTestEvidence: {
      withRuntimeTests: implementedWithRuntimeTests.length,
      withoutRuntimeTests: implementedWithoutRuntimeTests.length,
    },
    declaredUnimplemented: schemaMutationNames.filter(
      (name) => registryMutationByName.has(name) && registryMutationByName.get(name)?.implemented === false,
    ),
    unregistered: schemaMutationNames.filter((name) => !registryMutationByName.has(name)),
    registeredButMissingFromCapturedSchema: sortedStrings(
      registryMutations.map((entry) => entry.name).filter((name) => !schemaMutationNames.includes(name)),
    ),
  };
}

function capturedNodeImplementorNames() {
  const capture = parseJsonFileWithSchema(adminPlatformNodeCapturePath, capturedNodeInterfaceSchema);
  return sortedStrings(capture.introspection.nodeInterface.possibleTypes.map((possibleType) => possibleType.name));
}

function nodeResolverCoverageAudit() {
  const capturedTypeNames = capturedNodeImplementorNames();
  const localResolverTypeNames = sortedStrings(new Set(nodeResolverInventoryEntries.map((entry) => entry.typeName)));

  return {
    capturedNodeImplementorCount: capturedTypeNames.length,
    localNodeResolverTypeCount: localResolverTypeNames.length,
    localResolverBehaviorCounts: {
      projectLocalRecord: nodeResolverInventoryEntries.filter((entry) => entry.behavior === 'project-local-record')
        .length,
      returnKnownNull: nodeResolverInventoryEntries.filter((entry) => entry.behavior === 'return-known-null').length,
    },
    unsupported: capturedTypeNames.filter((typeName) => !localResolverTypeNames.includes(typeName)),
    localInventoryNotInCapturedNodeInterface: localResolverTypeNames.filter(
      (typeName) => !capturedTypeNames.includes(typeName),
    ),
  };
}

describe('operation registry', () => {
  it('keeps implemented capability names unique', () => {
    const implementedNames = listImplementedOperationRegistryEntries().map((entry) => entry.name);
    expect(new Set(implementedNames).size).toBe(implementedNames.length);
  });

  it('treats every runtime-tested operation as implemented', () => {
    // `implemented` spans the full locally-handled surface, so it is a superset of the
    // runtime-tested (uniform table-dispatch) operations.
    for (const entry of listRuntimeTestedOperationRegistryEntries()) {
      expect(entry.implemented, `${entry.name} declares runtime tests so it must be implemented`).toBe(true);
    }
  });

  it('requires runtime-tested operations to declare runtime tests without conformance metadata', () => {
    for (const entry of listRuntimeTestedOperationRegistryEntries()) {
      expect(entry.runtimeTests.length).toBeGreaterThan(0);
      expect('conformance' in entry).toBe(false);
    }
  });

  it('keeps runtime test references executable on disk', () => {
    for (const entry of listRuntimeTestedOperationRegistryEntries()) {
      for (const runtimeTest of entry.runtimeTests) {
        expect(
          existsSync(resolve(repoRoot, runtimeTest)),
          `${entry.name} runtime test should exist: ${runtimeTest}`,
        ).toBe(true);
      }
    }
  });

  it('exposes both overlay-read and stage-locally implemented operations', () => {
    const executions = new Set(listOperationRegistryEntries().map((entry) => entry.execution));
    expect(executions.has('overlay-read')).toBe(true);
    expect(executions.has('stage-locally')).toBe(true);
  });

  it('loads the Rust operation registry as the source of truth', () => {
    expect(listOperationRegistryEntries().length).toBeGreaterThan(0);
    expect(listOperationRegistryEntries().some((entry) => entry.name === 'productCreate')).toBe(true);
  });

  it('audits captured 2026-04 Admin mutation roots against the Rust registry', () => {
    expect(adminMutationCoverageAudit()).toEqual({
      capturedMutationCount: 514,
      registeredMutationCount: 438,
      implementedMutationCount: 403,
      implementedMutationRuntimeTestEvidence: {
        withRuntimeTests: 128,
        withoutRuntimeTests: 275,
      },
      declaredUnimplemented: [
        'companyContactSendWelcomeEmail',
        'consentPolicyUpdate',
        'customerPaymentMethodSendUpdateEmail',
        'deliveryPromiseParticipantsUpdate',
        'deliveryPromiseProviderUpsert',
        'deliverySettingUpdate',
        'discountAutomaticBulkDelete',
        'discountCodeBulkActivate',
        'discountCodeBulkDeactivate',
        'discountCodeBulkDelete',
        'disputeEvidenceUpdate',
        'fulfillmentOrderLineItemsPreparedForPickup',
        'fulfillmentTrackingInfoUpdateV2',
        'menuCreate',
        'menuDelete',
        'menuUpdate',
        'metaobjectBulkDelete',
        'mobilePlatformApplicationDelete',
        'orderRiskAssessmentCreate',
        'privacyFeaturesDisable',
        'serverPixelDelete',
        'shopifyPaymentsPayoutAlternateCurrencyCreate',
        'standardMetaobjectDefinitionEnable',
        'storefrontAccessTokenDelete',
        'taxSummaryCreate',
        'urlRedirectBulkDeleteAll',
        'urlRedirectBulkDeleteByIds',
        'urlRedirectBulkDeleteBySavedSearch',
        'urlRedirectBulkDeleteBySearch',
        'urlRedirectCreate',
        'urlRedirectDelete',
        'urlRedirectImportCreate',
        'urlRedirectImportSubmit',
        'urlRedirectUpdate',
        'webPixelDelete',
      ],
      unregistered: [
        'abandonmentEmailStateUpdate',
        'cashDrawerCreate',
        'cashDrawerFindOrCreate',
        'cashDrawerUpdate',
        'cashManagementReasonCodeCreate',
        'cashManagementReasonCodeDelete',
        'channelCreate',
        'channelDelete',
        'channelFullSync',
        'channelUpdate',
        'checkoutAndAccountsConfigurationUpdate',
        'checkoutBrandingUpsert',
        'collectionDuplicate',
        'collectionPublish',
        'collectionUnpublish',
        'companyLocationAssignTaxExemptions',
        'companyLocationCreateTaxRegistration',
        'companyLocationRevokeTaxExemptions',
        'companyLocationRevokeTaxRegistration',
        'deliveryShippingOriginAssign',
        'discountBulkTagsAdd',
        'discountBulkTagsRemove',
        'inventorySetScheduledChanges',
        'inventoryShipmentSetBarcode',
        'marketCurrencySettingsUpdate',
        'marketRegionDelete',
        'marketRegionsCreate',
        'marketRegionsDelete',
        'marketWebPresenceCreate',
        'marketWebPresenceDelete',
        'marketWebPresenceUpdate',
        'orderEditRemoveLineItemDiscount',
        'orderEditUpdateDiscount',
        'pointOfSaleDeviceAssignToCashDrawer',
        'pointOfSaleDevicePaymentSessionAdjust',
        'pointOfSaleDevicePaymentSessionClose',
        'pointOfSaleDevicePaymentSessionCount',
        'pointOfSaleDevicePaymentSessionOpen',
        'returnLineItemRemoveFromReturn',
        'returnRefund',
        'stagedUploadTargetGenerate',
        'stagedUploadTargetsGenerate',
        'subscriptionBillingAttemptCreate',
        'subscriptionBillingCycleBulkCharge',
        'subscriptionBillingCycleBulkSearch',
        'subscriptionBillingCycleCharge',
        'subscriptionBillingCycleContractDraftCommit',
        'subscriptionBillingCycleContractDraftConcatenate',
        'subscriptionBillingCycleContractEdit',
        'subscriptionBillingCycleEditDelete',
        'subscriptionBillingCycleEditsDelete',
        'subscriptionBillingCycleScheduleEdit',
        'subscriptionBillingCycleSkip',
        'subscriptionBillingCycleUnskip',
        'subscriptionContractActivate',
        'subscriptionContractAtomicCreate',
        'subscriptionContractCancel',
        'subscriptionContractCreate',
        'subscriptionContractExpire',
        'subscriptionContractFail',
        'subscriptionContractPause',
        'subscriptionContractProductChange',
        'subscriptionContractSetNextBillingDate',
        'subscriptionContractUpdate',
        'subscriptionDraftCommit',
        'subscriptionDraftDiscountAdd',
        'subscriptionDraftDiscountCodeApply',
        'subscriptionDraftDiscountRemove',
        'subscriptionDraftDiscountUpdate',
        'subscriptionDraftFreeShippingDiscountAdd',
        'subscriptionDraftFreeShippingDiscountUpdate',
        'subscriptionDraftLineAdd',
        'subscriptionDraftLineRemove',
        'subscriptionDraftLineUpdate',
        'subscriptionDraftUpdate',
        'themeDuplicate',
      ],
      registeredButMissingFromCapturedSchema: [
        'metafieldDelete',
        'productVariantCreate',
        'productVariantDelete',
        'productVariantUpdate',
      ],
    });
  });

  it('audits captured Shopify Node implementors against the explicit Rust resolver inventory', () => {
    expect(nodeResolverCoverageAudit()).toEqual({
      capturedNodeImplementorCount: 203,
      localNodeResolverTypeCount: 52,
      localResolverBehaviorCounts: {
        projectLocalRecord: 49,
        returnKnownNull: 3,
      },
      unsupported: [
        'AbandonedCheckout',
        'AbandonedCheckoutLineItem',
        'Abandonment',
        'AddAllProductsOperation',
        'AdditionalFee',
        'AppCatalog',
        'AppCredit',
        'AppRevenueAttributionRecord',
        'BasicEvent',
        'BulkOperation',
        'BusinessEntity',
        'CalculatedOrder',
        'CashDrawer',
        'CashManagementCustomReasonCode',
        'CashManagementDefaultReasonCode',
        'CashManagementSystemReasonCode',
        'CashTrackingAdjustment',
        'CatalogCsvOperation',
        'Channel',
        'ChannelDefinition',
        'ChannelInformation',
        'CheckoutAndAccountsConfiguration',
        'CheckoutAndAccountsConfigurationOverride',
        'CheckoutProfile',
        'CommentEvent',
        'CompanyLocationCatalog',
        'CompanyLocationStaffMemberAssignment',
        'ConsentPolicy',
        'CurrencyExchangeAdjustment',
        'Customer',
        'CustomerAccountAppExtensionPage',
        'CustomerAccountNativePage',
        'CustomerPaymentMethod',
        'CustomerVisit',
        'DeliveryCarrierService',
        'DeliveryCondition',
        'DeliveryCountry',
        'DeliveryCustomization',
        'DeliveryLocationGroup',
        'DeliveryMethod',
        'DeliveryMethodDefinition',
        'DeliveryParticipant',
        'DeliveryProfile',
        'DeliveryProfileItem',
        'DeliveryPromiseParticipant',
        'DeliveryPromiseProvider',
        'DeliveryProvince',
        'DeliveryRateDefinition',
        'DeliveryZone',
        'DiscountAutomaticBxgy',
        'DiscountNode',
        'DiscountRedeemCodeBulkCreation',
        'Domain',
        'DraftOrder',
        'DraftOrderLineItem',
        'DraftOrderTag',
        'Duty',
        'ExchangeLineItem',
        'ExchangeV2',
        'Fulfillment',
        'FulfillmentEvent',
        'FulfillmentHold',
        'FulfillmentLineItem',
        'FulfillmentOrder',
        'FulfillmentOrderDestination',
        'FulfillmentOrderLineItem',
        'FulfillmentOrderMerchantRequest',
        'GiftCardCreditTransaction',
        'GiftCardDebitTransaction',
        'InventoryItemMeasurement',
        'LineItem',
        'LineItemGroup',
        'MailingAddress',
        'Market',
        'MarketCatalog',
        'MarketingActivity',
        'MarketingEvent',
        'MarketWebPresence',
        'Menu',
        'Metafield',
        'MetafieldDefinition',
        'Metaobject',
        'MetaobjectDefinition',
        'OnlineStoreTheme',
        'Order',
        'OrderAdjustment',
        'OrderDisputeSummary',
        'OrderEditSession',
        'OrderTransaction',
        'PaymentCustomization',
        'PaymentMandate',
        'PaymentSchedule',
        'PaymentTerms',
        'PaymentTermsTemplate',
        'PointOfSaleDevicePaymentSession',
        'PriceList',
        'PriceRule',
        'PriceRuleDiscountCode',
        'ProductOption',
        'ProductOptionValue',
        'ProductTaxonomyNode',
        'ProductVariantComponent',
        'Publication',
        'PublicationResourceOperation',
        'QuantityPriceBreak',
        'Refund',
        'RefundShippingLine',
        'Return',
        'ReturnableFulfillment',
        'ReturnLineItem',
        'ReturnReasonDefinition',
        'ReverseDelivery',
        'ReverseDeliveryLineItem',
        'ReverseFulfillmentOrder',
        'ReverseFulfillmentOrderDisposition',
        'ReverseFulfillmentOrderLineItem',
        'SaleAdditionalFee',
        'SavedSearch',
        'ScriptTag',
        'SellingPlan',
        'SellingPlanGroup',
        'ServerPixel',
        'Shop',
        'ShopifyPaymentsAccount',
        'ShopifyPaymentsBalanceTransaction',
        'ShopifyPaymentsBankAccount',
        'ShopifyPaymentsDisputeEvidence',
        'ShopifyPaymentsDisputeFileUpload',
        'ShopifyPaymentsDisputeFulfillment',
        'ShopifyPaymentsPayout',
        'StaffMember',
        'StandardMetafieldDefinitionTemplate',
        'StoreCreditAccount',
        'StoreCreditAccountCreditTransaction',
        'StoreCreditAccountDebitRevertTransaction',
        'StoreCreditAccountDebitTransaction',
        'StorefrontAccessToken',
        'SubscriptionBillingAttempt',
        'SubscriptionContract',
        'SubscriptionDraft',
        'TaxonomyAttribute',
        'TaxonomyCategory',
        'TaxonomyChoiceListAttribute',
        'TaxonomyMeasurementAttribute',
        'TaxonomyValue',
        'TenderTransaction',
        'TransactionFee',
        'UnverifiedReturnLineItem',
        'UrlRedirect',
        'UrlRedirectImport',
        'WebhookSubscription',
        'WebPixel',
      ],
      localInventoryNotInCapturedNodeInterface: ['TaxAppConfiguration'],
    });
  });
});
