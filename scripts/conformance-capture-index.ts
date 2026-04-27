import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { z } from 'zod';

const domainSchema = z.enum([
  'admin-platform',
  'apps',
  'bulk-operations',
  'collections',
  'customers',
  'discounts',
  'draft-orders',
  'files',
  'gift-cards',
  'inventory',
  'marketing',
  'markets',
  'metafields',
  'metaobjects',
  'orders',
  'payments',
  'privacy',
  'products',
  'shipping-fulfillments',
  'store-properties',
  'webhooks',
]);

const statusCheckSchema = z.enum([
  'conformance:status',
  'conformance:check',
  'conformance:parity',
  'targeted-runtime-test',
  'manual-capture-review',
]);

export const captureIndexEntrySchema = z.strictObject({
  domain: domainSchema,
  packageScript: z.string().regex(/^conformance:capture-/u),
  scriptPath: z.string().regex(/^scripts\/.+\.(?:ts|mts)$/u),
  purpose: z.string().min(1),
  requiredAuthScopes: z.array(z.string().min(1)).min(1),
  fixtureOutputs: z.array(z.string().min(1)).min(1),
  cleanupBehavior: z.string().min(1),
  expectedStatusChecks: z.array(statusCheckSchema).min(1),
  notes: z.string().min(1).optional(),
});

const captureIndexSchema = z.array(captureIndexEntrySchema);
const packageJsonSchema = z.object({
  scripts: z.record(z.string(), z.string()),
});

export type ConformanceCaptureIndexEntry = z.infer<typeof captureIndexEntrySchema>;
type StatusCheck = z.infer<typeof statusCheckSchema>;

const DEFAULT_STATUS_CHECKS: StatusCheck[] = ['conformance:status', 'conformance:check', 'conformance:parity'];
const CAPTURE_ROOT = 'fixtures/conformance/<store>/<api-version>/';
const LOCAL_RUNTIME_ROOT = 'fixtures/conformance/local-runtime/<api-version>/';

function defineCaptureIndex(entries: Array<z.input<typeof captureIndexEntrySchema>>): ConformanceCaptureIndexEntry[] {
  return captureIndexSchema.parse(entries);
}

