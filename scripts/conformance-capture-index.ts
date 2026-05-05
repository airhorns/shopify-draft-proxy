import { readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { z } from 'zod';

const domainSchema = z.enum([
  'admin-platform',
  'apps',
  'b2b',
  'bulk-operations',
  'collections',
  'customers',
  'discounts',
  'draft-orders',
  'files',
  'gift-cards',
  'functions',
  'inventory',
  'localization',
  'marketing',
  'markets',
  'metafields',
  'metaobjects',
  'online-store',
  'orders',
  'payments',
  'privacy',
  'products',
  'saved-searches',
  'segments',
  'shipping-fulfillments',
  'store-properties',
  'webhooks',
]);

const statusCheckSchema = z.enum([
  'conformance:status',
  'conformance:check',
  'conformance:parity',
  'gleam:test',
  'targeted-runtime-test',
  'manual-capture-review',
]);

export const captureIndexEntrySchema = z.strictObject({
  domain: domainSchema,
  captureId: z.string().regex(/^[a-z0-9][a-z0-9-]*$/u),
  environment: z.record(z.string(), z.string().min(1)).optional(),
  scriptPath: z.string().regex(/^scripts\/.+\.(?:ts|mts)$/u),
  purpose: z.string().min(1),
  requiredAuthScopes: z.array(z.string().min(1)).min(1),
  fixtureOutputs: z.array(z.string().min(1)).min(1),
  cleanupBehavior: z.string().min(1),
  expectedStatusChecks: z.array(statusCheckSchema).min(1),
  notes: z.string().min(1).optional(),
});

const captureIndexSchema = z.array(captureIndexEntrySchema);

export type ConformanceCaptureIndexEntry = z.infer<typeof captureIndexEntrySchema>;
type StatusCheck = z.infer<typeof statusCheckSchema>;

const DEFAULT_STATUS_CHECKS: StatusCheck[] = ['conformance:status', 'conformance:check', 'gleam:test'];
const CAPTURE_ROOT = 'fixtures/conformance/<store>/<api-version>/<domain-folder>/';
const LOCAL_RUNTIME_ROOT = 'fixtures/conformance/local-runtime/<api-version>/<domain-folder>/';

function defineCaptureIndex(entries: Array<z.input<typeof captureIndexEntrySchema>>): ConformanceCaptureIndexEntry[] {
  return captureIndexSchema.parse(entries);
}

export const conformanceCaptureIndex = defineCaptureIndex([
  {
    domain: 'b2b',
    captureId: 'b2b-company-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-company-lifecycle-conformance.mts',
    purpose:
      'B2B company lifecycle, customer-as-contact assignment, main-contact assignment/revocation, wrong-company main-contact validation, main-contact delete clearing, bulk delete, explicit delete, and post-delete empty reads.',
    requiredAuthScopes: ['read_companies', 'write_companies', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-company-contact-main-delete.json`,
      'config/parity-specs/b2b/b2b-company-contact-main-delete.json',
    ],
    cleanupBehavior:
      'Creates disposable companies and a disposable customer; deletes companies during the scenario and deletes the customer in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-location-assignments-tax',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-location-assignment-conformance.mts',
    purpose:
      'B2B contact/location role assignments, automatic main-contact role assignment, address assignment/delete, tax settings, and downstream relationship reads.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-contact-location-assignments-tax.json`,
      'config/parity-specs/b2b/b2b-contact-location-assignments-tax.json',
    ],
    cleanupBehavior:
      'Creates one disposable company with additional disposable company locations; revokes explicit assignments, deletes the staged billing address, and deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      '`staffMembers(first:)` is access denied for the current conformance token, so staff assignment remains runtime-test-backed rather than live-parity-backed.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-staff-assignment-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-staff-assignment-validation-conformance.ts',
    purpose:
      'B2B company location staff-member assignment validation for unknown StaffMember IDs and unknown CompanyLocationStaffMemberAssignment IDs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-staff-assignment-validation.json`,
      'config/parity-specs/b2b/staff_assign_unknown_user.json',
      'config/parity-specs/b2b/staff_remove_unknown_assignment.json',
      'config/parity-requests/b2b/b2b-staff-assignment-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable company location, records validation failures that do not stage staff assignments, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The current conformance credential still receives ACCESS_DENIED for staffMembers(first:), so valid-staff partial-success assignment remains runtime-test-backed until an eligible staff catalog credential is available.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-bulk-mutation-field-paths',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-bulk-mutation-field-paths-conformance.mts',
    purpose:
      'B2B bulk mutation userErrors field paths for list-indexed company/contact/location delete, role assignment, role revoke, and staff assignment/removal validation branches.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-bulk-mutation-field-paths.json`,
      'config/parity-specs/b2b/b2b-bulk-mutation-field-paths.json',
      'config/parity-requests/b2b/b2b-bulk-field-paths-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable B2B companies, contacts, locations, and role assignments; bulk-delete/revoke scenario steps remove most setup records and the script deletes the primary company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Staff assignment evidence uses Shopify validation for an invalid StaffMember input shape because the current conformance token cannot read staffMembers/currentStaffMember IDs.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-removal-role-assignment-cascade',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-removal-role-assignment-cascade-conformance.mts',
    purpose:
      'B2B contact delete, bulk contact delete, and remove-from-company cascades that scrub location-side role assignments for the removed contact.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}contact-delete-cleans-role-assignments.json`,
      `${CAPTURE_ROOT}contacts-delete-cleans-role-assignments.json`,
      `${CAPTURE_ROOT}contact-remove-from-company-cleans-role-assignments.json`,
      'config/parity-specs/b2b/contact_delete_cleans_role_assignments.json',
      'config/parity-specs/b2b/contacts_delete_cleans_role_assignments.json',
      'config/parity-specs/b2b/contact_remove_from_company_cleans_role_assignments.json',
      'config/parity-requests/b2b/contact-role-cascade-*.graphql',
      'config/parity-requests/b2b/contact-delete-cleans-role-assignments.graphql',
      'config/parity-requests/b2b/contacts-delete-cleans-role-assignments.graphql',
      'config/parity-requests/b2b/contact-remove-from-company-cleans-role-assignments.graphql',
    ],
    cleanupBehavior:
      'Creates disposable B2B companies with an automatic main-location role assignment and an explicit second-location assignment; removes the contact through each supported path and deletes each company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-string-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-string-validation-conformance.mts',
    purpose: 'B2B company/contact/location free-text length and HTML validation branches.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-string-validation.json`,
      'config/parity-specs/b2b/b2b-string-validation.json',
      'config/parity-requests/b2b/b2b-string-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates one setup company for child mutation validation plus cleanup for any live branch that unexpectedly creates a company.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The capture intentionally records live HTML mismatch probes so reviewers can distinguish executable parity-backed validation branches from current Admin behavior that does not reproduce the internal B2B change rules.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-company-update-customer-since',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-company-update-customer-since-conformance.mts',
    purpose:
      'B2B companyUpdate customerSince create-only guard, including present timestamp, present alongside another update field, and present null inputs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-company-update-customer-since.json`,
      'config/parity-specs/b2b/company_update_rejects_customer_since.json',
      'config/parity-requests/b2b/b2b-company-update-customer-since-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable setup company with customerSince, records rejected update attempts and read-after-reject checks, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-external-id-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-external-id-validation-conformance.mts',
    purpose:
      'B2B company and company-location externalId charset, length, and uniqueness validation on create and update mutations.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-external-id-validation.json`,
      'config/parity-specs/b2b/external_id_charset.json',
      'config/parity-specs/b2b/external_id_too_long.json',
      'config/parity-specs/b2b/external_id_duplicate_company.json',
      'config/parity-specs/b2b/external_id_duplicate_location.json',
      'config/parity-requests/b2b/external-id-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable companies plus an extra location, records validation failures, then deletes the companies during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-input-normalization',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-input-normalization-conformance.mts',
    purpose:
      'B2B contact phone normalization, locale defaulting/format validation, and duplicate email/phone validation for create/update inputs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-contact-input-normalization.json`,
      'config/parity-specs/b2b/b2b-contact-input-normalization.json',
    ],
    cleanupBehavior: 'Creates one disposable company with B2B contacts, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The active Shopify Admin 2026-04 schema exposes CompanyContact phone/locale input validation but not note/notes fields, so note-to-notes behavior remains runtime-test-backed.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-billing-same-as-shipping-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-billing-same-as-shipping-conformance.mts',
    purpose:
      'B2B billingSameAsShipping/billingAddress mutual-exclusion and taxExempt null validation for company location create/update inputs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-billing-same-as-shipping-validation.json`,
      'config/parity-specs/b2b/b2b-billing-same-as-shipping-validation.json',
      'config/parity-requests/b2b/b2b-billing-same-as-shipping-company-create.graphql',
      'config/parity-requests/b2b/b2b-billing-same-as-shipping-location-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable setup company for direct companyLocationCreate/companyLocationUpdate validation, then deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-location-address-management',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-location-address-management-conformance.mts',
    purpose:
      'B2B location name fallback, duplicate address-type validation, shared billing/shipping address delete readback, and location-delete role-assignment cascade.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-location-address-management.json`,
      'config/parity-specs/b2b/b2b-location-address-management.json',
      'config/parity-requests/b2b/b2b-location-address-management-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable company with contact, locations, and addresses; deletes the company during scenario cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-business-rule-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-business-rule-preconditions-conformance.mts',
    purpose:
      'B2B contact role assignment one-role-per-location/resource lookup guards and contact delete order-history/main-contact preconditions.',
    requiredAuthScopes: ['read_companies', 'write_companies', 'write_draft_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-contact-business-rule-preconditions.json`,
      'config/parity-specs/b2b/b2b-contact-business-rule-preconditions.json',
      'config/parity-requests/b2b/b2b-contact-business-rules-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable companies and a B2B draft order completed into an order; cancels the order and attempts company deletes during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Shopify retains company order history after cancellation, so the order-history company delete may return a cleanup userError in the fixture.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-no-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-no-input-validation-conformance.mts',
    purpose:
      'B2B empty-object input validation for company/contact/location update roots and company contact create, plus readback proving the validation branches are no-ops.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-no-input-validation.json`,
      'config/parity-specs/b2b/b2b-no-input-validation.json',
      'config/parity-requests/b2b/b2b-no-input-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable company with a contact and location, records validation failures and unchanged readback, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The capture also records null-only probes showing the public schema does not treat null-only keys as a uniform NO_INPUT branch.',
  },
  {
    domain: 'products',
    captureId: 'products',
    scriptPath: 'scripts/capture-product-conformance.mts',
    purpose: 'Product read baselines, search grammar, selected product detail subresources.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-*.json`, 'product catalog/search parity specs when promoted'],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-invalid-search-query-syntax',
    scriptPath: 'scripts/capture-product-invalid-search-query-conformance.ts',
    purpose: 'Malformed product search query syntax behavior on a disposable product.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-invalid-search-query-syntax.json`,
      'config/parity-specs/products/product-invalid-search-query-syntax.json',
      'config/parity-requests/products/product-invalid-search-query-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, waits for tag search indexing, captures malformed search reads, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-mutations',
    scriptPath: 'scripts/capture-product-mutation-conformance.mts',
    purpose: 'productCreate/productUpdate/productDelete success and validation behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-mutation-*.json`, 'product mutation parity specs when promoted'],
    cleanupBehavior: 'Creates disposable products and deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-user-error-shapes',
    scriptPath: 'scripts/capture-product-user-error-shape-conformance.ts',
    purpose:
      'Product-domain userError field/message/code validation branches for blank titles, unknown product ids, and unknown inventory item ids.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-user-error-shape-parity.json`,
      'config/parity-specs/products/userError-shape-parity.json',
      'config/parity-requests/products/productUserErrorShape-*.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify objects should be created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-create-with-options',
    scriptPath: 'scripts/capture-product-create-with-options-conformance.mts',
    purpose:
      'productCreate invoked with `productOptions` input and productSet option-only validation, capturing option/variant graphs plus immediate downstream product reads.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-create-with-options-parity.json`,
      `${CAPTURE_ROOT}product-create-with-options-multi-value-parity.json`,
      `${CAPTURE_ROOT}product-set-options-only-requires-variants.json`,
      'config/parity-specs/products/productCreate-with-options-parity.json',
      'config/parity-specs/products/productCreate-with-options-multi-value-parity.json',
      'config/parity-specs/products/productSet-options-only-requires-variants.json',
    ],
    cleanupBehavior:
      'Creates disposable products for successful productCreate captures and deletes them in best-effort cleanup; the productSet validation branch must not create a product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-handle-dedup',
    scriptPath: 'scripts/capture-product-handle-dedup-conformance.mts',
    purpose:
      'Generated productCreate, productDuplicate, and collectionCreate handle de-duplication with incrementing numeric suffixes.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-handle-dedup-parity.json`,
      'config/parity-specs/products/productCreate-handle-dedup.json',
      'config/parity-requests/products/productCreate-handle-dedup.graphql',
      'config/parity-requests/products/productDuplicate-handle-dedup.graphql',
      'config/parity-requests/products/collectionCreate-handle-dedup.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products, one synchronous duplicate, and disposable collections, then deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-set-validator',
    scriptPath: 'scripts/capture-product-set-validator-conformance.ts',
    purpose:
      'productSet ProductSetShapeValidator guardrails, unknown-product validation, and asynchronous ProductSetOperation polling behavior.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-set-shape-validator-parity.json`,
      `${CAPTURE_ROOT}product-set-async-operation-parity.json`,
      'config/parity-specs/products/productSet-shape-validator-parity.json',
      'config/parity-specs/products/productSet-async-operation-parity.json',
    ],
    cleanupBehavior:
      'Validation branches create no products; async productSet creates one disposable product and deletes it in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-state-mutations',
    scriptPath: 'scripts/capture-product-state-mutation-conformance.mts',
    purpose: 'productChangeStatus/tagsAdd/tagsRemove mutation branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-state-mutation-*.json`],
    cleanupBehavior: 'Creates temporary products and resets/deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-publications',
    scriptPath: 'scripts/capture-product-publication-conformance.mts',
    purpose: 'Publication aggregate reads plus productPublish/productUnpublish probes.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-publication-*.json`, 'publication blocker notes when access is missing'],
    cleanupBehavior: 'Publishes/unpublishes disposable products only after publication target probes pass.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'products',
    captureId: 'product-media-mutations',
    scriptPath: 'scripts/capture-product-media-mutation-conformance.mts',
    purpose: 'Product media create/update/delete validation and downstream read branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-media-*.json`, 'config/parity-specs/products/product-media-*.json'],
    cleanupBehavior: 'Creates disposable product/media records and deletes the product during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'file-mutations',
    scriptPath: 'scripts/capture-file-mutation-conformance.mts',
    purpose: 'fileCreate/fileUpdate/fileDelete and staged upload interactions.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}file-mutation-*.json`,
      `${CAPTURE_ROOT}media-file-*.json`,
      `${LOCAL_RUNTIME_ROOT}files-upload-local-runtime.json`,
    ],
    cleanupBehavior:
      'Deletes created files when Shopify returns file IDs; local-runtime fixtures need no Shopify cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-update-validation-branches',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-update-validation-branches.ts',
    purpose: 'fileUpdate readiness, type, filename, source/version, and typed-GID validation branches.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [`${CAPTURE_ROOT}media-file-update-validation-branches.json`],
    cleanupBehavior: 'Creates disposable image/video files and deletes all returned file IDs during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'file-create-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-file-create-validation-conformance.mts',
    purpose:
      'fileCreate validation branches for source URLs, filename extensions, duplicate modes, and long alt input.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [`${CAPTURE_ROOT}media-file-create-validation-branches.json`],
    cleanupBehavior: 'Deletes any file successfully created by the acceptance branch.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'staged-upload-targets',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-staged-upload-target-conformance.ts',
    purpose: 'Representative stagedUploadsCreate target metadata for IMAGE, FILE, VIDEO, and MODEL_3D.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [`${CAPTURE_ROOT}staged-upload-targets-parity.json`],
    cleanupBehavior: 'Requests signed upload metadata only; does not upload bytes and creates no Shopify files.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'staged-upload-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-staged-upload-validation-conformance.ts',
    purpose: 'stagedUploadsCreate resource, fileSize, MIME validation, and representative success branches.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-staged-uploads-create-validation.json`,
      'config/parity-specs/media/media-staged-uploads-create-validation.json',
    ],
    cleanupBehavior: 'Requests signed upload metadata only; does not upload bytes and creates no Shopify files.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'file-acknowledge-update-failed',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-file-acknowledge-update-failed-conformance.ts',
    purpose: 'fileAcknowledgeUpdateFailed success and validation behavior.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}file-acknowledge-update-failed-parity.json`,
      `${LOCAL_RUNTIME_ROOT}file-acknowledge-update-failed-local-runtime.json`,
      'config/parity-specs/media/fileAcknowledgeUpdateFailed-local-staging.json',
      'config/parity-specs/media/media-file-acknowledge-update-failed-semantics.json',
    ],
    cleanupBehavior: 'Deletes disposable files created for READY acknowledgement and FAILED validation branches.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-option-mutations',
    scriptPath: 'scripts/capture-product-option-mutation-conformance.mts',
    purpose: 'productOptionsCreate/productOptionUpdate/productOptionsDelete mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-option-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products/options and deletes the products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-option-validation',
    scriptPath: 'scripts/capture-product-option-validation-conformance.mts',
    purpose:
      'productOptionsCreate option-limit, duplicate, required-value, and CREATE variant-limit validation branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-options-create-limits-and-duplicates-parity.json`],
    cleanupBehavior: 'Creates disposable products/options/variants and deletes the products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-option-variant-strategy-edges',
    scriptPath: 'scripts/capture-product-option-variant-strategy-edge-conformance.mts',
    purpose: 'product option variantStrategy and productVariantsBulkCreate.strategy edge behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-options-create-variant-strategy-*.json`,
      `${CAPTURE_ROOT}productVariantsBulkCreate-strategy-*.json`,
      'config/parity-specs/products/productOptionsCreate-variant-strategy-*.json',
      'config/parity-specs/products/productVariantsBulkCreate-strategy-*.json',
    ],
    cleanupBehavior: 'Creates disposable products/options/variants and deletes products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-mutations',
    scriptPath: 'scripts/capture-product-variant-mutation-conformance.mts',
    purpose: 'Product variant create/update/delete mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-variant-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products/variants and deletes the products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-validations',
    scriptPath: 'scripts/capture-product-variant-validation-conformance.mts',
    purpose: 'Bulk variant validation atomicity for create/update/delete.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variants-bulk-validation-atomicity.json`,
      'config/parity-specs/products/product-variants-bulk-validation-atomicity.json',
    ],
    cleanupBehavior: 'Creates disposable products and removes them after validation probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-scalar-validations',
    scriptPath: 'scripts/capture-product-variant-scalar-validation-conformance.ts',
    purpose:
      'productVariantsBulkCreate scalar validation for price, compareAtPrice, weight, inventory, SKU, barcode, option value length, and max input size.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}productVariantsBulkCreate-validation.json`,
      'config/parity-specs/products/productVariantsBulkCreate-validation.json',
      'config/parity-requests/products/productVariantsBulkCreate-validation*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, captures rejected validation branches, and deletes the product in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-derived-fields',
    scriptPath: 'scripts/capture-product-derived-fields-conformance.mts',
    purpose: 'Product derived aggregate fields after variant price creation and inventory quantity adjustments.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory', 'write_inventory', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-create-then-bulk-create-price-range-parity.json`,
      `${CAPTURE_ROOT}inventory-adjust-then-has-out-of-stock-variants-parity.json`,
      'config/parity-specs/products/productCreate-then-bulkCreate-priceRange-parity.json',
      'config/parity-specs/products/inventoryAdjust-then-hasOutOfStockVariants-parity.json',
      'config/parity-requests/products/productCreate-then-bulkCreate-derived-*.graphql',
      'config/parity-requests/products/inventoryAdjust-then-hasOutOfStockVariants-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products for price-range and inventory aggregate captures, then deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-item-mutations',
    scriptPath: 'scripts/capture-inventory-item-mutation-conformance.mts',
    purpose: 'inventoryItemUpdate and product-backed inventory item mutation behavior.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory', 'write_inventory'],
    fixtureOutputs: [`${CAPTURE_ROOT}inventory-item-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products to own inventory items and deletes those products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-linkage-mutations',
    scriptPath: 'scripts/capture-inventory-linkage-mutation-conformance.mts',
    purpose: 'inventoryActivate/inventoryDeactivate/inventoryBulkToggleActivation linkage behavior.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-linkage-parity.json`,
      `${CAPTURE_ROOT}inventory-inactive-level-lifecycle-2026-04.json`,
      'config/parity-specs/products/inventory-idempotency-directive-lifecycle-2026-04.json',
      'blocker notes when store topology is insufficient',
    ],
    cleanupBehavior: 'Creates disposable products; some success paths require a second safe location before capture.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'inventory',
    captureId: 'inventory-deactivate-validation',
    scriptPath: 'scripts/capture-inventory-deactivate-validation-conformance.mts',
    purpose:
      'inventoryDeactivate validation for 2026-04 non-zero committed/incoming/reserved quantities, missing inventory levels, only-location errors, and inventoryActivate available conflicts.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_inventory',
      'write_inventory',
      'read_locations',
      'write_orders',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-deactivate-validation-2026-04.json`,
      'config/parity-specs/products/inventoryDeactivate-non-zero-quantities-parity.json',
      'config/parity-specs/products/inventoryDeactivate-only-location-parity.json',
    ],
    cleanupBehavior: 'Creates disposable products and deletes them after recording validation branches.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'metafields',
    captureId: 'product-metafield-mutations',
    scriptPath: 'scripts/capture-product-metafield-mutation-conformance.mts',
    purpose: 'Product-scoped metafieldsSet/metafieldsDelete mutation behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-metafield-mutation-*.json`],
    cleanupBehavior: 'Creates disposable products/collections and removes them after metafield probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafields-set-input-validation',
    scriptPath: 'scripts/capture-metafields-set-input-validation-conformance.mts',
    purpose:
      'metafieldsSet namespace/key/type/value validation and reserved namespace userErrors on a disposable product owner.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafields-set-input-validation.json`,
      'config/parity-specs/metafields/metafields-set-input-validation.json',
      'config/parity-requests/metafields/metafields-set-input-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product owner, runs validation-only metafieldsSet probes, and deletes the product during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-pinning',
    scriptPath: 'scripts/capture-metafield-definition-pinning-conformance.mts',
    purpose: 'metafieldDefinitionPin/metafieldDefinitionUnpin behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}metafield-definition-pinning.json`],
    cleanupBehavior: 'Creates temporary product-owned definitions and deletes them after pinning probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-pin-limit-constraint-guard',
    scriptPath: 'scripts/capture-metafield-definition-pin-limit-constraint-guard.mts',
    purpose: 'metafieldDefinitionPin product-owner pin limit and constrained-definition validation.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-pin-limit-and-constraint-guard.json`,
      'config/parity-specs/metafields/metafield-definition-pin-limit-and-constraint-guard.json',
    ],
    cleanupBehavior:
      'Temporarily unpins existing product definitions, creates disposable product-owned definitions, deletes them, then restores original pins.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-update-delete-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-update-delete-preconditions-conformance.mts',
    purpose:
      'metafieldDefinitionDelete deleteAllAssociatedMetafields behavior and metafieldDefinitionUpdate identifier preconditions on product-owned definitions.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-update-delete-preconditions.json`,
      'config/parity-specs/metafields/metafield-definition-update-delete-preconditions.json',
      'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products and product-owned definitions, deletes definitions during the scenario, then deletes any remaining definitions and products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-graph-mutations',
    scriptPath: 'scripts/capture-product-graph-mutation-conformance.mts',
    purpose: 'Product graph mutation branches that span product/options/variants/media.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-graph-mutation-*.json`],
    cleanupBehavior: 'Uses disposable product graphs with best-effort product cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-duplicate-async',
    scriptPath: 'scripts/capture-product-duplicate-async-conformance.ts',
    purpose: 'Asynchronous productDuplicate operation success and missing-product completion behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-duplicate-async-success.json`,
      `${CAPTURE_ROOT}product-duplicate-async-missing.json`,
      'config/parity-specs/products/productDuplicate-async-*.json',
    ],
    cleanupBehavior: 'Creates disposable source/duplicate products and deletes both after operation completion.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'product-inventory-reads',
    scriptPath: 'scripts/capture-product-inventory-read-conformance.mts',
    purpose: 'Product-adjacent inventory read shapes and linkage baselines.',
    requiredAuthScopes: ['read_products', 'read_inventory', 'read_locations'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-inventory-*.json`],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-helper-reads',
    scriptPath: 'scripts/capture-product-helper-read-conformance.mts',
    purpose: 'Product helper roots and read-only compatibility wrappers.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-helper-roots-read.json`,
      'config/parity-specs/products/product-helper-roots-read.json',
    ],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-resource-roots',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-saved-search-resource-roots-conformance.ts',
    purpose:
      'SavedSearch create/read/delete behavior across resource-specific saved-search roots plus customer deprecation.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_orders',
      'write_orders',
      'read_draft_orders',
      'write_draft_orders',
      'read_files',
      'write_files',
      'discount redeem-code saved-search access',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-resource-roots.json`,
      'config/parity-specs/saved-searches/saved-search-resource-roots.json',
      'config/parity-requests/saved-searches/saved-search-resource-roots-*.graphql',
    ],
    cleanupBehavior: 'Creates disposable saved searches and deletes each successful create during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-query-grammar',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-query-grammar-conformance.ts',
    purpose: 'SavedSearch grouped/boolean query normalization, quoted field values, searchTerms, and negated filters.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-query-grammar.json`,
      'config/parity-specs/saved-searches/saved-search-query-grammar.json',
      'config/parity-requests/saved-searches/saved-search-query-grammar-*.graphql',
    ],
    cleanupBehavior: 'Creates one disposable product saved search and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-query-grammar-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-query-grammar-validation-conformance.ts',
    purpose: 'SavedSearch reserved-filter and per-resource filter-compatibility query validation for create/update.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-query-grammar-validation.json`,
      'config/parity-specs/saved-searches/saved-search-query-grammar-validation.json',
      'config/parity-requests/saved-searches/saved-search-query-grammar-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product saved search for positive/update validation and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-delete-shop-payload',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-delete-shop-payload-conformance.ts',
    purpose: 'savedSearchDelete success and missing-id payloads include non-null shop { id }.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-delete-shop-payload.json`,
      'config/parity-specs/saved-searches/saved-search-delete-shop-payload.json',
      'config/parity-requests/saved-searches/saved-search-delete-shop-payload-*.graphql',
    ],
    cleanupBehavior: 'Creates one disposable product saved search and deletes it during the scenario.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-name-uniqueness',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-name-uniqueness-conformance.ts',
    purpose: 'savedSearchCreate and savedSearchUpdate reject duplicate case-sensitive names per resource type.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-name-uniqueness.json`,
      'config/parity-specs/saved-searches/saved-search-name-uniqueness.json',
      'config/parity-requests/saved-searches/saved-search-name-uniqueness-*.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable product saved searches, captures duplicate create/update validation, then deletes both records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-required-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-required-input-validation-conformance.ts',
    purpose:
      'savedSearchCreate and savedSearchUpdate required input coercion plus explicit empty-query create success.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-required-input-validation.json`,
      'config/parity-specs/saved-searches/saved-search-required-input-validation.json',
      'config/parity-requests/saved-searches/saved-search-required-input-*.graphql',
    ],
    cleanupBehavior: 'Creates one disposable product saved search for the empty-query branch and deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-relationship-roots',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-relationship-roots-conformance.ts',
    purpose:
      'Product/variant relationship roots for option ordering, collection V2 membership, media attachment, and selling-plan membership.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-relationship-roots.json`,
      'config/parity-specs/products/product-relationship-roots-live-parity.json',
    ],
    cleanupBehavior:
      'Creates disposable products, collection, media, and selling-plan group, then deletes them during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'selling-plan-groups',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-conformance.ts',
    purpose: 'Selling-plan group lifecycle, membership mutation payloads, and downstream product/variant reads.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-lifecycle.json`,
      'config/parity-specs/products/selling-plan-group-lifecycle.json',
    ],
    cleanupBehavior: 'Creates a disposable product and selling-plan group, then deletes both during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-mutations',
    scriptPath: 'scripts/capture-metafield-definition-mutation-conformance.mts',
    purpose: 'Metafield definition mutation validation branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}standard-metafield-definition-enable-validation.json`],
    cleanupBehavior: 'Validation-oriented capture; success paths require explicit disposable setup/cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-lifecycle',
    scriptPath: 'scripts/capture-metafield-definition-lifecycle-conformance.mts',
    purpose: 'Product-owned metafieldDefinitionCreate/update/delete lifecycle.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-lifecycle-mutations.json`,
      'config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json',
    ],
    cleanupBehavior: 'Deletes created definitions and disposable product with captured cleanup steps.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-non-product-owner-types',
    scriptPath: 'scripts/capture-metafield-definition-non-product-owner-types-conformance.mts',
    purpose:
      'Non-product metafieldDefinitionCreate/update/delete lifecycle for CUSTOMER, ORDER, and COMPANY owner types.',
    requiredAuthScopes: [
      'read_customers',
      'write_customers',
      'read_orders',
      'write_orders',
      'read_companies',
      'write_companies',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-non-product-owner-types.json`,
      'config/parity-specs/metafields/metafield-definition-non-product-owner-types.json',
    ],
    cleanupBehavior:
      'Creates disposable metafield definitions for CUSTOMER, ORDER, and COMPANY owner types, deletes the CUSTOMER definition during the scenario, and deletes remaining definitions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-non-product-metafields',
    scriptPath: 'scripts/capture-metafield-definition-non-product-metafields-conformance.mts',
    purpose: 'Definition-backed metafieldsSet and owner-scoped reads for CUSTOMER, ORDER, and COMPANY owner types.',
    requiredAuthScopes: [
      'read_customers',
      'write_customers',
      'read_orders',
      'write_orders',
      'read_companies',
      'write_companies',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-non-product-metafields.json`,
      'config/parity-specs/metafields/metafield-definition-non-product-metafields.json',
    ],
    cleanupBehavior:
      'Creates disposable CUSTOMER, ORDER, and COMPANY owners plus matching metafield definitions; deletes definitions, customer, and company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'custom-data-field-types',
    scriptPath: 'scripts/capture-custom-data-field-type-conformance.ts',
    purpose: 'Metafield and metaobject custom-data field type value/jsonValue set-and-read matrix.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}custom-data-field-type-matrix.json`,
      'config/parity-specs/metafields/custom-data-metafield-type-matrix.json',
      'config/parity-specs/metaobjects/custom-data-metaobject-field-type-matrix.json',
    ],
    cleanupBehavior:
      'Creates a disposable product, collection, metaobject definitions, and metaobjects, then deletes all created resources during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobjects',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-read-conformance.mts',
    purpose: 'Metaobject definition/entry reads and minimal disposable seed behavior.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [`${CAPTURE_ROOT}metaobjects-read.json`, 'config/parity-specs/metaobjects/metaobjects-read.json'],
    cleanupBehavior: 'Deletes seeded metaobject entries/definitions after read capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-schema-change',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-schema-change-conformance.ts',
    purpose: 'Metaobject definition schema edits plus row add/update/delete behavior before and after the edit.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-schema-change-lifecycle.json`,
      'config/parity-specs/metaobjects/metaobject-schema-change-lifecycle.json',
    ],
    cleanupBehavior:
      'Deletes remaining seeded metaobject rows and definition after the schema-change lifecycle capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-create-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-create-validation-conformance.ts',
    purpose:
      'Metaobject definition type length/format validation, app namespace resolution, case-insensitive duplicates, and field key validation.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-definition-create-validation.json`,
      'config/parity-specs/metaobjects/metaobject-definition-create-validation.json',
      'config/parity-requests/metaobjects/metaobject-definition-create-validation-*.graphql',
    ],
    cleanupBehavior:
      'Validation branches create no records; successful app-prefixed and duplicate-case setup definitions are deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-delete-cascade',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-delete-cascade-conformance.ts',
    purpose:
      'Metaobject definition delete cascade with two associated entries plus immediate downstream definition, id, handle, and type-catalog reads.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-definition-delete-cascade.json`,
      'config/parity-specs/metaobjects/metaobject-definition-delete-cascade.json',
      'config/parity-requests/metaobjects/metaobject-definition-delete-cascade-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable definition and two rows, deletes the definition during the scenario, then best-effort deletes any remaining rows/definition during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'standard-metaobject-definition-enable-catalog',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-standard-metaobject-template-catalog-conformance.ts',
    purpose:
      'Standard metaobject definition template catalog, successful enablement, unknown-template RECORD_NOT_FOUND, idempotent duplicate enable, and read-after-enable behavior.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}standard-metaobject-templates.json`,
      `${CAPTURE_ROOT}standard-metaobject-definition-enable-catalog.json`,
      'src/shopify_draft_proxy/proxy/metaobject_standard_templates_data.gleam',
      'config/parity-specs/metaobjects/standard-metaobject-definition-enable-catalog.json',
      'config/parity-requests/metaobjects/standard-metaobject-definition-enable-*.graphql',
    ],
    cleanupBehavior:
      'Temporarily enables standard definitions on the disposable shop, captures their payloads, and deletes every created definition after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-field-validation-matrix',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-field-validation-matrix-conformance.ts',
    purpose:
      'MetaobjectCreate and metaobjectUpdate custom-data field value validation for scalar, measurement, reference, rating, URL/color/date/time, text max, and list field types.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-field-validation-matrix.json`,
      'config/parity-specs/metaobjects/metaobject-field-validation-matrix.json',
      'config/parity-requests/metaobjects/metaobject-field-validation-matrix-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and setup entry; rejected branches create no rows except captured scalar coercion branches, which are deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-upsert-recovery-and-prefixes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-upsert-recovery-and-prefixes-conformance.ts',
    purpose:
      'metaobjectUpsert create, exact-match no-op, conflicting handle prefix, missing required value prefix, and cold handle hydration.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-upsert-recovery-and-prefixes.json`,
      'config/parity-specs/metaobjects/metaobject-upsert-recovery-and-prefixes.json',
      'config/parity-requests/metaobjects/metaobject-upsert-recovery-and-prefixes.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and several disposable rows; deletes rows and definition during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-references',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-reference-conformance.ts',
    purpose: 'Metaobject reference field and reverse referencedBy read behavior.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-reference-lifecycle.json`,
      'config/parity-specs/metaobjects/metaobject-reference-lifecycle.json',
    ],
    cleanupBehavior: 'Deletes seeded parent/target metaobjects and definitions after reference capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-bulk-delete',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-bulk-delete-conformance.ts',
    purpose: 'Metaobject bulk delete by type plus downstream deleted-row and definition-count reads.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-bulk-delete-type-lifecycle.json`,
      'config/parity-specs/metaobjects/metaobject-bulk-delete-type-lifecycle.json',
    ],
    cleanupBehavior:
      'Creates a disposable definition and rows, bulk deletes rows by type, then deletes the definition.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-bulk-delete-edge-cases',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-bulk-delete-edge-cases-conformance.ts',
    purpose:
      'Metaobject bulk delete empty ids, unknown type, known empty type, and exactly-one-of GraphQL validation edge cases.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-bulk-delete-edge-cases.json`,
      'config/parity-specs/metaobjects/metaobject-bulk-delete-edge-cases.json',
      'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable definition and row, deletes the row before the known-empty-type branch, then deletes the definition in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-adjustments',
    scriptPath: 'scripts/capture-inventory-adjustment-conformance.mts',
    purpose: 'Inventory quantity adjustment/move/set mutation behavior.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-quantity-roots-parity.json`,
      'config/parity-specs/products/inventory-quantity-roots-parity.json',
    ],
    cleanupBehavior:
      'Uses disposable products/inventory levels where possible; review store topology before success captures.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'inventory',
    captureId: 'inventory-set-quantities-name-validation-2025',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-inventory-set-quantities-name-validation.ts',
    purpose:
      'inventorySetQuantities name, maximum quantity, and duplicate item/location validation against the 2025-01 contract.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventorySetQuantities-name-validation.json`,
      'config/parity-specs/products/inventorySetQuantities-name-validation.json',
    ],
    cleanupBehavior: 'Creates one disposable product per validation branch and deletes each product immediately.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-set-quantities-name-validation-2026',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-set-quantities-name-validation.ts',
    purpose:
      'inventorySetQuantities name, maximum quantity, and duplicate item/location validation against the 2026-04 @idempotent/changeFromQuantity contract.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventorySetQuantities-name-validation.json`,
      'config/parity-specs/products/inventorySetQuantities-name-validation-2026-04.json',
    ],
    cleanupBehavior: 'Creates one disposable product per validation branch and deletes each product immediately.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-quantity-contracts-2026',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-quantity-contracts-2026.ts',
    purpose: 'Admin GraphQL 2026-04 inventory quantity mutation request contracts.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-quantity-contracts-2026-04.json`,
      'config/parity-specs/products/inventory-quantity-contracts-2026-04.json',
      'config/parity-specs/products/inventory-quantity-idempotency-directive-2026-04.json',
    ],
    cleanupBehavior: 'Creates one disposable product, records set/adjust quantity contract branches, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-conformance.mts',
    purpose: 'Shop locale lifecycle and translation read-after-write cleanup behavior.',
    requiredAuthScopes: ['read_products', 'read_translations', 'write_translations', 'read_locales', 'write_locales'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-disable-clears-translations.json`,
      `${CAPTURE_ROOT}localization-shop-locale-primary-guards.json`,
      'config/parity-specs/localization/localization-disable-clears-translations.json',
      'config/parity-specs/localization/localization-shop-locale-primary-guards.json',
    ],
    cleanupBehavior:
      'Enables the French shop locale, registers one product-title translation, disables the locale, and leaves the locale/translation state cleaned up.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translation-error-codes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translation-error-codes-conformance.mts',
    purpose: 'translationsRegister/translationsRemove TranslationErrorCode validation branches.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translations-error-codes.json`,
      'config/parity-specs/localization/localization-translations-error-codes.json',
    ],
    cleanupBehavior:
      'Creates one disposable product, enables French only when needed, captures no-op/error translation mutation branches, deletes the product, and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-market-translations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-market-translations-conformance.mts',
    purpose: 'Market-scoped translationsRegister/translationsRemove product-title lifecycle.',
    requiredAuthScopes: [
      'read_markets',
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translations-market-scoped.json`,
      'config/parity-specs/localization/localization-translations-market-scoped.json',
    ],
    cleanupBehavior:
      'Creates one disposable product, enables Spanish only when needed, registers/removes one market-scoped title translation, deletes the product, and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-payload-shapes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-payload-shapes-conformance.mts',
    purpose:
      'ShopLocale marketWebPresences payload projection, primary-disable error locale nulling, and mixed translationsRegister partial-success payloads.',
    requiredAuthScopes: [
      'read_markets',
      'read_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-payload-shapes.json`,
      'config/parity-specs/localization/localization-payload-shapes.json',
      'config/parity-requests/localization/localization-payload-shapes-*.graphql',
    ],
    cleanupBehavior:
      'Enables French with an existing market web presence, removes the staged product-title translation, and disables or restores French locale settings according to the pre-capture shop state.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'markets',
    scriptPath: 'scripts/capture-market-conformance.mts',
    purpose: 'Markets read baselines and localization-adjacent validation probes.',
    requiredAuthScopes: ['read_markets', 'read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}markets-*.json`],
    cleanupBehavior:
      'Read/validation oriented; do not run market lifecycle writes without disposable setup and cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'markets',
    captureId: 'market-lifecycle-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-lifecycle-validation-conformance.mts',
    purpose:
      'Safe Markets lifecycle validation branches for blank marketCreate input and unknown marketUpdate/marketDelete IDs.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-lifecycle-validation.json`,
      'config/parity-specs/markets/market-create-blank-name-validation.json',
      'config/parity-specs/markets/market-update-not-found.json',
      'config/parity-specs/markets/market-delete-unknown-id-validation.json',
    ],
    cleanupBehavior:
      'Validation-only mutations reject before changing market state; no setup or cleanup records are created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'product-contextual-pricing',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-contextual-pricing-conformance.ts',
    purpose: 'Product and variant contextual pricing reads tied to Markets price-list fixed prices.',
    requiredAuthScopes: ['read_markets', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-contextual-pricing-price-list-parity.json`,
      'config/parity-specs/products/product-contextual-pricing-price-list-read.json',
    ],
    cleanupBehavior: 'Adds a disposable product fixed price to the Mexico price list, then deletes it after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-handle-dedupe',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-handle-dedupe-conformance.mts',
    purpose: 'marketCreate generated handle slug dedupe for distinct names that collide after Shopify slugification.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-handle-dedupe.json`,
      'config/parity-specs/markets/market-create-handle-dedupe.json',
      'config/parity-requests/markets/market-create-handle-dedupe.graphql',
    ],
    cleanupBehavior:
      'Creates disposable Europe and Europe! markets, records duplicate-name validation and generated handle dedupe, then deletes created markets in reverse creation order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-localization-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-localization-lifecycle-conformance.mts',
    purpose:
      'MarketLocalizableResource default product-metafield behavior plus marketLocalizationsRegister/remove validation.',
    requiredAuthScopes: [
      'read_markets',
      'write_markets',
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-localization-metafield-lifecycle-parity.json`,
      'config/parity-specs/markets/market-localization-metafield-lifecycle.json',
    ],
    cleanupBehavior:
      'Creates one disposable draft product with a product metafield, probes market localization behavior, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-web-presence-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-web-presence-lifecycle-conformance.mts',
    purpose:
      'Web presence create/update/delete lifecycle, downstream top-level webPresences reads, and multi-locale rootUrls.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-web-presence-lifecycle-parity.json`,
      'config/parity-specs/markets/web-presence-lifecycle-local-staging.json',
      'config/parity-specs/markets/web-presence-root-urls-multi-locale.json',
    ],
    cleanupBehavior:
      'Creates one disposable subfolder web presence, updates it, deletes it, records one multi-locale disposable web presence with subfolder suffix intl, deletes it, and verifies the baseline read after cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing',
    scriptPath: 'scripts/capture-marketing-conformance.mts',
    purpose: 'Marketing activity/event/engagement roots and mutation branches.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [`${CAPTURE_ROOT}marketing-*.json`],
    cleanupBehavior: 'Uses synthetic external IDs; cleanup depends on the branch captured.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-engagement',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-engagement-conformance.mts',
    purpose:
      'Marketing engagement activity-level success, 2026-04 conversion metrics, selector validation, delete branches, and recognized channel-handle probes.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [`${CAPTURE_ROOT}marketing-engagement-lifecycle.json`],
    cleanupBehavior:
      'Creates a disposable external marketing activity, writes activity-level engagement metrics, probes candidate channel handles with immediate channel cleanup if any succeeds, and deletes the disposable activity.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Recognized channel-handle success depends on the disposable shop exposing a valid marketing channel handle.',
  },
  {
    domain: 'marketing',
    captureId: 'marketing-activity-immutable-fields',
    scriptPath: 'scripts/capture-marketing-activity-immutable-fields-conformance.mts',
    purpose:
      'External marketing activity upsert/update immutable channel handle, URL parameter, UTM, parent remote ID, and hierarchy-level userErrors.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-upsert-immutable-fields.json`,
      'config/parity-specs/marketing/marketing-activity-upsert-immutable-fields.json',
      'config/parity-requests/marketing/marketing-activity-immutable-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable parent and child external marketing activities, captures rejected immutable-field updates, then deletes every disposable remote ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-activity-delete-external-guards',
    scriptPath: 'scripts/capture-marketing-activity-delete-external-guards-conformance.mts',
    purpose:
      'External marketing activity delete selector validation and delete-all in-flight write rejection userErrors.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-delete-external-guards.json`,
      'config/parity-specs/marketing/marketing-activity-delete-external-guards.json',
      'config/parity-requests/marketing/marketing-activity-delete-external-guards.graphql',
    ],
    cleanupBehavior:
      'Waits for any prior delete-all job to stop blocking writes, captures a delete-all job and blocked follow-up create, records parent/native setup blockers, and deletes the disposable follow-up remote ID if it unexpectedly exists.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-engagement-currency-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-engagement-currency-validation-conformance.mts',
    purpose: 'Marketing engagement adSpend/sales currency mismatch and engagement-vs-activity currency validation.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [`${CAPTURE_ROOT}marketing-engagement-currency-validation.json`],
    cleanupBehavior:
      'Creates one disposable USD-budget external marketing activity, captures currency validation branches, then deletes the activity.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segments',
    scriptPath: 'scripts/capture-segment-conformance.mts',
    purpose: 'Segment baseline read payloads for the checked-in segment parity request.',
    requiredAuthScopes: ['read_customers', 'customer segment access'],
    fixtureOutputs: [`${CAPTURE_ROOT}segments-baseline.json`],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segment-query-grammar',
    scriptPath: 'scripts/capture-segment-query-grammar-conformance.ts',
    purpose: 'Segment query grammar support for `NOT CONTAINS` customer-tag predicates.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [`${CAPTURE_ROOT}segment-query-grammar-not-contains.json`],
    cleanupBehavior:
      'Creates one disposable segment, deletes it during cleanup, and leaves only Shopify async query state.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segment-validation-limits',
    scriptPath: 'scripts/capture-segment-validation-limits-conformance.ts',
    purpose: 'segmentCreate/segmentUpdate name and query length validation plus local segment-limit replay setup.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segments-create-update-validation-limits.json`,
      'config/parity-specs/segments/segments-create-update-validation-limits.json',
      'config/parity-requests/segments/segment-*-validation-limits.graphql',
      'config/parity-requests/segments/segment-create-limit-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for update validation, deletes it during cleanup, and avoids thousands of live segment-limit setup writes by using local parity-runner setup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segments-user-errors-shape',
    scriptPath: 'scripts/capture-segments-user-errors-shape-conformance.ts',
    purpose:
      'segmentCreate/segmentUpdate/segmentDelete default UserError shape plus customerSegmentMembersQueryCreate typed userError code and field shape.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segments-user-errors-shape.json`,
      'config/parity-specs/segments/segments-user-errors-shape.json',
      'config/parity-requests/segments/segments-user-errors-shape-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for the segmentUpdate id-only validation branch and deletes it during cleanup; all other captured branches are validation-only.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'customer-segment-members-query-create-validation-and-shape',
    scriptPath: 'scripts/capture-customer-segment-members-query-create-conformance.ts',
    purpose:
      'customerSegmentMembersQueryCreate selector validation, INITIALIZED response shape, segmentId success branch, and immediate customerSegmentMembersQuery lookup consistency.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-segment-members-query-create-validation-and-shape.json`,
      'config/parity-specs/segments/customer-segment-members-query-create-validation-and-shape.json',
      'config/parity-requests/segments/customer-segment-members-query-*-validation-and-shape.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for the segmentId-backed branch and deletes it during cleanup; member-query jobs are async Shopify state without a cleanup mutation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-content-search',
    scriptPath: 'scripts/capture-online-store-content-search-conformance.ts',
    purpose: 'Online store article, blog, and page search filter behavior.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [`${CAPTURE_ROOT}online-store-content-search-filters.json`],
    cleanupBehavior: 'Creates disposable article, blog, and page records, then deletes them during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-article-create-validation',
    scriptPath: 'scripts/capture-online-store-article-create-validation-conformance.ts',
    purpose:
      'articleCreate blog-reference and author validation branches plus valid blogId and author.name success behavior.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-article-create-validation.json`,
      'config/parity-specs/online-store/online-store-article-create-validation.json',
      'config/parity-requests/online-store/online-store-article-create-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog for blogId-backed branches, deletes the success-path article, then deletes the blog.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-body-script-verbatim-2025-01',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-online-store-body-script-verbatim-conformance.ts',
    purpose:
      'pageCreate/articleCreate body HTML script and event-attribute persistence, including immediate downstream detail reads.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-body-script-verbatim.json`,
      'config/parity-specs/online-store/online-store-body-script-verbatim-2025-01.json',
      'config/parity-requests/online-store/online-store-body-script-*.graphql',
    ],
    cleanupBehavior: 'Creates one disposable blog, page, and article, then deletes all created records during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-body-script-verbatim-2026-04',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-online-store-body-script-verbatim-conformance.ts',
    purpose:
      'pageCreate/articleCreate body HTML script and event-attribute persistence, including immediate downstream detail reads.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-body-script-verbatim.json`,
      'config/parity-specs/online-store/online-store-body-script-verbatim-2026-04.json',
      'config/parity-requests/online-store/online-store-body-script-*.graphql',
    ],
    cleanupBehavior: 'Creates one disposable blog, page, and article, then deletes all created records during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-content-required-fields',
    scriptPath: 'scripts/capture-online-store-content-required-fields-conformance.ts',
    purpose:
      'pageCreate, articleCreate, and blogCreate title-required validation branches for missing and blank title inputs.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-content-required-fields.json`,
      'config/parity-specs/online-store/online-store-content-required-fields.json',
      'config/parity-requests/online-store/online-store-content-required-fields-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog for articleCreate blogId-backed validation, then deletes it during cleanup. Blank-title page/blog/article attempts do not create records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-page-handle-dedupe-and-takenness',
    scriptPath: 'scripts/capture-online-store-page-handle-conformance.ts',
    purpose:
      'pageCreate handle normalization, auto-dedupe for derived handle collisions, and explicit TAKEN userErrors.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-page-handle-dedupe-and-takenness.json`,
      'config/parity-specs/online-store/online-store-page-handle-dedupe-and-takenness.json',
      'config/parity-requests/online-store/online-store-page-handle-dedupe-and-takenness.graphql',
    ],
    cleanupBehavior: 'Creates disposable pages and deletes every successful pageCreate result during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collections',
    scriptPath: 'scripts/capture-collection-conformance.mts',
    purpose: 'Collection read baselines for custom/smart collections and product membership.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-*.json`],
    cleanupBehavior: 'Read-only capture against existing store collections; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-mutations',
    scriptPath: 'scripts/capture-collection-mutation-conformance.mts',
    purpose: 'collectionCreate/update/delete/addProducts/removeProducts mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-mutation-*.json`],
    cleanupBehavior: 'Creates disposable collections/products and deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-create-and-add-products-parity',
    scriptPath: 'scripts/capture-collection-create-and-add-products-parity.ts',
    purpose:
      'collectionCreate validation, sortOrder enum coercion, smart collection add/remove guards, and custom collection productsCount read-after-add behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-create-and-add-products-parity.json`,
      'config/parity-specs/products/collectionCreate-and-add-products-parity.json',
      'config/parity-requests/products/collectionCreate-and-add-products-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable reserved-like, smart, and custom collections and deletes every successful collectionCreate result during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-publications',
    scriptPath: 'scripts/capture-collection-mutation-conformance.mts',
    purpose: 'Collection publication behavior covered by the collection mutation harness when enabled.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-mutation-*.json`],
    cleanupBehavior: 'Shares disposable collection cleanup with the collection mutation harness.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    captureId: 'locations',
    scriptPath: 'scripts/capture-location-conformance.mts',
    purpose: 'Location roots and inventory/publication-adjacent store property reads.',
    requiredAuthScopes: ['read_locations'],
    fixtureOutputs: [`${CAPTURE_ROOT}locations-*.json`],
    cleanupBehavior: 'Read-only by default; location lifecycle writes need disposable location setup and cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    captureId: 'store-properties',
    scriptPath: 'scripts/capture-location-conformance.mts',
    purpose: 'Store property roots sharing the location capture harness.',
    requiredAuthScopes: ['read_locations', 'read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}store-properties-*.json`, `${CAPTURE_ROOT}locations-*.json`],
    cleanupBehavior: 'Read-only by default; avoid merchant-topology writes without explicit cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-lifecycle',
    scriptPath: 'scripts/capture-location-lifecycle-conformance.mts',
    purpose: 'locationActivate/locationDeactivate idempotency and read-after-write lifecycle behavior.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-activate-deactivate-with-idempotency-directive.json`,
      'config/parity-specs/store-properties/location-activate-deactivate-with-idempotency-directive.json',
    ],
    cleanupBehavior:
      'Creates one disposable non-online-fulfilling location, deactivates/reactivates it, then deactivates and deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-add',
    scriptPath: 'scripts/capture-location-add-conformance.mts',
    purpose:
      'locationAdd required-address validation, address/default staging, and immediate read-after-write behavior.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-add-required-address-and-defaults.json`,
      'config/parity-specs/store-properties/location-add-required-address-and-defaults.json',
    ],
    cleanupBehavior:
      'Creates disposable locations for default and explicit non-online fulfillment branches, then deactivates and deletes them.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-edit-fields-and-state-machine',
    scriptPath: 'scripts/capture-location-edit-fields-and-state-machine-conformance.mts',
    purpose:
      'locationEdit editable fields, typed userErrors, location-owned metafields, read-after-write, and fulfillsOnlineOrders state-machine branches.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-edit-fields-and-state-machine.json`,
      'config/parity-specs/store-properties/location-edit-fields-and-state-machine.json',
      'config/parity-requests/store-properties/location-edit-fields-and-state-machine*.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable locations, temporarily disables/restores pre-existing online-fulfilling locations for the only-online rejection branch, then deactivates/deletes the disposable locations.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-activate-deactivate-lifecycle',
    scriptPath: 'scripts/capture-location-activate-deactivate-lifecycle-conformance.mts',
    purpose:
      'locationActivate/locationDeactivate version-gated idempotency directive behavior across 2025-10 and 2026-04.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-activate-deactivate-lifecycle.json`,
      'config/parity-specs/store-properties/location-activate-deactivate-lifecycle.json',
    ],
    cleanupBehavior:
      'Creates one disposable non-online-fulfilling location, toggles it across optional/required directive API versions, then deactivates and deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-delete-state-and-scope',
    scriptPath: 'scripts/capture-location-delete-state-and-scope-conformance.mts',
    purpose:
      'locationDelete guard parity for active, inventory, primary, and fulfillment-service-managed Location state.',
    requiredAuthScopes: ['read_locations', 'write_locations', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-delete-state-and-scope.json`,
      'config/parity-specs/store-properties/location-delete-state-and-scope.json',
    ],
    cleanupBehavior:
      'Creates disposable merchant-managed locations, temporary products/inventory levels, and a fulfillment service, then cleans them up after guard capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'shop-policies',
    scriptPath: 'scripts/capture-shop-policy-conformance.ts',
    purpose: 'shopPolicyUpdate and legal-policy read/write behavior.',
    requiredAuthScopes: ['read_content', 'write_content or policy-management access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}shop-policy-*.json`,
      'config/parity-specs/store-properties/shop-policy-update-title-url-and-body-rendering.json',
    ],
    cleanupBehavior:
      'Restores prior policy content when a write branch is captured. Newly created policy rows may remain on shops where Shopify does not expose deletion, but their bodies are reset to the prior empty fallback.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'privacy',
    captureId: 'privacy',
    scriptPath: 'scripts/capture-privacy-conformance.ts',
    purpose: 'Privacy/data-sale read and mutation roots.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'privacy API access'],
    fixtureOutputs: [`${CAPTURE_ROOT}privacy-*.json`],
    cleanupBehavior: 'Uses disposable customer records where writes are captured.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'privacy',
    captureId: 'data-sale-opt-out',
    scriptPath: 'scripts/capture-data-sale-opt-out-conformance.ts',
    purpose: 'dataSaleOptOut behavior and downstream customer privacy read effects.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'privacy API access'],
    fixtureOutputs: [`${CAPTURE_ROOT}data-sale-opt-out-*.json`],
    cleanupBehavior: 'Creates/deletes disposable customer records for opt-out probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'root-operations',
    scriptPath: 'scripts/capture-admin-graphql-root-operation-introspection.mts',
    purpose: 'Admin GraphQL root operation introspection for coverage-map updates.',
    requiredAuthScopes: ['schema introspection access through the active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}root-operation-introspection.json`,
      'src/shopify_draft_proxy/proxy/operation_registry_data.gleam updates when intentionally edited',
    ],
    cleanupBehavior: 'Read-only introspection; no cleanup expected.',
    expectedStatusChecks: ['conformance:check', 'conformance:status'],
  },
  {
    domain: 'admin-platform',
    captureId: 'mutation-input-schema',
    scriptPath: 'scripts/capture-mutation-input-schema.mts',
    purpose:
      'Per-mutation argument and input-object field shapes (deprecated included) used by the central required-field validator.',
    requiredAuthScopes: ['schema introspection access through the active Admin token'],
    fixtureOutputs: ['config/admin-graphql-mutation-schema.json'],
    cleanupBehavior: 'Read-only introspection; no cleanup expected.',
    expectedStatusChecks: ['conformance:check', 'conformance:status'],
  },
  {
    domain: 'orders',
    captureId: 'orders',
    scriptPath: 'scripts/capture-order-conformance.mts',
    purpose: 'Order reads, orderCreate, order-edit, transaction, and downstream order-family behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}order-*.json`, 'order blocker notes when credential/store access is insufficient'],
    cleanupBehavior: 'Creates/cancels disposable orders only after credential and store-state probes pass.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'orders',
    captureId: 'order-lifecycle-noop',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-lifecycle-noop-conformance.mts',
    purpose:
      'Redundant orderClose on an already-closed order and orderOpen on a never-closed order preserve closedAt/updatedAt while returning silent-success payloads.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderClose-noop-on-already-closed.json`,
      `${CAPTURE_ROOT}orderOpen-noop-on-already-open.json`,
      'config/parity-specs/orders/orderClose-noop-on-already-closed.json',
      'config/parity-specs/orders/orderOpen-noop-on-already-open.json',
      'config/parity-requests/orders/orderClose-noop-on-already-closed.graphql',
      'config/parity-requests/orders/orderOpen-noop-on-already-open.graphql',
    ],
    cleanupBehavior:
      'Creates disposable test orders, reopens the closed-order probe after capture, and cancels both orders in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-lifecycle-user-errors',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-lifecycle-user-errors-conformance.mts',
    purpose:
      'orderEditBegin/AddVariant/SetQuantity/Commit missing-resource userError payload roots for lifecycle validation.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-edit-lifecycle-user-errors.json`,
      'config/parity-specs/orders/orderEdit-lifecycle-userErrors.json',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-*.graphql',
    ],
    cleanupBehavior: 'Validation-only order-edit probes use missing Shopify GIDs and do not create merchant resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-detail-events',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-detail-events-conformance.ts',
    purpose: 'Fulfillment detail event capture on disposable orders.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-detail-events-lifecycle.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-detail-events-lifecycle.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-detail-events-read.graphql',
    ],
    cleanupBehavior: 'Cancels/deletes disposable order state where Shopify permits cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-event-create-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-event-create-validation-conformance.ts',
    purpose:
      'fulfillmentEventCreate unknown-id validation, public GraphQL code/enum validation branches, cancelled-fulfillment probe, and valid event read-after-write behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-event-create-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-event-create-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-event-create-validation.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-event-create-detail-read.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable test order and fulfillment, probes public GraphQL validation/event behavior, cancels the fulfillment, records the public cancelled-event probe, and cancels the order in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-family',
    scriptPath: 'scripts/capture-draft-order-family-conformance.mts',
    purpose: 'Draft order create/update/delete/complete and downstream read behavior.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}draft-order-*.json`],
    cleanupBehavior: 'Creates disposable draft orders and deletes/completes/cancels them per branch.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-residual-helpers',
    scriptPath: 'scripts/capture-draft-order-residual-helper-conformance.mts',
    purpose: 'Residual draft-order helper roots such as calculate, bulk tags, invoices, and delivery options.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-residual-helper-roots.json`,
      'config/parity-specs/orders/draft-order-residual-helper-roots.json',
    ],
    cleanupBehavior: 'Creates disposable draft orders/products and removes them after helper probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-calculate-validation-and-shipping-rates',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-calculate-validation-and-shipping-rates-conformance.ts',
    purpose:
      'draftOrderCalculate validation branches and captured empty availableShippingRates behavior when no shipping address is present.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draftOrderCalculate-validation-and-shipping-rates.json`,
      'config/parity-specs/orders/draftOrderCalculate-validation-and-shipping-rates.json',
      'config/parity-requests/orders/draftOrderCalculate-validation-and-shipping-rates.graphql',
    ],
    cleanupBehavior: 'Validation/calculate-only probes do not create merchant resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-invoice-send-safety',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-invoice-send-safety-conformance.ts',
    purpose: 'Safety probes for draftOrderInvoiceSend side effects and validation branches.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [`${CAPTURE_ROOT}draft-order-invoice-send-safety.json`],
    cleanupBehavior: 'Uses safety-first validation branches; review manually before any customer-visible send path.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'discounts',
    captureId: 'discounts',
    scriptPath: 'scripts/capture-discount-conformance.ts',
    purpose: 'Discount read roots and baseline validation branches.',
    requiredAuthScopes: ['read_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-*.json`],
    cleanupBehavior: 'Read/validation oriented; lifecycle scripts own write cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-code-basic-lifecycle-conformance.ts',
    purpose: 'Code discount basic create/update/delete lifecycle.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-code-basic-lifecycle.json`],
    cleanupBehavior: 'Deletes created code discount during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-buyer-context',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-buyer-context-conformance.ts',
    purpose: 'Code and automatic basic discount customer/segment buyer context lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-buyer-context-lifecycle.json`],
    cleanupBehavior: 'Deletes created discounts, customer, and segment during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-bxgy-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-bxgy-lifecycle-conformance.ts',
    purpose: 'Buy-X-get-Y code and automatic discount lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-bxgy-lifecycle.json`],
    cleanupBehavior: 'Deletes created discounts/products/collections in reverse-order cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-bxgy-disallowed-value-shapes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-bxgy-disallowed-value-shapes-conformance.ts',
    purpose: 'Buy-X-get-Y customerGets value and subscription flag validation guardrails.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-bxgy-disallowed-value-shapes.json`],
    cleanupBehavior: 'Deletes temporary products after capturing rejected discount mutations.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-class-inference',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-class-inference-conformance.ts',
    purpose:
      'Discount class inference for basic all/product/collection entitlements, BXGY product class, free-shipping shipping class, and product-class catalog filtering.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-class-inference.json`,
      'config/parity-specs/discounts/discount-class-inference.json',
      'config/parity-requests/discounts/discount-class-inference-*.graphql',
    ],
    cleanupBehavior: 'Creates disposable products, collection, and discounts; deletes them in reverse-order cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-free-shipping-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-free-shipping-lifecycle-conformance.ts',
    purpose: 'Free-shipping discount lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-free-shipping-lifecycle.json`],
    cleanupBehavior: 'Deletes created free-shipping discounts during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-redeem-code-bulk',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-redeem-code-bulk-conformance.ts',
    purpose: 'Redeem-code bulk add/delete behavior and case-insensitive code lookup.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-redeem-code-bulk.json`,
      'config/parity-specs/discounts/discount-redeem-code-bulk.json',
    ],
    cleanupBehavior: 'Creates a disposable code discount and deletes it after redeem-code bulk probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-redeem-code-bulk-add-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-redeem-code-bulk-validation-conformance.ts',
    purpose:
      'discountRedeemCodeBulkAdd unknown discount, empty/oversized code list, and per-code async validation behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-redeem-code-bulk-add-validation.json`,
      'config/parity-specs/discounts/discount-redeem-code-bulk-add-validation.json',
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-*.graphql',
    ],
    cleanupBehavior: 'Creates a disposable code discount and deletes it after validation probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-update-edge-cases',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-update-edge-cases-conformance.ts',
    purpose:
      'discountCodeBasicUpdate update-only guardrails for redeem-code bulk rules, BXGY-to-basic coercion, and unknown-id errors.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-update-edge-cases.json`,
      'config/parity-specs/discounts/discount-update-edge-cases.json',
      'config/parity-requests/discounts/discount-update-edge-cases-*.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable products, one disposable code-basic discount, and one disposable code-BXGY discount; deletes discounts and products during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public 2026-04 conformance store still returns null error codes for the update-only rejection branches; HAR-605 intentionally models INVALID from the referenced Shopify source path.',
  },
  {
    domain: 'discounts',
    captureId: 'discount-timestamps-monotonic',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-timestamps-monotonic-conformance.ts',
    purpose: 'Code discount basic createdAt/updatedAt monotonic lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-timestamps-monotonic.json`,
      'config/parity-specs/discounts/discount-timestamps-monotonic.json',
    ],
    cleanupBehavior: 'Creates two disposable code discounts and deletes both after timestamp probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-status-time-window-derivation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-status-time-window-derivation-conformance.ts',
    purpose:
      'DiscountCodeBasic status derivation from startsAt/endsAt for create payloads, downstream reads, and status filters.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-status-time-window-derivation.json`,
      'config/parity-specs/discounts/discount-status-time-window-derivation.json',
      'config/parity-requests/discounts/discount-status-time-window-derivation-*.graphql',
    ],
    cleanupBehavior:
      'Creates three disposable code discounts with scheduled, expired, and active windows, then deletes them after read/filter capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-validation-conformance.ts',
    purpose: 'Discount validation guardrails without broad lifecycle side effects.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-validation-branches.json`,
      `${CAPTURE_ROOT}discount-code-required-blank-validation.json`,
    ],
    cleanupBehavior: 'Validation-oriented; deletes any created disposable discount artifacts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-app-function-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-app-function-validation-conformance.ts',
    purpose: 'App-discount functionId/functionHandle missing, multiple, unknown, and wrong-API validation guardrails.',
    requiredAuthScopes: [
      'read_discounts',
      'write_discounts',
      'shopifyFunctions read access',
      'released non-discount Shopify Function in the installed conformance app for wrong-API validation',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-app-function-validation.json`,
      'config/parity-specs/discounts/discount-app-function-validation.json',
      'config/parity-requests/discounts/discount-app-function-validation.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-context-customer-selection-conflict',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-context-customer-selection-conflict-conformance.ts',
    purpose:
      'Discount context and deprecated customerSelection mutual-exclusion userErrors across create roots that accept both fields.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-context-customer-selection-conflict.json`,
      'config/parity-specs/discounts/discount-context-customer-selection-conflict.json',
      'config/parity-requests/discounts/discount-context-customer-selection-conflict.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable customers for realistic conflict IDs and deletes them after validation capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-bulk-selector-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-bulk-selector-validation-conformance.ts',
    purpose:
      'Discount bulk selector missing, blank search, mutually exclusive selector, and saved-search validation guardrails.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-bulk-selector-validation.json`,
      'config/parity-specs/discounts/discount-bulk-selector-validation.json',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-customer-gets-value-multiple-types',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-customer-gets-value-multiple-types-conformance.ts',
    purpose: 'Discount customerGets.value multiple-branch BadRequest parity for basic create/update inputs.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-customer-gets-value-multiple-types.json`,
      'config/parity-specs/discounts/discount-customer-gets-value-multiple-types.json',
      'config/parity-requests/discounts/discount-customer-gets-value-multiple-types-*.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-minimum-requirement-exclusivity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-minimum-requirement-exclusivity-conformance.ts',
    purpose:
      'Discount minimumRequirement mutually exclusive quantity/subtotal branches and quantity/subtotal upper-bound validation guardrails.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-minimum-requirement-exclusivity.json`,
      'config/parity-specs/discounts/discount-minimum-requirement-exclusivity.json',
      'config/parity-requests/discounts/discount-minimum-requirement-exclusivity.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-combines-with-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-combines-with-validation-conformance.ts',
    purpose:
      'Discount combinesWith cart-line tag validation guardrails and free-shipping self-combine regression coverage.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [`${CAPTURE_ROOT}discount-combines-with-validation.json`],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-invalid-date-range-all-types',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-invalid-date-range-all-types-conformance.ts',
    purpose:
      'Discount startsAt/endsAt invalid date range validation guardrails across basic, BXGY, and free-shipping create inputs.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-invalid-date-range-all-types.json`,
      'config/parity-specs/discounts/discount-invalid-date-range-all-types.json',
      'config/parity-requests/discounts/discount-invalid-date-range-*.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Live Shopify 2026-04 returns `Ends at needs to be after starts_at`, which differs from the HAR-595 issue text but is preserved for captured Admin API parity.',
  },
  {
    domain: 'apps',
    captureId: 'app-billing',
    scriptPath: 'scripts/capture-app-billing-conformance.ts',
    purpose: 'App billing/access read roots and blocker evidence.',
    requiredAuthScopes: ['app billing access for the installed app'],
    fixtureOutputs: [`${CAPTURE_ROOT}app-billing-access-read.json`],
    cleanupBehavior: 'Read-only capture; no billing mutation cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'apps',
    captureId: 'delegate-access-token-shop-payload',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-delegate-access-token-shop-payload-conformance.ts',
    purpose: 'Delegate access token create/destroy payload shop nullability on success and userError branches.',
    requiredAuthScopes: ['delegate access token create/destroy for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}delegate-access-token-shop-payload.json`,
      'config/parity-specs/apps/delegate-access-token-shop-payload.json',
      'config/parity-requests/apps/delegateAccessTokenCreate-shop-payload.graphql',
      'config/parity-requests/apps/delegateAccessTokenDestroy-shop-payload.graphql',
      'config/parity-requests/apps/delegateAccessTokenDestroy-shop-payload-unknown.graphql',
    ],
    cleanupBehavior:
      'Creates one short-lived delegate access token and destroys it during the scenario; unknown-token validation has no cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'function-ownership',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-function-ownership-conformance.ts',
    purpose:
      'Live ShopifyFunction ownership metadata for released validation/cart-transform functions plus authority blockers for Function-backed mutation probes.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for mutation userError branches',
      'read_cart_transforms',
      'write_cart_transforms for mutation userError branches',
      'write_taxes plus tax calculations app status for taxAppConfigure',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-live-owner-metadata-read.json`,
      'config/parity-specs/functions/functions-live-owner-metadata-read.json',
    ],
    cleanupBehavior:
      'Creates validation/cart-transform probe resources only after validation branches are captured, then deletes HAR-416 validations and cart transforms for the captured Function; no Shopify Function execution or tax callbacks are invoked.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-delete-error-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-delete-error-shape-conformance.ts',
    purpose:
      'validationDelete/cartTransformDelete missing-id userError shape plus cassette-backed cartTransformCreate/delete canonical deletedId lifecycle.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for missing validationDelete userError capture',
      'read_cart_transforms',
      'write_cart_transforms for missing cartTransformDelete userError capture',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-delete-error-shape.json`,
      'config/parity-specs/functions/functions-delete-error-shape.json',
    ],
    cleanupBehavior:
      'Captures missing-delete userErrors only; no live resources are created. The local lifecycle leg is cassette-backed because the current unattended shop lacks released cart-transform/validation Function handles.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-validation-update-defaults',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-validation-update-defaults-conformance.ts',
    purpose:
      'validationUpdate omitted enable/blockOnFailure default resets, downstream readback, and unknown-id userError shape.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for disposable validationCreate/update/delete lifecycle capture',
      'released conformance-validation Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-validation-update-defaults.json`,
      'config/parity-specs/functions/functions-validation-update-defaults.json',
    ],
    cleanupBehavior:
      'Creates one disposable validation through conformance-validation, updates it, reads it back, captures the missing-id branch, then deletes the disposable validation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'finance-risk',
    scriptPath: 'scripts/capture-finance-risk-conformance.ts',
    purpose: 'Finance, risk, POS, dispute, and Shop Pay receipt read/access evidence.',
    requiredAuthScopes: ['Shopify Payments, finance, risk, and POS root access for the active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}finance-risk-access-read.json`,
      'config/parity-specs/payments/finance-risk-no-data-read.json',
    ],
    cleanupBehavior: 'Read/access capture only; do not create or invent sensitive financial records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-lifecycle-conformance.ts',
    purpose: 'paymentTermsCreate/paymentTermsUpdate/paymentTermsDelete lifecycle against a disposable draft order.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [`${CAPTURE_ROOT}payment-terms-lifecycle.json`],
    cleanupBehavior:
      'Creates a disposable draft order, deletes payment terms during the scenario, then deletes the draft order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-create-template-and-schedule-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-validation-conformance.ts',
    purpose:
      'paymentTermsCreate template lookup, unknown template, and template-specific schedule validation branches.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-create-template-and-schedule-validation.json`,
      'config/parity-specs/payments/payment-terms-create-template-and-schedule-validation.json',
    ],
    cleanupBehavior:
      'Creates a disposable draft order for each validation case, deletes payment terms for success cases, then deletes every draft order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-customization-metafields-and-handle-update',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-customization-metafields-conformance.ts',
    purpose:
      'paymentCustomizationCreate/paymentCustomizationUpdate metafield persistence, functionHandle input update, and downstream paymentCustomization readback.',
    requiredAuthScopes: ['read_payment_customizations', 'write_payment_customizations', 'shopifyFunctions read access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-customization-metafields-and-handle-update.json`,
      'config/parity-specs/payments/payment-customization-metafields-and-handle-update.json',
      'config/parity-requests/payments/payment-customization-metafields-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable payment customization, captures create/update/read behavior, then deletes the payment customization.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The active 2026-04 PaymentCustomization output type does not expose functionHandle, so parity compares Shopify’s resolved functionId and runtime tests cover local functionHandle projection.',
  },
  {
    domain: 'payments',
    captureId: 'payment-customization-update-immutable-function',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-customization-immutable-function-conformance.ts',
    purpose:
      'paymentCustomizationUpdate rejects replacement functionId input and downstream paymentCustomization readback keeps the original functionId.',
    requiredAuthScopes: ['read_payment_customizations', 'write_payment_customizations', 'shopifyFunctions read access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-customization-update-immutable-function.json`,
      'config/parity-specs/payments/payment-customization-update-immutable-function.json',
      'config/parity-requests/payments/payment-customization-immutable-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable payment customization, captures rejected functionId replacement and readback behavior, then deletes the payment customization.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-conformance.mts',
    purpose: 'Admin platform utility roots and staff/access blocker evidence.',
    requiredAuthScopes: ['active Admin API token; staff/utility roots may require plan or staff permissions'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-utility-roots.json`,
      `${CAPTURE_ROOT}admin-platform-taxonomy-hierarchy-node-reads.json`,
      'config/parity-specs/admin-platform/admin-platform-utility-reads.json',
      'config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-body-validation.json',
      'config/parity-specs/admin-platform/admin-platform-taxonomy-hierarchy-node-reads.json',
    ],
    cleanupBehavior: 'Read-only/blocked-root capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-backup-region-update-extended',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-backup-region-update-extended.mts',
    purpose:
      'backupRegionUpdate omitted/null current-state semantics, harry-test-heelo non-CA success, read-after-write, and REGION_NOT_FOUND validation.',
    requiredAuthScopes: ['active Admin API token with Markets/admin platform access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-backup-region-update-extended.json`,
      'config/parity-specs/admin-platform/admin-platform-backup-region-update-extended.json',
    ],
    cleanupBehavior: 'Temporarily stages AE as the backup region, then restores the store backup region to CA.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-backup-region-update-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-backup-region-update-validation.mts',
    purpose: 'backupRegionUpdate MarketUserError typename and region.countryCode input-object coercion validation.',
    requiredAuthScopes: ['active Admin API token with Markets/admin platform access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-backup-region-update-validation.json`,
      'config/parity-specs/admin-platform/admin-platform-backup-region-update-validation.json',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-validation-*.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; requests either short-circuit before resolver execution or return REGION_NOT_FOUND without mutating backup region state.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-refunds',
    scriptPath: 'scripts/capture-order-refund-conformance.mts',
    purpose: 'Order refund calculation/create behavior against disposable orders.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [`${CAPTURE_ROOT}order-refund-*.json`],
    cleanupBehavior: 'Uses disposable orders and records cleanup/cancel evidence where possible.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'orders',
    captureId: 'order-mark-as-paid-state-and-money',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-mark-as-paid-state-and-money-conformance.ts',
    purpose: 'orderMarkAsPaid invalid state validation and MoneyBag presentment-money shape.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderMarkAsPaid-state-and-money.json`,
      'config/parity-specs/orders/orderMarkAsPaid-state-and-money.json',
      'config/parity-requests/orders/orderMarkAsPaid-state-and-money.graphql',
    ],
    cleanupBehavior:
      'Creates disposable unpaid, paid, and multi-currency orders; marks the unpaid orders paid; then records best-effort orderCancel cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-reverse-logistics',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-reverse-logistics-conformance.mts',
    purpose:
      'Return request approval, reverse delivery create/update, reverse fulfillment disposal, return processing, and downstream reverse-logistics reads.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-reverse-logistics-recorded.json`,
      'config/parity-specs/orders/return-reverse-logistics-recorded.json',
    ],
    cleanupBehavior:
      'Creates and fulfills a disposable order, records return/reverse-logistics lifecycle evidence, then cancels the order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-lifecycle-conformance.ts',
    purpose: 'Fulfillment order hold/request/cancel/close lifecycle behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [`${CAPTURE_ROOT}fulfillment-order-lifecycle.json`],
    cleanupBehavior: 'Cancels disposable order and records cleanup captures.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-move-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-move-validation-conformance.ts',
    purpose:
      'fulfillmentOrderMove validation for closed, manually progress-reported, submitted-request, happy full-move, and invalid-destination branches.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-move-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-move-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable orders and a temporary API fulfillment service; rejects the submitted request, cancels orders, and deletes the temporary fulfillment service during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-hold-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-hold-validation-conformance.ts',
    purpose:
      'Fulfillment order hold validation for duplicate handles, max active holds, non-splittable partial holds, invalid quantities, and duplicate line-item inputs.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-hold-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-hold-validation.json',
    ],
    cleanupBehavior: 'Releases created holds when possible, then cancels the disposable order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-split-multi',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-split-multi-conformance.ts',
    purpose: 'fulfillmentOrderSplit multi-input quantity aggregation and indexed validation errors.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-split-multi.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-split-multi.json',
    ],
    cleanupBehavior:
      'Creates disposable orders, captures validation and success branches, merges split fulfillment orders back where possible, then cancels the orders.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-service-delete-transfer',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-delete-transfer-conformance.ts',
    purpose: 'fulfillmentServiceDelete TRANSFER destination validation and valid-delete behavior.',
    requiredAuthScopes: ['read_fulfillments', 'write_fulfillments', 'read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-delete-transfer.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-delete-transfer.json',
    ],
    cleanupBehavior:
      'Creates a disposable destination location and fulfillment service; attempts to deactivate/delete the destination location after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'shipping-user-error-codes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-shipping-user-error-codes-conformance.ts',
    purpose:
      'Typed carrier-service userError.code parity for blank-create and unknown-id update/delete validation branches.',
    requiredAuthScopes: ['read_shipping', 'write_shipping'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}shipping-user-error-codes.json`,
      'config/parity-specs/shipping-fulfillments/shipping-user-error-codes.json',
    ],
    cleanupBehavior: 'No persistent setup or cleanup; all captures are validation-only carrier-service branches.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'delivery-profiles',
    scriptPath: 'scripts/capture-delivery-profile-conformance.ts',
    purpose: 'Delivery profile read/write lifecycle behavior.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}delivery-profile-*.json`,
      'config/parity-specs/shipping-fulfillments/delivery-profile-*.json',
    ],
    cleanupBehavior: 'Removes or restores created delivery profile artifacts; review default-profile protections.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'shipping-settings',
    scriptPath: 'scripts/capture-shipping-settings-conformance.ts',
    purpose: 'Shipping package, local pickup, carrier availability, and constraint-root blocker evidence.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}shipping-settings-package-pickup-constraints.json`,
      'config/parity-specs/shipping-fulfillments/shipping-settings-package-pickup-constraints.json',
    ],
    cleanupBehavior: 'Enables and disables local pickup on an active location to restore the pre-capture setting.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-status-conformance.ts',
    purpose: 'Bulk operation status/catalog/cancel roots.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-status-catalog-cancel.json`,
      'config/parity-specs/bulk-operations/bulk-operation-status-catalog-cancel.json',
    ],
    cleanupBehavior: 'Starts/cancels safe bulk operations where the harness allows it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-query-validators',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-query-validators-conformance.ts',
    purpose: 'bulkOperationRunQuery AdminQuery validator userErrors for validation-only branches.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-query-validators.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-query-validators.json',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify data is created or mutated.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-query-group-objects',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-query-group-objects-conformance.ts',
    purpose:
      'bulkOperationRunQuery explicit groupObjects: true acceptance and omitted groupObjects default success behavior.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-query-group-objects.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-query-group-objects.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-query-group-objects-*.graphql',
    ],
    cleanupBehavior:
      'Starts safe product bulk query exports and polls them to terminal completion; no Shopify catalog data is created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-query-user-error-codes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-query-user-error-codes-conformance.ts',
    purpose:
      'bulkOperationRunQuery selected BulkOperationUserError.code behavior for no-connection and empty-query validation branches.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-query-user-error-codes.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-query-user-error-codes.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-query-with-code.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify data is created or mutated.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-mutation-user-errors',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-mutation-user-errors-conformance.ts',
    purpose:
      'bulkOperationRunMutation BulkMutationUserError code, field, and message behavior for no-such-file, parser, and disallowed-root validation branches.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-mutation-user-errors.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-user-errors.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-user-errors.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify data is created or mutated.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-mutation-created-status',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-mutation-created-status-conformance.ts',
    purpose:
      'bulkOperationRunMutation immediate CREATED response for valid uploaded JSONL plus no-such-file null-operation branch.',
    requiredAuthScopes: ['bulk operation access and product write access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-mutation-created-status.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-created-status.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-created-status.graphql',
    ],
    cleanupBehavior:
      'Uploads one JSONL file, submits one productCreate bulk mutation, waits for terminal status, and deletes the created product when the result JSONL exposes its id.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-in-progress-throttle',
    environment: { SHOPIFY_CONFORMANCE_BULK_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-bulk-operation-in-progress-conformance.ts',
    purpose:
      'bulkOperationRunQuery and bulkOperationRunMutation OPERATION_IN_PROGRESS throttles for two consecutive same-type submissions.',
    requiredAuthScopes: ['bulk operation access through active Admin token', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-query-operation-in-progress.json`,
      `${CAPTURE_ROOT}bulk-operation-run-mutation-operation-in-progress.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-query-operation-in-progress.json',
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-operation-in-progress.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-operation-in-progress.graphql',
    ],
    cleanupBehavior:
      'Cancels captured in-progress bulk operations and best-effort deletes any disposable product created by the bulk mutation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscriptions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-conformance.ts',
    purpose: 'Webhook subscription create/read/delete and access-scope observations.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [`${CAPTURE_ROOT}webhook-subscription-*.json`],
    cleanupBehavior: 'Deletes created API webhook subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-cloud-uri-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-cloud-uri-validation-conformance.ts',
    purpose: 'Webhook subscription EventBridge, Pub/Sub, and Kafka URI validation branches for create/update.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-cloud-uri-validation.json`,
      'config/parity-specs/webhooks/webhook-subscription-cloud-uri-validation.json',
    ],
    cleanupBehavior:
      'Creates one temporary HTTP webhook subscription for update validation, then deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Records current public Admin GraphQL userErrors, including generic URI errors emitted alongside the structural validator messages.',
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-topic-format-name-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-topic-format-name-validation.ts',
    purpose: 'Webhook subscription topic/format, cloud format, name, and duplicate active registration userErrors.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [`${CAPTURE_ROOT}webhook-subscription-topic-format-name-validation.json`],
    cleanupBehavior: 'Creates one temporary SHOP_UPDATE webhook subscription and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-uri-whitespace',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-uri-whitespace.ts',
    purpose: 'Webhook subscription URI whitespace validation branches for create.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-uri-whitespace.json`,
      'config/parity-specs/webhooks/webhook-subscription-uri-whitespace.json',
    ],
    cleanupBehavior:
      'Whitespace-only branch is validation-only; leading/trailing-whitespace HTTPS branch creates a temporary subscription and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-uri-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-uri-validation.ts',
    purpose: 'Webhook subscription URI validation userErrors for create and update.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-uri-validation.json`,
      'config/parity-specs/webhooks/webhook-subscription-uri-validation.json',
      'config/parity-requests/webhooks/webhook-subscription-uri-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one baseline API webhook subscription for invalid update validation, then deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-cards',
    scriptPath: 'scripts/capture-gift-card-conformance.ts',
    purpose:
      'Gift-card read/configuration/count behavior, advanced search filters, and create/update/credit/debit/deactivate lifecycle parity.',
    requiredAuthScopes: [
      'read_gift_cards',
      'write_gift_cards',
      'read_gift_card_transactions',
      'write_gift_card_transactions',
      'read_customers',
      'write_customers',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-lifecycle.json`,
      'config/parity-specs/gift-cards/gift-card-lifecycle.json',
    ],
    cleanupBehavior:
      'Creates a disposable customer and gift card, records transaction/search lifecycle behavior, deletes the customer when possible, and deactivates the gift card; notification roots are not executed.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-create-validation',
    scriptPath: 'scripts/capture-gift-card-create-validation-conformance.ts',
    purpose:
      'Gift-card create validation for initial value, code length/format/uniqueness, missing customer, and generated code behavior.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-create-validation.json`,
      'config/parity-specs/gift-cards/gift-card-create-validation.json',
    ],
    cleanupBehavior:
      'Creates two disposable gift cards for success/generated-code validation and deactivates them during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-create-initial-value-limit',
    scriptPath: 'scripts/capture-gift-card-create-initial-value-limit-conformance.ts',
    purpose:
      'Gift-card create initialValue validation at the configured issue limit, one cent over the issue limit, and a well-over-limit value.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-create-initial-value-limit.json`,
      'config/parity-specs/gift-cards/gift-card-create-initial-value-limit.json',
      'config/parity-requests/gift-cards/gift-card-create-initial-value-limit.graphql',
    ],
    cleanupBehavior:
      'Reads giftCardConfiguration.issueLimit, creates one boundary-success disposable gift card, deactivates any created gift cards during cleanup, and expects over-limit branches to create no gift cards.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-update-validation',
    scriptPath: 'scripts/capture-gift-card-update-validation-conformance.ts',
    purpose:
      'Gift-card update validation for deactivated-card protected fields, empty input, missing changed customerId, recipient text length, and success.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-update-validation.json`,
      'config/parity-specs/gift-cards/gift-card-update-validation.json',
      'config/parity-requests/gift-cards/gift-card-update-validation.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable customers plus active/deactivated gift cards, records validation branches, deactivates setup gift cards, and deletes setup customers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public Admin API exposes giftCardUpdate.userErrors as generic UserError in 2025-01, so the fixture records public field/message evidence and augments replay expectations with the internal typed code contract.',
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-update-noop',
    scriptPath: 'scripts/capture-gift-card-update-noop-conformance.ts',
    purpose:
      'Gift-card update no-op behavior for present note, expiresOn, and templateSuffix fields whose values equal current gift-card state, plus the truly empty input branch.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-update-noop.json`,
      'config/parity-specs/gift-cards/gift-card-update-noop.json',
      'config/parity-requests/gift-cards/gift-card-update-noop.graphql',
      'config/parity-requests/gift-cards/gift-card-update-noop-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable gift card with known editable fields, records no-op update branches, and deactivates the setup gift card.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public Admin API exposes giftCardUpdate.userErrors as generic UserError in 2025-01, so the fixture records public field/message evidence and augments replay expectations with the internal typed code contract for the empty-input branch.',
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-notification-validation',
    scriptPath: 'scripts/capture-gift-card-notification-validation-conformance.ts',
    purpose: 'Gift-card notification validation branches that fail before customer-visible notification dispatch.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-notification-validation.json`,
      'config/parity-specs/gift-cards/gift-card-notification-validation.json',
    ],
    cleanupBehavior:
      'Creates disposable customers and validation-only gift cards, records failing notification responses, deactivates gift cards, and deletes customers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-transaction-validation',
    scriptPath: 'scripts/capture-gift-card-transaction-validation-conformance.ts',
    purpose:
      'Gift-card credit/debit transaction validation for expired, deactivated, mismatched-currency, processedAt bounds, and typed success payloads.',
    requiredAuthScopes: [
      'read_gift_cards',
      'write_gift_cards',
      'read_gift_card_transactions',
      'write_gift_card_transactions',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-transaction-validation.json`,
      'config/parity-specs/gift-cards/gift-card-transaction-validation.json',
      'config/parity-requests/gift-cards/gift-card-transaction-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable active, expired, and deactivated gift cards; deactivates any setup cards not already deactivated during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customers',
    scriptPath: 'scripts/capture-customer-conformance.mts',
    purpose: 'Customer read baselines and nested customer subresources.',
    requiredAuthScopes: ['read_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-*.json`, 'customer read parity specs when promoted'],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-mutations',
    scriptPath: 'scripts/capture-customer-mutation-conformance.mts',
    purpose: 'customerCreate/customerUpdate/customerDelete mutation family.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-mutation-*.json`],
    cleanupBehavior: 'Creates disposable customers and deletes them in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-input-validation',
    scriptPath: 'scripts/capture-customer-input-validation-conformance.ts',
    purpose: 'Customer input validation, normalization, duplicate identity, and downstream read behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'read_customer_merge', 'write_customer_merge'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-input-validation-parity.json`,
      'config/parity-specs/customers/customerInputValidation-parity.json',
      'config/parity-requests/customers/customerInputValidation-*.graphql',
    ],
    cleanupBehavior: 'Creates disposable customers; deletes remaining records after delete and merge probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-input-inline-consent',
    scriptPath: 'scripts/capture-customer-input-consent-conformance.ts',
    purpose: 'CustomerInput inline marketing consent create semantics and update rejection behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-input-inline-consent-parity.json`,
      'config/parity-specs/customers/customerInputInlineConsent-parity.json',
      'config/parity-requests/customers/customerInputInlineConsent-*.graphql',
    ],
    cleanupBehavior: 'Creates one disposable customer, records inline consent create/update behavior, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-update-requires-identity',
    scriptPath: 'scripts/capture-customer-update-requires-identity-conformance.mts',
    purpose: 'customerUpdate rejects changes that would leave a customer without name, phone, or email identity.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-update-requires-identity.json`,
      'config/parity-specs/customers/customer_update_requires_identity.json',
      'config/parity-requests/customers/customer_update_requires_identity_*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable email-only, phone-only, and name-pair customers, records rejection/control branches, then deletes them.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-input-addresses',
    scriptPath: 'scripts/capture-customer-input-addresses-conformance.mts',
    purpose: 'CustomerInput.addresses create/update replacement behavior and downstream reads.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-input-addresses-parity.json`,
      'config/parity-specs/customers/customerInputAddresses-parity.json',
      'config/parity-requests/customers/customer-input-addresses-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer, records address-list create/update/read behavior, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-address-country-province-validation',
    scriptPath: 'scripts/capture-customer-address-country-province-validation.mts',
    purpose:
      'Customer address country/province Atlas validation and normalization across CustomerInput and dedicated address mutations.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-address-country-province-validation.json`,
      'config/parity-specs/customers/customer_address_country_province_validation.json',
      'config/parity-requests/customers/customer-address-country-province-*.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customers for valid, display-conflict, and no-zone branches; deletes them during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Captured evidence shows countryCode wins over conflicting country display text and SG province input is ignored because SG has no zones.',
  },
  {
    domain: 'customers',
    captureId: 'customer-account-page-data-erasure',
    scriptPath: 'scripts/capture-customer-account-page-data-erasure-conformance.ts',
    purpose: 'Customer Account page reads plus customer data-erasure request/cancel success and validation payloads.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'write_customer_data_erasure'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-account-page-data-erasure.json`,
      'config/parity-specs/customers/customer-account-page-data-erasure.json',
    ],
    cleanupBehavior:
      'Creates a disposable customer, requests and cancels data erasure, then cancels again and deletes the customer in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'store-credit',
    scriptPath: 'scripts/capture-store-credit-conformance.ts',
    purpose: 'Store credit account creation setup, account-id credit/debit mutations, and downstream balance reads.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'store credit account access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}store-credit-account-parity.json`,
      'config/parity-specs/customers/store-credit-account-local-staging.json',
    ],
    cleanupBehavior:
      'Creates a disposable customer, credits/debits a real store credit account, debits the remaining balance back to zero, then deletes the customer.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-set',
    scriptPath: 'scripts/capture-customer-set-conformance.mts',
    purpose: 'customerSet upsert/identifier semantics.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-set-*.json`],
    cleanupBehavior: 'Tracks all created/upserted customer IDs and deletes remaining records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-addresses',
    scriptPath: 'scripts/capture-customer-address-conformance.mts',
    purpose: 'Customer address lifecycle, normalization, defaulting, id matching, and validation.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-address-*.json`,
      'config/parity-specs/customers/customer_address_update_id_mismatch.json',
      'config/parity-requests/customers/customer-address-update-id-mismatch-read.graphql',
    ],
    cleanupBehavior: 'Creates disposable customers/addresses and deletes the customers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-merge',
    scriptPath: 'scripts/capture-customer-merge-conformance.mts',
    purpose: 'Base two-customer customerMerge behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'read_customer_merge', 'write_customer_merge'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-merge-parity.json`,
      'config/parity-specs/customers/customerMerge-parity.json',
    ],
    cleanupBehavior: 'Creates disposable customers; merge consumes source records and cleanup removes leftovers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-merge-attached-resources',
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
      'config/parity-specs/customers/customerMerge-attached-resources-parity.json',
    ],
    cleanupBehavior:
      'Creates disposable customer graph; merge consumes source and cleanup removes remaining artifacts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-consent',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-customer-consent-conformance.ts',
    purpose: 'Email/SMS marketing consent update behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-email-marketing-consent-update-parity.json`,
      `${CAPTURE_ROOT}customer-sms-marketing-consent-update-parity.json`,
      'config/parity-specs/customers/customerEmailMarketingConsentUpdate-disallowed-states-parity.json',
      'config/parity-specs/customers/customerSmsMarketingConsentUpdate-disallowed-states-parity.json',
    ],
    cleanupBehavior: 'Creates and deletes disposable customers for consent transitions.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-tax-exemptions',
    scriptPath: 'scripts/capture-customer-tax-exemption-conformance.ts',
    purpose: 'Customer tax exemption update behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-tax-exemption-*.json`],
    cleanupBehavior: 'Creates disposable customer and deletes it after tax-exemption probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-order-summary',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-customer-order-summary-conformance.ts',
    purpose: 'Customer order summary reads against order-linked customer state.',
    requiredAuthScopes: ['read_customers', 'read_orders', 'write_orders'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-order-summary-*.json`],
    cleanupBehavior: 'Creates disposable order/customer state and records cleanup/cancel result.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-outbound-email',
    scriptPath: 'scripts/capture-customer-outbound-email-conformance.mts',
    purpose: 'Validation payloads for customer outbound email side-effect roots without sending real email.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [`${CAPTURE_ROOT}customer-outbound-side-effect-validation-parity.json`],
    cleanupBehavior: 'Validation-only unknown-ID capture; no created Shopify resources to clean up.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-invite-email-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-customer-invite-email-validation-conformance.ts',
    purpose:
      'customerSendAccountInviteEmail nested EmailInput validation for subject, to, from, bcc, and customMessage.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-invite-email-validation.json`,
      'config/parity-specs/customers/customer_invite_email_validation.json',
      'config/parity-requests/customers/customer-invite-email-validation-*.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer per validation branch and deletes all created customers during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The valid phone-customer to override control remains runtime-test-backed because the live conformance shop currently returns a generic outbound-delivery failure for success-path invite attempts.',
  },
]);

