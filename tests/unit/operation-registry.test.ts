import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { buildSchema } from 'graphql';
import { z } from 'zod';
import { loadNodeResolverInventory, loadOperationRegistry } from '../../scripts/conformance-scenario-registry.js';
import { parseJsonFileWithSchema } from '../../scripts/support/json-schemas.js';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const operationRegistryEntries = loadOperationRegistry(repoRoot);
const nodeResolverInventoryEntries = loadNodeResolverInventory(repoRoot);

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

const adminSchemaPath = resolve(repoRoot, 'config/admin-graphql/2026-04/schema.graphql');
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

function listAdminOperationRegistryEntries() {
  return listOperationRegistryEntries().filter((entry) => entry.apiSurface === 'admin');
}

function listStorefrontOperationRegistryEntries() {
  return listOperationRegistryEntries().filter((entry) => entry.apiSurface === 'storefront');
}

function sortedStrings(values: Iterable<string>): string[] {
  return [...values].sort((left, right) => left.localeCompare(right));
}

function capturedMutationNames() {
  const schema = buildSchema(readFileSync(adminSchemaPath, 'utf8'));
  const mutationType = schema.getMutationType();
  if (mutationType === null || mutationType === undefined) {
    throw new Error('captured Admin schema must expose a mutation root');
  }
  return sortedStrings(Object.keys(mutationType.getFields()));
}

function adminMutationCoverageAudit() {
  const schemaMutationNames = capturedMutationNames();
  const registryMutations = listAdminOperationRegistryEntries().filter((entry) => entry.type === 'mutation');
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
  it('keeps implemented capability identities unique by API surface, type, and name', () => {
    const implementedIdentities = listImplementedOperationRegistryEntries().map(
      (entry) => `${entry.apiSurface}:${entry.type}:${entry.name}`,
    );
    expect(new Set(implementedIdentities).size).toBe(implementedIdentities.length);
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
    expect(
      listOperationRegistryEntries().some((entry) => entry.apiSurface === 'admin' && entry.name === 'productCreate'),
    ).toBe(true);
  });

  it('loads Storefront roots from the captured Storefront root inventory with promoted content roots implemented', () => {
    const storefrontEntries = listStorefrontOperationRegistryEntries();
    expect(storefrontEntries.length).toBeGreaterThan(0);
    for (const root of [
      'article',
      'articles',
      'blog',
      'blogByHandle',
      'blogs',
      'customer',
      'localization',
      'locations',
      'menu',
      'page',
      'pageByHandle',
      'pages',
      'paymentSettings',
      'product',
      'productByHandle',
      'products',
      'publicApiVersions',
      'shop',
      'sitemap',
      'urlRedirects',
    ]) {
      expect(storefrontEntries).toContainEqual(
        expect.objectContaining({
          apiSurface: 'storefront',
          name: root,
          type: 'query',
          domain: 'storefront',
          implemented: true,
          runtimeTests: ['tests/graphql_routes/storefront.rs'],
        }),
      );
    }
    for (const root of [
      'customerAccessTokenCreate',
      'customerAccessTokenCreateWithMultipass',
      'customerAccessTokenDelete',
      'customerAccessTokenRenew',
      'customerActivate',
      'customerActivateByUrl',
      'customerCreate',
      'customerRecover',
      'customerReset',
      'customerResetByUrl',
    ]) {
      expect(storefrontEntries).toContainEqual(
        expect.objectContaining({
          apiSurface: 'storefront',
          name: root,
          type: 'mutation',
          domain: 'storefront',
          implemented: true,
          runtimeTests: ['tests/graphql_routes/storefront.rs'],
        }),
      );
    }
    expect(listOperationRegistryEntries()).toContainEqual(
      expect.objectContaining({
        apiSurface: 'admin',
        name: 'customerCreate',
        type: 'mutation',
        implemented: true,
      }),
    );
  });

  it('audits captured 2026-04 Admin mutation roots against the Rust registry', () => {
    expect(adminMutationCoverageAudit()).toEqual({
      capturedMutationCount: 514,
      registeredMutationCount: 438,
      implementedMutationCount: 413,
      implementedMutationRuntimeTestEvidence: {
        withRuntimeTests: 138,
        withoutRuntimeTests: 275,
      },
      declaredUnimplemented: [
        'companyContactSendWelcomeEmail',
        'consentPolicyUpdate',
        'customerPaymentMethodSendUpdateEmail',
        'deliverySettingUpdate',
        'disputeEvidenceUpdate',
        'menuCreate',
        'menuDelete',
        'menuUpdate',
        'mobilePlatformApplicationDelete',
        'orderRiskAssessmentCreate',
        'privacyFeaturesDisable',
        'serverPixelDelete',
        'shopifyPaymentsPayoutAlternateCurrencyCreate',
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
      registeredButMissingFromCapturedSchema: ['productVariantCreate', 'productVariantDelete', 'productVariantUpdate'],
    });
  });

  it('audits captured Shopify Node implementors against the explicit Rust resolver inventory', () => {
    expect(nodeResolverCoverageAudit()).toEqual({
      capturedNodeImplementorCount: 203,
      localNodeResolverTypeCount: 84,
      localResolverBehaviorCounts: {
        projectLocalRecord: 81,
        returnKnownNull: 3,
      },
      unsupported: [
        'AbandonedCheckout',
        'AbandonedCheckoutLineItem',
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
        'CustomerAccountAppExtensionPage',
        'CustomerAccountNativePage',
        'CustomerVisit',
        'DeliveryCarrierService',
        'DeliveryCondition',
        'DeliveryCountry',
        'DeliveryLocationGroup',
        'DeliveryMethod',
        'DeliveryMethodDefinition',
        'DeliveryParticipant',
        'DeliveryProfile',
        'DeliveryProfileItem',
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
        'FulfillmentOrderDestination',
        'FulfillmentOrderMerchantRequest',
        'InventoryItemMeasurement',
        'LineItem',
        'LineItemGroup',
        'Market',
        'MarketCatalog',
        'MarketingActivity',
        'MarketingEvent',
        'MarketWebPresence',
        'Menu',
        'Metafield',
        'MetafieldDefinition',
        'OnlineStoreTheme',
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
        'ReturnReasonDefinition',
        'ReverseFulfillmentOrderDisposition',
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
        'UrlRedirect',
        'UrlRedirectImport',
        'WebhookSubscription',
        'WebPixel',
      ],
      localInventoryNotInCapturedNodeInterface: [
        'ShopifyFunction',
        'StoreCreditAccountTransaction',
        'TaxAppConfiguration',
      ],
    });
  });
});