export const conformanceCaptureIndex = defineCaptureIndex([
  {
    domain: 'products',
    packageScript: 'conformance:capture-products',
    scriptPath: 'scripts/capture-product-conformance.mts',
    purpose: 'Product read baselines, search grammar, selected product detail subresources.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-*.json`, 'product catalog/search parity specs when promoted'],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-mutations',
    scriptPath: 'scripts/capture-product-mutation-conformance.mts',
    purpose: 'productCreate/productUpdate/productDelete success and validation behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-mutation-*.json`, 'product mutation parity specs when promoted'],
    cleanupBehavior: 'Creates disposable products and deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-state-mutations',
    scriptPath: 'scripts/capture-product-state-mutation-conformance.mts',
    purpose: 'productChangeStatus/tagsAdd/tagsRemove mutation branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-state-mutation-*.json`],
    cleanupBehavior: 'Creates temporary products and resets/deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-publications',
    scriptPath: 'scripts/capture-product-publication-conformance.mts',
    purpose: 'Publication aggregate reads plus productPublish/productUnpublish probes.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-publication-*.json`, 'publication blocker notes when access is missing'],
    cleanupBehavior: 'Publishes/unpublishes disposable products only after publication target probes pass.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-media-mutations',
    scriptPath: 'scripts/capture-product-media-mutation-conformance.mts',
    purpose: 'Product media create/update/delete validation and downstream read branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-media-*.json`, 'config/parity-specs/product-media-*.json'],
    cleanupBehavior: 'Creates disposable product/media records and deletes the product during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    packageScript: 'conformance:capture-file-mutations',
    scriptPath: 'scripts/capture-file-mutation-conformance.mts',
    purpose: 'fileCreate/fileUpdate/fileDelete and staged upload interactions.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [`${CAPTURE_ROOT}file-mutation-*.json`, `${LOCAL_RUNTIME_ROOT}files-upload-local-runtime.json`],
    cleanupBehavior:
      'Deletes created files when Shopify returns file IDs; local-runtime fixtures need no Shopify cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-option-mutations',
    scriptPath: 'scripts/capture-product-option-mutation-conformance.mts',
    purpose: 'productOptionsCreate/productOptionUpdate/productOptionsDelete mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-option-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products/options and deletes the products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-variant-mutations',
    scriptPath: 'scripts/capture-product-variant-mutation-conformance.mts',
    purpose: 'Product variant create/update/delete mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-variant-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products/variants and deletes the products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-variant-validations',
    scriptPath: 'scripts/capture-product-variant-validation-conformance.mts',
    purpose: 'Bulk variant validation atomicity for create/update/delete.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variants-bulk-validation-atomicity.json`,
      'config/parity-specs/product-variants-bulk-validation-atomicity.json',
    ],
    cleanupBehavior: 'Creates disposable products and removes them after validation probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    packageScript: 'conformance:capture-inventory-item-mutations',
    scriptPath: 'scripts/capture-inventory-item-mutation-conformance.mts',
    purpose: 'inventoryItemUpdate and product-backed inventory item mutation behavior.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory', 'write_inventory'],
    fixtureOutputs: [`${CAPTURE_ROOT}inventory-item-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products to own inventory items and deletes those products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    packageScript: 'conformance:capture-inventory-linkage-mutations',
    scriptPath: 'scripts/capture-inventory-linkage-mutation-conformance.mts',
    purpose: 'inventoryActivate/inventoryDeactivate/inventoryBulkToggleActivation linkage behavior.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-linkage-mutation-*.json`,
      'blocker notes when store topology is insufficient',
    ],
    cleanupBehavior: 'Creates disposable products; some success paths require a second safe location before capture.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'metafields',
    packageScript: 'conformance:capture-product-metafield-mutations',
    scriptPath: 'scripts/capture-product-metafield-mutation-conformance.mts',
    purpose: 'Product-scoped metafieldsSet/metafieldsDelete mutation behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-metafield-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products/collections and removes them after metafield probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    packageScript: 'conformance:capture-metafield-definition-pinning',
    scriptPath: 'scripts/capture-metafield-definition-pinning-conformance.mts',
    purpose: 'metafieldDefinitionPin/metafieldDefinitionUnpin behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}metafield-definition-pinning.json`],
    cleanupBehavior: 'Creates temporary product-owned definitions and deletes them after pinning probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-graph-mutations',
    scriptPath: 'scripts/capture-product-graph-mutation-conformance.mts',
    purpose: 'Product graph mutation branches that span product/options/variants/media.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-graph-mutation-*.json`],
    cleanupBehavior: 'Uses disposable product graphs with best-effort product cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    packageScript: 'conformance:capture-product-inventory-reads',
    scriptPath: 'scripts/capture-product-inventory-read-conformance.mts',
    purpose: 'Product-adjacent inventory read shapes and linkage baselines.',
    requiredAuthScopes: ['read_products', 'read_inventory', 'read_locations'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-inventory-*.json`],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    packageScript: 'conformance:capture-product-helper-reads',
    scriptPath: 'scripts/capture-product-helper-read-conformance.mts',
    purpose: 'Product helper roots and read-only compatibility wrappers.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-helper-roots-read.json`,
      'config/parity-specs/product-helper-roots-read.json',
    ],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    packageScript: 'conformance:capture-metafield-definition-mutations',
    scriptPath: 'scripts/capture-metafield-definition-mutation-conformance.mts',
    purpose: 'Metafield definition mutation validation branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}standard-metafield-definition-enable-validation.json`],
    cleanupBehavior: 'Validation-oriented capture; success paths require explicit disposable setup/cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'metafields',
    packageScript: 'conformance:capture-metafield-definition-lifecycle',
    scriptPath: 'scripts/capture-metafield-definition-lifecycle-conformance.mts',
    purpose: 'Product-owned metafieldDefinitionCreate/update/delete lifecycle.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-lifecycle-mutations.json`,
      'config/parity-specs/metafield-definition-lifecycle-mutations.json',
    ],
    cleanupBehavior: 'Deletes created definitions and disposable product with captured cleanup steps.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    packageScript: 'conformance:capture-metaobjects',
    scriptPath: 'scripts/capture-metaobject-read-conformance.mts',
    purpose: 'Metaobject definition/entry reads and minimal disposable seed behavior.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [`${CAPTURE_ROOT}metaobjects-read.json`, 'config/parity-specs/metaobjects-read.json'],
    cleanupBehavior: 'Deletes seeded metaobject entries/definitions after read capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    packageScript: 'conformance:capture-inventory-adjustments',
    scriptPath: 'scripts/capture-inventory-adjustment-conformance.mts',
    purpose: 'Inventory quantity adjustment/move/set mutation behavior.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-quantity-roots-parity.json`,
      'config/parity-specs/inventory-quantity-roots-parity.json',
    ],
    cleanupBehavior:
      'Uses disposable products/inventory levels where possible; review store topology before success captures.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'markets',
    packageScript: 'conformance:capture-markets',
    scriptPath: 'scripts/capture-market-conformance.mts',
    purpose: 'Markets read baselines and localization-adjacent validation probes.',
    requiredAuthScopes: ['read_markets', 'read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}markets-*.json`],
    cleanupBehavior:
      'Read/validation oriented; do not run market lifecycle writes without disposable setup and cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'marketing',
    packageScript: 'conformance:capture-marketing',
    scriptPath: 'scripts/capture-marketing-conformance.mts',
    purpose: 'Marketing activity/event/engagement roots and mutation branches.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [`${CAPTURE_ROOT}marketing-*.json`],
    cleanupBehavior: 'Uses synthetic external IDs; cleanup depends on the branch captured.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    packageScript: 'conformance:capture-collections',
    scriptPath: 'scripts/capture-collection-conformance.mts',
    purpose: 'Collection read baselines for custom/smart collections and product membership.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-*.json`],
    cleanupBehavior: 'Read-only capture against existing store collections; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    packageScript: 'conformance:capture-collection-mutations',
    scriptPath: 'scripts/capture-collection-mutation-conformance.mts',
    purpose: 'collectionCreate/update/delete/addProducts/removeProducts mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-mutation-*.json`],
    cleanupBehavior: 'Creates disposable collections/products and deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    packageScript: 'conformance:capture-collection-publications',
    scriptPath: 'scripts/capture-collection-mutation-conformance.mts',
    purpose: 'Collection publication behavior covered by the collection mutation harness when enabled.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-mutation-*.json`],
    cleanupBehavior: 'Shares disposable collection cleanup with the collection mutation harness.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    packageScript: 'conformance:capture-locations',
    scriptPath: 'scripts/capture-location-conformance.mts',
    purpose: 'Location roots and inventory/publication-adjacent store property reads.',
    requiredAuthScopes: ['read_locations'],
    fixtureOutputs: [`${CAPTURE_ROOT}locations-*.json`],
    cleanupBehavior: 'Read-only by default; location lifecycle writes need disposable location setup and cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    packageScript: 'conformance:capture-store-properties',
    scriptPath: 'scripts/capture-location-conformance.mts',
    purpose: 'Store property roots sharing the location capture harness.',
    requiredAuthScopes: ['read_locations', 'read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}store-properties-*.json`, `${CAPTURE_ROOT}locations-*.json`],
    cleanupBehavior: 'Read-only by default; avoid merchant-topology writes without explicit cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    packageScript: 'conformance:capture-shop-policies',
    scriptPath: 'scripts/capture-shop-policy-conformance.ts',
    purpose: 'shopPolicyUpdate and legal-policy read/write behavior.',
    requiredAuthScopes: ['read_content', 'write_content or policy-management access'],
    fixtureOutputs: [`${CAPTURE_ROOT}shop-policy-*.json`],
    cleanupBehavior: 'Restores prior policy content when a write branch is captured.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'privacy',
    packageScript: 'conformance:capture-privacy',
    scriptPath: 'scripts/capture-privacy-conformance.ts',
    purpose: 'Privacy/data-sale read and mutation roots.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'privacy API access'],
    fixtureOutputs: [`${CAPTURE_ROOT}privacy-*.json`],
    cleanupBehavior: 'Uses disposable customer records where writes are captured.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'privacy',
    packageScript: 'conformance:capture-data-sale-opt-out',
    scriptPath: 'scripts/capture-data-sale-opt-out-conformance.ts',
    purpose: 'dataSaleOptOut behavior and downstream customer privacy read effects.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'privacy API access'],
    fixtureOutputs: [`${CAPTURE_ROOT}data-sale-opt-out-*.json`],
    cleanupBehavior: 'Creates/deletes disposable customer records for opt-out probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    packageScript: 'conformance:capture-root-operations',
    scriptPath: 'scripts/capture-admin-graphql-root-operation-introspection.mts',
    purpose: 'Admin GraphQL root operation introspection for coverage-map updates.',
    requiredAuthScopes: ['schema introspection access through the active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}root-operation-introspection.json`,
      'config/operation-registry.json updates when intentionally edited',
    ],
    cleanupBehavior: 'Read-only introspection; no cleanup expected.',
    expectedStatusChecks: ['conformance:check', 'conformance:status'],
  },
  {
    domain: 'orders',
    packageScript: 'conformance:capture-orders',
    scriptPath: 'scripts/capture-order-conformance.mts',
    purpose: 'Order reads, orderCreate, order-edit, transaction, and downstream order-family behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}order-*.json`, 'order blocker notes when credential/store access is insufficient'],
    cleanupBehavior: 'Creates/cancels disposable orders only after credential and store-state probes pass.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'shipping-fulfillments',
    packageScript: 'conformance:capture-fulfillment-detail-events',
    scriptPath: 'scripts/capture-fulfillment-detail-events-conformance.ts',
    purpose: 'Fulfillment detail event capture on disposable orders.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [`${CAPTURE_ROOT}fulfillment-detail-events.json`],
    cleanupBehavior: 'Cancels/deletes disposable order state where Shopify permits cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    packageScript: 'conformance:capture-draft-order-family',
    scriptPath: 'scripts/capture-draft-order-family-conformance.mts',
    purpose: 'Draft order create/update/delete/complete and downstream read behavior.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}draft-order-*.json`],
    cleanupBehavior: 'Creates disposable draft orders and deletes/completes/cancels them per branch.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    packageScript: 'conformance:capture-draft-order-residual-helpers',
    scriptPath: 'scripts/capture-draft-order-residual-helper-conformance.mts',
    purpose: 'Residual draft-order helper roots such as calculate, bulk tags, invoices, and delivery options.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-residual-helper-roots.json`,
      'config/parity-specs/draft-order-residual-helper-roots.json',
    ],
    cleanupBehavior: 'Creates disposable draft orders/products and removes them after helper probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    packageScript: 'conformance:capture-draft-order-invoice-send-safety',
    scriptPath: 'scripts/capture-draft-order-invoice-send-safety-conformance.ts',
    purpose: 'Safety probes for draftOrderInvoiceSend side effects and validation branches.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [`${CAPTURE_ROOT}draft-order-invoice-send-safety.json`],
    cleanupBehavior: 'Uses safety-first validation branches; review manually before any customer-visible send path.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'discounts',
    packageScript: 'conformance:capture-discounts',
    scriptPath: 'scripts/capture-discount-conformance.ts',
    purpose: 'Discount read roots and baseline validation branches.',
    requiredAuthScopes: ['read_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-*.json`],
    cleanupBehavior: 'Read/validation oriented; lifecycle scripts own write cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    packageScript: 'conformance:capture-discount-lifecycle',
    scriptPath: 'scripts/capture-discount-code-basic-lifecycle-conformance.ts',
    purpose: 'Code discount basic create/update/delete lifecycle.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-code-basic-lifecycle.json`],
    cleanupBehavior: 'Deletes created code discount during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    packageScript: 'conformance:capture-discount-bxgy-lifecycle',
    scriptPath: 'scripts/capture-discount-bxgy-lifecycle-conformance.ts',
    purpose: 'Buy-X-get-Y code and automatic discount lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-bxgy-lifecycle.json`],
    cleanupBehavior: 'Deletes created discounts/products/collections in reverse-order cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    packageScript: 'conformance:capture-discount-free-shipping-lifecycle',
    scriptPath: 'scripts/capture-discount-free-shipping-lifecycle-conformance.ts',
    purpose: 'Free-shipping discount lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-free-shipping-lifecycle.json`],
    cleanupBehavior: 'Deletes created free-shipping discounts during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    packageScript: 'conformance:capture-discount-validation',
    scriptPath: 'scripts/capture-discount-validation-conformance.ts',
    purpose: 'Discount validation guardrails without broad lifecycle side effects.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-validation.json`],
    cleanupBehavior: 'Validation-oriented; deletes any created disposable discount artifacts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'apps',
    packageScript: 'conformance:capture-app-billing',
    scriptPath: 'scripts/capture-app-billing-conformance.ts',
    purpose: 'App billing/access read roots and blocker evidence.',
    requiredAuthScopes: ['app billing access for the installed app'],
    fixtureOutputs: [`${CAPTURE_ROOT}app-billing-access-read.json`],
    cleanupBehavior: 'Read-only capture; no billing mutation cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    packageScript: 'conformance:capture-finance-risk',
    scriptPath: 'scripts/capture-finance-risk-conformance.ts',
    purpose: 'Finance, risk, POS, dispute, and Shop Pay receipt read/access evidence.',
    requiredAuthScopes: ['Shopify Payments, finance, risk, and POS root access for the active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}finance-risk-access-read.json`,
      'config/parity-specs/finance-risk-no-data-read.json',
    ],
    cleanupBehavior: 'Read/access capture only; do not create or invent sensitive financial records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    packageScript: 'conformance:capture-admin-platform',
    scriptPath: 'scripts/capture-admin-platform-conformance.mts',
    purpose: 'Admin platform utility roots and staff/access blocker evidence.',
    requiredAuthScopes: ['active Admin API token; staff/utility roots may require plan or staff permissions'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-utility-roots.json`,
      'config/parity-specs/admin-platform-utility-reads.json',
    ],
    cleanupBehavior: 'Read-only/blocked-root capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    packageScript: 'conformance:capture-order-refunds',
    scriptPath: 'scripts/capture-order-refund-conformance.mts',
    purpose: 'Order refund calculation/create behavior against disposable orders.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [`${CAPTURE_ROOT}order-refund-*.json`],
    cleanupBehavior: 'Uses disposable orders and records cleanup/cancel evidence where possible.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'shipping-fulfillments',
    packageScript: 'conformance:capture-fulfillment-order-lifecycle',
    scriptPath: 'scripts/capture-fulfillment-order-lifecycle-conformance.ts',
    purpose: 'Fulfillment order hold/request/cancel/close lifecycle behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [`${CAPTURE_ROOT}fulfillment-order-lifecycle.json`],
    cleanupBehavior: 'Cancels disposable order and records cleanup captures.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    packageScript: 'conformance:capture-delivery-profiles',
    scriptPath: 'scripts/capture-delivery-profile-conformance.ts',
    purpose: 'Delivery profile read/write lifecycle behavior.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [`${CAPTURE_ROOT}delivery-profile-*.json`, 'config/parity-specs/delivery-profile-*.json'],
    cleanupBehavior: 'Removes or restores created delivery profile artifacts; review default-profile protections.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'bulk-operations',
    packageScript: 'conformance:capture-bulk-operations',
    scriptPath: 'scripts/capture-bulk-operation-status-conformance.ts',
    purpose: 'Bulk operation status/catalog/cancel roots.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-status-catalog-cancel.json`,
      'config/parity-specs/bulk-operation-status-catalog-cancel.json',
    ],
    cleanupBehavior: 'Starts/cancels safe bulk operations where the harness allows it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    packageScript: 'conformance:capture-webhook-subscriptions',
    scriptPath: 'scripts/capture-webhook-subscription-conformance.ts',
    purpose: 'Webhook subscription create/read/delete and access-scope observations.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [`${CAPTURE_ROOT}webhook-subscription-*.json`],
    cleanupBehavior: 'Deletes created API webhook subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    packageScript: 'conformance:capture-gift-cards',
    scriptPath: 'scripts/capture-gift-card-conformance.ts',
    purpose: 'Gift-card read/configuration/count behavior plus create/update/credit/debit/deactivate lifecycle parity.',
    requiredAuthScopes: [
      'read_gift_cards',
      'write_gift_cards',
      'read_gift_card_transactions',
      'write_gift_card_transactions',
    ],
    fixtureOutputs: [`${CAPTURE_ROOT}gift-card-lifecycle.json`, 'config/parity-specs/gift-card-lifecycle.json'],
    cleanupBehavior:
      'Creates a disposable gift card, records transaction lifecycle behavior, and deactivates it; notification roots are not executed.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customers',
    scriptPath: 'scripts/capture-customer-conformance.mts',
    purpose: 'Customer read baselines and nested customer subresources.',
    requiredAuthScopes: ['read_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-*.json`, 'customer read parity specs when promoted'],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-mutations',
    scriptPath: 'scripts/capture-customer-mutation-conformance.mts',
    purpose: 'customerCreate/customerUpdate/customerDelete mutation family.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-mutation-*.json`],
    cleanupBehavior: 'Creates disposable customers and deletes them in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-input-validation',
    scriptPath: 'scripts/capture-customer-input-validation-conformance.ts',
    purpose: 'Customer input validation, normalization, duplicate identity, and downstream read behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'read_customer_merge', 'write_customer_merge'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-input-validation-parity.json`,
      'config/parity-specs/customerInputValidation-parity.json',
      'config/parity-requests/customerInputValidation-*.graphql',
    ],
    cleanupBehavior: 'Creates disposable customers; deletes remaining records after delete and merge probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-account-page-data-erasure',
    scriptPath: 'scripts/capture-customer-account-page-data-erasure-conformance.ts',
    purpose: 'Customer Account page reads plus customer data-erasure request/cancel success and validation payloads.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'write_customer_data_erasure'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-account-page-data-erasure.json`,
      'config/parity-specs/customer-account-page-data-erasure.json',
    ],
    cleanupBehavior:
      'Creates a disposable customer, requests and cancels data erasure, then cancels again and deletes the customer in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-store-credit',
    scriptPath: 'scripts/capture-store-credit-conformance.ts',
    purpose: 'Store credit account creation setup, account-id credit/debit mutations, and downstream balance reads.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'store credit account access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}store-credit-account-parity.json`,
      'config/parity-specs/store-credit-account-local-staging.json',
    ],
    cleanupBehavior:
      'Creates a disposable customer, credits/debits a real store credit account, debits the remaining balance back to zero, then deletes the customer.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-set',
    scriptPath: 'scripts/capture-customer-set-conformance.mts',
    purpose: 'customerSet upsert/identifier semantics.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-set-*.json`],
    cleanupBehavior: 'Tracks all created/upserted customer IDs and deletes remaining records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-addresses',
    scriptPath: 'scripts/capture-customer-address-conformance.mts',
    purpose: 'Customer address lifecycle, normalization, defaulting, and validation.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-address-*.json`],
    cleanupBehavior: 'Creates disposable customers/addresses and deletes the customers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-merge',
    scriptPath: 'scripts/capture-customer-merge-conformance.mts',
    purpose: 'Base two-customer customerMerge behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'read_customer_merge', 'write_customer_merge'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-merge-parity.json`, 'config/parity-specs/customerMerge-parity.json'],
    cleanupBehavior: 'Creates disposable customers; merge consumes source records and cleanup removes leftovers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-merge-attached-resources',
    scriptPath: 'scripts/capture-customer-merge-attached-resources-conformance.mts',
    purpose: 'customerMerge with attached address/metafield/order resources.',
    requiredAuthScopes: [
      'read_customers',
      'write_customers',
      'read_customer_merge',
      'write_customer_merge',
      'read_orders',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-merge-attached-resources-parity.json`,
      'config/parity-specs/customerMerge-attached-resources-parity.json',
    ],
    cleanupBehavior:
      'Creates disposable customer graph; merge consumes source and cleanup removes remaining artifacts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-consent',
    scriptPath: 'scripts/capture-customer-consent-conformance.ts',
    purpose: 'Email/SMS marketing consent update behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-email-marketing-consent-update-parity.json`,
      `${CAPTURE_ROOT}customer-sms-marketing-consent-update-parity.json`,
    ],
    cleanupBehavior: 'Creates and deletes disposable customers for consent transitions.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-tax-exemptions',
    scriptPath: 'scripts/capture-customer-tax-exemption-conformance.ts',
    purpose: 'Customer tax exemption update behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-tax-exemption-*.json`],
    cleanupBehavior: 'Creates disposable customer and deletes it after tax-exemption probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    packageScript: 'conformance:capture-customer-order-summary',
    scriptPath: 'scripts/capture-customer-order-summary-conformance.ts',
    purpose: 'Customer order summary reads against order-linked customer state.',
    requiredAuthScopes: ['read_customers', 'read_orders', 'write_orders'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-order-summary-*.json`],
    cleanupBehavior: 'Creates disposable order/customer state and records cleanup/cancel result.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
]);

export function loadPackageCaptureScripts(repoRoot = process.cwd()): Map<string, string> {
  const packageJsonPath = path.join(repoRoot, 'package.json');
  const parsed = packageJsonSchema.parse(JSON.parse(readFileSync(packageJsonPath, 'utf8')) as unknown);
  return new Map(Object.entries(parsed.scripts).filter(([name]) => name.startsWith('conformance:capture-')));
}

export function validateCaptureIndexAgainstPackageScripts(
  entries: ConformanceCaptureIndexEntry[] = conformanceCaptureIndex,
  packageScripts: Map<string, string> = loadPackageCaptureScripts(),
): { missingFromIndex: string[]; missingFromPackage: string[]; scriptPathMismatches: string[] } {
  const indexedScripts = new Set(entries.map((entry) => entry.packageScript));
  const missingFromIndex = [...packageScripts.keys()].filter((script) => !indexedScripts.has(script)).sort();
  const missingFromPackage = entries
    .map((entry) => entry.packageScript)
    .filter((script) => !packageScripts.has(script))
    .sort();
  const scriptPathMismatches = entries
    .filter((entry) => {
      const packageCommand = packageScripts.get(entry.packageScript);
      return typeof packageCommand === 'string' && !packageCommand.includes(entry.scriptPath);
    })
    .map((entry) => entry.packageScript)
    .sort();

  return {
    missingFromIndex,
    missingFromPackage,
    scriptPathMismatches,
  };
}

export function renderCaptureIndexMarkdown(entries: ConformanceCaptureIndexEntry[] = conformanceCaptureIndex): string {
  const lines = [
    '# Conformance Capture Command Index',
    '',
    'Run capture commands with `corepack pnpm <packageScript>` after `corepack pnpm conformance:probe` confirms the active Shopify credential and store.',
    '',
  ];

  const domains = [...new Set(entries.map((entry) => entry.domain))].sort();
  for (const domain of domains) {
    lines.push(`## ${domain}`, '');
    lines.push('| Command | Script | Purpose | Required auth/scopes | Outputs | Cleanup | Status checks |');
    lines.push('| --- | --- | --- | --- | --- | --- | --- |');

    for (const entry of entries.filter((candidate) => candidate.domain === domain)) {
      const cells = [
        `\`corepack pnpm ${entry.packageScript}\``,
        `\`${entry.scriptPath}\``,
        escapeTableCell(entry.purpose),
        entry.requiredAuthScopes.map((scope) => `\`${scope}\``).join('<br>'),
        entry.fixtureOutputs.map((output) => `\`${output}\``).join('<br>'),
        escapeTableCell(entry.cleanupBehavior),
        entry.expectedStatusChecks.map(renderStatusCheck).join('<br>'),
      ];
      lines.push(`| ${cells.join(' | ')} |`);
    }

    lines.push('');
  }

  return `${lines.join('\n')}\n`;
}