export function loadConformanceCaptureScriptPaths(repoRoot = process.cwd()): string[] {
  return readdirSync(path.join(repoRoot, 'scripts'))
    .filter((fileName) => /^capture-.*\.(?:ts|mts)$/u.test(fileName))
    .map((fileName) => `scripts/${fileName}`)
    .sort();
}

export function validateCaptureIndexAgainstScriptFiles(
  entries: ConformanceCaptureIndexEntry[] = conformanceCaptureIndex,
  scriptPaths: string[] = loadConformanceCaptureScriptPaths(),
): { duplicateCaptureIds: string[]; missingFromIndex: string[]; missingFromDisk: string[] } {
  const indexedPaths = new Set(entries.map((entry) => entry.scriptPath));
  const diskPaths = new Set(scriptPaths);
  const captureIdCounts = new Map<string, number>();

  for (const entry of entries) {
    captureIdCounts.set(entry.captureId, (captureIdCounts.get(entry.captureId) ?? 0) + 1);
  }

  return {
    duplicateCaptureIds: [...captureIdCounts]
      .filter(([, count]) => count > 1)
      .map(([captureId]) => captureId)
      .sort(),
    missingFromIndex: scriptPaths.filter((scriptPath) => !indexedPaths.has(scriptPath)).sort(),
    missingFromDisk: entries
      .map((entry) => entry.scriptPath)
      .filter((scriptPath) => !diskPaths.has(scriptPath))
      .sort(),
  };
}