function escapeTableCell(value: string): string {
  return value.replace(/\|/gu, '\\|').replace(/\n/gu, '<br>');
}

function renderStatusCheck(check: ConformanceCaptureIndexEntry['expectedStatusChecks'][number]): string {
  if (check === 'manual-capture-review') {
    return '`manual-capture-review`';
  }

  if (check === 'targeted-runtime-test') {
    return '`targeted-runtime-test`';
  }

  return `\`corepack pnpm ${check}\``;
}

function readFlagValue(args: string[], flag: string): string | null {
  const index = args.indexOf(flag);
  if (index === -1) {
    return null;
  }

  return args[index + 1] ?? null;
}

function runCli(): void {
  const args = process.argv.slice(2);
  const domain = readFlagValue(args, '--domain');
  const packageScript = readFlagValue(args, '--script');
  const outputJson = args.includes('--json');
  const validation = validateCaptureIndexAgainstPackageScripts();
  if (
    validation.missingFromIndex.length > 0 ||
    validation.missingFromPackage.length > 0 ||
    validation.scriptPathMismatches.length > 0
  ) {
    throw new Error(`Conformance capture index is out of sync: ${JSON.stringify(validation, null, 2)}`);
  }

  let entries = conformanceCaptureIndex;
  if (domain) {
    entries = entries.filter((entry) => entry.domain === domain);
  }
  if (packageScript) {
    entries = entries.filter((entry) => entry.packageScript === packageScript);
  }

  process.stdout.write(outputJson ? `${JSON.stringify(entries, null, 2)}\n` : renderCaptureIndexMarkdown(entries));
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  runCli();
}