export function renderCaptureIndexMarkdown(entries: ConformanceCaptureIndexEntry[] = conformanceCaptureIndex): string {
  const lines = [
    '# Conformance Capture Runner Index',
    '',
    'Run capture scripts directly with `corepack pnpm exec tsx <scriptPath>`, or run one through the meta runner with `corepack pnpm conformance:capture -- --run <captureId>` after `corepack pnpm conformance:probe` confirms the active Shopify credential and store.',
    '',
  ];

  const domains = [...new Set(entries.map((entry) => entry.domain))].sort();
  for (const domain of domains) {
    lines.push(`## ${domain}`, '');
    lines.push(
      '| Capture ID | Meta runner | Direct script | Purpose | Required auth/scopes | Outputs | Cleanup | Status checks |',
    );
    lines.push('| --- | --- | --- | --- | --- | --- | --- | --- |');

    for (const entry of entries.filter((candidate) => candidate.domain === domain)) {
      const cells = [
        `\`${entry.captureId}\``,
        `\`${renderRunnerCommand(entry)}\``,
        `\`${renderDirectCommand(entry)}\``,
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

function renderEnvironmentPrefix(entry: ConformanceCaptureIndexEntry): string {
  return Object.entries(entry.environment ?? {})
    .map(([key, value]) => `${key}=${value}`)
    .join(' ');
}

function renderDirectCommand(entry: ConformanceCaptureIndexEntry): string {
  const environmentPrefix = renderEnvironmentPrefix(entry);
  const command = `corepack pnpm exec tsx ./${entry.scriptPath}`;
  return environmentPrefix ? `${environmentPrefix} ${command}` : command;
}

function renderRunnerCommand(entry: ConformanceCaptureIndexEntry): string {
  return `corepack pnpm conformance:capture -- --run ${entry.captureId}`;
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
  const script = readFlagValue(args, '--script');
  const run = readFlagValue(args, '--run');
  const outputJson = args.includes('--json');
  const validation = validateCaptureIndexAgainstScriptFiles();
  if (
    validation.duplicateCaptureIds.length > 0 ||
    validation.missingFromIndex.length > 0 ||
    validation.missingFromDisk.length > 0
  ) {
    throw new Error(`Conformance capture index is out of sync: ${JSON.stringify(validation, null, 2)}`);
  }

  if (run) {
    const entry = findEntry(run);
    if (!entry) {
      throw new Error(`Unknown conformance capture script: ${run}`);
    }

    const result = spawnSync('tsx', [`./${entry.scriptPath}`], {
      env: { ...process.env, ...entry.environment },
      shell: process.platform === 'win32',
      stdio: 'inherit',
    });
    process.exit(typeof result.status === 'number' ? result.status : 1);
  }

  let entries = conformanceCaptureIndex;
  if (domain) {
    entries = entries.filter((entry) => entry.domain === domain);
  }
  if (script) {
    entries = entries.filter((entry) => entry.captureId === script || entry.scriptPath === script);
  }

  process.stdout.write(outputJson ? `${JSON.stringify(entries, null, 2)}\n` : renderCaptureIndexMarkdown(entries));
}

function findEntry(script: string): ConformanceCaptureIndexEntry | undefined {
  return conformanceCaptureIndex.find((entry) => entry.captureId === script || entry.scriptPath === script);
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  runCli();
}
